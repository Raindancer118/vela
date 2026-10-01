//! NixOS with Home Manager: the module writes `~/.config/vela/nixos.toml`
//! naming a directory inside the user's NixOS repository. While it exists
//! (and the system is NixOS), vela keeps config.toml and its Hyprland
//! overrides there instead of ~/.config and ~/.local/state, so they are part
//! of the declarative configuration; Settings offers to commit and rebuild.

use crate::config::Terminal;
use crate::launch::{self, LaunchError, SpawnSpec};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

/// Settings made before NixOS mode was switched on: copied into the state
/// directory once, so they carry over instead of starting from defaults.
/// Files the state directory already has are never touched.
pub fn adopt_previous(mode: &NixosMode, old_config_dir: &Path) {
    for name in ["config.toml", "hyprland.toml"] {
        let (from, to) = (old_config_dir.join(name), mode.state_dir.join(name));
        if to.exists() || !from.is_file() {
            continue;
        }
        let copied = std::fs::create_dir_all(&mode.state_dir).and_then(|()| std::fs::copy(&from, &to));
        match copied {
            Ok(_) => log::info!("NixOS mode: took over {} as {}", from.display(), to.display()),
            Err(e) => log::warn!("cannot copy {} to {}: {e}", from.display(), to.display()),
        }
    }
}

/// Detected once per process.
pub fn status() -> &'static Status {
    static STATUS: OnceLock<Status> = OnceLock::new();
    STATUS.get_or_init(|| {
        let os = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let ptr = std::fs::read_to_string(pointer_file()).ok();
        let status = detect(&os, ptr.as_deref());
        if let Status::Active(mode) = &status {
            adopt_previous(mode, &crate::paths::config_dir());
        }
        status
    })
}

pub fn active() -> Option<&'static NixosMode> {
    match status() {
        Status::Active(m) => Some(m),
        _ => None,
    }
}

/// Runs the command given as arguments and keeps the terminal open so its
/// result (and a sudo prompt) can be read.
pub const WAIT_SCRIPT: &str = r#""$@"; s=$?; echo; if [ "$s" -eq 0 ]; then echo "vela: rebuild finished"; else echo "vela: rebuild failed ($s)"; fi; printf 'Press Enter to close '; read -r _"#;

pub fn porcelain_dirty(stdout: &str) -> bool {
    stdout.lines().any(|l| !l.trim().is_empty())
}

/// Whether the state directory has changes git hasn't committed yet.
/// None if that can't be told (no git, no repository, no directory).
pub fn dirty(mode: &NixosMode) -> Option<bool> {
    if !mode.state_dir.is_dir() {
        return None;
    }
    // No index refresh (and no index.lock): the rebuild may be committing.
    let out = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(&mode.state_dir)
        .args(["status", "--porcelain", "--untracked-files=all", "--", "."])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| porcelain_dirty(&String::from_utf8_lossy(&out.stdout)))
}

