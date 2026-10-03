//! Groups processes into the things a person thinks of: apps with windows,
//! apps running in the background, commands running in a terminal, services
//! and the system.
//!
//! The systemd cgroup is the main signal (vela, uwsm, Flatpak and systemd
//! put every app in its own `app-….scope`/`.service`); processes in the
//! login session scope (started by Hyprland's exec without a scope) are
//! grouped by process tree below the compositor instead.

use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: i32,
    pub ppid: i32,
    pub uid: u32,
    pub comm: String,
    /// Target of /proc/<pid>/exe (empty when unreadable).
    pub exe: String,
    pub cmdline: Vec<String>,
    pub cgroup: String,
    pub kernel: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub pid: i32,
    pub address: String,
    pub class: String,
    pub title: String,
    pub workspace: String,
    pub focused: bool,
}

/// What we need from a desktop entry to name and relaunch an app.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopApp {
    /// Desktop file id without `.desktop`.
    pub id: String,
    pub name: String,
    pub icon: String,
    pub wm_class: String,
    /// Basename of the program in Exec (after env/flatpak run handling).
    pub exec_bin: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Has at least one window.
    Window,
    /// An app without a window (tray apps, daemons started from autostart).
    Background,
    /// A command running in a terminal tab.
    Task,
    /// A systemd user service.
    Service,
    /// System services, the session, the compositor.
    System,
    Kernel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub key: String,
    pub kind: Kind,
    pub name: String,
    pub icon: String,
    pub desktop_id: Option<String>,
    /// systemd unit the group lives in (for restart/stop).
    pub unit: Option<String>,
    pub user_unit: bool,
    pub main_pid: i32,
    pub pids: Vec<i32>,
    pub windows: Vec<WindowInfo>,
}

const SHELLS: [&str; 8] = ["sh", "bash", "dash", "zsh", "fish", "nu", "xonsh", "elvish"];
/// Processes that start the session's apps; their children are the apps.
const SESSION_HOSTS: [&str; 9] = [
    "Hyprland",
    "hyprland",
    "start-hyprland",
    "uwsm",
    "sway",
    "niri",
    "kwin_wayland",
    "gnome-shell",
    "labwc",
];

pub fn is_shell(comm: &str) -> bool {
    SHELLS.contains(&comm)
}

