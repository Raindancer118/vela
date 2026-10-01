//! vela updating itself (Settings → System, notification on a new release).
//!
//! Only installs made by install.sh are updated: the release (on NixOS the
//! source, which install.sh builds with nix) is downloaded and its install.sh
//! runs with `-y`, so it keeps the selection in components.toml, or with the
//! selection picked in Settings. Home Manager and package installs are
//! updated the way they were installed.

use crate::components::{Component, Installed};
use crate::update::Step;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const REPO: &str = "Raindancer118/vela";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum How {
    /// install.sh wrote components.toml; on NixOS it builds with nix.
    Script { nixos: bool },
    /// The Home Manager module (or another Nix setup) manages vela.
    Nix,
    /// A distribution package or a copy by hand.
    Package,
}

pub const SCRIPT_MARKER: &str = "Written by install.sh";

pub fn how(manifest: Option<&str>, nixos: bool) -> How {
    match manifest {
        Some(m) if m.contains(SCRIPT_MARKER) => How::Script { nixos },
        _ if nixos => How::Nix,
        _ => How::Package,
    }
}

/// How this installation was made.
pub fn detect() -> How {
    let manifest = crate::components::manifest_file().and_then(|p| std::fs::read_to_string(p).ok());
    how(manifest.as_deref(), crate::nixos::running_nixos())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    /// Prebuilt archive for this machine's architecture, if the release has one.
    pub archive: Option<String>,
    pub source: String,
}

pub fn archive_name(version: &str, arch: &str) -> String {
    format!("vela-{version}-{arch}-linux.tar.gz")
}

/// GitHub's `releases/latest` answer.
pub fn parse_latest(json: &str, arch: &str) -> Result<Release, String> {
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(Deserialize)]
    struct Latest {
        tag_name: String,
        #[serde(default)]
        assets: Vec<Asset>,
    }
    let latest: Latest = serde_json::from_str(json).map_err(|e| format!("unexpected answer from GitHub: {e}"))?;
    let version = latest.tag_name.trim_start_matches('v').to_owned();
    if parse_version(&version).is_none() {
        return Err(format!("unexpected release tag “{}”", latest.tag_name));
    }
    let wanted = archive_name(&version, arch);
    Ok(Release {
        archive: latest.assets.into_iter().find(|a| a.name == wanted).map(|a| a.browser_download_url),
        source: format!("https://github.com/{REPO}/archive/refs/tags/{}.tar.gz", latest.tag_name),
        version,
    })
}

/// The release of a known version (to change the selection without updating).
pub fn release_of(version: &str, arch: &str) -> Release {
    Release {
        version: version.to_owned(),
        archive: (arch == "x86_64").then(|| format!("https://github.com/{REPO}/releases/download/{version}/{}", archive_name(version, arch))),
        source: format!("https://github.com/{REPO}/archive/refs/tags/{version}.tar.gz"),
    }
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.split('.').map(|p| p.parse::<u64>().ok());
    let r = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(r)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// install.sh arguments: `-y` keeps the last selection, a target installs
/// exactly that one (install.sh names it after the profile if it matches).
pub fn install_args(target: Option<&Installed>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(t) = target {
        let profile = t.profile.as_deref().filter(|p| *p != "custom").unwrap_or("full");
        args.extend(["--profile".to_owned(), profile.to_owned()]);
        let ids = |on: bool| {
            Component::ALL
                .into_iter()
                .filter(|c| t.has(*c) == on)
                .map(Component::id)
                .collect::<Vec<_>>()
                .join(",")
        };
        let (with, without) = (ids(true), ids(false));
        if !with.is_empty() {
            args.extend(["--with".to_owned(), with]);
        }
        if !without.is_empty() {
            args.extend(["--without".to_owned(), without]);
        }
    }
    args.push("-y".to_owned());
    args
}

/// Download, unpack, run install.sh. `dir` is emptied by the caller.
pub fn plan(release: &Release, nixos: bool, dir: &Path, target: Option<&Installed>) -> Result<Vec<Step>, String> {
    // NixOS builds from source with nix; the prebuilt binaries can't run there.
    let url = if nixos {
        release.source.clone()
    } else {
        release
            .archive
            .clone()
            .ok_or_else(|| format!("vela {} has no prebuilt release for {}", release.version, std::env::consts::ARCH))?
    };
    let archive = dir.join("vela.tar.gz");
    let tree = dir.join("vela");
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let mut install = vec![path(&tree.join("install.sh"))];
    install.extend(install_args(target));
    Ok(vec![
        Step {
            title: format!("Downloading vela {}", release.version),
            argv: vec!["curl".into(), "-fSL".into(), "--retry".into(), "2".into(), "-o".into(), path(&archive), url],
        },
        Step {
            title: "Unpacking".into(),
            argv: vec![
                "sh".into(),
                "-c".into(),
                r#"mkdir -p "$2" && tar -xzf "$1" -C "$2" --strip-components=1"#.into(),
                "sh".into(),
                path(&archive),
                path(&tree),
            ],
        },
        Step {
            title: if nixos {
                "Building with nix and installing".into()
            } else {
                "Installing".into()
            },
            argv: install,
        },
    ])
}

/// Asks GitHub for the latest release (curl, a few seconds at most).
pub fn fetch_latest() -> Result<Release, String> {
    let out = Command::new("curl")
        .args(["-fsSL", "--max-time", "15", "-H", "Accept: application/vnd.github+json"])
        .arg(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("cannot reach GitHub: {}", err.trim().trim_start_matches("curl: ")));
    }
    parse_latest(&String::from_utf8_lossy(&out.stdout), std::env::consts::ARCH)
}