/// `terminal [exec args] sh -c WAIT_SCRIPT sh <rebuild words…>`: the
/// configured command as argv, never parsed by a shell.
pub fn rebuild_spec(mode: &NixosMode, term: &Terminal) -> Result<SpawnSpec, LaunchError> {
    let words = shlex::split(&mode.rebuild)
        .filter(|w| !w.is_empty())
        .ok_or_else(|| LaunchError::InvalidExec(format!("rebuild command “{}”", mode.rebuild)))?;
    let mut command = vec!["sh".to_owned(), "-c".to_owned(), WAIT_SCRIPT.to_owned(), "sh".to_owned()];
    command.extend(words);
    Ok(SpawnSpec {
        argv: launch::in_terminal(term, command)?,
        cwd: Some(mode.state_dir.clone()),
        env: Vec::new(),
        name: "vela-rebuild".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(dir: &std::path::Path, rebuild: &str) -> NixosMode {
        NixosMode {
            state_dir: dir.to_path_buf(),
            rebuild: rebuild.into(),
        }
    }

    fn term() -> Terminal {
        Terminal {
            executable: "/bin/sh".into(),
            exec_args: Some(vec!["-e".into()]),
        }
    }

    #[test]
    fn previous_settings_are_copied_once_into_an_empty_state_dir() {
        let old = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let state = repo.path().join("vela");
        std::fs::write(old.path().join("config.toml"), "old config").unwrap();
        std::fs::write(old.path().join("hyprland.toml"), "old hypr").unwrap();
        adopt_previous(&mode(&state, "rebuild"), old.path());
        assert_eq!(std::fs::read_to_string(state.join("config.toml")).unwrap(), "old config");
        assert_eq!(std::fs::read_to_string(state.join("hyprland.toml")).unwrap(), "old hypr");
        // Never over what the repository already has.
        std::fs::write(old.path().join("config.toml"), "newer elsewhere").unwrap();
        adopt_previous(&mode(&state, "rebuild"), old.path());
        assert_eq!(std::fs::read_to_string(state.join("config.toml")).unwrap(), "old config");
    }

    #[test]
    fn nothing_to_adopt_leaves_the_state_dir_alone() {
        let old = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let state = repo.path().join("vela");
        adopt_previous(&mode(&state, "rebuild"), old.path());
        assert!(!state.exists());
    }

    #[test]
    fn porcelain_output_means_dirty() {
        assert!(!porcelain_dirty(""));
        assert!(!porcelain_dirty("\n"));
        assert!(porcelain_dirty(" M config.toml\n"));
        assert!(porcelain_dirty("?? hyprland.lua\n"));
    }

    #[test]
    fn dirty_is_unknown_outside_a_repository_or_without_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(dirty(&mode(dir.path(), "rebuild")), None);
        assert_eq!(dirty(&mode(&dir.path().join("missing"), "rebuild")), None);
    }

    #[test]
    fn dirty_follows_git_status_of_the_state_dir() {
        let repo = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        let state = repo.path().join("vela");
        std::fs::create_dir(&state).unwrap();
        std::fs::write(repo.path().join("other.nix"), "{}").unwrap();
        // Changes outside the state directory don't count.
        assert_eq!(dirty(&mode(&state, "rebuild")), Some(false));
        std::fs::write(state.join("config.toml"), "x").unwrap();
        assert_eq!(dirty(&mode(&state, "rebuild")), Some(true));
        git(&["add", "-A"]);
        git(&["commit", "-qm", "c"]);
        assert_eq!(dirty(&mode(&state, "rebuild")), Some(false));
    }

    #[test]
    fn dirty_never_rewrites_the_index_a_rebuild_may_be_committing() {
        let repo = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        let state = repo.path().join("vela");
        std::fs::create_dir(&state).unwrap();
        std::fs::write(state.join("config.toml"), "x").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "c"]);
        // Same content, newer mtime: a plain `git status` refreshes the index.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(state.join("config.toml"), "x").unwrap();
        let index = repo.path().join(".git/index");
        let before = std::fs::metadata(&index).unwrap().modified().unwrap();
        assert_eq!(dirty(&mode(&state, "rebuild")), Some(false));
        assert_eq!(std::fs::metadata(&index).unwrap().modified().unwrap(), before);
    }

    #[test]
    fn rebuild_runs_split_words_in_the_terminal_and_waits() {
        let dir = tempfile::tempdir().unwrap();
        let spec = rebuild_spec(&mode(dir.path(), "rebuild 'vela: apply'"), &term()).unwrap();
        assert_eq!(spec.argv[..2], ["/bin/sh".to_owned(), "-e".to_owned()]);
        assert_eq!(spec.argv[2..5], ["sh".to_owned(), "-c".to_owned(), WAIT_SCRIPT.to_owned()]);
        assert_eq!(spec.argv[5..], ["sh".to_owned(), "rebuild".to_owned(), "vela: apply".to_owned()]);
        assert_eq!(spec.cwd.as_deref(), Some(dir.path()));
    }

    #[test]
    fn empty_or_unbalanced_rebuild_command_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(rebuild_spec(&mode(dir.path(), "  "), &term()), Err(LaunchError::InvalidExec(_))));
        assert!(matches!(
            rebuild_spec(&mode(dir.path(), "rebuild 'x"), &term()),
            Err(LaunchError::InvalidExec(_))
        ));
    }

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
