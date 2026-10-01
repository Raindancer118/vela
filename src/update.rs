//! System updates (Settings → Updates): what is pending (pacman through
//! `checkupdates`, AUR through paru/yay, Flatpak), the commands for each mode
//! (everything, only the package database, selected packages), running them
//! into a log file, and the prompt that lets Claude Code repair a failed run.

use crate::config::{Claude, Terminal};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Repo,
    Aur,
    Flatpak,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Repo => "Repository",
            Source::Aur => "AUR",
            Source::Flatpak => "Flatpak",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub source: Source,
    pub name: String,
    /// Installed version (may be empty for Flatpak).
    pub old: String,
    pub new: String,
    /// What the update command takes: the package name, or the Flatpak ref.
    pub id: String,
}

/// `name old -> new` lines of `checkupdates` and `paru -Qua`; ignored
/// packages (`[ignored]`) and anything else (warnings) are skipped.
pub fn parse_package_updates(text: &str, source: Source) -> Vec<Pending> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let (name, old, arrow, new) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            if arrow != "->" || parts.next() == Some("[ignored]") {
                return None;
            }
            Some(Pending {
                source,
                name: name.to_owned(),
                old: old.to_owned(),
                new: new.to_owned(),
                id: name.to_owned(),
            })
        })
        .collect()
}