/// Kept across restarts: install.sh restarts the daemon that started it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Status {
    pub checked_at: i64,
    pub latest: Option<Release>,
    pub check_error: Option<String>,
    /// Version a desktop notification was shown for (once per version).
    pub notified: Option<String>,
    /// Set while install.sh runs: (version, log file).
    pub installing: Option<(String, PathBuf)>,
    /// Last run that failed: (summary, log file).
    pub failed: Option<(String, PathBuf)>,
}

pub fn status_file() -> PathBuf {
    crate::paths::state_dir().join("self-update.json")
}

pub fn work_dir() -> PathBuf {
    crate::paths::cache_dir().join("self-update")
}

/// What became of an install that was running when the daemon restarted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    None,
    Updated(String),
    /// Still the old version after install.sh: it failed (the log tells).
    Failed(String, PathBuf),
}

pub fn outcome(installing: Option<&(String, PathBuf)>, running: &str, finished: bool) -> Outcome {
    match installing {
        None => Outcome::None,
        Some((v, _)) if v == running => Outcome::Updated(v.clone()),
        Some((v, log)) if finished => Outcome::Failed(v.clone(), log.clone()),
        Some(_) => Outcome::None,
    }
}

/// NixOS with Home Manager: update the flake input in the repository that
/// holds the state directory, then rebuild. Run in a terminal.
pub const NIX_UPDATE_SCRIPT: &str = r#"cd "$(git rev-parse --show-toplevel)" && nix flake update vela && "$@"; s=$?; echo; if [ "$s" -eq 0 ]; then echo "vela: updated"; else echo "vela: update failed ($s)"; fi; printf 'Press Enter to close '; read -r _"#;

