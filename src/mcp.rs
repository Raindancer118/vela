//! `vela mcp`: an MCP server (stdio, JSON-RPC) so Claude can change Hyprland
//! and vela settings. Changes take the same path as the settings window:
//! applied live, kept in hyprland.toml (+ generated Lua) or config.toml; a
//! running vela daemon picks the files up.

use crate::hypranim::{self, Anim};
use crate::hyprconf::{self, Overrides};
use crate::hyprmon::{self, MonitorRule};
use crate::{config, hyprland, ipc, paths};
use serde_json::{Value as J, json};
use std::io::{BufRead, Write};

const PROTOCOL: &str = "2025-06-18";

pub fn run() -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<J>(&line) {
            Ok(req) => handle(&req),
            Err(e) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } })),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// Answer to one JSON-RPC message; None for notifications.
pub fn handle(req: &J) -> Option<J> {
    let id = req.get("id").cloned()?;
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or_default();
    let params = req.get("params").cloned().unwrap_or(J::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "vela", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or_default();
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            Ok(match call(name, &args) {
                Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            })
        }
        _ => Err(json!({ "code": -32601, "message": format!("unknown method {method}") })),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
    })
}

const INSTRUCTIONS: &str = "Changes Hyprland (the Wayland compositor) and the vela launcher/control center. \
Hyprland changes apply instantly and persist; they override the user's hyprland.lua only for the options touched \
(reset_hyprland_options undoes them). Find option names with search_hyprland_options before setting. \
Gaps between windows: general:gaps_in; gaps to the screen edges: general:gaps_out; blur: decoration:blur:*.";

fn tool(name: &str, description: &str, properties: J, required: &[&str]) -> J {
    json!({
        "name": name,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
    })
}

fn tools() -> J {
    json!([
        tool(
            "search_hyprland_options",
            "Search Hyprland's config options by words in their name or description. Returns name, description, type, current value, default, range and named choices. Empty query with only_changed lists what vela overrides.",
            json!({
                "query": { "type": "string", "description": "e.g. \"gaps\", \"blur size\", \"touchpad scroll\"" },
                "only_changed": { "type": "boolean", "description": "Only options vela currently overrides" },
                "limit": { "type": "integer", "description": "Max results (default 25)" }
            }),
            &[],
        ),
        tool(
            "set_hyprland_options",
            "Set Hyprland options live and keep them. Values: numbers, true/false, strings, colours as \"#rrggbb[aa]\", gaps as a number or {top,right,bottom,left}, gradients as {colors:[…],angle}, choices by number or label.",
            json!({ "changes": { "type": "object", "description": "Option name → value, e.g. {\"general:gaps_in\": 2}", "additionalProperties": true } }),
            &["changes"],
        ),
        tool(
            "reset_hyprland_options",
            "Drop vela's overrides so the values from hyprland.lua apply again. Names may be options, \"anim:<leaf>\" or \"monitor:<output>\".",
            json!({
                "names": { "type": "array", "items": { "type": "string" } },
                "all": { "type": "boolean", "description": "Reset everything vela changed" }
            }),
            &[],
        ),
        tool(
            "list_hyprland_animations",
            "Animations (windows, windowsIn, workspaces, fade, layers, border, …) with what they do now and the curves that can be used.",
            json!({}),
            &[],
        ),
        tool(
            "set_hyprland_animation",
            "Change one animation. Leaves without own settings inherit from their parent (windowsIn ← windows ← global).",
            json!({
                "leaf": { "type": "string" },
                "enabled": { "type": "boolean" },
                "duration_ms": { "type": "number", "description": "Duration in milliseconds" },
                "curve": { "type": "string", "description": "Bezier name or spring:<name>, see list_hyprland_animations" },
                "style": { "type": "string", "description": "e.g. slide, popin 80%, fade, slidefade 20%" }
            }),
            &["leaf"],
        ),
        tool(
            "list_monitors",
            "Connected monitors with modes, scale, valid scales, position and rotation.",
            json!({}),
            &[]
        ),
        tool(
            "set_monitor",
            "Change a monitor. A wrong mode or position can leave a screen dark; ask the user before turning monitors off or changing modes.",
            json!({
                "monitor": { "type": "string", "description": "Connector name like DP-6 or eDP-1" },
                "mode": { "type": "string", "description": "WIDTHxHEIGHT@HZ from list_monitors, or preferred" },
                "scale": { "type": "number" },
                "position": { "type": "array", "items": { "type": "integer" }, "description": "[x, y] in layout pixels" },
                "rotation": { "type": "integer", "description": "0 normal, 1 90°, 2 180°, 3 270°, 4–7 flipped" },
                "vrr": { "type": "integer", "description": "0 off, 1 on, 2 fullscreen, 3 fullscreen games; omit for the global setting" },
                "enabled": { "type": "boolean" }
            }),
            &["monitor"],
        ),
        tool(
            "get_vela_settings",
            "vela's own settings (launcher, control center, appearance) as TOML.",
            json!({}),
            &[]
        ),
        tool(
            "set_vela_setting",
            "Set one vela setting by its TOML path, e.g. appearance.theme or panel.width (see get_vela_settings).",
            json!({ "path": { "type": "string" }, "value": { "type": "string", "description": "The value as you would write it in TOML" } }),
            &["path", "value"],
        ),
        tool(
            "open_vela_settings",
            "Show the vela settings window to the user, optionally on a page.",
            json!({ "page": { "type": "string", "enum": ipc::SETTINGS_PAGES } }),
            &[],
        ),
    ])
}

