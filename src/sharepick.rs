//! Screen-share picker for xdg-desktop-portal-hyprland (`custom_picker_binary`).
//!
//! xdph runs the picker with the shareable windows in `XDPH_WINDOW_SHARING_LIST`
//! and reads `[SELECTION]<flags>/<what>` from its stdout. The picker itself is
//! Quickshell (`shell/share-picker.qml`, live previews); this module does the
//! protocol on both ends and the region selection (slurp) afterwards.

use crate::hyprland::Monitor;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareWindow {
    /// Low 32 bits of the foreign-toplevel handle; what xdph wants back.
    pub handle: u32,
    pub class: String,
    pub title: String,
    /// Hyprland window address (0 if xdph could not map it).
    pub address: u64,
}

/// Entries are `<handle>[HC>]<class>[HT>]<title>[HE>]<address>[HA>]`.
pub fn parse_window_list(list: &str) -> Vec<ShareWindow> {
    list.split("[HA>]")
        .filter_map(|entry| {
            let (handle, rest) = entry.split_once("[HC>]")?;
            let (class, rest) = rest.split_once("[HT>]")?;
            let (title, address) = rest.split_once("[HE>]")?;
            Some(ShareWindow {
                handle: handle.trim().parse().ok()?,
                class: class.to_owned(),
                title: title.to_owned(),
                address: address.trim().parse().unwrap_or(0),
            })
        })
        .collect()
}

/// For the QML side; addresses in Quickshell's form (hex without `0x`).
pub fn windows_json(windows: &[ShareWindow]) -> String {
    let list: Vec<_> = windows
        .iter()
        .map(|w| {
            serde_json::json!({
                "handle": w.handle,
                "class": w.class,
                "title": w.title,
                "address": if w.address == 0 { String::new() } else { format!("{:x}", w.address) },
            })
        })
        .collect();
    serde_json::Value::Array(list).to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Screen(String),
    Window(u32),
    /// Drawn with slurp after the picker has closed.
    Region,
}

#[derive(Deserialize)]
struct RawChoice {
    kind: String,
    #[serde(default)]
    output: String,
    #[serde(default)]
    handle: u32,
    #[serde(default)]
    token: bool,
}

/// The picker's answer: `{"kind":"screen|window|region", "output", "handle", "token"}`.
pub fn parse_choice(json: &str) -> Option<(Choice, bool)> {
    let raw: RawChoice = serde_json::from_str(json.trim()).ok()?;
    let choice = match raw.kind.as_str() {
        "screen" if !raw.output.is_empty() => Choice::Screen(raw.output),
        "window" => Choice::Window(raw.handle),
        "region" => Choice::Region,
        _ => return None,
    };
    Some((choice, raw.token))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub output: String,
    /// Logical, relative to the output.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// `slurp -f "%x %y %w %h"` (global logical coordinates) → the region on the
/// monitor holding its top-left corner, clipped to that monitor.
pub fn region_from_slurp(out: &str, monitors: &[Monitor]) -> Option<Region> {
    let n: Vec<i32> = out.split_whitespace().map(str::parse).collect::<Result<_, _>>().ok()?;
    let [x, y, w, h] = n[..] else { return None };
    if w <= 0 || h <= 0 {
        return None;
    }
    let m = monitors.iter().find(|m| x >= m.x && x < m.x + m.width && y >= m.y && y < m.y + m.height)?;
    let (rx, ry) = (x - m.x, y - m.y);
    Some(Region {
        output: m.name.clone(),
        x: rx,
        y: ry,
        w: w.min(m.width - rx),
        h: h.min(m.height - ry),
    })
}

pub enum Target {
    Screen(String),
    Window(u32),
    Region(Region),
}

/// What xdph parses; the newline matters (it drops the last character of
/// an output name).
pub fn selection_line(target: &Target, token: bool) -> String {
    let what = match target {
        Target::Screen(name) => format!("screen:{name}"),
        Target::Window(handle) => format!("window:{handle}"),
        Target::Region(r) => format!("region:{}@{},{},{},{}", r.output, r.x, r.y, r.w, r.h),
    };
    format!("[SELECTION]{}/{what}\n", if token { "r" } else { "" })
}

/// Shows the picker and returns the line for xdph; `None` = cancelled.
/// `vela_bin` is handed to the shell for `vela shell-config`.
pub fn run(vela_bin: &Path, allow_token: bool) -> Result<Option<String>, String> {
    let exe = std::env::current_exe().ok();
    let dir = crate::shell::find_shell_dir(
        std::env::var_os("VELA_SHELL_DIR").as_deref(),
        &crate::shell::shell_dir_candidates(exe.as_deref()),
    )
    .ok_or("control center files not found (set VELA_SHELL_DIR or reinstall vela)")?;
    let qs = crate::paths::find_executable("qs")
        .or_else(|| crate::paths::find_executable("quickshell"))
        .ok_or("Quickshell (qs) is not installed")?;
    let windows = parse_window_list(&std::env::var("XDPH_WINDOW_SHARING_LIST").unwrap_or_default());
    let out = answer_file();
    let _ = std::fs::remove_file(&out);
    // qs logs must not reach our stdout: xdph scans it for the selection.
    let status = std::process::Command::new(qs)
        .arg("-p")
        .arg(dir.join("share-picker.qml"))
        .env("VELA_BIN", vela_bin)
        .env("VELA_SHARE_WINDOWS", windows_json(&windows))
        .env("VELA_SHARE_TOKEN", if allow_token { "1" } else { "0" })
        .env("VELA_SHARE_OUT", &out)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map_err(|e| format!("cannot start Quickshell: {e}"))?;
    let answer = std::fs::read_to_string(&out).ok();
    let _ = std::fs::remove_file(&out);
    let Some((choice, token)) = answer.as_deref().and_then(parse_choice) else {
        if !status.success() {
            return Err(format!("the picker failed ({status})"));
        }
        return Ok(None);
    };
    let target = match choice {
        Choice::Screen(name) => Target::Screen(name),
        Choice::Window(handle) => Target::Window(handle),
        Choice::Region => match pick_region()? {
            Some(r) => Target::Region(r),
            None => return Ok(None),
        },
    };
    Ok(Some(selection_line(&target, token)))
}

fn answer_file() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    dir.join(format!("vela-share-{}.json", std::process::id()))
}

