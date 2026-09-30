//! Turning catalog entries, files and Claude prompts into processes.
//!
//! Every command is an argv vector handed to `execve` — no shell is ever
//! involved, so arbitrary user input (quotes, newlines, `$()`, …) cannot be
//! interpreted as shell syntax.

use crate::apps::catalog::{Entry, Target};
use crate::apps::exec::{self, FieldContext};
use crate::config::{Claude, Terminal};
use crate::paths;
use std::fmt;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchError {
    NotFound { what: &'static str, command: String },
    InvalidExec(String),
    EmptyPrompt,
    Unsupported(String),
    Spawn(String),
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchError::NotFound { what, command } => write!(f, "{what} “{command}” was not found"),
            LaunchError::InvalidExec(e) => write!(f, "invalid Exec line: {e}"),
            LaunchError::EmptyPrompt => write!(f, "type a prompt for Claude first"),
            LaunchError::Unsupported(e) => write!(f, "{e}"),
            LaunchError::Spawn(e) => write!(f, "could not start process: {e}"),
        }
    }
}

impl std::error::Error for LaunchError {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpawnSpec {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    /// Used for the systemd scope name.
    pub name: String,
}

fn resolve(what: &'static str, command: &str) -> Result<String, LaunchError> {
    paths::find_executable(command)
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| LaunchError::NotFound {
            what,
            command: command.trim().to_owned(),
        })
}

/// `terminal [exec args] command…`
pub fn in_terminal(term: &Terminal, command: Vec<String>) -> Result<Vec<String>, LaunchError> {
    let mut argv = vec![resolve("Terminal", &term.executable)?];
    argv.extend(term.effective_exec_args());
    argv.extend(command);
    Ok(argv)
}

/// `terminal [exec args] claude <args…> -- <prompt>`
///
/// The prompt is a single argv element. `--` ends option parsing so a prompt
/// starting with `-` or matching a subcommand name (e.g. `mcp`) is still
/// treated as the prompt.
pub fn claude_spec(claude: &Claude, term: &Terminal, prompt: &str) -> Result<SpawnSpec, LaunchError> {
    if prompt.trim().is_empty() {
        return Err(LaunchError::EmptyPrompt);
    }
    let mut command = vec![resolve("Claude executable", &claude.executable)?];
    command.extend(claude.args.iter().cloned());
    if !claude.args.iter().any(|a| a == "--") {
        command.push("--".into());
    }
    command.push(prompt.to_owned());
    Ok(SpawnSpec {
        argv: in_terminal(term, command)?,
        cwd: Some(paths::expand_tilde(claude.working_dir.trim())),
        env: Vec::new(),
        name: "claude".into(),
    })
}

fn desktop_spec(path: &Path, entry: &crate::apps::desktop_entry::DesktopEntry, exec_line: Option<&str>, term: &Terminal) -> Result<SpawnSpec, LaunchError> {
    let path_str = path.to_string_lossy();
    let ctx = FieldContext {
        name: &entry.name,
        icon: entry.icon.as_deref(),
        desktop_file: Some(&path_str),
    };
    let argv = match exec_line {
        Some(line) => {
            let mut argv = exec::exec_to_argv(line, &ctx).map_err(|e| LaunchError::InvalidExec(e.to_string()))?;
            argv[0] = resolve("Program", &argv[0])?;
            if entry.terminal { in_terminal(term, argv)? } else { argv }
        }
        None if entry.dbus_activatable => dbus_activate_argv(path)?,
        None => return Err(LaunchError::InvalidExec("no Exec key".into())),
    };
    Ok(SpawnSpec {
        argv,
        cwd: entry.working_dir.as_deref().map(paths::expand_tilde),
        env: vec![("GIO_LAUNCHED_DESKTOP_FILE".into(), path_str.into_owned())],
        name: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
    })
}

/// org.freedesktop.Application.Activate via gdbus for Exec-less entries.
fn dbus_activate_argv(path: &Path) -> Result<Vec<String>, LaunchError> {
    let bus_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
    let object_path = format!("/{}", bus_name.replace('.', "/").replace('-', "_"));
    Ok(vec![
        resolve("gdbus", "gdbus")?,
        "call".into(),
        "--session".into(),
        "--dest".into(),
        bus_name,
        "--object-path".into(),
        object_path,
        "--method".into(),
        "org.freedesktop.Application.Activate".into(),
        "{}".into(),
    ])
}