/// systemd unit name escaping (`\x2d` → `-`).
pub fn unescape_unit(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("\\x") {
        out.push_str(&rest[..i]);
        let hex = rest.get(i + 2..i + 4).and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(b) => {
                out.push(b as char);
                rest = &rest[i + 4..];
            }
            None => {
                out.push_str("\\x");
                rest = &rest[i + 2..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The innermost systemd unit in a cgroup path.
pub fn unit_of(cgroup: &str) -> Option<&str> {
    cgroup.rsplit('/').find(|c| c.ends_with(".service") || c.ends_with(".scope"))
}

/// The app name hidden in an app unit: `app-vela-spotify-48302b.scope` →
/// spotify, `app-flatpak-com.spotify.Client-123.scope` → com.spotify.Client,
/// `app-firefox@ab12.service` → firefox, `app-Hyprland-kitty-1a2b.scope` → kitty.
pub fn app_hint(unit: &str) -> Option<String> {
    let body = unit.strip_prefix("app-")?;
    let body = body.strip_suffix(".scope").or_else(|| body.strip_suffix(".service"))?;
    if let Some((name, _)) = body.split_once('@') {
        return Some(unescape_unit(name));
    }
    let mut body = body;
    // Escaped dashes (\x2d) belong to the name, so this runs before unescaping.
    for p in ["vela-", "flatpak-", "Hyprland-", "hyprland-", "uwsm-"] {
        if let Some(rest) = body.strip_prefix(p) {
            body = rest;
            break;
        }
    }
    // Drop the instance suffix: digits, or vela's pid+nanos hex.
    let name = match body.rsplit_once('-') {
        Some((head, tail)) if !head.is_empty() && tail.chars().all(|c| c.is_ascii_hexdigit()) && tail.chars().any(|c| c.is_ascii_digit()) => head,
        _ => body,
    };
    Some(unescape_unit(name))
}

/// Lowercased keys a desktop app can be found by.
fn keys(app: &DesktopApp) -> Vec<String> {
    let mut k = vec![app.id.to_lowercase()];
    if let Some(last) = app.id.rsplit('.').next() {
        k.push(last.to_lowercase());
    }
    if !app.wm_class.is_empty() {
        k.push(app.wm_class.to_lowercase());
    }
    if !app.exec_bin.is_empty() {
        k.push(app.exec_bin.to_lowercase());
    }
    k
}

pub struct Matcher<'a> {
    by_key: HashMap<String, &'a DesktopApp>,
    by_bin: HashMap<String, &'a DesktopApp>,
}

impl<'a> Matcher<'a> {
    pub fn new(apps: &'a [DesktopApp]) -> Self {
        let mut by_key = HashMap::new();
        let mut by_bin = HashMap::new();
        // Later (lower priority) entries don't overwrite earlier ones.
        for a in apps {
            for k in keys(a) {
                by_key.entry(k).or_insert(a);
            }
            if !a.exec_bin.is_empty() {
                by_bin.entry(a.exec_bin.to_lowercase()).or_insert(a);
            }
        }
        Matcher { by_key, by_bin }
    }

    pub fn by_name(&self, name: &str) -> Option<&'a DesktopApp> {
        let n = name.to_lowercase();
        if n.is_empty() {
            return None;
        }
        self.by_key.get(&n).copied()
    }

    pub fn by_bin(&self, bin: &str) -> Option<&'a DesktopApp> {
        self.by_bin.get(&bin.to_lowercase()).copied()
    }
}

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Program name for display: exe basename, else argv[0], else comm.
pub fn program(p: &ProcInfo) -> String {
    let exe = basename(p.exe.trim_end_matches(" (deleted)"));
    if !exe.is_empty() {
        return exe.to_owned();
    }
    p.cmdline
        .first()
        .map(|a| basename(a).to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| p.comm.clone())
}

fn pretty_unit(unit: &str) -> String {
    let u = unescape_unit(unit);
    let stem = u.trim_end_matches(".service").trim_end_matches(".scope");
    stem.split_once('@').map_or(stem, |(a, _)| a).to_owned()
}

enum Seed {
    Kernel,
    Unit { unit: String, user: bool, app: bool },
    Tree { root: i32 },
    Host(i32),
    Session(String),
    Loose,
}

/// Groups `procs` (all of /proc) with the open windows and desktop entries.
pub fn group(procs: &[ProcInfo], windows: &[WindowInfo], desktop: &[DesktopApp]) -> Vec<Group> {
    let matcher = Matcher::new(desktop);
    let by_pid: HashMap<i32, &ProcInfo> = procs.iter().map(|p| (p.pid, p)).collect();

    let is_host = |p: &ProcInfo| SESSION_HOSTS.contains(&p.comm.as_str()) || SESSION_HOSTS.contains(&program(p).as_str());

    let seed = |p: &ProcInfo| -> Seed {
        if p.kernel {
            return Seed::Kernel;
        }
        match unit_of(&p.cgroup) {
            Some(u) if u.starts_with("session-") && u.ends_with(".scope") => {
                if is_host(p) {
                    return Seed::Host(p.pid);
                }
                // The child of the nearest session host is the app.
                let mut cur = p;
                let mut guard = 0;
                while let Some(parent) = by_pid.get(&cur.ppid) {
                    if is_host(parent) {
                        return Seed::Tree { root: cur.pid };
                    }
                    if parent.cgroup != p.cgroup || guard > 64 {
                        break;
                    }
                    cur = parent;
                    guard += 1;
                }
                Seed::Session(u.to_owned())
            }
            Some(u) => {
                let user = p.cgroup.contains("/user@");
                let app = u.starts_with("app-") && !u.starts_with("app-dbus") && (u.ends_with(".scope") || u.contains('@') || p.cgroup.contains("/app.slice/"));
                Seed::Unit { unit: u.to_owned(), user, app }
            }
            None => Seed::Loose,
        }
    };

    struct Acc {
        kind_hint: Kind,
        unit: Option<String>,
        user_unit: bool,
        hint: Option<String>,
        pids: Vec<i32>,
        host: bool,
        tree: bool,
    }
    let mut groups: BTreeMap<String, Acc> = BTreeMap::new();
    for p in procs {
        let (key, acc) = match seed(p) {
            Seed::Kernel => (
                "kernel".to_owned(),
                Acc {
                    kind_hint: Kind::Kernel,
                    unit: None,
                    user_unit: false,
                    hint: Some("Kernel".into()),
                    pids: vec![],
                    host: false,
                    tree: false,
                },
            ),
            Seed::Unit { unit, user, app } => {
                let kind = if app {
                    Kind::Background
                } else if user {
                    Kind::Service
                } else {
                    Kind::System
                };
                let hint = if app {
                    app_hint(&unit)
                } else if unit == "init.scope" {
                    None
                } else {
                    Some(pretty_unit(&unit))
                };
                (
                    format!("unit:{}", p.cgroup),
                    Acc {
                        kind_hint: kind,
                        unit: Some(unit),
                        user_unit: user,
                        hint,
                        pids: vec![],
                        host: false,
                        tree: false,
                    },
                )
            }
            Seed::Tree { root } => (
                format!("tree:{root}"),
                Acc {
                    kind_hint: Kind::Background,
                    unit: None,
                    user_unit: false,
                    hint: None,
                    pids: vec![],
                    host: false,
                    tree: true,
                },
            ),
            Seed::Host(pid) => (
                format!("host:{pid}"),
                Acc {
                    kind_hint: Kind::System,
                    unit: None,
                    user_unit: false,
                    hint: None,
                    pids: vec![],
                    host: true,
                    tree: false,
                },
            ),
            Seed::Session(u) => (
                format!("session:{u}"),
                Acc {
                    kind_hint: Kind::System,
                    unit: Some(u),
                    user_unit: false,
                    hint: Some("Login session".into()),
                    pids: vec![],
                    host: false,
                    tree: false,
                },
            ),
            Seed::Loose => (
                format!("loose:{}", p.comm),
                Acc {
                    kind_hint: Kind::System,
                    unit: None,
                    user_unit: false,
                    hint: Some(p.comm.clone()),
                    pids: vec![],
                    host: false,
                    tree: false,
                },
            ),
        };
        groups.entry(key).or_insert(acc).pids.push(p.pid);
    }

    let depth = |pid: i32, members: &[i32]| -> usize {
        let mut d = 0;
        let mut cur = pid;
        while let Some(p) = by_pid.get(&cur) {
            if !members.contains(&p.ppid) || d > 64 {
                break;
            }
            cur = p.ppid;
            d += 1;
        }
        d
    };

    let mut out: Vec<Group> = Vec::new();
    for (key, mut acc) in groups {
        acc.pids.sort_unstable();
        let wins: Vec<WindowInfo> = windows.iter().filter(|w| acc.pids.contains(&w.pid)).cloned().collect();
        let top = acc.pids.iter().copied().min_by_key(|pid| (depth(*pid, &acc.pids), *pid)).unwrap_or(0);
        let mut main_pid = wins.first().map_or(top, |w| w.pid);
        // Hyprland's exec runs apps through `sh -c`: the app is below it.
        if acc.tree
            && wins.is_empty()
            && by_pid.get(&main_pid).is_some_and(|m| is_shell(&m.comm))
            && let Some(job) = foreground(&acc.pids, &by_pid, main_pid)
        {
            main_pid = job.pid;
        }
        let main = by_pid.get(&main_pid).copied();

        let mut kind = acc.kind_hint;
        if !wins.is_empty() && kind != Kind::Kernel {
            kind = Kind::Window;
        }
        // A terminal tab: the scope starts with a shell and has no window.
        let mut task_leaf: Option<&ProcInfo> = None;
        let terminal_scope = acc.hint.as_deref().is_some_and(is_terminal_hint);
        if matches!(kind, Kind::Background) && !acc.tree && (terminal_scope || main.is_some_and(|m| is_shell(&m.comm))) {
            kind = Kind::Task;
            task_leaf = foreground(&acc.pids, &by_pid, main_pid);
        }

        let desktop_app = match kind {
            Kind::Window | Kind::Background => wins
                .iter()
                .find_map(|w| matcher.by_name(&w.class))
                .or_else(|| acc.hint.as_deref().and_then(|h| matcher.by_name(h)))
                .or_else(|| main.and_then(|m| matcher.by_bin(&program(m))))
                .or_else(|| main.and_then(|m| matcher.by_name(&m.comm))),
            _ => None,
        };

        // Pulse is a Quickshell window: name it after itself, not Quickshell.
        let pulse_window = wins.iter().any(|w| w.class == "org.quickshell" && w.title.starts_with("Pulse "));
        let desktop_app = if pulse_window {
            matcher.by_name("vela-pulse").or(desktop_app)
        } else {
            desktop_app
        };
        let (name, icon) = if pulse_window && desktop_app.is_none_or(|a| a.id != "vela-pulse") {
            ("Pulse".to_owned(), "vela-pulse".to_owned())
        } else if let Some(app) = desktop_app {
            (app.name.clone(), app.icon.clone())
        } else if kind == Kind::Task {
            let leaf = task_leaf.or(main);
            let n = leaf.map(task_name).unwrap_or_else(|| "shell".into());
            (n, "utilities-terminal".into())
        } else if acc.host {
            let n = main.map(program).unwrap_or_default();
            (n.clone(), n.to_lowercase())
        } else if let Some(w) = wins.first() {
            (pretty_class(&w.class), w.class.to_lowercase())
        } else if let Some(h) = acc.hint.clone().filter(|h| !h.is_empty()) {
            let icon = if matches!(kind, Kind::Background | Kind::Window) {
                h.to_lowercase()
            } else {
                String::new()
            };
            (h, icon)
        } else {
            let n = main.map(program).unwrap_or_else(|| "?".into());
            (n.clone(), n.to_lowercase())
        };

        out.push(Group {
            key,
            kind,
            name,
            icon,
            desktop_id: desktop_app.map(|a| a.id.clone()),
            unit: acc.unit,
            user_unit: acc.user_unit,
            main_pid,
            pids: acc.pids,
            windows: wins,
        });
    }
    merge_same_app(out)
}

/// One app started twice (two scopes, or a scope plus a tree) is one entry,
/// like other task managers show it.
fn merge_same_app(groups: Vec<Group>) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::with_capacity(groups.len());
    for g in groups {
        let mergeable = matches!(g.kind, Kind::Window | Kind::Background) && g.desktop_id.is_some();
        if mergeable
            && let Some(prev) = out
                .iter_mut()
                .find(|o| matches!(o.kind, Kind::Window | Kind::Background) && o.desktop_id == g.desktop_id)
        {
            prev.pids.extend(g.pids);
            prev.pids.sort_unstable();
            if prev.windows.is_empty() && !g.windows.is_empty() {
                prev.main_pid = g.main_pid;
                prev.key = g.key;
                prev.unit = g.unit;
            }
            prev.windows.extend(g.windows);
            if !prev.windows.is_empty() {
                prev.kind = Kind::Window;
            }
            continue;
        }
        out.push(g);
    }
    out
}

