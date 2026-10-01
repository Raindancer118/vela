//! Tiny client for Hyprland's request socket, used to put the launcher on
//! the focused monitor. Everything degrades to `None` outside Hyprland.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

fn socket_path() -> Option<PathBuf> {
    let runtime = PathBuf::from(std::env::var("XDG_RUNTIME_DIR").ok().filter(|s| !s.is_empty())?).join("hypr");
    if let Some(sig) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().filter(|s| !s.is_empty()) {
        return Some(runtime.join(sig).join(".socket.sock"));
    }
    // Started outside the session (e.g. an MCP server): the newest instance.
    std::fs::read_dir(&runtime)
        .ok()?
        .flatten()
        .map(|e| e.path().join(".socket.sock"))
        .filter(|p| p.exists())
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
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

/// Animations and curve names (`j/animations`).
pub fn animations() -> Option<(std::collections::BTreeMap<String, crate::hypranim::AnimState>, Vec<String>)> {
    let json = request_timeout("j/animations", 1000)?;
    Some(crate::hypranim::parse(&json))
}

/// Every monitor, disabled ones included.
pub fn monitors_all() -> Option<Vec<crate::hyprmon::MonitorInfo>> {
    Some(crate::hyprmon::parse_monitors(&request_timeout("j/monitors all", 1000)?))
}

/// Full reload: monitor rules only take effect again with this.
pub fn reload_all() -> bool {
    request_timeout("reload", 5000).is_some_and(|r| r.trim() == "ok")
}

/// A shortcut Hyprland knows (from hyprland.lua or vela).
#[derive(Debug, Clone, PartialEq)]
pub struct Bind {
    pub combo: String,
    pub description: String,
    pub mouse: bool,
    pub submap: String,
}

pub fn parse_binds(json: &str) -> Vec<Bind> {
    let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|b| {
            let key = b.get("key")?.as_str()?;
            let mask = b.get("modmask").and_then(|m| m.as_u64()).unwrap_or(0) as u32;
            Some(Bind {
                combo: crate::hyprextra::combo_from_mask(mask, key),
                description: b.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_owned(),
                mouse: b.get("mouse").and_then(|m| m.as_bool()).unwrap_or(false),
                submap: b.get("submap").and_then(|d| d.as_str()).unwrap_or_default().to_owned(),
            })
        })
        .filter(|b| !b.combo.is_empty())
        .collect()
}

pub fn binds() -> Option<Vec<Bind>> {
    Some(parse_binds(&request_timeout("j/binds", 1000)?))
}

/// `j/clients` as Hyprland sends it.
pub fn clients_json() -> Option<String> {
    request_timeout("j/clients", 1000)
}

