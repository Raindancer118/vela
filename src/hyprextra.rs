//! What vela adds to Hyprland besides options: shortcuts (`hl.bind`, and
//! `hl.unbind` for ones from hyprland.lua the user switched off), window
//! rules (`hl.window_rule`) and autostart commands (`hyprland.start`).

use crate::hyprconf::lua_string;
use std::fmt::Write as _;

/// Modifier bits as `j/binds` reports them, in the order vela writes them.
const MODS: [(u32, &str); 4] = [(64, "SUPER"), (4, "CTRL"), (8, "ALT"), (1, "SHIFT")];

/// `SUPER + SHIFT + Q`: modifiers in a fixed order, the key as given
/// (single letters upper case), so the same shortcut always reads the same.
pub fn combo(mods: &[&str], key: &str) -> String {
    let mut parts: Vec<String> = MODS
        .iter()
        .filter(|(_, m)| {
            mods.iter()
                .any(|x| x.eq_ignore_ascii_case(m) || (m == &"CTRL" && x.eq_ignore_ascii_case("control")))
        })
        .map(|(_, m)| (*m).to_owned())
        .collect();
    parts.push(if key.chars().count() == 1 { key.to_uppercase() } else { key.to_owned() });
    parts.join(" + ")
}

/// A combo typed by hand (`super+shift+q`, `CTRL + ALT + T`) in canonical form.
pub fn normalize(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    let (key, mods) = parts.split_last()?;
    if MODS.iter().any(|(_, m)| m.eq_ignore_ascii_case(key)) {
        return None;
    }
    Some(combo(mods, key))
}

pub fn combo_from_mask(modmask: u32, key: &str) -> String {
    let mods: Vec<&str> = MODS.iter().filter(|(bit, _)| modmask & bit != 0).map(|(_, m)| *m).collect();
    combo(&mods, key)
}

/// Same shortcut? Keys compare without case (`left` = `LEFT`).
pub fn same_combo(a: &str, b: &str) -> bool {
    match (normalize(a), normalize(b)) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(&b),
        _ => false,
    }
}

/// GTK accelerator for `gtk::ShortcutLabel`, e.g. `<Super><Shift>q`.
pub fn gtk_accel(combo: &str) -> Option<String> {
    let parts: Vec<&str> = combo.split('+').map(str::trim).collect();
    let (key, mods) = parts.split_last()?;
    let mut out = String::new();
    for m in mods {
        out.push_str(match m.to_uppercase().as_str() {
            "SUPER" => "<Super>",
            "CTRL" | "CONTROL" => "<Control>",
            "ALT" => "<Alt>",
            "SHIFT" => "<Shift>",
            _ => return None,
        });
    }
    // GTK knows keysym names, mostly in this spelling.
    let key = match key.to_lowercase().as_str() {
        "space" => "space".to_owned(),
        "return" | "enter" => "Return".to_owned(),
        "left" | "right" | "up" | "down" => {
            let mut c = key.to_lowercase();
            c[..1].make_ascii_uppercase();
            c
        }
        k if k.chars().count() == 1 => k.to_owned(),
        _ => (*key).to_owned(),
    };
    out.push_str(&key);
    Some(out)
}

/// What an action needs besides the shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg {
    None,
    /// Free text: a command, a workspace, a global name, Lua.
    Text(&'static str),
    Direction,
    Monitor,
}

pub struct ActionDef {
    pub id: &'static str,
    pub label: &'static str,
    pub arg: Arg,
    lua: fn(&str) -> String,
}

fn workspace_arg(a: &str) -> String {
    match a.trim().parse::<i64>() {
        Ok(n) => n.to_string(),
        Err(_) => lua_string(a.trim()),
    }
}