/// Scopes terminals create per tab (Ghostty: `app-ghostty-surface-transient-N`).
fn is_terminal_hint(hint: &str) -> bool {
    hint.contains("surface-transient")
        || ["kitty", "alacritty", "foot", "wezterm", "konsole", "gnome-terminal", "ptyxis", "tmux-spawn"]
            .iter()
            .any(|t| hint.starts_with(t))
}

/// What a terminal tab is running: the newest non-shell process directly
/// below a shell, following single-child chains.
fn foreground<'a>(pids: &[i32], by_pid: &HashMap<i32, &'a ProcInfo>, top: i32) -> Option<&'a ProcInfo> {
    let children = |pid: i32| -> Vec<&'a ProcInfo> {
        let mut c: Vec<&ProcInfo> = pids.iter().filter_map(|p| by_pid.get(p).copied()).filter(|p| p.ppid == pid).collect();
        c.sort_by_key(|p| std::cmp::Reverse(p.pid));
        c
    };
    let mut cur = *by_pid.get(&top)?;
    for _ in 0..16 {
        let kids = children(cur.pid);
        if let Some(job) = kids.iter().find(|k| !is_shell(&k.comm)) {
            return Some(job);
        }
        match kids.first() {
            Some(sub) => cur = sub,
            None => return Some(cur),
        }
    }
    Some(cur)
}

