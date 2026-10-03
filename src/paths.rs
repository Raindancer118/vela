//! XDG base directories, `~` expansion and executable lookup.

use std::env;
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const APP_ID: &str = "vela";

pub fn home_dir() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    match env::var_os(var).map(PathBuf::from) {
        Some(p) if p.is_absolute() => p,
        _ => home_dir().join(fallback),
    }
}

/// `~/.config/hypr`.
pub fn hypr_config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("hypr")
}

pub fn config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join(APP_ID)
}

/// In NixOS mode (see `nixos`) vela's own files live in the state
/// directory of the user's NixOS repository.
fn in_state_dir(mode: Option<&crate::nixos::NixosMode>, name: &str, default: PathBuf) -> PathBuf {
    match mode {
        Some(m) => m.state_dir.join(name),
        None => default,
    }
}

pub fn config_file() -> PathBuf {
    in_state_dir(crate::nixos::active(), "config.toml", config_dir().join("config.toml"))
}

pub fn state_dir() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state").join(APP_ID)
}

/// vela's Hyprland overrides (Settings → Hyprland, `vela mcp`).
pub fn hypr_overrides_file() -> PathBuf {
    in_state_dir(crate::nixos::active(), "hyprland.toml", config_dir().join("hyprland.toml"))
}

/// The Lua generated from them; contrib/hyprland/vela.lua loads this path
/// (in NixOS mode the Home Manager module passes it as `settings_file`).
pub fn hypr_lua_file() -> PathBuf {
    in_state_dir(crate::nixos::active(), "hyprland.lua", state_dir().join("hyprland.lua"))
}

pub fn cache_dir() -> PathBuf {
    xdg_dir("XDG_CACHE_HOME", ".cache").join(APP_ID)
}

pub fn socket_path() -> PathBuf {
    // Lets tests run a second daemon next to the real one.
    if let Some(p) = env::var_os("VELA_SOCKET").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    match env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join(format!("{APP_ID}.sock")),
        // SAFETY: getuid has no preconditions.
        _ => env::temp_dir().join(format!("{APP_ID}-{}.sock", unsafe { libc::getuid() })),
    }
}

/// Application data directories in priority order (highest first).
pub fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![xdg_dir("XDG_DATA_HOME", ".local/share")];
    let system = env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(system.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    // A systemd user service often starts with a reduced environment, so make
    // sure flatpak/snap exports are found even if XDG_DATA_DIRS lacks them.
    for extra in extra_data_dirs(&home_dir(), &user_name(), crate::nixos::running_nixos()) {
        if !dirs.contains(&extra) && extra.is_dir() {
            dirs.push(extra);
        }
    }
    dirs
}

/// Expands a leading `~` or `~/`.
pub fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        home_dir()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(path)
    }
}

/// Shortens a path under `$HOME` to `~/…` for display.
pub fn display_path(path: &Path) -> String {
    let home = home_dir();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) if home != Path::new("/") => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

fn user_name() -> String {
    env::var("USER").unwrap_or_default()
}

/// Where NixOS puts programs and their data, in its own PATH order
/// (`/run/wrappers/bin` holds the setuid sudo).
fn nix_profile_dirs(home: &Path, user: &str, sub: &str) -> Vec<PathBuf> {
    let mut dirs = vec![
        home.join(".nix-profile").join(sub),
        home.join(".local/state/nix/profile").join(sub),
        PathBuf::from(format!("/etc/profiles/per-user/{user}/{sub}")),
        PathBuf::from("/nix/var/nix/profiles/default").join(sub),
        PathBuf::from("/run/current-system/sw").join(sub),
    ];
    if sub == "bin" {
        dirs.insert(0, PathBuf::from("/run/wrappers/bin"));
    }
    dirs
}

fn extra_bin_dirs(home: &Path, user: &str, nixos: bool) -> Vec<PathBuf> {
    let mut dirs = vec![home.join(".local/bin"), home.join(".cargo/bin"), home.join("bin")];
    if nixos {
        dirs.extend(nix_profile_dirs(home, user, "bin"));
    } else {
        dirs.extend([PathBuf::from("/usr/local/bin"), PathBuf::from("/usr/bin")]);
    }
    dirs
}

fn extra_data_dirs(home: &Path, user: &str, nixos: bool) -> Vec<PathBuf> {
    let mut dirs = vec![
        home.join(".local/share/flatpak/exports/share"),
        PathBuf::from("/var/lib/flatpak/exports/share"),
        PathBuf::from("/var/lib/snapd/desktop"),
    ];
    if nixos {
        dirs.extend(nix_profile_dirs(home, user, "share"));
    } else {
        dirs.push(PathBuf::from("/usr/share"));
    }
    dirs
}

/// `current` (a PATH-like list) with the missing ones of `extra` at the end.
fn appended(current: Option<&OsStr>, extra: &[PathBuf]) -> OsString {
    let mut parts: Vec<PathBuf> = current
        .map(|p| env::split_paths(p).filter(|d| !d.as_os_str().is_empty()).collect())
        .unwrap_or_default();
    for dir in extra {
        if !parts.contains(dir) {
            parts.push(dir.clone());
        }
    }
    env::join_paths(parts).unwrap_or_default()
}