/// The line under "vela <version>" in Settings → System.
pub fn describe(status: &Status, checking: bool, step: Option<&(usize, usize, String)>, how: &How) -> String {
    if let Some((i, n, title)) = step {
        return format!("{title}… ({}/{n})", i + 1);
    }
    if let Some((summary, _)) = &status.failed {
        return summary.clone();
    }
    if checking {
        return "Looking for a new version…".into();
    }
    match status.latest.as_ref().filter(|r| is_newer(&r.version, VERSION)) {
        Some(r) => match how {
            How::Script { .. } => format!("vela {} is available", r.version),
            How::Nix => format!("vela {} is available: update the vela input of your flake and rebuild", r.version),
            How::Package => format!("vela {} is available: update it with your package manager", r.version),
        },
        None if status.checked_at == 0 => "Not checked yet".into(),
        None => match &status.check_error {
            Some(e) => format!("Check failed: {e}"),
            None => "Up to date".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_line_says_what_matters_most() {
        let script = How::Script { nixos: false };
        let mut s = Status::default();
        assert_eq!(describe(&s, false, None, &script), "Not checked yet");
        assert_eq!(describe(&s, true, None, &script), "Looking for a new version…");
        s.checked_at = 1;
        assert_eq!(describe(&s, false, None, &script), "Up to date");
        s.check_error = Some("cannot reach GitHub".into());
        assert_eq!(describe(&s, false, None, &script), "Check failed: cannot reach GitHub");
        s.latest = Some(release_of("999.0.0", "x86_64"));
        assert_eq!(describe(&s, false, None, &script), "vela 999.0.0 is available");
        assert!(describe(&s, false, None, &How::Nix).contains("flake"));
        assert!(describe(&s, false, None, &How::Package).contains("package manager"));
        s.latest = Some(release_of(VERSION, "x86_64"));
        assert_eq!(describe(&s, false, None, &script), "Check failed: cannot reach GitHub");
        s.failed = Some(("Installing failed: x".into(), "/l".into()));
        assert_eq!(describe(&s, false, None, &script), "Installing failed: x");
        assert_eq!(describe(&s, false, Some(&(1, 3, "Unpacking".into())), &script), "Unpacking… (2/3)");
    }

    const LATEST: &str = r#"{"tag_name":"0.36.0","assets":[
        {"name":"vela-0.36.0-x86_64-linux.tar.gz","browser_download_url":"https://x/vela-0.36.0-x86_64-linux.tar.gz"},
        {"name":"other.txt","browser_download_url":"https://x/other.txt"}]}"#;

    #[test]
    fn the_latest_release_has_the_archive_for_this_architecture() {
        let r = parse_latest(LATEST, "x86_64").unwrap();
        assert_eq!(r.version, "0.36.0");
        assert_eq!(r.archive.as_deref(), Some("https://x/vela-0.36.0-x86_64-linux.tar.gz"));
        assert_eq!(r.source, "https://github.com/Raindancer118/vela/archive/refs/tags/0.36.0.tar.gz");
        assert_eq!(parse_latest(LATEST, "aarch64").unwrap().archive, None);
    }

    #[test]
    fn odd_answers_are_errors() {
        assert!(parse_latest("{\"message\":\"Not Found\"}", "x86_64").is_err());
        assert!(parse_latest("{\"tag_name\":\"nightly\"}", "x86_64").is_err());
        assert!(parse_latest("<html>", "x86_64").is_err());
        assert_eq!(parse_latest("{\"tag_name\":\"v1.2.3\"}", "x86_64").unwrap().version, "1.2.3");
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.36.0", "0.35.0"));
        assert!(is_newer("0.35.10", "0.35.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.35.0", "0.35.0"));
        assert!(!is_newer("0.34.9", "0.35.0"));
        assert!(!is_newer("0.36", "0.35.0"));
        assert!(!is_newer("0.36.0-rc1", "0.35.0"));
    }

    #[test]
    fn only_install_sh_installs_update_themselves() {
        let script = "# Written by install.sh; run it again to change the selection.\nprofile = \"full\"\n";
        assert_eq!(how(Some(script), false), How::Script { nixos: false });
        assert_eq!(how(Some(script), true), How::Script { nixos: true });
        // The Home Manager module writes the file without the marker.
        assert_eq!(how(Some("profile = \"custom\"\n"), true), How::Nix);
        assert_eq!(how(None, true), How::Nix);
        assert_eq!(how(None, false), How::Package);
    }

    #[test]
    fn without_a_target_install_sh_keeps_the_last_selection() {
        assert_eq!(install_args(None), ["-y"]);
    }

    #[test]
    fn a_target_selection_is_installed_exactly() {
        let mut t = Installed::parse("profile = \"full\"\nshare_picker = false\n").unwrap();
        assert_eq!(
            install_args(Some(&t)),
            [
                "--profile",
                "full",
                "--with",
                "launcher,claude,hyprland,panel,idle,updates",
                "--without",
                "share-picker",
                "-y"
            ]
        );
        t = Installed::only(&[Component::Launcher]);
        assert_eq!(
            install_args(Some(&t)),
            [
                "--profile",
                "full",
                "--with",
                "launcher",
                "--without",
                "claude,hyprland,panel,idle,share-picker,updates",
                "-y"
            ]
        );
        t = Installed::parse("profile = \"full\"\n").unwrap();
        assert_eq!(
            install_args(Some(&t)),
            ["--profile", "full", "--with", "launcher,claude,hyprland,panel,idle,share-picker,updates", "-y"]
        );
    }

    #[test]
    fn the_plan_downloads_the_archive_and_runs_its_install_sh() {
        let r = parse_latest(LATEST, "x86_64").unwrap();
        let steps = plan(&r, false, Path::new("/c/su"), None).unwrap();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].argv.last().unwrap(), "https://x/vela-0.36.0-x86_64-linux.tar.gz");
        assert!(steps[0].argv.contains(&"/c/su/vela.tar.gz".to_owned()));
        assert_eq!(steps[1].argv[4..], ["/c/su/vela.tar.gz".to_owned(), "/c/su/vela".to_owned()]);
        assert_eq!(steps[2].argv, ["/c/su/vela/install.sh", "-y"]);
    }

    #[test]
    fn nixos_builds_the_source_and_other_architectures_need_an_archive() {
        let mut r = parse_latest(LATEST, "aarch64").unwrap();
        let steps = plan(&r, true, Path::new("/c/su"), None).unwrap();
        assert_eq!(steps[0].argv.last().unwrap(), &r.source);
        assert!(plan(&r, false, Path::new("/c/su"), None).is_err());
        r.archive = Some("a".into());
        assert_eq!(plan(&r, false, Path::new("/c/su"), None).unwrap()[0].argv.last().unwrap(), "a");
    }

    #[test]
    fn the_unpack_step_really_unpacks_into_the_tree() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("vela-0.36.0");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("install.sh"), "#!/bin/sh\n").unwrap();
        let archive = dir.path().join("vela.tar.gz");
        assert!(
            Command::new("tar")
                .arg("-czf")
                .arg(&archive)
                .arg("-C")
                .arg(dir.path())
                .arg("vela-0.36.0")
                .status()
                .unwrap()
                .success()
        );
        let r = release_of("0.36.0", "x86_64");
        let steps = plan(&r, false, dir.path(), None).unwrap();
        assert!(Command::new(&steps[1].argv[0]).args(&steps[1].argv[1..]).status().unwrap().success());
        assert!(dir.path().join("vela/install.sh").is_file());
    }

    /// The whole run against a local release: download (file://), unpack,
    /// install.sh with the selection.
    #[test]
    fn a_run_installs_the_downloaded_release_with_the_selection() {
        let dir = tempfile::tempdir().unwrap();
        let rel = dir.path().join("vela-0.99.0-x86_64-linux");
        std::fs::create_dir(&rel).unwrap();
        let args = dir.path().join("args");
        std::fs::write(rel.join("install.sh"), format!("#!/bin/sh\necho \"$@\" >{}\n", args.display())).unwrap();
        std::fs::set_permissions(rel.join("install.sh"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let archive = dir.path().join("release.tar.gz");
        assert!(
            Command::new("tar")
                .arg("-czf")
                .arg(&archive)
                .arg("-C")
                .arg(dir.path())
                .arg(rel.file_name().unwrap())
                .status()
                .unwrap()
                .success()
        );
        let release = Release {
            version: "0.99.0".into(),
            archive: Some(format!("file://{}", archive.display())),
            source: String::new(),
        };
        let work = dir.path().join("work");
        std::fs::create_dir(&work).unwrap();
        let target = Installed::profile("minimal", false);
        let steps = plan(&release, false, &work, Some(&target)).unwrap();
        crate::update::run("vela", &steps, &dir.path().join("log"), false, &mut |_| {}).unwrap();
        assert_eq!(
            std::fs::read_to_string(&args).unwrap().trim(),
            "--profile minimal --with launcher --without claude,hyprland,panel,idle,share-picker,updates -y"
        );
    }

    #[test]
    fn the_release_of_a_version_points_to_its_assets() {
        let r = release_of("0.35.0", "x86_64");
        assert_eq!(
            r.archive.as_deref(),
            Some("https://github.com/Raindancer118/vela/releases/download/0.35.0/vela-0.35.0-x86_64-linux.tar.gz")
        );
        assert_eq!(release_of("0.35.0", "aarch64").archive, None);
    }

    #[test]
    fn after_a_restart_the_running_version_tells_how_it_went() {
        let log = PathBuf::from("/l");
        assert_eq!(outcome(None, "0.35.0", true), Outcome::None);
        assert_eq!(
            outcome(Some(&("0.36.0".into(), log.clone())), "0.36.0", false),
            Outcome::Updated("0.36.0".into())
        );
        assert_eq!(
            outcome(Some(&("0.36.0".into(), log.clone())), "0.35.0", true),
            Outcome::Failed("0.36.0".into(), log.clone())
        );
        assert_eq!(outcome(Some(&("0.36.0".into(), log)), "0.35.0", false), Outcome::None);
    }
}
