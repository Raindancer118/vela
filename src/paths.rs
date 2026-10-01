//! XDG base directories, `~` expansion and executable lookup.

use std::env;
use std::ffi::OsString;
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

pub fn config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join(APP_ID)
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn state_dir() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state").join(APP_ID)
}

/// vela's Hyprland overrides (Settings → Hyprland, `vela mcp`).
pub fn hypr_overrides_file() -> PathBuf {
    config_dir().join("hyprland.toml")
}

/// The Lua generated from them; contrib/hyprland/vela.lua loads this path.
pub fn hypr_lua_file() -> PathBuf {
    state_dir().join("hyprland.lua")
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
    for extra in [
        home_dir().join(".local/share/flatpak/exports/share"),
        PathBuf::from("/var/lib/flatpak/exports/share"),
        PathBuf::from("/var/lib/snapd/desktop"),
        PathBuf::from("/usr/share"),
    ] {
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

fn user_bin_dirs() -> Vec<PathBuf> {
    let home = home_dir();
    vec![
        home.join(".local/bin"),
        home.join(".cargo/bin"),
        home.join("bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]
}

/// PATH for launched children: the inherited PATH plus common user binary
/// directories that a systemd user service usually doesn't have.
pub fn child_path_env() -> OsString {
    let mut parts: Vec<PathBuf> = env::var_os("PATH").map(|p| env::split_paths(&p).collect()).unwrap_or_default();
    for dir in user_bin_dirs() {
        if !parts.contains(&dir) {
            parts.push(dir);
        }
    }
    env::join_paths(parts).unwrap_or_default()
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
    fn display_path_shortens_home() {
        let home = home_dir();
        assert_eq!(display_path(&home.join("Docs/a.pdf")), "~/Docs/a.pdf");
        assert_eq!(display_path(Path::new("/etc/fstab")), "/etc/fstab");
    }
}