/// PATH for launched children: the inherited PATH plus common user binary
/// directories that a systemd user service usually doesn't have.
pub fn child_path_env() -> OsString {
    appended(
        env::var_os("PATH").as_deref(),
        &extra_bin_dirs(&home_dir(), &user_name(), crate::nixos::running_nixos()),
    )
}

/// On NixOS a user service or a bare exec may start without the profiles in
/// PATH and XDG_DATA_DIRS (setuid sudo, systemctl, claude, apps, icons).
/// Completes both for this process and everything it starts; call first
/// thing in main, before any thread exists.
pub fn complete_nixos_env() {
    if !crate::nixos::running_nixos() {
        return;
    }
    let (home, user) = (home_dir(), user_name());
    let path = child_path_env();
    let data = appended(env::var_os("XDG_DATA_DIRS").as_deref(), &nix_profile_dirs(&home, &user, "share"));
    // SAFETY: called at the start of main, while the process is single-threaded.
    unsafe {
        env::set_var("PATH", path);
        env::set_var("XDG_DATA_DIRS", data);
    }
}

fn is_executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Resolves a command name like `claude` or `~/bin/x` to an absolute path.
pub fn find_executable(cmd: &str) -> Option<PathBuf> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }
    if cmd.contains('/') {
        let p = expand_tilde(cmd);
        return is_executable(&p).then_some(p);
    }
    env::split_paths(&child_path_env()).map(|dir| dir.join(cmd)).find(|p| is_executable(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expansion() {
        let home = home_dir();
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("~/a/b"), home.join("a/b"));
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
        assert_eq!(expand_tilde("~user/x"), PathBuf::from("~user/x"));
    }

    #[test]
    fn finds_executables() {
        assert!(find_executable("sh").is_some());
        assert!(find_executable("/bin/sh").is_some());
        assert!(find_executable("definitely-not-a-real-binary-vela").is_none());
        assert!(find_executable("").is_none());
        // Directories and non-executable files are not executables.
        assert!(find_executable("/etc/hostname").is_none());
        assert!(find_executable("/usr").is_none());
    }

    #[test]
    fn nixos_program_dirs_only_on_nixos() {
        let home = Path::new("/home/u");
        let nix = extra_bin_dirs(home, "u", true);
        let expected = ["/run/wrappers/bin", "/etc/profiles/per-user/u/bin", "/home/u/.nix-profile/bin"];
        for d in expected.into_iter().chain(["/run/current-system/sw/bin"]) {
            assert!(nix.contains(&PathBuf::from(d)), "{d} missing");
        }
        // The setuid sudo comes before every other place that could have one.
        let pos = |d: &str| nix.iter().position(|p| p == Path::new(d)).unwrap();
        assert!(pos("/run/wrappers/bin") < pos("/run/current-system/sw/bin"));
        let arch = extra_bin_dirs(home, "u", false);
        assert!(arch.contains(&PathBuf::from("/usr/bin")));
        assert!(!arch.iter().any(|p| p.starts_with("/run") || p.starts_with("/etc")));
    }

    #[test]
    fn appends_missing_dirs_once() {
        let extra = [PathBuf::from("/b"), PathBuf::from("/c")];
        assert_eq!(appended(Some(OsStr::new("/a:/b")), &extra), OsString::from("/a:/b:/c"));
        assert_eq!(appended(None, &extra), OsString::from("/b:/c"));
        assert_eq!(appended(Some(OsStr::new("")), &extra), OsString::from("/b:/c"));
    }

    #[test]
    fn nixos_data_dirs_only_on_nixos() {
        let home = Path::new("/home/u");
        let nix = extra_data_dirs(home, "u", true);
        for d in ["/etc/profiles/per-user/u/share", "/home/u/.nix-profile/share", "/run/current-system/sw/share"] {
            assert!(nix.contains(&PathBuf::from(d)), "{d} missing");
        }
        assert!(!extra_data_dirs(home, "u", false).iter().any(|p| p.starts_with("/run/current-system")));
    }

    #[test]
    fn display_path_shortens_home() {
        let home = home_dir();
        assert_eq!(display_path(&home.join("Docs/a.pdf")), "~/Docs/a.pdf");
        assert_eq!(display_path(Path::new("/etc/fstab")), "/etc/fstab");
    }

    #[test]
    fn state_dir_redirects_vela_files_only_in_nixos_mode() {
        let mode = crate::nixos::NixosMode {
            state_dir: "/repo/vela".into(),
            rebuild: "rebuild".into(),
        };
        assert_eq!(
            in_state_dir(Some(&mode), "config.toml", "/home/u/.config/vela/config.toml".into()),
            PathBuf::from("/repo/vela/config.toml")
        );
        assert_eq!(
            in_state_dir(None, "config.toml", "/home/u/.config/vela/config.toml".into()),
            PathBuf::from("/home/u/.config/vela/config.toml")
        );
    }
}