/// Builds the process for a catalog entry. `Target::Settings` is handled by
/// the UI and is not a process.
pub fn entry_spec(entry: &Entry, term: &Terminal) -> Result<SpawnSpec, LaunchError> {
    match &entry.target {
        Target::Desktop { path, entry: de } => desktop_spec(path, de, de.exec.as_deref(), term),
        Target::DesktopAction { path, entry: de, action } => {
            let a = de.actions.get(*action).ok_or_else(|| LaunchError::InvalidExec("unknown action".into()))?;
            let mut de = de.clone();
            if a.icon.is_some() {
                de.icon = a.icon.clone();
            }
            desktop_spec(path, &de, a.exec.as_deref(), term)
        }
        Target::Custom(c) => {
            let mut argv = c.command.clone();
            if argv.is_empty() {
                return Err(LaunchError::InvalidExec("custom action has no command".into()));
            }
            argv[0] = resolve("Program", &argv[0])?;
            let argv = if c.terminal { in_terminal(term, argv)? } else { argv };
            Ok(SpawnSpec {
                argv,
                cwd: None,
                env: Vec::new(),
                name: c.id.clone(),
            })
        }
        Target::Settings => Err(LaunchError::Unsupported("settings are opened internally".into())),
    }
}

/// Opens a file or directory with the default handler.
pub fn open_path_spec(path: &Path) -> Result<SpawnSpec, LaunchError> {
    if !path.exists() {
        return Err(LaunchError::Unsupported(format!("{} no longer exists", paths::display_path(path))));
    }
    let path = path.to_string_lossy().into_owned();
    let argv = if let Some(xdg) = paths::find_executable("xdg-open") {
        vec![xdg.to_string_lossy().into_owned(), path]
    } else if let Some(gio) = paths::find_executable("gio") {
        vec![gio.to_string_lossy().into_owned(), "open".into(), path]
    } else {
        return Err(LaunchError::NotFound {
            what: "Opener",
            command: "xdg-open".into(),
        });
    };
    Ok(SpawnSpec {
        argv,
        cwd: None,
        env: Vec::new(),
        name: "open".into(),
    })
}

/// Whether a user systemd instance is reachable for transient scopes.
pub fn systemd_scope_available() -> bool {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    runtime.is_some_and(|r| r.join("systemd/private").exists()) && paths::find_executable("systemd-run").is_some()
}

fn scope_unit_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '.' { c } else { '_' })
        .take(64)
        .collect();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("app-vela-{clean}-{}{:x}.scope", std::process::id(), nanos)
}

/// Wraps argv so the process gets its own transient scope below `app.slice`
/// instead of living (and dying) in vela's service cgroup.
pub fn wrap_in_scope(spec: &SpawnSpec) -> Vec<String> {
    let mut argv = vec![
        "systemd-run".into(),
        "--user".into(),
        "--scope".into(),
        "--quiet".into(),
        "--collect".into(),
        "--slice=app.slice".into(),
        format!("--unit={}", scope_unit_name(&spec.name)),
        "--".into(),
    ];
    argv.extend(spec.argv.iter().cloned());
    argv
}

/// Variables Claude Code sets for its own child processes. If the daemon was
/// started from inside a Claude Code session, a Claude started by vela would
/// otherwise think it is a sub-session (and e.g. not save its transcript).
fn is_claude_session_marker(key: &str) -> bool {
    key == "CLAUDECODE" || key == "CLAUDE_PID" || key.starts_with("CLAUDE_CODE_")
}

