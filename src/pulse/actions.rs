//! What Pulse can do to processes, apps and services. Own processes are
//! handled directly; others go through `sudo -A` with a password dialog
//! (the same askpass the updates use), system units through polkit first.

use super::apps::Kind;
use super::engine::AppFrame;
use crate::config::{Config, Terminal};
use crate::launch::{self, SpawnSpec};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn signal_number(name: &str) -> Option<i32> {
    let upper = name.to_uppercase();
    Some(match upper.trim_start_matches("SIG") {
        "TERM" => libc::SIGTERM,
        "KILL" => libc::SIGKILL,
        "STOP" => libc::SIGSTOP,
        "CONT" => libc::SIGCONT,
        "HUP" => libc::SIGHUP,
        "INT" => libc::SIGINT,
        "USR1" => libc::SIGUSR1,
        "USR2" => libc::SIGUSR2,
        _ => return None,
    })
}

/// Start time of a running process (unix seconds), like the sampler computes it.
pub fn start_of(pid: i32) -> Option<u64> {
    let st = super::procfs::parse_pid_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)?;
    let boot = super::procfs::parse_stat(&std::fs::read_to_string("/proc/stat").ok()?).boot_time;
    // SAFETY: sysconf has no preconditions.
    let clk = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    Some(boot + (st.start_ticks as f64 / clk) as u64)
}

/// The pids that still belong to the processes seen (pid, start); a pid
/// reused since then is left alone. Start 0 = unknown, kept.
pub fn still_same(pids: &[i32], starts: &[u64]) -> Vec<i32> {
    pids.iter()
        .enumerate()
        .filter(|(i, pid)| match starts.get(*i).copied().unwrap_or(0) {
            0 => true,
            s => start_of(**pid).is_some_and(|now| now.abs_diff(s) <= 1),
        })
        .map(|(_, p)| *p)
        .collect()
}

fn alive(pid: i32) -> bool {
    // A zombie is gone for our purposes.
    let state = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|t| super::procfs::parse_pid_stat(&t))
        .map(|s| s.state);
    state.is_some_and(|s| s != 'Z' && s != 'X')
}

/// Waits until none of `pids` is alive.
pub fn wait_gone(pids: &[i32], timeout: Duration) -> bool {
    let end = Instant::now() + timeout;
    loop {
        if !pids.iter().any(|p| alive(*p)) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `sudo -A <argv>` with a password dialog.
pub fn privileged(argv: &[String]) -> Result<(), String> {
    authenticate()?;
    privileged_now(argv)
}

/// Asks for the password (sudo -A -v), so the actual command can run right
/// after with `privileged_now` — the dialog may stay open for minutes.
pub fn authenticate() -> Result<(), String> {
    let askpass = crate::update::find_askpass(std::env::var_os("SUDO_ASKPASS").as_deref())
        .ok_or("needs administrator rights, but no password dialog (ksshaskpass, ssh-askpass) is installed")?;
    let out = Command::new("sudo")
        .args(["-A", "-v"])
        .env("SUDO_ASKPASS", askpass)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run sudo: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(if err.is_empty() { "authentication failed".into() } else { err })
    }
}

/// `sudo -n` with the credentials `authenticate` just cached.
pub fn privileged_now(argv: &[String]) -> Result<(), String> {
    let out = Command::new("sudo")
        .args(["-n", "--"])
        .args(argv)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run sudo: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(if err.is_empty() { format!("{} failed", argv.join(" ")) } else { err })
    }
}