/// `node /usr/bin/claude …` → claude; `python3 manage.py` → python3 manage.py.
fn task_name(p: &ProcInfo) -> String {
    let prog = program(p);
    let interpreters = ["node", "python", "python3", "ruby", "perl", "java", "bun", "deno"];
    if interpreters.iter().any(|i| prog == *i || prog.starts_with(&format!("{i}."))) {
        if let Some(script) = p.cmdline.iter().skip(1).find(|a| !a.starts_with('-')) {
            let b = basename(script);
            // Installed entry points (no extension) are the program's name.
            let plain = !b.contains('.');
            return if prog.starts_with("node") || prog == "bun" || prog == "deno" || plain {
                b.trim_end_matches(".js").to_owned()
            } else {
                format!("{prog} {b}")
            };
        }
    }
    // Versioned binaries (~/.local/share/claude/versions/2.1.288): the
    // directory above says what it is.
    if prog.starts_with(|c: char| c.is_ascii_digit()) {
        let path = if p.exe.is_empty() {
            p.cmdline.first().cloned().unwrap_or_default()
        } else {
            p.exe.clone()
        };
        let named = path
            .trim_end_matches(" (deleted)")
            .rsplit('/')
            .skip(1)
            .find(|c| !c.is_empty() && !["versions", "bin", "lib", "libexec", "current"].contains(c) && !c.starts_with(|ch: char| ch.is_ascii_digit()));
        if let Some(n) = named {
            return n.trim_start_matches('.').to_owned();
        }
    }
    if p.comm.len() > prog.len() && p.comm.starts_with(&prog) {
        return p.comm.clone();
    }
    prog
}