/// `flatpak remote-ls --updates --columns=application,version,ref` with the
/// installed versions from `flatpak list --columns=application,version,ref`.
pub fn parse_flatpak_updates(updates: &str, installed: &str) -> Vec<Pending> {
    // remote-ls prefixes refs with app/ or runtime/, list doesn't.
    let bare = |r: &str| r.strip_prefix("app/").or_else(|| r.strip_prefix("runtime/")).unwrap_or(r).to_owned();
    let versions: std::collections::HashMap<String, String> = installed
        .lines()
        .filter_map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            Some((bare(cols.get(2)?), cols.get(1)?.trim().to_owned()))
        })
        .collect();
    updates
        .lines()
        .filter_map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            let (name, new, id) = (cols.first()?.trim(), cols.get(1)?.trim(), cols.get(2)?.trim());
            if name.is_empty() || id.is_empty() {
                return None;
            }
            Some(Pending {
                source: Source::Flatpak,
                name: name.to_owned(),
                old: versions.get(&bare(id)).cloned().unwrap_or_default(),
                new: new.to_owned(),
                id: id.to_owned(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Packages, AUR and Flatpaks.
    Full,
    /// Only sync the package database (and Flatpak's app info).
    Database,
    Selected(Vec<Pending>),
}

impl Mode {
    pub fn label(&self) -> &'static str {
        match self {
            Mode::Full => "Updating everything",
            Mode::Database => "Refreshing the package database",
            Mode::Selected(_) => "Updating selected apps",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Helper {
    Paru,
    Yay,
}

/// The programs found on this system (absolute paths).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tools {
    pub pacman: Option<String>,
    pub sudo: Option<String>,
    pub checkupdates: Option<String>,
    pub helper: Option<(Helper, String)>,
    pub flatpak: Option<String>,
}

impl Tools {
    pub fn detect(cfg: &crate::config::Updates) -> Tools {
        let find = |name: &str| crate::paths::find_executable(name).map(|p| p.to_string_lossy().into_owned());
        let helper = if cfg.aur {
            find("paru").map(|p| (Helper::Paru, p)).or_else(|| find("yay").map(|p| (Helper::Yay, p)))
        } else {
            None
        };
        Tools {
            pacman: find("pacman"),
            sudo: find("sudo"),
            checkupdates: find("checkupdates"),
            helper,
            flatpak: if cfg.flatpak { find("flatpak") } else { None },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub title: String,
    pub argv: Vec<String>,
}

/// The commands of one run, in order; the run stops at the first failure.
pub fn plan(mode: &Mode, tools: &Tools) -> Result<Vec<Step>, String> {
    let pacman = tools.pacman.clone().ok_or("pacman was not found")?;
    let sudo = || tools.sudo.clone().ok_or_else(|| "sudo was not found".to_owned());
    let as_root = |args: &[&str]| -> Result<Vec<String>, String> {
        let mut argv = vec![sudo()?, "-A".into(), pacman.clone()];
        argv.extend(args.iter().map(|a| a.to_string()));
        Ok(argv)
    };
    // The helper calls sudo itself; -A makes it ask through SUDO_ASKPASS.
    let helper = |args: &[&str]| {
        tools.helper.as_ref().map(|(h, path)| {
            let mut argv = vec![path.clone()];
            argv.extend(args.iter().map(|a| a.to_string()));
            argv.extend(["--color", "never", "--sudoflags", "-A"].map(String::from));
            match h {
                Helper::Paru => argv.push("--skipreview".into()),
                Helper::Yay => argv.extend(["--answerdiff", "None", "--answerclean", "None"].map(String::from)),
            }
            argv
        })
    };
    let step = |title: &str, argv: Vec<String>| Step { title: title.to_owned(), argv };
    let mut steps = Vec::new();
    match mode {
        Mode::Full => {
            match helper(&["-Syu", "--noconfirm"]) {
                Some(argv) => steps.push(step("Packages and AUR", argv)),
                None => steps.push(step("Packages", as_root(&["-Syu", "--noconfirm"])?)),
            }
            if let Some(fp) = &tools.flatpak {
                steps.push(step("Flatpak apps", vec![fp.clone(), "update".into(), "-y".into(), "--noninteractive".into()]));
            }
        }
        Mode::Database => {
            steps.push(step("Package database", as_root(&["-Sy"])?));
            if let Some(fp) = &tools.flatpak {
                steps.push(step(
                    "Flatpak app info",
                    [fp.as_str(), "update", "--appstream", "-y", "--noninteractive"].map(String::from).to_vec(),
                ));
            }
        }
        Mode::Selected(items) => {
            if items.is_empty() {
                return Err("nothing selected".into());
            }
            if let Some(bad) = items.iter().find(|p| p.id.is_empty() || p.id.starts_with('-')) {
                return Err(format!("invalid package name {:?}", bad.id));
            }
            let ids = |src: &[Source]| -> Vec<String> { items.iter().filter(|p| src.contains(&p.source)).map(|p| p.id.clone()).collect() };
            let packages = ids(&[Source::Repo, Source::Aur]);
            if !packages.is_empty() {
                let base = ["-Sy", "--needed", "--noconfirm"];
                let mut argv = match helper(&base) {
                    Some(argv) => argv,
                    None if items.iter().any(|p| p.source == Source::Aur) => return Err("AUR packages need paru or yay".into()),
                    None => as_root(&base)?,
                };
                argv.extend(packages);
                steps.push(step("Packages", argv));
            }
            let refs = ids(&[Source::Flatpak]);
            if !refs.is_empty() {
                let fp = tools.flatpak.clone().ok_or("flatpak was not found")?;
                let mut argv = vec![fp, "update".into(), "-y".into(), "--noninteractive".into()];
                argv.extend(refs);
                steps.push(step("Flatpak apps", argv));
            }
        }
    }
    Ok(steps)
}

/// The lines that explain a failure: pacman/makepkg/flatpak error lines, or
/// the last lines of output when there are none.
pub fn summarize(log: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let errors: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.starts_with("error") || lower.starts_with("fehler") || lower.contains("errors occurred") || lower.starts_with("==> error")
        })
        .filter(|l| seen.insert(*l))
        .take(8)
        .collect();
    if !errors.is_empty() {
        return errors.join("\n");
    }
    let lines: Vec<&str> = log.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines[lines.len().saturating_sub(3)..].join("\n")
}

/// The last `n` lines.
pub fn tail(text: &str, n: usize) -> &str {
    let body = text.strip_suffix('\n').unwrap_or(text);
    match body.rmatch_indices('\n').nth(n.saturating_sub(1)) {
        Some((i, _)) if n > 0 => &text[i + 1..],
        _ if n == 0 => "",
        _ => text,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub mode: String,
    pub step: String,
    pub command: Vec<String>,
    /// None = killed by a signal or could not start.
    pub code: Option<i32>,
    pub summary: String,
    pub log_path: PathBuf,
}

/// What Claude gets to fix a failed run.
pub fn fix_prompt(f: &Failure, log_tail: &str) -> String {
    let command = shlex::try_join(f.command.iter().map(String::as_str)).unwrap_or_else(|_| f.command.join(" "));
    let code = f.code.map_or("no exit code (killed or not started)".to_owned(), |c| format!("exit code {c}"));
    format!(
        "A system update started from vela's update window failed on this Arch-based system (CachyOS, Hyprland). \
Find the cause and fix it, then finish the update.\n\n\
Run: {mode}\nFailed step: {step}\nCommand: {command}\nResult: {code}\n\n\
Error lines:\n{summary}\n\n\
Full log: {log}\nLast lines of the log:\n```\n{log_tail}\n```\n\n\
Notes:\n\
- There is no terminal password prompt you can answer: run root commands with `sudo -A` (SUDO_ASKPASS shows a password dialog).\n\
- Never delete user data, never remove packages the user installed on purpose without asking, and don't use \
`--overwrite` or remove the pacman lock file before checking that no pacman process is running.\n\
- Prefer the fix the Arch Wiki/news recommends (e.g. keyring updates, conflicting files, replaced packages).\n\
- When fixed, run the failed command again, then `vela updates check` so vela's update window shows the new state.\n\
- Finish with a short summary of what was wrong and what you changed.",
        mode = f.mode,
        step = f.step,
        summary = f.summary,
        log = f.log_path.display(),
    )
}

/// `cd <dir> && exec [env SUDO_ASKPASS=…] <terminal> … claude … -- <prompt>`
/// for Hyprland's `exec_cmd`, which runs it through `sh -c`.
pub fn claude_shell_command(claude: &Claude, term: &Terminal, prompt: &str, askpass: Option<&Path>) -> Result<String, String> {
    let spec = crate::launch::claude_spec(claude, term, prompt).map_err(|e| e.to_string())?;
    let quote = |parts: Vec<String>| shlex::try_join(parts.iter().map(String::as_str)).map_err(|e| e.to_string());
    let cwd = spec.cwd.filter(|d| d.is_dir()).unwrap_or_else(crate::paths::home_dir);
    let mut argv = Vec::new();
    if let Some(p) = askpass {
        argv.extend(["env".to_owned(), format!("SUDO_ASKPASS={}", p.display())]);
    }
    argv.extend(spec.argv);
    Ok(format!("cd {} && exec {}", quote(vec![cwd.to_string_lossy().into_owned()])?, quote(argv)?))
}

/// Lua for `hyprctl eval`: run the command on `workspace` without switching
/// there or taking the focus. An empty workspace opens it normally.
pub fn exec_lua(command: &str, workspace: &str) -> String {
    use crate::hyprconf::lua_string;
    let workspace = workspace.trim();
    if workspace.is_empty() {
        return format!("hl.exec_cmd({})", lua_string(command));
    }
    format!(
        "hl.exec_cmd({}, {{ workspace = {} }})",
        lua_string(command),
        lua_string(&format!("{workspace} silent"))
    )
}

/// Lua that moves a window onto the active workspace and focuses it.
pub fn show_window_lua(address: &str) -> String {
    let hex: String = address.trim_start_matches("0x").chars().filter(char::is_ascii_hexdigit).collect();
    format!(
        "local w = hl.get_window(\"address:0x{hex}\") local ws = hl.get_active_workspace() \
if w and ws then hl.dispatch(hl.dsp.window.move({{ workspace = ws.id, window = w }})) hl.dispatch(hl.dsp.focus({{ window = w }})) end"
    )
}

/// Addresses of the windows on a workspace, from `j/clients`.
pub fn windows_on(clients_json: &str, workspace: &str) -> Vec<String> {
    let list: Vec<serde_json::Value> = serde_json::from_str(clients_json).unwrap_or_default();
    list.iter()
        .filter(|c| c.pointer("/workspace/name").and_then(|n| n.as_str()) == Some(workspace))
        .filter_map(|c| Some(c.get("address")?.as_str()?.to_owned()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Step {
        index: usize,
        count: usize,
        title: String,
    },
    /// New output (whole lines), also written to the log.
    Output(String),
}

/// Runs the steps one after another with all output in `log_path`. The
/// commands write straight into the file (not a pipe): if vela dies
/// mid-run, pacman must not get SIGPIPE in the middle of a transaction.
/// `use_scope` puts each step into its own systemd scope so that restarting
/// vela.service doesn't kill it either.
pub fn run(mode_label: &str, steps: &[Step], log_path: &Path, use_scope: bool, on: &mut dyn FnMut(Event)) -> Result<(), Box<Failure>> {
    use std::io::Write;
    let failure = |step: &Step, code: Option<i32>, summary: String| {
        Box::new(Failure {
            mode: mode_label.to_owned(),
            step: step.title.clone(),
            command: step.argv.clone(),
            code,
            summary,
            log_path: log_path.to_owned(),
        })
    };
    let first = steps.first().ok_or_else(|| {
        failure(
            &Step {
                title: String::new(),
                argv: Vec::new(),
            },
            None,
            "nothing to do".into(),
        )
    })?;
    let open = || -> std::io::Result<(std::fs::File, std::fs::File)> {
        if let Some(dir) = log_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let writer = std::fs::OpenOptions::new().create(true).append(true).open(log_path)?;
        let mut reader = std::fs::File::open(log_path)?;
        std::io::Seek::seek(&mut reader, std::io::SeekFrom::End(0))?;
        Ok((writer, reader))
    };
    let (mut log, mut reader) = open().map_err(|e| failure(first, None, format!("cannot write {}: {e}", log_path.display())))?;
    let mut partial = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        on(Event::Step {
            index,
            count: steps.len(),
            title: step.title.clone(),
        });
        let mut output = String::new();
        let shown = shlex::try_join(step.argv.iter().map(String::as_str)).unwrap_or_else(|_| step.argv.join(" "));
        let _ = writeln!(log, "==> {}\n$ {shown}", step.title);
        let status = spawn_into(&step.argv, &log, use_scope).map(|mut child| {
            loop {
                drain(&mut reader, &mut partial, &mut output, on);
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(150)),
                    Err(_) => break None,
                }
            }
        });
        let status = match status {
            Ok(s) => s,
            Err(e) => {
                let _ = writeln!(log, "vela: cannot start {}: {e}", step.argv.first().map_or("", String::as_str));
                None
            }
        };
        let _ = writeln!(log);
        drain(&mut reader, &mut partial, &mut output, on);
        if !status.is_some_and(|s| s.success()) {
            return Err(failure(step, status.and_then(|s| s.code()), summarize(&output)));
        }
    }
    Ok(())
}

fn spawn_into(argv: &[String], log: &std::fs::File, use_scope: bool) -> std::io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    let argv = if use_scope {
        crate::launch::wrap_in_scope(&crate::launch::SpawnSpec {
            argv: argv.to_vec(),
            name: "update".into(),
            ..Default::default()
        })
    } else {
        argv.to_vec()
    };
    let (program, args) = argv.split_first().ok_or_else(|| std::io::Error::other("empty command"))?;
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log.try_clone()?)
        // English messages: the error lines are what Claude and summarize() read.
        .env("LC_ALL", "C.UTF-8")
        .env("PATH", crate::paths::child_path_env());
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()
}

/// Reads what was appended to the log and reports whole lines.
fn drain(reader: &mut std::fs::File, partial: &mut Vec<u8>, output: &mut String, on: &mut dyn FnMut(Event)) {
    use std::io::Read;
    if reader.read_to_end(partial).is_err() {
        return;
    }
    let Some(end) = partial.iter().rposition(|b| *b == b'\n') else { return };
    let lines: Vec<u8> = partial.drain(..=end).collect();
    let text = String::from_utf8_lossy(&lines).into_owned();
    output.push_str(&text);
    // Build logs can be huge; the summary only needs the end.
    if output.len() > 1 << 20 {
        let cut = output.len() - (1 << 19);
        let cut = (cut..output.len()).find(|i| output.is_char_boundary(*i)).unwrap_or(0);
        output.drain(..cut);
    }
    on(Event::Output(text));
}

/// Pending updates from every available source, sorted; plus one message per
/// source that could not be asked.
pub fn check(tools: &Tools) -> (Vec<Pending>, Vec<String>) {
    fn output(argv: &[&str]) -> Result<(Option<i32>, String, String), String> {
        let out = std::process::Command::new(argv[0])
            .args(&argv[1..])
            .stdin(std::process::Stdio::null())
            .env("LC_ALL", "C.UTF-8")
            .env("PATH", crate::paths::child_path_env())
            .output()
            .map_err(|e| format!("{}: {e}", argv[0]))?;
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        Ok((out.status.code(), text(&out.stdout), text(&out.stderr)))
    }
    fn reason(code: Option<i32>, stderr: &str) -> String {
        stderr
            .lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty())
            .map_or_else(|| format!("exit code {}", code.map_or("?".into(), |c| c.to_string())), str::to_owned)
    }
    let (mut pending, mut errors) = (Vec::new(), Vec::new());
    match &tools.checkupdates {
        None => errors.push("Repositories: checkupdates was not found (install pacman-contrib)".to_owned()),
        Some(cu) => match output(&[cu, "--nocolor"]) {
            // 2 = no updates.
            Ok((Some(0 | 2), out, _)) => pending.extend(parse_package_updates(&out, Source::Repo)),
            Ok((code, _, err)) => errors.push(format!("Repositories: {}", reason(code, &err))),
            Err(e) => errors.push(format!("Repositories: {e}")),
        },
    }
    if let Some((_, helper)) = &tools.helper {
        match output(&[helper, "-Qua", "--color", "never"]) {
            Ok((code, out, err)) => {
                let found = parse_package_updates(&out, Source::Aur);
                // Exit code 1 without output only means "nothing to update".
                if found.is_empty() && code != Some(0) && err.to_lowercase().contains("error") {
                    errors.push(format!("AUR: {}", reason(code, &err)));
                }
                pending.extend(found);
            }
            Err(e) => errors.push(format!("AUR: {e}")),
        }
    }
    if let Some(fp) = &tools.flatpak {
        let updates = output(&[fp, "remote-ls", "--updates", "--columns=application,version,ref"]);
        let installed = output(&[fp, "list", "--columns=application,version,ref"]);
        match (updates, installed) {
            (Ok((Some(0), up, _)), Ok((_, inst, _))) => pending.extend(parse_flatpak_updates(&up, &inst)),
            (Ok((code, _, err)), _) => errors.push(format!("Flatpak: {}", reason(code, &err))),
            (Err(e), _) => errors.push(format!("Flatpak: {e}")),
        }
    }
    pending.sort_by(|a, b| (a.source, &a.name).cmp(&(b.source, &b.name)));
    (pending, errors)
}