/// Sends a signal; processes of other users through sudo.
pub fn signal(pids: &[i32], sig: i32) -> Result<(), String> {
    let mut denied = Vec::new();
    let mut errors = Vec::new();
    for &pid in pids {
        if pid <= 1 {
            errors.push(format!("refusing to signal pid {pid}"));
            continue;
        }
        // SAFETY: kill(2) with a plain pid and signal number.
        if unsafe { libc::kill(pid, sig) } != 0 {
            let e = std::io::Error::last_os_error();
            match e.raw_os_error() {
                Some(libc::EPERM) => denied.push(pid),
                Some(libc::ESRCH) => {}
                _ => errors.push(format!("{pid}: {e}")),
            }
        }
    }
    if !denied.is_empty() {
        // Remember who they are before the password dialog: a pid freed and
        // reused while it is open must not be hit.
        let starts: Vec<u64> = denied.iter().map(|p| start_of(*p).unwrap_or(u64::MAX)).collect();
        authenticate()?;
        let same = still_same(&denied, &starts);
        if !same.is_empty() {
            let mut argv = vec!["kill".to_owned(), "-s".to_owned(), sig.to_string()];
            argv.extend(same.iter().map(|p| p.to_string()));
            privileged_now(&argv)?;
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
}

fn systemctl(user: bool, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new("systemctl");
    if user {
        cmd.arg("--user");
    }
    let out = cmd
        .args(args)
        .stdin(Stdio::null())
        .env("SYSTEMD_COLORS", "0")
        .output()
        .map_err(|e| format!("cannot run systemctl: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    // No polkit agent (or it was dismissed): ask for the password ourselves.
    if !user && (err.contains("Interactive authentication required") || err.contains("Access denied")) {
        let mut argv = vec!["systemctl".to_owned()];
        argv.extend(args.iter().map(|a| (*a).to_owned()));
        return privileged(&argv);
    }
    Err(if err.is_empty() {
        format!("systemctl {} failed", args.join(" "))
    } else {
        err
    })
}

pub fn unit_action(unit: &str, user: bool, action: &str) -> Result<(), String> {
    let allowed = ["start", "stop", "restart", "reset-failed", "enable", "disable", "freeze", "thaw"];
    if !allowed.contains(&action) {
        return Err(format!("unknown unit action {action}"));
    }
    if unit.is_empty() || unit.starts_with('-') || unit.contains('/') {
        return Err("invalid unit name".into());
    }
    match action {
        "enable" | "disable" => systemctl(user, &[action, "--now", unit]),
        // A unit that is no longer loaded has no failed state left: done.
        "reset-failed" => systemctl(user, &[action, unit]).or_else(|e| if e.contains("not loaded") { Ok(()) } else { Err(e) }),
        _ => systemctl(user, &["--no-block", action, unit]),
    }
}

fn service_unit(app: &AppFrame) -> Option<(&str, bool)> {
    let unit = app.unit.as_deref()?;
    (unit.ends_with(".service") && matches!(app.kind, Kind::Service | Kind::System | Kind::Background | Kind::Window)).then_some((unit, app.user_unit))
}

/// Ends an app: SIGTERM (or SIGKILL with `force`) to all its processes,
/// the main one first.
pub fn end_app(app: &AppFrame, force: bool) -> Result<(), String> {
    if app.kind == Kind::Kernel {
        return Err("kernel threads can't be ended".into());
    }
    if let Some((unit, user)) = service_unit(app)
        && matches!(app.kind, Kind::Service | Kind::System)
        && !force
    {
        return unit_action(unit, user, "stop");
    }
    let sig = if force { libc::SIGKILL } else { libc::SIGTERM };
    let current = still_same(&app.pids, &app.starts);
    let mut pids: Vec<i32> = current.iter().copied().filter(|p| *p == app.main_pid).collect();
    pids.extend(current.iter().copied().filter(|p| *p != app.main_pid));
    if pids.is_empty() {
        return Ok(());
    }
    // Stopped processes don't handle SIGTERM until they continue.
    if !force {
        let _ = signal(&pids, libc::SIGCONT);
    }
    signal(&pids, sig)
}

/// How to start an app again.
pub fn relaunch_spec(app: &AppFrame, desktop_file: Option<&Path>, term: &Terminal) -> Result<SpawnSpec, String> {
    if let Some(path) = desktop_file
        && matches!(app.kind, Kind::Window | Kind::Background)
    {
        return launch::desktop_file_spec(path, term).map_err(|e| e.to_string());
    }
    if app.command.is_empty() {
        return Err("don't know how to start it again".into());
    }
    let pid = app.main_pid;
    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
    let env: Vec<(String, String)> = std::fs::read(format!("/proc/{pid}/environ"))
        .map(|raw| {
            super::procfs::parse_cmdline(&raw)
                .into_iter()
                .filter_map(|kv| kv.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let mut argv = app.command.clone();
    if !argv[0].contains('/') {
        argv[0] = crate::paths::find_executable(&argv[0])
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| argv[0].clone());
    } else if !Path::new(&argv[0]).exists() && !app.exe.is_empty() {
        argv[0] = app.exe.clone();
    }
    Ok(SpawnSpec {
        argv,
        cwd,
        env,
        name: app.name.clone(),
    })
}

/// Restarts an app: services through systemd, everything else by ending it
/// and starting it again the way it was started.
pub fn restart_app(app: &AppFrame, desktop_file: Option<&Path>, cfg: &Config) -> Result<(), String> {
    if let Some((unit, user)) = service_unit(app) {
        return unit_action(unit, user, "restart");
    }
    if app.kind == Kind::Kernel || app.kind == Kind::System {
        return Err("system parts can't be restarted from here".into());
    }
    let spec = relaunch_spec(app, desktop_file, &cfg.terminal)?;
    end_app(app, false)?;
    if !wait_gone(&app.pids, Duration::from_secs(6)) {
        end_app(app, true)?;
        wait_gone(&app.pids, Duration::from_secs(2));
    }
    launch::spawn_detached(&spec, cfg.general.systemd_scope && launch::systemd_scope_available()).map_err(|e| e.to_string())
}

fn threads(pid: i32) -> Vec<i32> {
    std::fs::read_dir(format!("/proc/{pid}/task"))
        .map(|r| r.flatten().filter_map(|e| e.file_name().to_str()?.parse().ok()).collect())
        .unwrap_or_else(|_| vec![pid])
}

/// I/O scheduling class of a thread (3 = idle), None if unknown.
pub fn ioprio_class(tid: i32) -> Option<i32> {
    // SAFETY: ioprio_get takes two integers.
    let v = unsafe { libc::syscall(libc::SYS_ioprio_get, IOPRIO_WHO_PROCESS, tid) };
    (v >= 0).then_some((v as i32) >> IOPRIO_CLASS_SHIFT)
}

/// Efficiency mode as Pulse sets it: an idle cgroup (CPUWeight=idle), or
/// nice 19 with idle I/O. Plain nice values (ananicy, `nice`) don't count.
pub fn in_efficiency_mode(pid: i32, nice: i64) -> bool {
    let cg = std::fs::read_to_string(format!("/proc/{pid}/cgroup"))
        .map(|t| super::procfs::parse_cgroup(&t))
        .unwrap_or_default();
    if !cg.is_empty() && std::fs::read_to_string(format!("/sys/fs/cgroup{cg}/cpu.idle")).is_ok_and(|v| v.trim() == "1") {
        return true;
    }
    nice == 19 && ioprio_class(pid) == Some(3)
}

const IOPRIO_WHO_PROCESS: libc::c_int = 1;
const IOPRIO_CLASS_SHIFT: libc::c_int = 13;

fn set_ioprio(tid: i32, class: libc::c_int, data: libc::c_int) -> bool {
    // SAFETY: ioprio_set takes three integers.
    unsafe { libc::syscall(libc::SYS_ioprio_set, IOPRIO_WHO_PROCESS, tid, (class << IOPRIO_CLASS_SHIFT) | data) == 0 }
}

/// Efficiency mode (like Windows 11's): the app only gets CPU and disk
/// time nobody else wants. App units get it from systemd (CPUWeight=idle);
/// every thread is also reniced to 19 with idle I/O priority.
pub fn efficiency(app: &AppFrame, on: bool) -> Result<(), String> {
    if matches!(app.kind, Kind::Kernel | Kind::System) {
        return Err("only apps and user services can be put into efficiency mode".into());
    }
    if let Some(unit) = app.unit.as_deref().filter(|_| app.user_unit) {
        let props: &[&str] = if on {
            &["CPUWeight=idle", "IOWeight=1"]
        } else {
            &["CPUWeight=100", "IOWeight=100"]
        };
        let mut args = vec!["set-property", "--runtime", unit];
        args.extend_from_slice(props);
        let _ = systemctl(true, &args);
    }
    let nice = if on { 19 } else { 0 };
    let mut failed = 0;
    for pid in &app.pids {
        for tid in threads(*pid) {
            // SAFETY: setpriority with a plain id.
            let ok = unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) } == 0;
            let io = if on { set_ioprio(tid, 3, 0) } else { set_ioprio(tid, 2, 4) };
            if !ok || !io {
                failed += 1;
            }
        }
    }
    if failed > 0 && !on {
        // Raising priority again can need privileges (RLIMIT_NICE).
        let pids: Vec<String> = still_same(&app.pids, &app.starts).iter().map(|p| p.to_string()).collect();
        if pids.is_empty() {
            return Ok(());
        }
        authenticate()?;
        let mut renice = vec!["renice".to_owned(), "-n".to_owned(), "0".to_owned(), "-p".to_owned()];
        renice.extend(pids.iter().cloned());
        privileged_now(&renice)?;
        let mut ionice = vec![
            "ionice".to_owned(),
            "-c".to_owned(),
            "2".to_owned(),
            "-n".to_owned(),
            "4".to_owned(),
            "-p".to_owned(),
        ];
        ionice.extend(pids);
        return privileged_now(&ionice);
    }
    Ok(())
}

/// Pauses or resumes an app: systemd freezes app units (the whole cgroup at
/// once), other processes get SIGSTOP/SIGCONT.
pub fn pause(app: &AppFrame, on: bool) -> Result<(), String> {
    if matches!(app.kind, Kind::Kernel | Kind::System) {
        return Err("system parts can't be paused".into());
    }
    if let Some(unit) = app.unit.as_deref().filter(|u| app.user_unit && u.starts_with("app-"))
        && systemctl(true, &[if on { "freeze" } else { "thaw" }, unit]).is_ok()
    {
        return Ok(());
    }
    signal(&still_same(&app.pids, &app.starts), if on { libc::SIGSTOP } else { libc::SIGCONT })
}

pub fn set_power_profile(profile: &str) -> Result<(), String> {
    if !["power-saver", "balanced", "performance"].contains(&profile) {
        return Err(format!("unknown power profile {profile}"));
    }
    let ppc = crate::paths::find_executable("powerprofilesctl").ok_or("power-profiles-daemon (powerprofilesctl) is not installed")?;
    let out = Command::new(ppc)
        .args(["set", profile])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// "Run new task": a command line, optionally in the terminal.
pub fn run_command(line: &str, in_terminal: bool, cfg: &Config) -> Result<(), String> {
    let mut argv = shlex::split(line.trim()).ok_or("unbalanced quotes")?;
    if argv.is_empty() {
        return Err("type a command first".into());
    }
    if !argv[0].contains('/') {
        argv[0] = crate::paths::find_executable(&argv[0])
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| format!("“{}” was not found", argv[0]))?;
    }
    let name = argv[0].rsplit('/').next().unwrap_or("task").to_owned();
    let argv = if in_terminal {
        launch::in_terminal(&cfg.terminal, argv).map_err(|e| e.to_string())?
    } else {
        argv
    };
    let spec = SpawnSpec {
        argv,
        cwd: None,
        env: Vec::new(),
        name,
    };
    launch::spawn_detached(&spec, cfg.general.systemd_scope && launch::systemd_scope_available()).map_err(|e| e.to_string())
}

pub fn launch_desktop(path: &Path, cfg: &Config) -> Result<(), String> {
    let spec = launch::desktop_file_spec(path, &cfg.terminal).map_err(|e| e.to_string())?;
    launch::spawn_detached(&spec, cfg.general.systemd_scope && launch::systemd_scope_available()).map_err(|e| e.to_string())
}

pub fn focus_window(address: &str) -> Result<(), String> {
    let hex: String = address.trim_start_matches("0x").chars().filter(char::is_ascii_hexdigit).collect();
    if hex.is_empty() {
        return Err("no window".into());
    }
    crate::hyprland::eval(&[format!(
        "local w = hl.get_window(\"address:0x{hex}\") if w then hl.dispatch(hl.dsp.focus({{ window = w }})) end"
    )])
}

pub fn ask_claude(prompt: &str, cfg: &Config) -> Result<(), String> {
    let spec = launch::claude_spec(&cfg.claude, &cfg.terminal, prompt).map_err(|e| e.to_string())?;
    launch::spawn_detached(&spec, launch::systemd_scope_available()).map_err(|e| e.to_string())
}

pub fn reboot() -> Result<(), String> {
    systemctl(false, &["reboot"])
}

/// vela's config without creating the file (Pulse only reads it).
pub fn load_config() -> Config {
    std::fs::read_to_string(crate::paths::config_file())
        .ok()
        .and_then(|t| Config::from_toml(&t).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signals_by_name() {
        assert_eq!(signal_number("TERM"), Some(libc::SIGTERM));
        assert_eq!(signal_number("sigkill"), Some(libc::SIGKILL));
        assert_eq!(signal_number("NOPE"), None);
    }

    #[test]
    fn a_reused_pid_is_left_alone() {
        let me = std::process::id() as i32;
        let start = start_of(me).unwrap();
        assert_eq!(still_same(&[me], &[start]), vec![me]);
        assert!(still_same(&[me], &[start - 100]).is_empty());
        assert_eq!(still_same(&[me], &[]), vec![me]);
        assert!(still_same(&[i32::MAX], &[start]).is_empty());
    }

    #[test]
    fn refuses_init_and_bad_units() {
        assert!(signal(&[1], libc::SIGTERM).is_err());
        assert!(unit_action("../x", true, "start").is_err());
        assert!(unit_action("x.service", true, "mask").is_err());
        assert!(set_power_profile("turbo").is_err());
    }

    /// Ends and restarts a real child process (a `sleep`).
    #[test]
    fn ends_a_real_process() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id() as i32;
        let app = AppFrame {
            key: "t".into(),
            kind: Kind::Task,
            name: "sleep".into(),
            icon: String::new(),
            desktop_id: None,
            unit: None,
            user_unit: false,
            main_pid: pid,
            pids: vec![pid],
            starts: vec![start_of(pid).unwrap()],
            windows: vec![],
            cpu: 0.0,
            cpu_core: 0.0,
            mem: 0,
            swap: 0,
            gpu: 0.0,
            vram: 0,
            read_bps: 0.0,
            write_bps: 0.0,
            threads: 1,
            started: 0,
            user: String::new(),
            uid: 0,
            flags: vec![],
            health: 0,
            leak: None,
            gpus: vec![],
            command: vec!["sleep".into(), "30".into()],
            exe: "/usr/bin/sleep".into(),
            restart_reasons: vec![],
            focused: false,
        };
        let spec = relaunch_spec(&app, None, &Terminal::default()).unwrap();
        assert!(spec.argv[0].ends_with("/sleep") && spec.argv[1] == "30");
        assert!(spec.cwd.is_some());
        pause(&app, true).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let state = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        assert!(super::super::procfs::parse_pid_stat(&state).unwrap().state == 'T');
        end_app(&app, false).unwrap();
        let mut child = child;
        let _ = child.wait();
        assert!(wait_gone(&[pid], Duration::from_secs(2)));
    }
}