/// `org.gnome.Nautilus` → Nautilus, `firefox` → Firefox.
pub fn pretty_class(class: &str) -> String {
    let last = class.rsplit('.').next().unwrap_or(class);
    let mut c = last.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => class.to_owned(),
    }
}

/// Program basename of a desktop Exec line (`env A=1 foo %U` → foo,
/// `flatpak run org.x.Y` → org.x.Y).
pub fn exec_bin(exec: &str) -> String {
    let argv = shlex::split(exec).unwrap_or_default();
    let mut it = argv.iter().map(String::as_str).peekable();
    while let Some(a) = it.next() {
        if a == "env" || a.contains('=') && !a.starts_with('-') && !a.contains('/') {
            continue;
        }
        if a.starts_with('-') {
            continue;
        }
        let b = basename(a);
        if b == "flatpak" {
            return it.find(|x| !x.starts_with('-') && *x != "run").map(str::to_owned).unwrap_or_default();
        }
        return b.to_owned();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: i32, ppid: i32, comm: &str, cgroup: &str) -> ProcInfo {
        ProcInfo {
            pid,
            ppid,
            uid: 1000,
            comm: comm.into(),
            exe: format!("/usr/bin/{comm}"),
            cmdline: vec![comm.into()],
            cgroup: cgroup.into(),
            kernel: false,
        }
    }

    fn app(id: &str, name: &str, bin: &str, wm: &str) -> DesktopApp {
        DesktopApp {
            id: id.into(),
            name: name.into(),
            icon: id.into(),
            wm_class: wm.into(),
            exec_bin: bin.into(),
        }
    }

    const APP: &str = "/user.slice/user-1000.slice/user@1000.service/app.slice/";
    const SESSION: &str = "/user.slice/user-1000.slice/session-2.scope";

    #[test]
    fn unit_hints() {
        assert_eq!(app_hint("app-vela-spotify-4830242334b6c9.scope").as_deref(), Some("spotify"));
        assert_eq!(
            app_hint("app-vela-org.gnome.Nautilus-11455672b7a114d.scope").as_deref(),
            Some("org.gnome.Nautilus")
        );
        assert_eq!(app_hint("app-vesktop-1963356.scope").as_deref(), Some("vesktop"));
        assert_eq!(app_hint("app-flatpak-com.spotify.Client-12345.scope").as_deref(), Some("com.spotify.Client"));
        assert_eq!(app_hint("app-firefox@ab12cd.service").as_deref(), Some("firefox"));
        assert_eq!(app_hint("app-Hyprland-kitty-1a2b.scope").as_deref(), Some("kitty"));
        assert_eq!(app_hint("app-gnome\\x2dterminal-42.scope").as_deref(), Some("gnome-terminal"));
        assert_eq!(app_hint("app-obsidian.scope").as_deref(), Some("obsidian"));
        assert_eq!(app_hint("pipewire.service"), None);
        assert_eq!(unit_of("/system.slice/postgresql.service"), Some("postgresql.service"));
        assert_eq!(unit_of("/"), None);
    }

    #[test]
    fn exec_bins() {
        assert_eq!(exec_bin("/usr/bin/firefox %u"), "firefox");
        assert_eq!(exec_bin("env GDK_BACKEND=x11 code --new-window %F"), "code");
        assert_eq!(
            exec_bin("/usr/bin/flatpak run --branch=stable --arch=x86_64 com.spotify.Client @@u %U @@"),
            "com.spotify.Client"
        );
        assert_eq!(pretty_class("org.gnome.Nautilus"), "Nautilus");
    }

    #[test]
    fn groups_apps_services_tasks_and_session_trees() {
        let mut k = p(2, 0, "kthreadd", "");
        k.kernel = true;
        let procs = vec![
            p(1, 0, "systemd", "/init.scope"),
            k,
            // Spotify launched by vela: a scope with a zygote and children.
            p(100, 1, "spotify", &format!("{APP}app-vela-spotify-4830242334b6c9.scope")),
            p(101, 100, "spotify", &format!("{APP}app-vela-spotify-4830242334b6c9.scope")),
            // Vesktop started twice: two scopes, one app.
            p(200, 1, "electron", &format!("{APP}app-vela-vesktop-1a2b3c4d.scope")),
            p(210, 1, "electron", &format!("{APP}app-vesktop-1963356.scope")),
            // A terminal tab running Claude.
            p(300, 1, "fish", &format!("{APP}app-ghostty-surface-transient-42.scope")),
            {
                let mut c = p(301, 300, "claude", &format!("{APP}app-ghostty-surface-transient-42.scope"));
                c.exe = "/usr/bin/node".into();
                c.cmdline = vec!["node".into(), "/usr/bin/claude".into()];
                c
            },
            p(
                400,
                1,
                "pipewire",
                "/user.slice/user-1000.slice/user@1000.service/session.slice/pipewire.service",
            ),
            p(500, 1, "postgres", "/system.slice/postgresql.service"),
            p(501, 500, "postgres", "/system.slice/postgresql.service"),
            // Session: Hyprland and an app it exec'd through sh.
            p(600, 1, "Hyprland", SESSION),
            p(601, 600, "sh", SESSION),
            p(602, 601, "waybar", SESSION),
            p(610, 600, "nm-applet", SESSION),
            p(700, 1, "qs", &format!("{APP}app-vela-vela_pulse-ab12cd34.scope")),
        ];
        let windows = vec![
            WindowInfo {
                pid: 210,
                class: "vesktop".into(),
                title: "Discord".into(),
                ..Default::default()
            },
            WindowInfo {
                pid: 700,
                class: "org.quickshell".into(),
                title: "Pulse — Task manager".into(),
                ..Default::default()
            },
        ];
        let desktop = vec![
            app("spotify", "Spotify", "spotify", ""),
            app("vesktop", "Vesktop", "vesktop", ""),
            app("nm-applet", "Network", "nm-applet", ""),
        ];
        let g = group(&procs, &windows, &desktop);
        let find = |name: &str| {
            g.iter()
                .find(|x| x.name == name)
                .unwrap_or_else(|| panic!("{name} missing in {:#?}", g.iter().map(|x| &x.name).collect::<Vec<_>>()))
        };

        let sp = find("Spotify");
        assert_eq!((sp.kind, sp.pids.clone(), sp.main_pid), (Kind::Background, vec![100, 101], 100));
        assert_eq!(sp.unit.as_deref(), Some("app-vela-spotify-4830242334b6c9.scope"));

        let ves = find("Vesktop");
        assert_eq!((ves.kind, ves.pids.clone(), ves.main_pid), (Kind::Window, vec![200, 210], 210));
        assert_eq!(g.iter().filter(|x| x.name == "Vesktop").count(), 1);

        let task = find("claude");
        assert_eq!((task.kind, task.icon.as_str()), (Kind::Task, "utilities-terminal"));

        assert_eq!(find("pipewire").kind, Kind::Service);
        let pg = find("postgresql");
        assert_eq!((pg.kind, pg.pids.len(), pg.user_unit), (Kind::System, 2, false));

        assert_eq!(find("Hyprland").kind, Kind::System);
        let wb = find("waybar");
        assert_eq!((wb.kind, wb.pids.clone()), (Kind::Background, vec![601, 602]));
        assert_eq!(find("Network").kind, Kind::Background);
        assert_eq!(find("Kernel").kind, Kind::Kernel);
        let pulse = find("Pulse");
        assert_eq!((pulse.kind, pulse.icon.as_str()), (Kind::Window, "vela-pulse"));
        assert_eq!(find("systemd").kind, Kind::System);
    }

    #[test]
    fn task_names() {
        let mut n = p(1, 0, "MainThread", "");
        n.exe = "/usr/bin/python3.13".into();
        n.cmdline = vec!["python3".into(), "-u".into(), "/srv/manage.py".into(), "runserver".into()];
        assert_eq!(task_name(&n), "python3.13 manage.py");
        let mut c = p(1, 0, "cargo", "");
        c.exe = "/home/x/.cargo/bin/cargo".into();
        assert_eq!(task_name(&c), "cargo");
        let mut v = p(1, 0, "2.1.288", "");
        v.exe = "/home/x/.local/share/claude/versions/2.1.288".into();
        assert_eq!(task_name(&v), "claude");
        let mut e = p(1, 0, "diary-web", "");
        e.exe = "/x/bin/python3.13".into();
        e.cmdline = vec!["/x/bin/python".into(), "/home/x/.local/bin/diary-web".into()];
        assert_eq!(task_name(&e), "diary-web");
    }
}
