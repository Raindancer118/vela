//! Tiny client for Hyprland's request socket, used to put the launcher on
//! the focused monitor. Everything degrades to `None` outside Hyprland.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

fn socket_path() -> Option<PathBuf> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().filter(|s| !s.is_empty())?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok().filter(|s| !s.is_empty())?;
    Some(PathBuf::from(runtime).join("hypr").join(sig).join(".socket.sock"))
}

fn request(cmd: &str) -> Option<String> {
    let mut stream = UnixStream::connect(socket_path()?).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(250))).ok()?;
    stream.set_write_timeout(Some(Duration::from_millis(250))).ok()?;
    stream.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    stream.read_to_string(&mut out).ok()?;
    Some(out)
}

/// Name of the focused monitor from `j/monitors` output.
pub fn parse_focused_monitor(json: &str) -> Option<String> {
    let monitors: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    monitors
        .iter()
        .find(|m| m.get("focused").and_then(|f| f.as_bool()) == Some(true))?
        .get("name")?
        .as_str()
        .map(str::to_owned)
}

pub fn focused_monitor() -> Option<String> {
    parse_focused_monitor(&request("j/monitors")?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub name: String,
    pub x: i32,
    pub y: i32,
    /// Logical size (scale and rotation applied), matching `x`/`y`.
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Monitors from `j/monitors` output; mirrors are left out.
pub fn parse_monitors(json: &str) -> Vec<Monitor> {
    let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
        return Vec::new();
    };
    list.iter()
        .filter(|m| m.get("mirrorOf").and_then(|v| v.as_str()).is_none_or(|v| v == "none"))
        .filter(|m| m.get("disabled").and_then(|v| v.as_bool()) != Some(true))
        .filter_map(|m| {
            let int = |k: &str| m.get(k).and_then(|v| v.as_i64()).map(|v| v as i32);
            let scale = m.get("scale").and_then(|v| v.as_f64()).filter(|s| *s > 0.0).unwrap_or(1.0);
            let (w, h) = (f64::from(int("width")?) / scale, f64::from(int("height")?) / scale);
            let (w, h) = if int("transform").unwrap_or(0) % 2 == 1 { (h, w) } else { (w, h) };
            Some(Monitor {
                name: m.get("name")?.as_str()?.to_owned(),
                x: int("x")?,
                y: int("y")?,
                width: w.round() as i32,
                height: h.round() as i32,
            })
        })
        .collect()
}

/// The monitor directly to one side of `from`: among those whose centre lies
/// on that side, one sharing a horizontal band wins, then the nearest.
pub fn neighbor<'a>(monitors: &'a [Monitor], from: &str, side: Side) -> Option<&'a str> {
    let cur = monitors.iter().find(|m| m.name == from)?;
    let cx = |m: &Monitor| 2 * m.x + m.width;
    monitors
        .iter()
        .filter(|m| m.name != cur.name)
        .filter(|m| match side {
            Side::Left => cx(m) < cx(cur),
            Side::Right => cx(m) > cx(cur),
        })
        .min_by_key(|m| {
            let overlaps = m.y < cur.y + cur.height && cur.y < m.y + m.height;
            (!overlaps, (cx(m) - cx(cur)).abs())
        })
        .map(|m| m.name.as_str())
}

pub fn monitors() -> Vec<Monitor> {
    request("j/monitors").map(|j| parse_monitors(&j)).unwrap_or_default()
}

/// Focuses a monitor, so windows mapped next open there. Hyprland also warps
/// the pointer onto it (unless `cursor:no_warps`).
pub fn focus_monitor(name: &str) -> bool {
    let name = name.replace(['\\', '"'], "");
    request(&format!("dispatch hl.dsp.focus({{ monitor = \"{name}\" }})")).is_some_and(|r| r.trim() == "ok")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_focused_monitor() {
        let json = r#"[{"id":0,"name":"eDP-1","focused":false},{"id":1,"name":"DP-6","focused":true}]"#;
        assert_eq!(parse_focused_monitor(json).as_deref(), Some("DP-6"));
        assert_eq!(parse_focused_monitor("[]"), None);
        assert_eq!(parse_focused_monitor("not json"), None);
    }

    // Tom's dock: two 1920x1200 screens left of the laptop panel (1920x1080 @ 1.5).
    const DOCK: &str = r#"[
        {"name":"DP-6","x":0,"y":0,"width":1920,"height":1200,"scale":1,"transform":0,"mirrorOf":"none","focused":false},
        {"name":"DP-7","x":1920,"y":0,"width":1920,"height":1200,"scale":1,"transform":0,"mirrorOf":"none","focused":true},
        {"name":"eDP-1","x":3840,"y":0,"width":1920,"height":1080,"scale":1.5,"transform":0,"mirrorOf":"none","focused":false}
    ]"#;

    #[test]
    fn parses_monitor_layout_in_logical_pixels() {
        let m = parse_monitors(DOCK);
        assert_eq!(m.len(), 3);
        assert_eq!(
            m[2],
            Monitor {
                name: "eDP-1".into(),
                x: 3840,
                y: 0,
                width: 1280,
                height: 720
            }
        );
        let rotated = r#"[{"name":"HDMI-A-1","x":0,"y":0,"width":1920,"height":1080,"scale":1,"transform":1,"mirrorOf":"none"}]"#;
        assert_eq!((parse_monitors(rotated)[0].width, parse_monitors(rotated)[0].height), (1080, 1920));
        let mirrored = r#"[{"name":"A","x":0,"y":0,"width":10,"height":10,"scale":1,"transform":0,"mirrorOf":"none"},
                           {"name":"B","x":0,"y":0,"width":10,"height":10,"scale":1,"transform":0,"mirrorOf":"A"}]"#;
        assert_eq!(parse_monitors(mirrored).len(), 1, "mirrors are not a place to open windows");
        assert!(parse_monitors("garbage").is_empty());
    }

    #[test]
    fn finds_the_monitor_next_to_the_current_one() {
        let m = parse_monitors(DOCK);
        assert_eq!(neighbor(&m, "DP-7", Side::Left), Some("DP-6"));
        assert_eq!(neighbor(&m, "DP-7", Side::Right), Some("eDP-1"));
        assert_eq!(neighbor(&m, "DP-6", Side::Left), None);
        assert_eq!(neighbor(&m, "eDP-1", Side::Right), None);
        assert_eq!(neighbor(&m, "eDP-1", Side::Left), Some("DP-7"), "the adjacent one, not the farthest");
        assert_eq!(neighbor(&m, "HDMI-A-9", Side::Left), None);
    }

    #[test]
    fn prefers_a_monitor_at_the_same_height_over_one_above() {
        let json = r#"[
            {"name":"main","x":1920,"y":1080,"width":1920,"height":1080,"scale":1,"transform":0,"mirrorOf":"none"},
            {"name":"above-left","x":1200,"y":0,"width":1920,"height":1080,"scale":1,"transform":0,"mirrorOf":"none"},
            {"name":"left","x":0,"y":1080,"width":1920,"height":1080,"scale":1,"transform":0,"mirrorOf":"none"}
        ]"#;
        assert_eq!(neighbor(&parse_monitors(json), "main", Side::Left), Some("left"));
    }
}
