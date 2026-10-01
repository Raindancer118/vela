//! NixOS with Home Manager: the module writes `~/.config/vela/nixos.toml`
//! naming a directory inside the user's NixOS repository. While it exists
//! (and the system is NixOS), vela keeps config.toml and its Hyprland
//! overrides there instead of ~/.config and ~/.local/state, so they are part
//! of the declarative configuration; Settings offers to commit and rebuild.

use serde::Deserialize;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NixosMode {
    /// Absolute directory in the NixOS repository vela writes to.
    pub state_dir: PathBuf,
    /// Command run in a terminal by "Apply & rebuild".
    pub rebuild: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    NotNixos,
    /// NixOS, but the Home Manager module did not name a state directory.
    Unconfigured,
    Active(NixosMode),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    state_dir: PathBuf,
    #[serde(default = "default_rebuild")]
    rebuild: String,
}

fn default_rebuild() -> String {
    "sudo nixos-rebuild switch --flake .".into()
}

fn is_nixos(os_release: &str) -> bool {
    os_release.lines().any(|line| {
        line.split_once('=')
            .is_some_and(|(k, v)| k.trim() == "ID" && v.trim().trim_matches('"') == "nixos")
    })
}

pub fn detect(os_release: &str, pointer: Option<&str>) -> Status {
    if !is_nixos(os_release) {
        return Status::NotNixos;
    }
    let Some(text) = pointer else {
        return Status::Unconfigured;
    };
    match toml::from_str::<Pointer>(text) {
        Ok(p) if p.state_dir.is_absolute() => Status::Active(NixosMode {
            state_dir: p.state_dir,
            rebuild: p.rebuild,
        }),
        Ok(p) => {
            log::warn!("{}: state_dir must be absolute, got {}", pointer_file().display(), p.state_dir.display());
            Status::Unconfigured
        }
        Err(e) => {
            log::warn!("{}: {e}", pointer_file().display());
            Status::Unconfigured
        }
    }
}

pub fn pointer_file() -> PathBuf {
    crate::paths::config_dir().join("nixos.toml")
}

/// Detected once per process.
pub fn status() -> &'static Status {
    static STATUS: OnceLock<Status> = OnceLock::new();
    STATUS.get_or_init(|| {
        let os = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let ptr = std::fs::read_to_string(pointer_file()).ok();
        detect(&os, ptr.as_deref())
    })
}

pub fn active() -> Option<&'static NixosMode> {
    match status() {
        Status::Active(m) => Some(m),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NIXOS: &str = "NAME=NixOS\nID=nixos\nVERSION_ID=\"26.11\"\n";
    const ARCH: &str = "NAME=\"Arch Linux\"\nID=arch\n";
    const PTR: &str = "state_dir = \"/home/u/nixos/vela/state\"\nrebuild = \"rebuild --fast\"\n";

    #[test]
    fn not_nixos_ignores_the_pointer() {
        assert_eq!(detect(ARCH, Some(PTR)), Status::NotNixos);
    }

    #[test]
    fn id_like_alone_is_not_nixos() {
        assert_eq!(detect("ID=foo\nID_LIKE=nixos\n", Some(PTR)), Status::NotNixos);
    }

    #[test]
    fn quoted_and_crlf_ids_are_recognised() {
        assert!(matches!(detect("ID=\"nixos\"\r\n", Some(PTR)), Status::Active(_)));
        assert!(matches!(detect("  ID = nixos \n", Some(PTR)), Status::Active(_)));
    }

    #[test]
    fn nixos_without_pointer_is_unconfigured() {
        assert_eq!(detect(NIXOS, None), Status::Unconfigured);
    }

    #[test]
    fn broken_or_relative_pointer_is_unconfigured() {
        assert_eq!(detect(NIXOS, Some("state_dir = [")), Status::Unconfigured);
        assert_eq!(detect(NIXOS, Some("state_dir = \"nixos/state\"")), Status::Unconfigured);
        assert_eq!(detect(NIXOS, Some("state_dir = \"~/nixos/state\"")), Status::Unconfigured);
        assert_eq!(detect(NIXOS, Some("rebuild = \"x\"")), Status::Unconfigured);
    }

    #[test]
    fn valid_pointer_activates_with_default_rebuild() {
        assert_eq!(
            detect(NIXOS, Some("state_dir = \"/s\"")),
            Status::Active(NixosMode {
                state_dir: "/s".into(),
                rebuild: "sudo nixos-rebuild switch --flake .".into()
            })
        );
        assert_eq!(
            detect(NIXOS, Some(PTR)),
            Status::Active(NixosMode {
                state_dir: "/home/u/nixos/vela/state".into(),
                rebuild: "rebuild --fast".into()
            })
        );
    }
}