/// Everything a vela shortcut can do; each turns into an `hl.dsp` value.
pub const ACTIONS: &[ActionDef] = &[
    ActionDef {
        id: "exec",
        label: "Run a command",
        arg: Arg::Text("Command"),
        lua: |a| format!("hl.dsp.exec_cmd({})", lua_string(a)),
    },
    ActionDef {
        id: "close",
        label: "Close window",
        arg: Arg::None,
        lua: |_| "hl.dsp.window.close()".into(),
    },
    ActionDef {
        id: "kill",
        label: "Kill window",
        arg: Arg::None,
        lua: |_| "hl.dsp.window.kill()".into(),
    },
    ActionDef {
        id: "float",
        label: "Toggle floating",
        arg: Arg::None,
        lua: |_| r#"hl.dsp.window.float({ action = "toggle" })"#.into(),
    },
    ActionDef {
        id: "fullscreen",
        label: "Toggle fullscreen",
        arg: Arg::None,
        lua: |_| r#"hl.dsp.window.fullscreen({ mode = "fullscreen", action = "toggle" })"#.into(),
    },
    ActionDef {
        id: "maximize",
        label: "Toggle maximized",
        arg: Arg::None,
        lua: |_| r#"hl.dsp.window.fullscreen({ mode = "maximized", action = "toggle" })"#.into(),
    },
    ActionDef {
        id: "pin",
        label: "Pin window (all workspaces)",
        arg: Arg::None,
        lua: |_| "hl.dsp.window.pin()".into(),
    },
    ActionDef {
        id: "center",
        label: "Centre window",
        arg: Arg::None,
        lua: |_| "hl.dsp.window.center()".into(),
    },
    ActionDef {
        id: "pseudo",
        label: "Toggle pseudo-tiling",
        arg: Arg::None,
        lua: |_| "hl.dsp.window.pseudo()".into(),
    },
    ActionDef {
        id: "togglesplit",
        label: "Toggle split direction",
        arg: Arg::None,
        lua: |_| r#"hl.dsp.layout("togglesplit")"#.into(),
    },
    ActionDef {
        id: "group",
        label: "Toggle window group",
        arg: Arg::None,
        lua: |_| "hl.dsp.group.toggle()".into(),
    },
    ActionDef {
        id: "cycle",
        label: "Next window",
        arg: Arg::None,
        lua: |_| "hl.dsp.window.cycle_next({ next = true })".into(),
    },
    ActionDef {
        id: "focus",
        label: "Move focus",
        arg: Arg::Direction,
        lua: |a| format!("hl.dsp.focus({{ direction = {} }})", lua_string(a)),
    },
    ActionDef {
        id: "move",
        label: "Move window",
        arg: Arg::Direction,
        lua: |a| format!("hl.dsp.window.move({{ direction = {} }})", lua_string(a)),
    },
    ActionDef {
        id: "workspace",
        label: "Go to workspace",
        arg: Arg::Text("Workspace (number, e+1, name)"),
        lua: |a| format!("hl.dsp.focus({{ workspace = {} }})", workspace_arg(a)),
    },
    ActionDef {
        id: "movetoworkspace",
        label: "Move window to workspace",
        arg: Arg::Text("Workspace (number, e+1, name)"),
        lua: |a| format!("hl.dsp.window.move({{ workspace = {} }})", workspace_arg(a)),
    },
    ActionDef {
        id: "special",
        label: "Toggle scratchpad",
        arg: Arg::Text("Scratchpad name"),
        lua: |a| format!("hl.dsp.workspace.toggle_special({})", lua_string(a)),
    },
    ActionDef {
        id: "focusmonitor",
        label: "Focus monitor",
        arg: Arg::Monitor,
        lua: |a| format!("hl.dsp.focus({{ monitor = {} }})", lua_string(a)),
    },
    ActionDef {
        id: "movetomonitor",
        label: "Move window to monitor",
        arg: Arg::Monitor,
        lua: |a| format!("hl.dsp.window.move({{ monitor = {} }})", lua_string(a)),
    },
    ActionDef {
        id: "global",
        label: "Global shortcut of an app",
        arg: Arg::Text("Name, e.g. quickshell:panelToggle"),
        lua: |a| format!("hl.dsp.global({})", lua_string(a)),
    },
    ActionDef {
        id: "exit",
        label: "Quit Hyprland",
        arg: Arg::None,
        lua: |_| "hl.dsp.exit()".into(),
    },
    ActionDef {
        id: "lua",
        label: "Lua dispatcher (advanced)",
        arg: Arg::Text("e.g. hl.dsp.window.swap({ direction = \"l\" })"),
        lua: |a| a.trim().to_owned(),
    },
];

pub const DIRECTIONS: [(&str, &str); 4] = [("left", "Left"), ("right", "Right"), ("up", "Up"), ("down", "Down")];

pub fn action(id: &str) -> Option<&'static ActionDef> {
    ACTIONS.iter().find(|a| a.id == id)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shortcut {
    pub keys: String,
    pub action: String,
    pub arg: String,
    pub repeat: bool,
    /// Also works on the lock screen.
    pub locked: bool,
    pub description: String,
    pub enabled: bool,
}