/// Window classes and titles that are open now (to build a rule from).
pub fn open_windows() -> Vec<(String, String)> {
    let Some(json) = request_timeout("j/clients", 1000) else { return Vec::new() };
    let list: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap_or_default();
    let mut out: Vec<(String, String)> = list
        .iter()
        .filter_map(|c| Some((c.get("class")?.as_str()?.to_owned(), c.get("title")?.as_str()?.to_owned())))
        .filter(|(c, _)| !c.is_empty())
        .collect();
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    out
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
    fn binds_read_as_combos() {
        let json = r#"[{"modmask":65,"key":"Q","mouse":false,"submap":"","description":""},
                       {"modmask":64,"key":"mouse:272","mouse":true,"submap":"","description":"drag"}]"#;
        let b = parse_binds(json);
        assert_eq!(b[0].combo, "SUPER + SHIFT + Q");
        assert!(b[1].mouse);
        assert_eq!(b[1].combo, "SUPER + mouse:272");
        assert!(parse_binds("x").is_empty());
    }

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

    #[test]
    #[ignore]
    fn live_vela_curves_and_every_animation_apply() {
        super::eval(&crate::hypranim::curves_lua()).expect("vela curves");
        let (anims, curves) = super::animations().expect("Hyprland reachable");
        assert!(curves.iter().any(|c| c == "velaSmooth"), "{curves:?}");
        let mut failed = Vec::new();
        for (leaf, state) in anims.iter().filter(|(l, s)| s.overridden && !l.starts_with("__")) {
            let code = state.anim.eval_code(leaf);
            if let Err(e) = super::eval(std::slice::from_ref(&code)) {
                failed.push(format!("{code}: {e}"));
            }
        }
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }

    #[test]
    #[ignore]
    fn live_monitor_rules_are_accepted() {
        let monitors = super::monitors_all().expect("Hyprland reachable");
        assert!(!monitors.is_empty());
        for m in monitors {
            // A made-up output: checks the rule without touching a screen.
            let mut rule = m.current_rule();
            rule.output = format!("desc:vela live test {}", m.name);
            rule.vrr = Some(0);
            super::eval(&[rule.eval_code()]).unwrap_or_else(|e| panic!("{}: {e}", rule.eval_code()));
        }
    }

    #[test]
    #[ignore]
    fn live_every_shortcut_action_and_rule_effect_is_accepted() {
        use crate::hyprextra::{self, Arg, EffectKind, Shortcut, WindowRule};
        const KEYS: &str = "SUPER + CTRL + ALT + SHIFT + F24";
        for a in hyprextra::ACTIONS {
            let arg = match a.arg {
                Arg::None => "",
                Arg::Direction => "left",
                Arg::Monitor => "+1",
                Arg::Text(_) if a.id == "workspace" || a.id == "movetoworkspace" => "3",
                Arg::Text(_) if a.id == "lua" => "hl.dsp.window.center()",
                Arg::Text(_) => "true",
            };
            let s = Shortcut {
                keys: KEYS.into(),
                action: a.id.into(),
                arg: arg.into(),
                repeat: true,
                locked: true,
                description: String::new(),
                enabled: true,
            };
            let bind = format!("hl.bind({})", s.bind_lua().expect(a.id));
            super::eval(std::slice::from_ref(&bind)).unwrap_or_else(|e| panic!("{bind}: {e}"));
            let bound = super::binds().unwrap().iter().any(|b| b.combo == KEYS && b.description.starts_with("vela: "));
            super::eval(&[format!("hl.unbind(\"{KEYS}\")")]).unwrap();
            assert!(bound, "{} was not bound", a.id);
        }
        for (key, _, kind) in hyprextra::EFFECTS {
            let value: toml::Value = match kind {
                EffectKind::Bool => true.into(),
                EffectKind::Int => 0.into(),
                EffectKind::Float => 0.9.into(),
                EffectKind::Text if *key == "idle_inhibit" => "fullscreen".into(),
                EffectKind::Text if *key == "workspace" => "3 silent".into(),
                EffectKind::Text if *key == "monitor" => "0".into(),
                EffectKind::Text => "800 600".into(),
            };
            let r = WindowRule {
                name: "live-test".into(),
                enabled: true,
                class: "^vela-live-test-nonexistent$".into(),
                title: String::new(),
                effects: vec![((*key).into(), value)],
            };
            let code = format!("local r = hl.window_rule({}) r:set_enabled(false)", r.lua().unwrap());
            super::eval(std::slice::from_ref(&code)).unwrap_or_else(|e| panic!("{code}: {e}"));
        }
    }

    #[test]
    #[ignore]
    fn live_recording_submap_enters_and_always_leaves() {
        super::eval(&crate::hyprextra::record_start_lua()).expect("enter");
        let out = std::env::temp_dir().join(format!("vela-submap-{}", std::process::id()));
        let probe = |p: &std::path::Path| {
            super::eval(&[format!("hl.exec_cmd(\"echo \" .. tostring(hl.get_current_submap()) .. \" > {}\")", p.display())]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(400));
            std::fs::read_to_string(p).unwrap_or_default().trim().to_owned()
        };
        let inside = probe(&out);
        super::eval(&[crate::hyprextra::RECORD_STOP_LUA.to_owned()]).expect("leave");
        let after = probe(&out);
        let _ = std::fs::remove_file(&out);
        assert_eq!(inside, crate::hyprextra::RECORD_SUBMAP);
        assert_eq!(after, "");
    }
}
