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
    request_timeout(cmd, 250)
}

fn request_timeout(cmd: &str, ms: u64) -> Option<String> {
    let mut stream = UnixStream::connect(socket_path()?).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(ms))).ok()?;
    stream.set_write_timeout(Some(Duration::from_millis(ms))).ok()?;
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
    /// Make, model and serial; stable across connectors, unlike `name`.
    pub description: String,
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
                description: m.get("description").and_then(|v| v.as_str()).unwrap_or_default().trim().to_owned(),
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

/// Connector of the monitor a config value names, if connected. The value
/// is `desc:<description>` (as in Hyprland monitor rules) or a connector.
pub fn resolve_monitor<'a>(spec: &str, monitors: &'a [Monitor]) -> Option<&'a str> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    let found = match spec.strip_prefix("desc:") {
        Some(desc) => monitors.iter().find(|m| !m.description.is_empty() && m.description == desc.trim()),
        None => monitors.iter().find(|m| m.name == spec),
    };
    found.map(|m| m.name.as_str())
}

/// Config value that keeps naming this monitor on another connector.
pub fn monitor_spec(m: &Monitor) -> String {
    if m.description.is_empty() {
        m.name.clone()
    } else {
        format!("desc:{}", m.description)
    }
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

/// Runs Lua code in Hyprland (`hyprctl eval`), one request per snippet
/// (batches split on `;`, which strings may contain). The error is
/// Hyprland's message for the first snippet that failed.
pub fn eval(snippets: &[String]) -> Result<(), String> {
    let mut first_err = None;
    for code in snippets {
        let out = request_timeout(&format!("eval {code}"), 1000).ok_or_else(|| "Hyprland is not reachable".to_owned())?;
        let out = out.trim();
        if out != "ok" && first_err.is_none() {
            first_err = Some(out.strip_prefix("error: ").unwrap_or(out).to_owned());
        }
    }
    first_err.map_or(Ok(()), Err)
}

/// Re-reads hyprland.lua (and with it vela's overrides), monitors untouched.
pub fn reload_config() -> bool {
    request_timeout("reload config-only", 3000).is_some_and(|r| r.trim() == "ok")
}

/// Every option Hyprland describes, with its current value.
pub fn options() -> Option<(Vec<crate::hyprconf::OptionInfo>, std::collections::BTreeMap<String, crate::hyprconf::Value>)> {
    let descriptions = request_timeout("j/descriptions", 2000)?;
    let names: Vec<String> = serde_json::from_str::<Vec<serde_json::Value>>(&descriptions)
        .ok()?
        .iter()
        .map(|d| d.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_owned())
        .collect();
    let batch = format!("[[BATCH]]{}", names.iter().map(|n| format!("j/getoption {n}")).collect::<Vec<_>>().join(";"));
    let got = request_timeout(&batch, 2000).unwrap_or_default();
    Some(crate::hyprconf::catalogue(&descriptions, &crate::hyprconf::split_batch(&got, names.len())))
}

/// Current value of one option.
pub fn option_value(info: &crate::hyprconf::OptionInfo) -> Option<crate::hyprconf::Value> {
    let out = request(&format!("j/getoption {}", info.name))?;
    let j: serde_json::Value = serde_json::from_str(&out).ok()?;
    let (_, v) = j.as_object()?.iter().find(|(k, _)| *k != "option" && *k != "set")?;
    crate::hyprconf::value_from_json(info.kind, v)
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
        {"name":"DP-6","description":"HP Inc. HP E243i 6CM8430N46","x":0,"y":0,"width":1920,"height":1200,"scale":1,"transform":0,"mirrorOf":"none","focused":false},
        {"name":"DP-7","description":"HP Inc. HP E243i 6CM8191WP7","x":1920,"y":0,"width":1920,"height":1200,"scale":1,"transform":0,"mirrorOf":"none","focused":true},
        {"name":"eDP-1","description":"Chimei Innolux Corporation 0x150C","x":3840,"y":0,"width":1920,"height":1080,"scale":1.5,"transform":0,"mirrorOf":"none","focused":false}
    ]"#;

    #[test]
    fn parses_monitor_layout_in_logical_pixels() {
        let m = parse_monitors(DOCK);
        assert_eq!(m.len(), 3);
        assert_eq!(
            m[2],
            Monitor {
                name: "eDP-1".into(),
                description: "Chimei Innolux Corporation 0x150C".into(),
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
    fn main_monitor_is_found_by_description_or_connector() {
        let m = parse_monitors(DOCK);
        assert_eq!(resolve_monitor("desc:HP Inc. HP E243i 6CM8191WP7", &m), Some("DP-7"));
        // The dock may hand the same screen another connector next time.
        let swapped = DOCK.replace("\"DP-7\"", "\"DP-9\"");
        assert_eq!(resolve_monitor("desc:HP Inc. HP E243i 6CM8191WP7", &parse_monitors(&swapped)), Some("DP-9"));
        assert_eq!(resolve_monitor("eDP-1", &m), Some("eDP-1"));
        assert_eq!(resolve_monitor("desc:Some Other Screen", &m), None, "not connected");
        assert_eq!(resolve_monitor("", &m), None, "unset = follow focus");
        assert_eq!(monitor_spec(&m[1]), "desc:HP Inc. HP E243i 6CM8191WP7");
        let nameless = Monitor {
            description: String::new(),
            ..m[0].clone()
        };
        assert_eq!(monitor_spec(&nameless), "DP-6");
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

/// Needs a running Hyprland: `cargo test -- --ignored live_`.
#[cfg(test)]
mod live {
    #[test]
    #[ignore]
    fn live_every_current_value_roundtrips_through_generated_lua() {
        let (infos, current) = super::options().expect("Hyprland reachable");
        assert!(infos.len() > 300, "only {} options", infos.len());
        let mut failed = Vec::new();
        for info in infos.iter().filter(|i| !i.name.starts_with("debug:")) {
            let Some(v) = current.get(&info.name) else { continue };
            if let Err(e) = super::eval(&[crate::hyprconf::eval_code(&info.name, v)]) {
                failed.push(format!("{} = {}: {e}", info.name, v.to_lua()));
            }
        }
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }
}