impl Shortcut {
    /// What it does, for lists and the bind description.
    pub fn summary(&self) -> String {
        let label = action(&self.action).map_or(self.action.as_str(), |a| a.label);
        if !self.description.trim().is_empty() {
            return self.description.trim().to_owned();
        }
        match action(&self.action).map(|a| a.arg) {
            Some(Arg::None) | None => label.to_owned(),
            Some(_) => format!("{label}: {}", self.arg.trim()),
        }
    }

    pub fn dispatcher_lua(&self) -> Option<String> {
        let def = action(&self.action)?;
        if def.arg != Arg::None && self.arg.trim().is_empty() {
            return None;
        }
        Some((def.lua)(&self.arg))
    }

    /// `hl.bind(…)` arguments; None for an incomplete shortcut.
    pub fn bind_lua(&self) -> Option<String> {
        let keys = normalize(&self.keys)?;
        let mut opts = format!("description = {}", lua_string(&format!("vela: {}", self.summary())));
        if self.repeat {
            opts.push_str(", repeating = true");
        }
        if self.locked {
            opts.push_str(", locked = true");
        }
        Some(format!("{}, {}, {{ {opts} }}", lua_string(&keys), self.dispatcher_lua()?))
    }

    fn to_toml(&self) -> toml::Value {
        let mut t = toml::Table::new();
        t.insert("keys".into(), self.keys.clone().into());
        t.insert("action".into(), self.action.clone().into());
        if !self.arg.is_empty() {
            t.insert("arg".into(), self.arg.clone().into());
        }
        t.insert("repeat".into(), self.repeat.into());
        t.insert("locked".into(), self.locked.into());
        if !self.description.is_empty() {
            t.insert("description".into(), self.description.clone().into());
        }
        t.insert("enabled".into(), self.enabled.into());
        toml::Value::Table(t)
    }

    fn from_toml(v: &toml::Value) -> Option<Shortcut> {
        let t = v.as_table()?;
        let s = |k: &str| t.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        let b = |k: &str, d: bool| t.get(k).and_then(|v| v.as_bool()).unwrap_or(d);
        Some(Shortcut {
            keys: t.get("keys")?.as_str()?.to_owned(),
            action: t.get("action")?.as_str()?.to_owned(),
            arg: s("arg"),
            repeat: b("repeat", false),
            locked: b("locked", false),
            description: s("description"),
            enabled: b("enabled", true),
        })
    }
}

/// What a window rule can do, with the value type the settings offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    Bool,
    Int,
    Float,
    Text,
}

pub const EFFECTS: &[(&str, &str, EffectKind)] = &[
    ("float", "Float", EffectKind::Bool),
    ("tile", "Tile", EffectKind::Bool),
    ("center", "Centre", EffectKind::Bool),
    ("size", "Size (width height, e.g. 1200 800 or 60% 70%)", EffectKind::Text),
    ("move", "Position (x y)", EffectKind::Text),
    ("workspace", "Workspace (add “silent” to stay)", EffectKind::Text),
    ("monitor", "Monitor", EffectKind::Text),
    ("opacity", "Opacity", EffectKind::Float),
    ("pin", "Pin (all workspaces)", EffectKind::Bool),
    ("fullscreen", "Fullscreen", EffectKind::Bool),
    ("maximize", "Maximized", EffectKind::Bool),
    ("pseudo", "Pseudo-tiled", EffectKind::Bool),
    ("no_blur", "No blur", EffectKind::Bool),
    ("no_shadow", "No shadow", EffectKind::Bool),
    ("no_anim", "No animation", EffectKind::Bool),
    ("no_dim", "Never dimmed", EffectKind::Bool),
    ("opaque", "Always opaque", EffectKind::Bool),
    ("rounding", "Corner radius", EffectKind::Int),
    ("border_size", "Border width", EffectKind::Int),
    ("no_focus", "Never focused", EffectKind::Bool),
    ("stay_focused", "Keep focus", EffectKind::Bool),
    ("keep_aspect_ratio", "Keep aspect ratio", EffectKind::Bool),
    ("dim_around", "Dim everything else", EffectKind::Bool),
    ("idle_inhibit", "Keep the screen on (always, focus, fullscreen)", EffectKind::Text),
];

