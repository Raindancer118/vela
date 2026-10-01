//! vela's picker in `~/.config/hypr/xdph.conf`: the block install.sh writes,
//! kept by the daemon when Home Manager leaves the file to vela (NixOS mode,
//! where the selection lives in the state directory).

use std::path::{Path, PathBuf};

const MARK: &str = "# vela share picker";

#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Keep,
    /// New contents; empty means delete the file.
    Write(String),
    /// Another picker is configured; vela's would be this line.
    Foreign(String),
}

fn line(picker: &Path) -> String {
    format!("custom_picker_binary = {}", picker.display())
}

fn without_block(text: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for l in text.lines() {
        if l.trim_start().starts_with(MARK) {
            skipping = true;
        } else if skipping {
            skipping = !l.starts_with('}');
        } else {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

/// What to do with xdph.conf (`existing`, None if missing) so that it uses
/// `picker`, or no longer uses vela's picker if None.
pub fn plan(existing: Option<&str>, picker: Option<&Path>) -> Change {
    let text = existing.unwrap_or_default();
    match picker {
        Some(p) => {
            let want = line(p);
            if text.lines().any(|l| l.trim() == want) {
                Change::Keep
            } else if text.contains("custom_picker_binary") && !text.contains("vela-share-picker") {
                Change::Foreign(want)
            } else {
                let mut out = without_block(text);
                out.push_str(&format!("{MARK} (vela keeps this block)\nscreencopy {{\n    {want}\n}}\n"));
                Change::Write(out)
            }
        }
        None if text.contains(MARK) => {
            let out = without_block(text);
            Change::Write(if out.trim().is_empty() { String::new() } else { out })
        }
        None => Change::Keep,
    }
}

pub fn path() -> PathBuf {
    crate::paths::hypr_config_dir().join("xdph.conf")
}

/// Applies `plan` to `file`; true if it changed (xdph reads it only at start).
/// A file Home Manager links from the Nix store is left alone.
pub fn sync(file: &Path, picker: Option<&Path>) -> Result<bool, String> {
    if std::fs::read_link(file).is_ok_and(|t| t.starts_with("/nix/store")) {
        return Err(format!("{} comes from Home Manager; vela leaves it alone", file.display()));
    }
    let existing = std::fs::read_to_string(file).ok();
    match plan(existing.as_deref(), picker) {
        Change::Keep => Ok(false),
        Change::Foreign(want) => Err(format!("{} sets another share picker; vela's: {want}", file.display())),
        Change::Write(t) if t.is_empty() => std::fs::remove_file(file)
            .map(|()| true)
            .map_err(|e| format!("cannot remove {}: {e}", file.display())),
        Change::Write(t) => crate::config::write_atomic(file, &t)
            .map(|()| true)
            .map_err(|e| format!("cannot write {}: {e}", file.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PICKER: &str = "/nix/store/abc-vela/bin/vela-share-picker";

    fn written(c: Change) -> String {
        match c {
            Change::Write(t) => t,
            other => panic!("expected a write, got {other:?}"),
        }
    }

    #[test]
    fn the_picker_is_added_once_and_kept() {
        let p = Path::new(PICKER);
        let t = written(plan(None, Some(p)));
        assert!(t.contains(&format!("    custom_picker_binary = {PICKER}\n")));
        assert_eq!(plan(Some(&t), Some(p)), Change::Keep);
    }

    #[test]
    fn a_new_store_path_replaces_the_old_block_and_keeps_the_rest() {
        let old = format!(
            "general {{\n}}\n{}",
            written(plan(None, Some(Path::new("/nix/store/old-vela/bin/vela-share-picker"))))
        );
        let t = written(plan(Some(&old), Some(Path::new(PICKER))));
        assert!(t.starts_with("general {\n}\n"));
        assert!(!t.contains("old-vela") && t.contains(PICKER));
        assert_eq!(t.matches("screencopy").count(), 1);
    }

    #[test]
    fn install_sh_s_block_is_taken_over() {
        let old = "# vela share picker (removed by uninstall.sh)\nscreencopy {\n    custom_picker_binary = /home/u/.local/bin/vela-share-picker\n}\n";
        let t = written(plan(Some(old), Some(Path::new(PICKER))));
        assert!(!t.contains(".local/bin") && t.contains(PICKER));
    }

    #[test]
    fn another_picker_is_never_replaced() {
        let other = "screencopy {\n    custom_picker_binary = /usr/bin/hyprland-share-picker\n}\n";
        assert!(matches!(plan(Some(other), Some(Path::new(PICKER))), Change::Foreign(_)));
        assert_eq!(plan(Some(other), None), Change::Keep);
    }

    #[test]
    fn switching_off_removes_the_block_or_the_whole_file() {
        let ours = written(plan(None, Some(Path::new(PICKER))));
        assert_eq!(plan(Some(&ours), None), Change::Write(String::new()));
        let mixed = format!("general {{\n}}\n{ours}");
        assert_eq!(plan(Some(&mixed), None), Change::Write("general {\n}\n".into()));
        assert_eq!(plan(None, None), Change::Keep);
    }

    #[test]
    fn sync_writes_removes_and_leaves_store_links_alone() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hypr/xdph.conf");
        let p = Path::new(PICKER);
        assert_eq!(sync(&file, Some(p)), Ok(true));
        assert_eq!(sync(&file, Some(p)), Ok(false));
        assert_eq!(sync(&file, None), Ok(true));
        assert!(!file.exists());
        assert_eq!(sync(&file, None), Ok(false));
        std::os::unix::fs::symlink("/nix/store/x-hm/xdph.conf", &file).unwrap();
        assert!(sync(&file, Some(p)).is_err());
    }
}