/// Where the run started at `now_secs` logs; keeps the newest `keep` logs.
pub fn new_log_path(dir: &Path, now_secs: u64, keep: usize) -> PathBuf {
    let mut logs: Vec<(u64, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let n = name.to_str()?.strip_prefix("update-")?.strip_suffix(".log")?.parse().ok()?;
            Some((n, e.path()))
        })
        .collect();
    logs.sort();
    let excess = (logs.len() + 1).saturating_sub(keep.max(1));
    for (_, old) in logs.into_iter().take(excess) {
        let _ = std::fs::remove_file(old);
    }
    dir.join(format!("update-{now_secs}.log"))
}

pub fn log_dir() -> PathBuf {
    crate::paths::state_dir().join("updates")
}

/// State for the control center (`vela updates watch`).
pub fn status_file() -> PathBuf {
    crate::paths::cache_dir().join("updates.json")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// ms since epoch, 0 = never.
    pub checked_at: i64,
    pub checking: bool,
    pub pending: Vec<Pending>,
    pub check_errors: Vec<String>,
    /// Title of the running step.
    pub running: Option<String>,
    /// Summary of the last failed run (cleared by the next check or run).
    pub failed: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Tools {
        Tools {
            pacman: Some("/usr/bin/pacman".into()),
            sudo: Some("/usr/bin/sudo".into()),
            checkupdates: Some("/usr/bin/checkupdates".into()),
            helper: Some((Helper::Paru, "/usr/bin/paru".into())),
            flatpak: Some("/usr/bin/flatpak".into()),
        }
    }

    fn pending(source: Source, name: &str, id: &str) -> Pending {
        Pending {
            source,
            name: name.into(),
            old: "1".into(),
            new: "2".into(),
            id: id.into(),
        }
    }

    #[test]
    fn package_lists() {
        let text = "brave-bin 1:1.96.59-1 -> 1:1.96.60-1\n\
                    cifs-utils 7.5-1.1 -> 7.8-1.1\n\
                    linux 6.1-1 -> 6.2-1 [ignored]\n\
                    warning: something odd\n\
                    :: Synchronizing package databases...\n\n";
        let got = parse_package_updates(text, Source::Repo);
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0],
            Pending {
                source: Source::Repo,
                name: "brave-bin".into(),
                old: "1:1.96.59-1".into(),
                new: "1:1.96.60-1".into(),
                id: "brave-bin".into(),
            }
        );
        assert_eq!(parse_package_updates("cachy-update 4.4.1-1 -> 4.4.2-1", Source::Aur)[0].source, Source::Aur);
        assert!(parse_package_updates("", Source::Repo).is_empty());
    }

    #[test]
    fn flatpak_lists() {
        let updates = "org.freedesktop.Platform.GL.nvidia-615\t\truntime/org.freedesktop.Platform.GL.nvidia-615/x86_64/1.4\n\
                       ch.threema.threema-web-desktop\t1.2.50\tapp/ch.threema.threema-web-desktop/x86_64/stable\n";
        let installed = "ch.threema.threema-web-desktop\t1.2.49\tch.threema.threema-web-desktop/x86_64/stable\tsystem\n";
        let got = parse_flatpak_updates(updates, installed);
        assert_eq!(got.len(), 2);
        let threema = got.iter().find(|p| p.name == "ch.threema.threema-web-desktop").unwrap();
        assert_eq!((threema.old.as_str(), threema.new.as_str()), ("1.2.49", "1.2.50"));
        assert_eq!(threema.id, "app/ch.threema.threema-web-desktop/x86_64/stable");
        assert_eq!(threema.source, Source::Flatpak);
        let gl = got.iter().find(|p| p.name.contains("nvidia")).unwrap();
        assert_eq!((gl.old.as_str(), gl.new.as_str()), ("", ""));
    }

    #[test]
    fn full_update_plan() {
        let steps = plan(&Mode::Full, &tools()).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0].argv,
            ["/usr/bin/paru", "-Syu", "--noconfirm", "--color", "never", "--sudoflags", "-A", "--skipreview"]
        );
        assert_eq!(steps[1].argv, ["/usr/bin/flatpak", "update", "-y", "--noninteractive"]);

        let mut t = tools();
        t.helper = None;
        t.flatpak = None;
        let steps = plan(&Mode::Full, &t).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].argv, ["/usr/bin/sudo", "-A", "/usr/bin/pacman", "-Syu", "--noconfirm"]);

        t.helper = Some((Helper::Yay, "/usr/bin/yay".into()));
        let yay = &plan(&Mode::Full, &t).unwrap()[0].argv;
        assert!(yay.starts_with(&["/usr/bin/yay".to_owned(), "-Syu".into(), "--noconfirm".into()]));
        assert!(yay.contains(&"--answerdiff".to_owned()) && !yay.contains(&"--skipreview".to_owned()));

        assert!(plan(&Mode::Full, &Tools::default()).is_err(), "no pacman");
    }

    #[test]
    fn database_plan() {
        let steps = plan(&Mode::Database, &tools()).unwrap();
        assert_eq!(steps[0].argv, ["/usr/bin/sudo", "-A", "/usr/bin/pacman", "-Sy"]);
        assert_eq!(steps[1].argv, ["/usr/bin/flatpak", "update", "--appstream", "-y", "--noninteractive"]);
        let mut t = tools();
        t.sudo = None;
        assert!(plan(&Mode::Database, &t).is_err(), "no sudo");
    }

    #[test]
    fn selected_plan() {
        let sel = Mode::Selected(vec![
            pending(Source::Repo, "firefox", "firefox"),
            pending(Source::Flatpak, "org.x.App", "app/org.x.App/x86_64/stable"),
            pending(Source::Aur, "brave-bin", "brave-bin"),
        ]);
        let steps = plan(&sel, &tools()).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0].argv,
            [
                "/usr/bin/paru",
                "-Sy",
                "--needed",
                "--noconfirm",
                "--color",
                "never",
                "--sudoflags",
                "-A",
                "--skipreview",
                "firefox",
                "brave-bin"
            ]
        );
        assert_eq!(
            steps[1].argv,
            ["/usr/bin/flatpak", "update", "-y", "--noninteractive", "app/org.x.App/x86_64/stable"]
        );

        // Repository packages work without a helper, AUR ones don't.
        let mut t = tools();
        t.helper = None;
        let only_repo = Mode::Selected(vec![pending(Source::Repo, "firefox", "firefox")]);
        assert_eq!(
            plan(&only_repo, &t).unwrap()[0].argv,
            ["/usr/bin/sudo", "-A", "/usr/bin/pacman", "-Sy", "--needed", "--noconfirm", "firefox"]
        );
        assert!(plan(&sel, &t).is_err());
        assert!(plan(&Mode::Selected(vec![]), &tools()).is_err(), "nothing selected");
        // An id must never be read as an option.
        assert!(plan(&Mode::Selected(vec![pending(Source::Repo, "-x", "--overwrite=*")]), &tools()).is_err());
    }

    #[test]
    fn summaries() {
        let log = "==> Packages\n:: Starte Update...\n\
                   error: failed to commit transaction (conflicting files)\n\
                   python-foo: /usr/lib/x exists in filesystem\n\
                   Errors occurred, no packages were upgraded.\n\
                   error: failed to commit transaction (conflicting files)\n";
        let s = summarize(log);
        assert!(s.contains("error: failed to commit transaction (conflicting files)"));
        assert_eq!(s.matches("failed to commit").count(), 1, "deduplicated: {s}");
        assert!(s.contains("Errors occurred"));

        let plain = summarize("one\ntwo\n\nthree\nfour\n\n");
        assert_eq!(plain, "two\nthree\nfour");

        assert_eq!(tail("a\nb\nc\n", 2), "b\nc\n");
        assert_eq!(tail("a\nb", 5), "a\nb");
    }

    #[test]
    fn prompt_carries_everything_needed() {
        let f = Failure {
            mode: "Updating everything".into(),
            step: "Packages and AUR".into(),
            command: vec!["/usr/bin/paru".into(), "-Syu".into()],
            code: Some(1),
            summary: "error: failed to commit transaction".into(),
            log_path: "/home/u/.local/state/vela/updates/update-1.log".into(),
        };
        let p = fix_prompt(&f, "LAST LINES");
        for needle in [
            "/usr/bin/paru -Syu",
            "exit code 1",
            "error: failed to commit transaction",
            "/home/u/.local/state/vela/updates/update-1.log",
            "LAST LINES",
            "sudo -A",
            "vela updates check",
        ] {
            assert!(p.contains(needle), "missing {needle}:\n{p}");
        }
    }

    #[test]
    fn claude_command_for_hyprland() {
        let dir = tempfile::tempdir().unwrap();
        let bin = |n: &str| {
            let p = dir.path().join(n);
            std::fs::write(&p, "#!/bin/sh\n").unwrap();
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p.to_string_lossy().into_owned()
        };
        let claude = Claude {
            executable: bin("claude"),
            args: vec!["--dangerously-skip-permissions".into()],
            working_dir: dir.path().to_string_lossy().into_owned(),
            ..Claude::default()
        };
        let term = Terminal {
            executable: bin("ghostty"),
            exec_args: None,
        };
        let prompt = "it's $(broken)\n`now` \"x\"";
        let cmd = claude_shell_command(&claude, &term, prompt, Some(Path::new("/usr/bin/ksshaskpass"))).unwrap();
        // sh must hand the prompt through unchanged as one argument.
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd.replacen("exec ", "exec printf '%s\\0' ", 1))
            .output()
            .unwrap();
        let args: Vec<String> = String::from_utf8(out.stdout)
            .unwrap()
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        assert_eq!(args[0], "env");
        assert_eq!(args[1], "SUDO_ASKPASS=/usr/bin/ksshaskpass");
        assert_eq!(args[2], term.executable);
        assert_eq!(args[3], "-e");
        assert_eq!(args.last().unwrap(), prompt);
        assert_eq!(args[args.len() - 2], "--");

        let no_pass = claude_shell_command(&claude, &term, "x", None).unwrap();
        assert!(!no_pass.contains("SUDO_ASKPASS"));
    }

    #[test]
    fn hyprland_lua() {
        assert_eq!(
            exec_lua("echo \"hi\"", "special:minimized"),
            r#"hl.exec_cmd("echo \"hi\"", { workspace = "special:minimized silent" })"#
        );
        assert_eq!(exec_lua("x", ""), r#"hl.exec_cmd("x")"#);
        let show = show_window_lua("0x55aa");
        assert!(show.contains(r#"hl.get_window("address:0x55aa")"#), "{show}");
        assert!(show.contains("hl.dsp.window.move") && show.contains("hl.dsp.focus"));
        assert!(!show_window_lua("0x1\") os.exit(").contains("os.exit"), "only hex addresses");

        let clients = r#"[{"address":"0x1","workspace":{"id":-98,"name":"special:minimized"}},
                          {"address":"0x2","workspace":{"id":1,"name":"1"}},
                          {"address":"0x3","workspace":{"id":-98,"name":"special:minimized"}}]"#;
        assert_eq!(windows_on(clients, "special:minimized"), ["0x1", "0x3"]);
        assert!(windows_on("garbage", "1").is_empty());
    }

    fn script(dir: &Path, name: &str, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt as _;
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn run_logs_everything_and_stops_at_the_first_failure() {
        let dir = tempfile::tempdir().unwrap();
        let ok = script(dir.path(), "ok", "echo out-$1; echo err-$1 >&2; echo \"LC=$LC_ALL\"");
        let bad = script(dir.path(), "bad", "echo 'error: failed to commit transaction'; exit 3");
        let never = dir.path().join("never");
        let steps = vec![
            Step {
                title: "One".into(),
                argv: vec![ok.clone(), "a".into()],
            },
            Step {
                title: "Two".into(),
                argv: vec![bad.clone()],
            },
            Step {
                title: "Three".into(),
                argv: vec![script(dir.path(), "three", &format!("touch {}", never.display()))],
            },
        ];
        let log = dir.path().join("run.log");
        let mut events = Vec::new();
        let err = run("Updating everything", &steps, &log, false, &mut |e| events.push(e)).unwrap_err();
        assert_eq!(err.step, "Two");
        assert_eq!(err.code, Some(3));
        assert_eq!(err.command, vec![bad]);
        assert_eq!(err.summary, "error: failed to commit transaction");
        assert_eq!(err.log_path, log);
        assert!(!never.exists(), "steps after a failure don't run");

        let text = std::fs::read_to_string(&log).unwrap();
        for needle in ["==> One", "out-a", "err-a", "LC=C.UTF-8", "==> Two", "error: failed to commit"] {
            assert!(text.contains(needle), "{needle} missing:\n{text}");
        }
        assert_eq!(
            events[0],
            Event::Step {
                index: 0,
                count: 3,
                title: "One".into()
            }
        );
        let streamed: String = events
            .iter()
            .filter_map(|e| if let Event::Output(o) = e { Some(o.as_str()) } else { None })
            .collect();
        assert!(streamed.contains("out-a\n") && streamed.contains("error: failed"), "{streamed}");
        assert!(events.contains(&Event::Step {
            index: 1,
            count: 3,
            title: "Two".into()
        }));

        let ok_steps = vec![Step {
            title: "One".into(),
            argv: vec![ok],
        }];
        assert!(run("x", &ok_steps, &dir.path().join("ok.log"), false, &mut |_| {}).is_ok());

        let missing = vec![Step {
            title: "Gone".into(),
            argv: vec!["/nonexistent/vela-test".into()],
        }];
        let err = run("x", &missing, &dir.path().join("missing.log"), false, &mut |_| {}).unwrap_err();
        assert_eq!(err.code, None);
        assert!(err.summary.contains("/nonexistent/vela-test"), "{}", err.summary);
    }

    #[test]
    fn check_asks_every_source() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let t = Tools {
            pacman: Some("/usr/bin/pacman".into()),
            sudo: None,
            checkupdates: Some(script(d, "checkupdates", "echo 'zlib 1-1 -> 1-2'; echo 'curl 8-1 -> 8-2'")),
            helper: Some((Helper::Paru, script(d, "paru", "[ \"$1\" = -Qua ] && echo 'brave-bin 1 -> 2'"))),
            flatpak: Some(script(
                d,
                "flatpak",
                "if [ \"$1\" = remote-ls ]; then printf 'org.x.App\\t2\\tapp/org.x.App/x86_64/stable\\n'; \
                 else printf 'org.x.App\\t1\\torg.x.App/x86_64/stable\\tuser\\n'; fi",
            )),
        };
        let (pending, errors) = check(&t);
        assert!(errors.is_empty(), "{errors:?}");
        let names: Vec<(Source, &str)> = pending.iter().map(|p| (p.source, p.name.as_str())).collect();
        assert_eq!(
            names,
            [
                (Source::Repo, "curl"),
                (Source::Repo, "zlib"),
                (Source::Aur, "brave-bin"),
                (Source::Flatpak, "org.x.App")
            ]
        );
        assert_eq!(pending[3].old, "1");

        // checkupdates: exit 2 = nothing to do; paru -Qua exits 1 without updates.
        let quiet = Tools {
            checkupdates: Some(script(d, "cu-none", "exit 2")),
            helper: Some((Helper::Paru, script(d, "paru-none", "exit 1"))),
            flatpak: None,
            ..t.clone()
        };
        assert_eq!(check(&quiet), (vec![], vec![]));

        let broken = Tools {
            checkupdates: Some(script(d, "cu-bad", "echo '==> ERROR: Cannot fetch updates' >&2; exit 1")),
            helper: None,
            flatpak: None,
            ..t
        };
        let (pending, errors) = check(&broken);
        assert!(pending.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Cannot fetch updates"), "{errors:?}");
        let (_, errors) = check(&Tools::default());
        assert!(errors[0].contains("checkupdates"), "pacman-contrib missing is reported: {errors:?}");
    }

    #[test]
    fn logs_are_rotated() {
        let dir = tempfile::tempdir().unwrap();
        for i in 1..=5u64 {
            let p = new_log_path(dir.path(), i, 3);
            assert_eq!(p, dir.path().join(format!("update-{i}.log")));
            std::fs::write(&p, "x").unwrap();
        }
        std::fs::write(dir.path().join("keep-me.txt"), "").unwrap();
        new_log_path(dir.path(), 6, 3);
        let mut left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["keep-me.txt", "update-4.log", "update-5.log"],
            "room for the new one, foreign files untouched"
        );
    }
}