pub fn effect(key: &str) -> Option<(&'static str, &'static str, EffectKind)> {
    EFFECTS.iter().find(|(k, _, _)| *k == key).copied()
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowRule {
    pub name: String,
    pub enabled: bool,
    /// Regexes; empty ones aren't matched on.
    pub class: String,
    pub title: String,
    /// Effect key → value (`true`, `0.9`, `"1200 800"`), in EFFECTS order.
    pub effects: Vec<(String, toml::Value)>,
}

impl WindowRule {
    pub fn lua(&self) -> Option<String> {
        let mut m = Vec::new();
        if !self.class.trim().is_empty() {
            m.push(format!("class = {}", lua_string(self.class.trim())));
        }
        if !self.title.trim().is_empty() {
            m.push(format!("title = {}", lua_string(self.title.trim())));
        }
        if m.is_empty() || self.effects.is_empty() {
            return None;
        }
        let mut out = format!("{{ name = {}, match = {{ {} }}", lua_string(&format!("vela-{}", self.name)), m.join(", "));
        for (k, v) in &self.effects {
            let (_, _, kind) = effect(k)?;
            let val = match (kind, v) {
                (EffectKind::Bool, toml::Value::Boolean(b)) => b.to_string(),
                (EffectKind::Int, toml::Value::Integer(i)) => i.to_string(),
                (EffectKind::Float, v) => format!("{:?}", v.as_float().or_else(|| v.as_integer().map(|i| i as f64))?),
                (EffectKind::Text, toml::Value::String(s)) if !s.trim().is_empty() => lua_string(s.trim()),
                _ => return None,
            };
            let _ = write!(out, ", {k} = {val}");
        }
        out.push_str(" }");
        Some(out)
    }

    fn to_toml(&self) -> toml::Value {
        let mut t = toml::Table::new();
        t.insert("name".into(), self.name.clone().into());
        t.insert("enabled".into(), self.enabled.into());
        t.insert("class".into(), self.class.clone().into());
        t.insert("title".into(), self.title.clone().into());
        t.insert("effects".into(), toml::Value::Table(self.effects.iter().cloned().collect()));
        toml::Value::Table(t)
    }

    fn from_toml(v: &toml::Value) -> Option<WindowRule> {
        let t = v.as_table()?;
        let s = |k: &str| t.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        let mut effects: Vec<(String, toml::Value)> = t
            .get("effects")
            .and_then(|e| e.as_table())
            .map(|e| e.iter().filter(|(k, _)| effect(k).is_some()).map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        effects.sort_by_key(|(k, _)| EFFECTS.iter().position(|(e, _, _)| e == k));
        Some(WindowRule {
            name: t.get("name")?.as_str()?.to_owned(),
            enabled: t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
            class: s("class"),
            title: s("title"),
            effects,
        })
    }
}

/// `^class$` for a window class taken from an open window.
pub fn exact_regex(s: &str) -> String {
    let mut out = String::from("^");
    for c in s.chars() {
        if ".^$*+?()[]{}|\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('$');
    out
}

/// `^org\.foo$` → `org.foo`, to show a regex made by `exact_regex`.
pub fn plain_regex(s: &str) -> String {
    let inner = s.strip_prefix('^').unwrap_or(s);
    let inner = inner.strip_suffix('$').unwrap_or(inner);
    let mut out = String::new();
    let mut esc = false;
    for c in inner.chars() {
        if c == '\\' && !esc {
            esc = true;
            continue;
        }
        esc = false;
        out.push(c);
    }
    out
}

/// Lua that enters / leaves a submap without binds, so a shortcut being
/// recorded reaches the settings instead of triggering its bind. Its one
/// bind (Ctrl+Alt+Esc) leaves it, should vela ever not.
pub const RECORD_SUBMAP: &str = "vela-record";

pub fn record_start_lua() -> [String; 2] {
    [
        format!(
            "hl.define_submap({0}, function() hl.bind(\"CTRL + ALT + Escape\", hl.dsp.submap(\"reset\")) end)",
            lua_string(RECORD_SUBMAP)
        ),
        format!("hl.dispatch(hl.dsp.submap({}))", lua_string(RECORD_SUBMAP)),
    ]
}

pub const RECORD_STOP_LUA: &str = "hl.dispatch(hl.dsp.submap(\"reset\"))";

/// Rule names become `vela-<name>` in Hyprland: letters, digits and dashes.
pub fn rule_name(from: &str) -> String {
    let s: String = from.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "rule".into() } else { s }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Autostart {
    pub command: String,
    pub enabled: bool,
}

/// Shortcuts, switched-off binds, rules and autostart in hyprland.toml.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Extras {
    pub shortcuts: Vec<Shortcut>,
    /// Shortcuts from hyprland.lua that vela turns off (`hl.unbind`).
    pub unbind: Vec<String>,
    pub rules: Vec<WindowRule>,
    pub autostart: Vec<Autostart>,
}

impl Extras {
    pub fn is_empty(&self) -> bool {
        self.shortcuts.is_empty() && self.unbind.is_empty() && self.rules.is_empty() && self.autostart.is_empty()
    }

    pub fn read(table: &toml::Table) -> Extras {
        let arr = |k: &str| table.get(k).and_then(|v| v.as_array()).cloned().unwrap_or_default();
        Extras {
            shortcuts: arr("shortcuts").iter().filter_map(Shortcut::from_toml).collect(),
            unbind: arr("unbind").iter().filter_map(|v| normalize(v.as_str()?)).collect(),
            rules: arr("rules").iter().filter_map(WindowRule::from_toml).collect(),
            autostart: arr("autostart")
                .iter()
                .filter_map(|v| {
                    let t = v.as_table()?;
                    Some(Autostart {
                        command: t.get("command")?.as_str()?.to_owned(),
                        enabled: t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                    })
                })
                .collect(),
        }
    }

    pub fn write(&self, table: &mut toml::Table) {
        if !self.shortcuts.is_empty() {
            table.insert("shortcuts".into(), toml::Value::Array(self.shortcuts.iter().map(Shortcut::to_toml).collect()));
        }
        if !self.unbind.is_empty() {
            table.insert("unbind".into(), toml::Value::Array(self.unbind.iter().map(|s| s.clone().into()).collect()));
        }
        if !self.rules.is_empty() {
            table.insert("rules".into(), toml::Value::Array(self.rules.iter().map(WindowRule::to_toml).collect()));
        }
        if !self.autostart.is_empty() {
            let items = self
                .autostart
                .iter()
                .map(|a| {
                    let mut t = toml::Table::new();
                    t.insert("command".into(), a.command.clone().into());
                    t.insert("enabled".into(), a.enabled.into());
                    toml::Value::Table(t)
                })
                .collect();
            table.insert("autostart".into(), toml::Value::Array(items));
        }
    }

    /// Lua lines, using the generated file's `try`. Unbinds first, so a
    /// vela shortcut can take over a key from hyprland.lua.
    pub fn lua(&self) -> String {
        let mut out = String::new();
        let mut taken: Vec<String> = Vec::new();
        for k in &self.unbind {
            let _ = writeln!(out, "try(hl.unbind, {})", lua_string(k));
            taken.push(k.clone());
        }
        for s in self.shortcuts.iter().filter(|s| s.enabled) {
            let Some(args) = s.bind_lua() else { continue };
            let keys = normalize(&s.keys).unwrap_or_default();
            // Replaces whatever hyprland.lua bound to these keys.
            if !taken.iter().any(|t| same_combo(t, &keys)) {
                let _ = writeln!(out, "try(hl.unbind, {})", lua_string(&keys));
                taken.push(keys);
            }
            let _ = writeln!(out, "try(hl.bind, {args})");
        }
        for r in self.rules.iter().filter(|r| r.enabled) {
            if let Some(l) = r.lua() {
                let _ = writeln!(out, "try(hl.window_rule, {l})");
            }
        }
        for a in self.autostart.iter().filter(|a| a.enabled && !a.command.trim().is_empty()) {
            let _ = writeln!(
                out,
                "try(hl.on, \"hyprland.start\", function() hl.exec_cmd({}) end)",
                lua_string(a.command.trim())
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(keys: &str, action: &str, arg: &str) -> Shortcut {
        Shortcut {
            keys: keys.into(),
            action: action.into(),
            arg: arg.into(),
            repeat: false,
            locked: false,
            description: String::new(),
            enabled: true,
        }
    }

    #[test]
    fn classes_from_open_windows_match_exactly() {
        assert_eq!(exact_regex("org.gnome.Nautilus"), r"^org\.gnome\.Nautilus$");
        assert_eq!(exact_regex("a+b(c)"), r"^a\+b\(c\)$");
        assert_eq!(plain_regex(&exact_regex("org.gnome.Nautilus")), "org.gnome.Nautilus");
    }

    #[test]
    fn combos_read_the_same_however_written() {
        assert_eq!(normalize("shift+super+q").as_deref(), Some("SUPER + SHIFT + Q"));
        assert_eq!(normalize("CTRL + ALT + T").as_deref(), Some("CTRL + ALT + T"));
        assert_eq!(normalize("control+Return").as_deref(), Some("CTRL + Return"));
        assert_eq!(normalize("SUPER"), None);
        assert_eq!(combo_from_mask(64 + 1, "Q"), "SUPER + SHIFT + Q");
        assert_eq!(combo_from_mask(77, "F24"), "SUPER + CTRL + ALT + SHIFT + F24");
        assert!(same_combo("SUPER + left", "super+LEFT"));
        assert!(!same_combo("SUPER + Q", "SUPER + SHIFT + Q"));
        assert_eq!(gtk_accel("SUPER + SHIFT + Q").as_deref(), Some("<Super><Shift>q"));
        assert_eq!(gtk_accel("SUPER + left").as_deref(), Some("<Super>Left"));
        assert_eq!(gtk_accel("CTRL + ALT + Delete").as_deref(), Some("<Control><Alt>Delete"));
    }

    #[test]
    fn shortcuts_become_binds() {
        let s = shortcut("super+b", "exec", "firefox --new-window");
        assert_eq!(
            s.bind_lua().unwrap(),
            r#""SUPER + B", hl.dsp.exec_cmd("firefox --new-window"), { description = "vela: Run a command: firefox --new-window" }"#
        );
        let mut w = shortcut("SUPER + 3", "workspace", "3");
        w.repeat = true;
        assert_eq!(
            w.bind_lua().unwrap(),
            r#""SUPER + 3", hl.dsp.focus({ workspace = 3 }), { description = "vela: Go to workspace: 3", repeating = true }"#
        );
        assert_eq!(
            shortcut("SUPER + S", "special", "magic").dispatcher_lua().unwrap(),
            r#"hl.dsp.workspace.toggle_special("magic")"#
        );
        assert_eq!(
            shortcut("SUPER + E", "workspace", "e+1").dispatcher_lua().unwrap(),
            r#"hl.dsp.focus({ workspace = "e+1" })"#
        );
        assert!(shortcut("SUPER + X", "exec", " ").bind_lua().is_none(), "a command is needed");
        assert!(shortcut("SUPER + X", "nope", "").bind_lua().is_none());
        assert!(shortcut("SUPER", "close", "").bind_lua().is_none(), "a key is needed");
    }

    #[test]
    fn vela_shortcuts_take_keys_over_from_hyprland_lua() {
        let mut e = Extras {
            shortcuts: vec![shortcut("SUPER + Q", "exec", "kitty"), shortcut("SUPER + W", "close", "")],
            unbind: vec!["SUPER + M".into()],
            ..Default::default()
        };
        e.shortcuts[1].enabled = false;
        let lua = e.lua();
        assert!(lua.contains("try(hl.unbind, \"SUPER + M\")\n"));
        assert!(lua.find("try(hl.unbind, \"SUPER + Q\")").unwrap() < lua.find("try(hl.bind, \"SUPER + Q\"").unwrap());
        assert!(!lua.contains("SUPER + W"), "disabled shortcuts aren't bound");
    }

    #[test]
    fn window_rules_and_autostart_become_lua() {
        let r = WindowRule {
            name: rule_name("Firefox — Picture-in-Picture"),
            enabled: true,
            class: "^firefox$".into(),
            title: "^Picture-in-Picture$".into(),
            effects: vec![
                ("float".into(), true.into()),
                ("size".into(), "640 360".into()),
                ("opacity".into(), 0.9.into()),
                ("rounding".into(), 0.into()),
            ],
        };
        assert_eq!(r.name, "firefox-picture-in-picture");
        assert_eq!(
            r.lua().unwrap(),
            r#"{ name = "vela-firefox-picture-in-picture", match = { class = "^firefox$", title = "^Picture-in-Picture$" }, float = true, size = "640 360", opacity = 0.9, rounding = 0 }"#
        );
        assert!(
            WindowRule {
                class: String::new(),
                title: String::new(),
                ..r.clone()
            }
            .lua()
            .is_none(),
            "matches nothing"
        );
        let e = Extras {
            rules: vec![r],
            autostart: vec![
                Autostart {
                    command: "nm-applet --indicator".into(),
                    enabled: true,
                },
                Autostart {
                    command: "off".into(),
                    enabled: false,
                },
            ],
            ..Default::default()
        };
        let lua = e.lua();
        assert!(lua.contains(r#"try(hl.on, "hyprland.start", function() hl.exec_cmd("nm-applet --indicator") end)"#));
        assert!(!lua.contains("\"off\""));
        // Round trip through hyprland.toml.
        let mut t = toml::Table::new();
        e.write(&mut t);
        let text = toml::to_string(&t).unwrap();
        assert_eq!(Extras::read(&toml::from_str(&text).unwrap()), e);
    }
}