/// Starts the process fully detached (double fork + setsid) so it neither
/// becomes a zombie of the daemon nor shares its session.
pub fn spawn_detached(spec: &SpawnSpec, use_scope: bool) -> Result<(), LaunchError> {
    let argv = if use_scope { wrap_in_scope(spec) } else { spec.argv.clone() };
    let (program, args) = argv.split_first().ok_or_else(|| LaunchError::Spawn("empty command".into()))?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("PATH", paths::child_path_env());
    for (k, v) in spec.env.iter().filter(|(k, _)| !is_claude_session_marker(k)) {
        cmd.env(k, v);
    }
    for (k, _) in std::env::vars_os() {
        if k.to_str().is_some_and(is_claude_session_marker) {
            cmd.env_remove(k);
        }
    }
    let cwd = spec.cwd.clone().filter(|d| d.is_dir()).unwrap_or_else(paths::home_dir);
    cmd.current_dir(cwd);
    // SAFETY: only async-signal-safe calls (fork, setsid, _exit) run between
    // fork and exec. Exec errors of the grandchild still reach `spawn()`
    // through std's CLOEXEC error pipe.
    unsafe {
        cmd.pre_exec(|| match libc::fork() {
            -1 => Err(io::Error::last_os_error()),
            0 => {
                libc::setsid();
                Ok(())
            }
            _ => libc::_exit(0),
        });
    }
    let mut child = cmd.spawn().map_err(|e| LaunchError::Spawn(format!("{program}: {e}")))?;
    child.wait().map_err(|e| LaunchError::Spawn(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::desktop_entry::DesktopEntry;
    use crate::config::CustomAction;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    /// A fake program that records its argv (NUL separated) into `$OUT`.
    fn recorder(dir: &Path, name: &str) -> String {
        let p = dir.join(name);
        fs::write(
            &p,
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\0' \"$a\"; done > \"$OUT.tmp\"\nmv \"$OUT.tmp\" \"$OUT\"\n",
        )
        .unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        p.to_string_lossy().into_owned()
    }

    fn wait_for(path: &Path) -> Vec<String> {
        let start = Instant::now();
        while !path.exists() {
            assert!(start.elapsed() < Duration::from_secs(5), "process did not run");
            std::thread::sleep(Duration::from_millis(10));
        }
        let data = fs::read(path).unwrap();
        data.split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8(s.to_vec()).unwrap())
            .collect()
    }

    const NASTY: &str = "Explain \"RSA\" to me; $(rm -rf ~) `id` 'quoted' \\n\nnew line & | > < * ? # 🚀 ü --help";

    #[test]
    fn claude_argv_keeps_prompt_as_single_argument() {
        let dir = tempfile::tempdir().unwrap();
        let term = Terminal {
            executable: recorder(dir.path(), "term"),
            exec_args: Some(vec!["-e".into()]),
        };
        let claude = Claude {
            executable: recorder(dir.path(), "claude"),
            ..Claude::default()
        };
        let spec = claude_spec(&claude, &term, NASTY).unwrap();
        assert_eq!(spec.argv[1], "-e");
        assert_eq!(&spec.argv[3..], ["--dangerously-skip-permissions", "--", NASTY]);
    }

    #[test]
    fn claude_prompt_reaches_the_process_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let term_path = recorder(dir.path(), "term");
        let claude = Claude {
            executable: "/usr/bin/true".into(),
            working_dir: dir.path().to_string_lossy().into(),
            ..Claude::default()
        };
        let term = Terminal {
            executable: term_path,
            exec_args: Some(vec![]),
        };
        let mut spec = claude_spec(&claude, &term, NASTY).unwrap();
        spec.env.push(("OUT".into(), out.to_string_lossy().into()));
        spawn_detached(&spec, false).unwrap();
        let args = wait_for(&out);
        assert_eq!(args, vec!["/usr/bin/true", "--dangerously-skip-permissions", "--", NASTY]);
    }

    #[test]
    fn claude_errors() {
        let term = Terminal {
            executable: "sh".into(),
            exec_args: None,
        };
        assert_eq!(claude_spec(&Claude::default(), &term, "  "), Err(LaunchError::EmptyPrompt));
        let missing = Claude {
            executable: "no-such-claude-vela".into(),
            ..Claude::default()
        };
        assert!(matches!(
            claude_spec(&missing, &term, "hi"),
            Err(LaunchError::NotFound { what: "Claude executable", .. })
        ));
        let no_term = Terminal {
            executable: "no-such-term-vela".into(),
            exec_args: None,
        };
        let claude = Claude {
            executable: "sh".into(),
            ..Claude::default()
        };
        assert!(matches!(
            claude_spec(&claude, &no_term, "hi"),
            Err(LaunchError::NotFound { what: "Terminal", .. })
        ));
    }

    #[test]
    fn explicit_separator_is_not_duplicated() {
        let claude = Claude {
            executable: "sh".into(),
            args: vec!["--".into()],
            ..Claude::default()
        };
        let term = Terminal {
            executable: "sh".into(),
            exec_args: Some(vec![]),
        };
        let spec = claude_spec(&claude, &term, "x").unwrap();
        assert_eq!(spec.argv.iter().filter(|a| *a == "--").count(), 1);
    }

    fn desktop(exec: &str, terminal: bool) -> Entry {
        Entry {
            key: "t.desktop".into(),
            name: "T".into(),
            subtitle: String::new(),
            icon: None,
            keywords: vec![],
            is_action: false,
            target: Target::Desktop {
                path: PathBuf::from("/usr/share/applications/t.desktop"),
                entry: DesktopEntry {
                    name: "T".into(),
                    exec: Some(exec.into()),
                    terminal,
                    icon: Some("t-icon".into()),
                    ..Default::default()
                },
            },
        }
    }

    #[test]
    fn desktop_entries_become_argv() {
        let term = Terminal {
            executable: "sh".into(),
            exec_args: Some(vec!["-e".into()]),
        };
        let spec = entry_spec(&desktop("sh --flag \"two words\" %U %i", false), &term).unwrap();
        assert!(spec.argv[0].ends_with("/sh"));
        assert_eq!(&spec.argv[1..], ["--flag", "two words", "--icon", "t-icon"]);
        assert_eq!(spec.env[0].1, "/usr/share/applications/t.desktop");

        let spec = entry_spec(&desktop("sh -c top", true), &term).unwrap();
        assert!(spec.argv[0].ends_with("/sh"));
        assert_eq!(spec.argv[1], "-e");
        assert!(spec.argv[2].ends_with("/sh"));

        assert!(matches!(
            entry_spec(&desktop("no-such-program-vela", false), &term),
            Err(LaunchError::NotFound { .. })
        ));
        assert!(matches!(
            entry_spec(&desktop("sh \"unterminated", false), &term),
            Err(LaunchError::InvalidExec(_))
        ));
    }

    #[test]
    fn launching_a_desktop_entry_runs_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let rec = recorder(dir.path(), "app");
        let term = Terminal::default();
        let mut spec = entry_spec(&desktop(&format!("\"{rec}\" --new-window %u"), false), &term).unwrap();
        spec.env.push(("OUT".into(), out.to_string_lossy().into()));
        spawn_detached(&spec, false).unwrap();
        assert_eq!(wait_for(&out), vec!["--new-window"]);
    }

    #[test]
    fn custom_actions() {
        let term = Terminal {
            executable: "sh".into(),
            exec_args: Some(vec!["-e".into()]),
        };
        let entry = Entry {
            key: "custom:x".into(),
            name: "X".into(),
            subtitle: String::new(),
            icon: None,
            keywords: vec![],
            is_action: false,
            target: Target::Custom(CustomAction {
                id: "x".into(),
                name: "X".into(),
                command: vec!["sh".into(), "a b".into()],
                terminal: true,
                ..Default::default()
            }),
        };
        let spec = entry_spec(&entry, &term).unwrap();
        assert_eq!(spec.argv[1], "-e");
        assert_eq!(spec.argv[3], "a b");
    }

    #[test]
    fn claude_session_markers_are_not_inherited() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("env");
        let script = dir.path().join("dump-env");
        fs::write(&script, "#!/bin/sh\nenv > \"$OUT.tmp\"\nmv \"$OUT.tmp\" \"$OUT\"\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let spec = SpawnSpec {
            argv: vec![script.to_string_lossy().into()],
            env: vec![("OUT".into(), out.to_string_lossy().into()), ("CLAUDE_CODE_CHILD_SESSION".into(), "1".into())],
            ..Default::default()
        };
        spawn_detached(&spec, false).unwrap();
        let start = Instant::now();
        while !out.exists() {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
        let env = fs::read_to_string(&out).unwrap();
        assert!(
            env.lines()
                .all(|l| !l.starts_with("CLAUDECODE=") && !l.starts_with("CLAUDE_CODE_") && !l.starts_with("CLAUDE_PID=")),
            "{env}"
        );
        assert!(env.contains("PATH="));
    }

    #[test]
    fn spawn_reports_missing_program() {
        let spec = SpawnSpec {
            argv: vec!["/nonexistent/vela-bin".into()],
            ..Default::default()
        };
        assert!(matches!(spawn_detached(&spec, false), Err(LaunchError::Spawn(_))));
    }

    #[test]
    fn open_path_rejects_vanished_files() {
        assert!(open_path_spec(Path::new("/nonexistent/vela/file.pdf")).is_err());
        assert!(open_path_spec(Path::new("/etc")).is_ok());
    }

    #[test]
    fn scope_wrapper() {
        let spec = SpawnSpec {
            argv: vec!["/usr/bin/firefox".into()],
            name: "fire fox/x".into(),
            ..Default::default()
        };
        let argv = wrap_in_scope(&spec);
        assert_eq!(argv[0], "systemd-run");
        let unit = argv.iter().find(|a| a.starts_with("--unit=")).unwrap();
        assert!(unit.starts_with("--unit=app-vela-fire_fox_x-") && unit.ends_with(".scope"));
        assert_eq!(argv.last().unwrap(), "/usr/bin/firefox");
        assert_eq!(argv[argv.len() - 2], "--");
    }
}