fn pick_region() -> Result<Option<Region>, String> {
    let slurp = crate::paths::find_executable("slurp").ok_or("slurp is not installed (needed for a region)")?;
    let out = std::process::Command::new(slurp)
        .args(["-f", "%x %y %w %h"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot start slurp: {e}"))?;
    // slurp exits with 1 when the selection is cancelled (Esc).
    if !out.status.success() {
        return Ok(None);
    }
    Ok(region_from_slurp(&String::from_utf8_lossy(&out.stdout), &crate::hyprland::monitors()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(name: &str, x: i32, w: i32, h: i32) -> Monitor {
        Monitor {
            name: name.into(),
            description: String::new(),
            x,
            y: 0,
            width: w,
            height: h,
        }
    }

    #[test]
    fn parses_xdph_window_list() {
        let list = "12[HC>]firefox[HT>]Mozilla Firefox[HE>]94763282598576[HA>]7[HC>]kitty[HT>][HE>]0[HA>]";
        assert_eq!(
            parse_window_list(list),
            vec![
                ShareWindow {
                    handle: 12,
                    class: "firefox".into(),
                    title: "Mozilla Firefox".into(),
                    address: 94763282598576
                },
                ShareWindow {
                    handle: 7,
                    class: "kitty".into(),
                    title: String::new(),
                    address: 0
                },
            ]
        );
        assert!(parse_window_list("").is_empty());
        assert_eq!(parse_window_list("x[HC>]a[HT>]b[HE>]1[HA>]garbage").len(), 0);
    }

    #[test]
    fn window_json_uses_quickshell_addresses() {
        let w = [ShareWindow {
            handle: 3,
            class: "c".into(),
            title: "t \"q\"".into(),
            address: 0x562a_d8ae_0ab0,
        }];
        let v: serde_json::Value = serde_json::from_str(&windows_json(&w)).unwrap();
        assert_eq!(v[0]["address"], "562ad8ae0ab0");
        assert_eq!(v[0]["handle"], 3);
        assert_eq!(v[0]["title"], "t \"q\"");
        let none = [ShareWindow { address: 0, ..w[0].clone() }];
        assert_eq!(serde_json::from_str::<serde_json::Value>(&windows_json(&none)).unwrap()[0]["address"], "");
    }

    #[test]
    fn parses_the_pickers_answer() {
        assert_eq!(
            parse_choice(r#"{"kind":"screen","output":"DP-6","token":true}"#),
            Some((Choice::Screen("DP-6".into()), true))
        );
        assert_eq!(parse_choice(r#"{"kind":"window","handle":42}"#), Some((Choice::Window(42), false)));
        assert_eq!(parse_choice("{\"kind\":\"region\",\"token\":false}\n"), Some((Choice::Region, false)));
        assert_eq!(parse_choice(r#"{"kind":"screen","output":""}"#), None);
        assert_eq!(parse_choice(r#"{"kind":"nope"}"#), None);
        assert_eq!(parse_choice(""), None);
    }

    #[test]
    fn selection_lines_match_xdph() {
        assert_eq!(selection_line(&Target::Screen("DP-6".into()), true), "[SELECTION]r/screen:DP-6\n");
        assert_eq!(selection_line(&Target::Window(42), false), "[SELECTION]/window:42\n");
        let r = Region {
            output: "eDP-1".into(),
            x: 10,
            y: 20,
            w: 300,
            h: 200,
        };
        assert_eq!(selection_line(&Target::Region(r), false), "[SELECTION]/region:eDP-1@10,20,300,200\n");
    }

    #[test]
    fn slurp_region_becomes_monitor_relative_and_clipped() {
        let mons = [mon("DP-6", 0, 1920, 1200), mon("DP-7", 1920, 1920, 1200), mon("eDP-1", 3840, 1600, 900)];
        assert_eq!(
            region_from_slurp("2000 100 400 300\n", &mons),
            Some(Region {
                output: "DP-7".into(),
                x: 80,
                y: 100,
                w: 400,
                h: 300
            })
        );
        // Reaching into the next monitor: cut at the edge.
        assert_eq!(
            region_from_slurp("1800 0 400 50", &mons),
            Some(Region {
                output: "DP-6".into(),
                x: 1800,
                y: 0,
                w: 120,
                h: 50
            })
        );
        assert_eq!(region_from_slurp("3900 850 10 100", &mons).map(|r| r.h), Some(50));
        assert_eq!(region_from_slurp("9000 0 10 10", &mons), None);
        assert_eq!(region_from_slurp("0 0 0 10", &mons), None);
        assert_eq!(region_from_slurp("", &mons), None);
    }
}