fn load() -> Result<Overrides, String> {
    let path = paths::hypr_overrides_file();
    match std::fs::read_to_string(&path) {
        Ok(t) => Overrides::from_toml(&t).map_err(|e| format!("{} has an error: {e:#}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Overrides::default()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Lua first, like the daemon: a reload caused by the toml must see it.
fn save(o: &Overrides) -> Result<(), String> {
    config::write_atomic(&paths::hypr_lua_file(), &o.to_lua())
        .and_then(|()| config::write_atomic(&paths::hypr_overrides_file(), &o.to_toml()))
        .map_err(|e| format!("saving failed: {e:#}"))
}

fn pretty(v: &J) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

pub fn call(name: &str, args: &J) -> Result<String, String> {
    let s = |k: &str| args.get(k).and_then(|v| v.as_str());
    match name {
        "search_hyprland_options" => {
            let (infos, current) = hyprland::options().ok_or("Hyprland is not reachable")?;
            let overrides = load()?;
            let words: Vec<String> = s("query").unwrap_or_default().to_lowercase().split_whitespace().map(str::to_owned).collect();
            let only = args.get("only_changed").and_then(|v| v.as_bool()).unwrap_or(false);
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(25) as usize;
            let hits: Vec<J> = infos
                .iter()
                .filter(|i| {
                    let hay = format!("{} {}", i.name.replace([':', '_', '.'], " "), i.description).to_lowercase();
                    words.iter().all(|w| hay.contains(w.as_str()) || i.name.contains(w.as_str()))
                })
                .filter(|i| !only || overrides.options.contains_key(&i.name))
                .take(limit)
                .map(|i| {
                    json!({
                        "name": i.name,
                        "description": i.description,
                        "expects": hyprconf::kind_hint(i),
                        "value": current.get(&i.name).map(hyprconf::value_to_json),
                        "default": i.default.as_ref().map(hyprconf::value_to_json),
                        "min": i.min,
                        "max": i.max,
                        "changed_by_vela": overrides.options.contains_key(&i.name),
                    })
                })
                .collect();
            if hits.is_empty() {
                return Ok("No option matches. Try fewer or other words.".into());
            }
            Ok(pretty(&J::Array(hits)))
        }
        "set_hyprland_options" => {
            let changes = args.get("changes").and_then(|c| c.as_object()).ok_or("changes must be an object")?;
            let (infos, _) = hyprland::options().ok_or("Hyprland is not reachable")?;
            let mut overrides = load()?;
            let mut done = serde_json::Map::new();
            let mut errors = Vec::new();
            for (name, raw) in changes {
                let Some(info) = infos.iter().find(|i| &i.name == name) else {
                    errors.push(format!("{name}: no such option (search_hyprland_options finds names)"));
                    continue;
                };
                match hyprconf::value_from_input(info, raw) {
                    Ok(v) => match hyprland::eval(&[hyprconf::eval_code(name, &v)]) {
                        Ok(()) => {
                            done.insert(name.clone(), hyprconf::value_to_json(&v));
                            overrides.options.insert(name.clone(), v);
                        }
                        Err(e) => errors.push(format!("{name}: Hyprland refused it: {e}")),
                    },
                    Err(e) => errors.push(e),
                }
            }
            if !done.is_empty() {
                save(&overrides)?;
            }
            let mut out = json!({ "applied": done });
            if !errors.is_empty() {
                out["errors"] = json!(errors);
            }
            if done.is_empty() { Err(pretty(&out)) } else { Ok(pretty(&out)) }
        }
        "reset_hyprland_options" => {
            let mut overrides = load()?;
            let names: Vec<String> = if args.get("all").and_then(|v| v.as_bool()) == Some(true) {
                overrides
                    .options
                    .keys()
                    .cloned()
                    .chain(overrides.animations.keys().map(|l| format!("anim:{l}")))
                    .chain(overrides.monitors.keys().map(|m| format!("monitor:{m}")))
                    .collect()
            } else {
                args.get("names")
                    .and_then(|n| n.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
                    .unwrap_or_default()
            };
            let mut removed = Vec::new();
            for n in &names {
                let hit = if let Some(leaf) = n.strip_prefix("anim:") {
                    overrides.animations.remove(leaf).is_some()
                } else if let Some(out) = n.strip_prefix("monitor:") {
                    overrides.monitors.remove(out).is_some()
                } else {
                    overrides.options.remove(n).is_some()
                };
                if hit {
                    removed.push(n.clone());
                }
            }
            if removed.is_empty() {
                return Ok("Nothing to reset: vela doesn't override these.".into());
            }
            save(&overrides)?;
            let monitors = removed.iter().any(|n| n.starts_with("monitor:"));
            let ok = if monitors { hyprland::reload_all() } else { hyprland::reload_config() };
            Ok(format!(
                "Reset {}{}",
                removed.join(", "),
                if ok { "" } else { " (Hyprland didn't reload; it will on the next reload)" }
            ))
        }
        "list_hyprland_animations" => {
            let (anims, curves) = hyprland::animations().ok_or("Hyprland is not reachable")?;
            let overrides = load()?;
            let list: Vec<J> = anims
                .keys()
                .filter(|l| !l.starts_with("__"))
                .filter_map(|leaf| {
                    let a = hypranim::effective(&anims, leaf)?;
                    Some(json!({
                        "leaf": leaf,
                        "inherits_from": if anims[leaf].overridden { None } else { hypranim::parent(leaf) },
                        "enabled": a.enabled,
                        "duration_ms": (a.speed * 100.0).round(),
                        "curve": a.curve,
                        "style": a.style,
                        "changed_by_vela": overrides.animations.contains_key(leaf),
                    }))
                })
                .collect();
            let mut curves = curves;
            for c in hypranim::vela_curve_names() {
                if !curves.contains(&c) {
                    curves.push(c);
                }
            }
            Ok(pretty(&json!({ "animations": list, "curves": curves })))
        }
        "set_hyprland_animation" => {
            let leaf = s("leaf").ok_or("leaf is required")?;
            let (anims, _) = hyprland::animations().ok_or("Hyprland is not reachable")?;
            let base = hypranim::effective(&anims, leaf).ok_or_else(|| format!("unknown animation {leaf}"))?;
            let anim = Anim {
                enabled: args.get("enabled").and_then(|v| v.as_bool()).unwrap_or(base.enabled),
                speed: args.get("duration_ms").and_then(|v| v.as_f64()).map_or(base.speed, |ms| (ms / 100.0).max(0.01)),
                curve: s("curve").map_or(base.curve.clone(), str::to_owned),
                style: s("style").map_or(base.style.clone(), str::to_owned),
            };
            let mut code = hypranim::curves_lua();
            code.push(anim.eval_code(leaf));
            hyprland::eval(&code).map_err(|e| format!("Hyprland refused it: {e}"))?;
            let mut overrides = load()?;
            overrides.animations.insert(leaf.to_owned(), anim.clone());
            save(&overrides)?;
            Ok(format!("{leaf}: {}", anim.to_lua(leaf)))
        }
        "list_monitors" => {
            let monitors = hyprland::monitors_all().ok_or("Hyprland is not reachable")?;
            let list: Vec<J> = monitors
                .iter()
                .map(|m| {
                    let mut modes: Vec<String> = m.modes.iter().map(|x| x.spec()).collect();
                    modes.dedup();
                    json!({
                        "monitor": m.name,
                        "model": m.title(),
                        "enabled": !m.disabled,
                        "mode": m.mode.spec(),
                        "modes": modes,
                        "scale": m.scale,
                        "valid_scales": hyprmon::valid_scales(m.mode.width, m.mode.height, m.scale),
                        "position": [m.x, m.y],
                        "layout_size": m.logical_size(),
                        "rotation": m.transform,
                    })
                })
                .collect();
            Ok(pretty(&J::Array(list)))
        }
        "set_monitor" => {
            let name = s("monitor").ok_or("monitor is required")?;
            let monitors = hyprland::monitors_all().ok_or("Hyprland is not reachable")?;
            let m = monitors
                .iter()
                .find(|m| m.name == name || m.output() == name)
                .ok_or_else(|| format!("no monitor {name}; list_monitors shows them"))?;
            let mut overrides = load()?;
            let mut rule: MonitorRule = hyprmon::rule_for(overrides.monitors.get(&m.output()), m);
            if let Some(v) = s("mode") {
                rule.mode = v.to_owned();
            }
            if let Some(v) = args.get("scale").and_then(|v| v.as_f64()) {
                rule.scale = v;
            }
            if let Some(p) = args.get("position").and_then(|v| v.as_array())
                && let [x, y] = p.as_slice()
            {
                rule.position = (x.as_i64().unwrap_or(0) as i32, y.as_i64().unwrap_or(0) as i32);
            }
            if let Some(v) = args.get("rotation").and_then(|v| v.as_i64()) {
                rule.transform = v.clamp(0, 7);
            }
            if let Some(v) = args.get("vrr") {
                rule.vrr = v.as_i64();
            }
            if let Some(on) = args.get("enabled").and_then(|v| v.as_bool()) {
                if !on && !monitors.iter().any(|x| x.name != m.name && !x.disabled) {
                    return Err(format!("{name} is the only screen that is on; refusing to turn it off"));
                }
                rule.disabled = !on;
                if on && m.disabled {
                    rule.mode = "preferred".into();
                }
            }
            hyprland::eval(&[rule.eval_code()]).map_err(|e| format!("Hyprland refused it: {e}"))?;
            overrides.monitors.insert(rule.output.clone(), rule.clone());
            save(&overrides)?;
            Ok(format!("{name}: {}", rule.to_lua()))
        }
        "get_vela_settings" => std::fs::read_to_string(paths::config_file()).map_err(|e| format!("cannot read config: {e}")),
        "set_vela_setting" => {
            let (path, value) = (s("path").ok_or("path is required")?, s("value").ok_or("value is required")?);
            let file = paths::config_file();
            let result = config::load_or_create(&file).and_then(|outcome| {
                let mut cfg = match outcome {
                    config::LoadOutcome::Loaded(c) | config::LoadOutcome::Created(c) => c,
                };
                cfg.set_value(path, value)?;
                config::save(&file, &cfg)
            });
            result.map(|()| format!("{path} = {value}")).map_err(|e| format!("{e:#}"))
        }
        "open_vela_settings" => {
            let page = s("page").and_then(|p| ipc::SETTINGS_PAGES.iter().copied().find(|x| *x == p));
            ipc::send(&paths::socket_path(), ipc::Command::Settings(page))
                .map(|_| "opened".into())
                .map_err(|e| format!("vela isn't running: {e:?}"))
        }
        _ => Err(format!("unknown tool {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speaks_the_mcp_handshake() {
        let init = handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } })).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], "vela");
        assert!(handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let list = handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"set_hyprland_options"));
        assert!(names.contains(&"set_monitor"));
        for t in list["result"]["tools"].as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        }
        let unknown = handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "nope" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
        let bad = handle(&json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "nope" } })).unwrap();
        assert_eq!(bad["result"]["isError"], true);
    }
}
