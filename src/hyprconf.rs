//! Hyprland options vela can change: the catalogue Hyprland reports
//! (`j/descriptions` + `j/getoption`), the overrides vela keeps in
//! `~/.config/vela/hyprland.toml` and the Lua they turn into. `vela.lua`
//! loads that Lua after the user's own config, so overrides win and every
//! option vela never touched stays exactly as hyprland.lua sets it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Int,
    Float,
    Str,
    /// Single colour, an int option such as `misc:col.splash`.
    Color,
    Gradient,
    /// Four sides, like `general:gaps_out`.
    Gaps,
    Vec2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// 0xAARRGGBB, as Hyprland reports colours.
    pub colors: Vec<u32>,
    pub angle: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gaps {
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
    pub left: i64,
}

impl Gaps {
    pub fn uniform(v: i64) -> Gaps {
        Gaps {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    pub fn max(&self) -> i64 {
        self.top.max(self.right).max(self.bottom).max(self.left)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Color(u32),
    Gradient(Gradient),
    Gaps(Gaps),
    Vec2([f64; 2]),
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(*v as f64),
            Value::Float(v) => Some(*v),
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Int(v) => Some(*v != 0),
            _ => None,
        }
    }

    /// Converts a number from a widget into the option's kind.
    pub fn number(kind: Kind, v: f64) -> Option<Value> {
        match kind {
            Kind::Int => Some(Value::Int(v.round() as i64)),
            Kind::Float => Some(Value::Float(v)),
            Kind::Bool => Some(Value::Bool(v != 0.0)),
            _ => None,
        }
    }

    pub fn to_lua(&self) -> String {
        match self {
            Value::Bool(b) => b.to_string(),
            Value::Int(v) => v.to_string(),
            Value::Float(v) => lua_float(*v),
            Value::Str(s) => lua_string(s),
            Value::Color(c) => lua_string(&rgba(*c)),
            Value::Gradient(g) if g.colors.len() == 1 && g.angle == 0 => lua_string(&rgba(g.colors[0])),
            Value::Gradient(g) => {
                let colors: Vec<String> = g.colors.iter().map(|c| lua_string(&rgba(*c))).collect();
                format!("{{ colors = {{ {} }}, angle = {} }}", colors.join(", "), g.angle)
            }
            Value::Gaps(g) => format!("{{ top = {}, right = {}, bottom = {}, left = {} }}", g.top, g.right, g.bottom, g.left),
            Value::Vec2([x, y]) => format!("{{ {}, {} }}", lua_float(*x), lua_float(*y)),
        }
    }

    /// TOML form for hyprland.toml; `from_toml` reads it back without
    /// knowing the option (colours are `rgba(…)` strings, gaps and gradients
    /// tables with their own keys).
    pub fn to_toml(&self) -> toml::Value {
        match self {
            Value::Bool(b) => toml::Value::Boolean(*b),
            Value::Int(v) => toml::Value::Integer(*v),
            Value::Float(v) => toml::Value::Float(*v),
            Value::Str(s) => toml::Value::String(s.clone()),
            Value::Color(c) => toml::Value::String(rgba(*c)),
            Value::Gradient(g) => {
                let mut t = toml::Table::new();
                t.insert(
                    "colors".into(),
                    toml::Value::Array(g.colors.iter().map(|c| toml::Value::String(rgba(*c))).collect()),
                );
                t.insert("angle".into(), toml::Value::Integer(g.angle));
                toml::Value::Table(t)
            }
            Value::Gaps(g) => {
                let mut t = toml::Table::new();
                for (k, v) in [("top", g.top), ("right", g.right), ("bottom", g.bottom), ("left", g.left)] {
                    t.insert(k.into(), toml::Value::Integer(v));
                }
                toml::Value::Table(t)
            }
            Value::Vec2([x, y]) => toml::Value::Array(vec![toml::Value::Float(*x), toml::Value::Float(*y)]),
        }
    }

    pub fn from_toml(v: &toml::Value) -> Option<Value> {
        let num = |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
        Some(match v {
            toml::Value::Boolean(b) => Value::Bool(*b),
            toml::Value::Integer(i) => Value::Int(*i),
            toml::Value::Float(f) => Value::Float(*f),
            toml::Value::String(s) => match parse_rgba(s) {
                Some(c) => Value::Color(c),
                None => Value::Str(s.clone()),
            },
            toml::Value::Array(a) if a.len() == 2 => Value::Vec2([num(&a[0])?, num(&a[1])?]),
            toml::Value::Table(t) if t.contains_key("colors") => {
                let colors: Option<Vec<u32>> = t.get("colors")?.as_array()?.iter().map(|c| parse_rgba(c.as_str()?)).collect();
                let colors = colors.filter(|c| !c.is_empty())?;
                Value::Gradient(Gradient {
                    colors,
                    angle: t.get("angle").and_then(|a| a.as_integer()).unwrap_or(0),
                })
            }
            toml::Value::Table(t) if t.contains_key("top") => {
                let side = |k: &str| t.get(k).and_then(|v| v.as_integer()).unwrap_or(0);
                Value::Gaps(Gaps {
                    top: side("top"),
                    right: side("right"),
                    bottom: side("bottom"),
                    left: side("left"),
                })
            }
            _ => return None,
        })
    }

    /// Brings a value into the option's kind, e.g. `1` for a float option or
    /// a plain number for gaps; None if it can't be.
    pub fn coerce(self, kind: Kind) -> Option<Value> {
        Some(match (kind, self) {
            (Kind::Bool, Value::Bool(b)) => Value::Bool(b),
            (Kind::Bool, Value::Int(i)) => Value::Bool(i != 0),
            (Kind::Int, Value::Int(i)) => Value::Int(i),
            (Kind::Int, Value::Bool(b)) => Value::Int(i64::from(b)),
            (Kind::Int, Value::Float(f)) => Value::Int(f.round() as i64),
            (Kind::Float, Value::Float(f)) => Value::Float(f),
            (Kind::Float, Value::Int(i)) => Value::Float(i as f64),
            (Kind::Str, Value::Str(s)) => Value::Str(s),
            (Kind::Color, Value::Color(c)) => Value::Color(c),
            (Kind::Color, Value::Gradient(g)) => Value::Color(g.colors[0]),
            (Kind::Gradient, Value::Gradient(g)) => Value::Gradient(g),
            (Kind::Gradient, Value::Color(c)) => Value::Gradient(Gradient { colors: vec![c], angle: 0 }),
            (Kind::Gaps, Value::Gaps(g)) => Value::Gaps(g),
            (Kind::Gaps, Value::Int(i)) => Value::Gaps(Gaps::uniform(i)),
            (Kind::Vec2, Value::Vec2(v)) => Value::Vec2(v),
            _ => return None,
        })
    }
}

fn lua_float(v: f64) -> String {
    if v.is_finite() { format!("{v:?}") } else { "0.0".into() }
}

/// Lua that loads vela's generated settings at `path` into the running
/// Hyprland, but only if it is the file vela.lua itself loads (it records
/// that in `vela_settings_file`; older vela.lua: the default state path): a
/// vela started with another XDG_STATE_HOME (tests) must not change it.
pub fn apply_if_loaded(path: &str) -> String {
    format!(
        "local want = vela_settings_file if not want then \
         local s = os.getenv(\"XDG_STATE_HOME\") if not s or s == \"\" then s = os.getenv(\"HOME\") .. \"/.local/state\" end \
         want = s .. \"/vela/hyprland.lua\" end \
         local p = {} if want == p then dofile(p) end",
        lua_string(path)
    )
}

pub fn lua_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 0xAARRGGBB → Hyprland's `rgba(rrggbbaa)`.
pub fn rgba(c: u32) -> String {
    format!("rgba({:06x}{:02x})", c & 0x00ff_ffff, c >> 24)
}

/// `rgba(rrggbbaa)` → 0xAARRGGBB.
pub fn parse_rgba(s: &str) -> Option<u32> {
    let hex = s.trim().strip_prefix("rgba(")?.strip_suffix(')')?;
    if hex.len() != 8 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some((v >> 8) | ((v & 0xff) << 24))
}

/// `"b3ffffff 0deg"` or `"ff33ccee ff00ff99 45deg"` (getoption's gradient
/// form, colours as AARRGGBB).
pub fn parse_gradient(s: &str) -> Option<Gradient> {
    let mut colors = Vec::new();
    let mut angle = 0;
    for tok in s.split_whitespace() {
        if let Some(deg) = tok.strip_suffix("deg") {
            angle = deg.parse::<f64>().ok()?.round() as i64;
        } else {
            colors.push(u32::from_str_radix(tok, 16).ok()?);
        }
    }
    (!colors.is_empty()).then_some(Gradient { colors, angle })
}

/// `"8 8 8 8"` (CSS order top right bottom left), also 1–3 values.
pub fn parse_gaps(s: &str) -> Option<Gaps> {
    let v: Option<Vec<i64>> = s.split_whitespace().map(|t| t.parse().ok()).collect();
    let v = v?;
    Some(match v.as_slice() {
        [a] => Gaps::uniform(*a),
        [a, b] => Gaps {
            top: *a,
            right: *b,
            bottom: *a,
            left: *b,
        },
        [a, b, c] => Gaps {
            top: *a,
            right: *b,
            bottom: *c,
            left: *b,
        },
        [a, b, c, d] => Gaps {
            top: *a,
            right: *b,
            bottom: *c,
            left: *d,
        },
        _ => return None,
    })
}

/// Lua table that sets one option, e.g. `general:col.active_border` →
/// `{ general = { col = { active_border = … } } }`. The Lua config spells
/// `tap-to-click` and `input-capture` with underscores.
pub fn lua_table(name: &str, value: &Value) -> String {
    let name = name.replace('-', "_");
    let parts: Vec<&str> = name.split([':', '.']).collect();
    let mut out = String::new();
    for p in &parts {
        if is_identifier(p) {
            let _ = write!(out, "{{ {p} = ");
        } else {
            let _ = write!(out, "{{ [{}] = ", lua_string(p));
        }
    }
    out.push_str(&value.to_lua());
    for _ in &parts {
        out.push_str(" }");
    }
    out
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Code for `hyprctl eval` that applies one option live.
pub fn eval_code(name: &str, value: &Value) -> String {
    format!("hl.config({})", lua_table(name, value))
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptionInfo {
    pub name: String,
    pub description: String,
    pub kind: Kind,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Named values of an int option, e.g. follow_mouse (0 disabled, …).
    pub choices: Vec<(i64, String)>,
    pub default: Option<Value>,
}

impl OptionInfo {
    /// Section shown in the settings, e.g. `decoration:blur`.
    pub fn section(&self) -> &str {
        self.name.rsplit_once(':').map_or("", |(s, _)| s)
    }

    pub fn key(&self) -> &str {
        self.name.rsplit_once(':').map_or(&self.name, |(_, k)| k)
    }
}

/// Type a `j/getoption` answer reports: its one key besides option/set.
fn getoption_type(j: &serde_json::Value) -> Option<(&str, &serde_json::Value)> {
    j.as_object()?.iter().find(|(k, _)| *k != "option" && *k != "set").map(|(k, v)| (k.as_str(), v))
}

fn kind_from(getoption: Option<&str>, default: &serde_json::Value) -> Kind {
    match getoption {
        Some("bool") => Kind::Bool,
        // Colours are ints in getoption but hex strings in descriptions.
        Some("int") if default.is_string() => Kind::Color,
        Some("int") => Kind::Int,
        Some("float") => Kind::Float,
        Some("gradient") => Kind::Gradient,
        Some("css") => Kind::Gaps,
        Some("vec2") => Kind::Vec2,
        Some(_) => Kind::Str,
        None => match default {
            serde_json::Value::Bool(_) => Kind::Bool,
            serde_json::Value::Number(n) if n.is_f64() => Kind::Float,
            serde_json::Value::Number(_) => Kind::Int,
            serde_json::Value::Array(_) => Kind::Vec2,
            _ => Kind::Str,
        },
    }
}

/// A value as `j/getoption` (or a descriptions default) gives it.
pub fn value_from_json(kind: Kind, v: &serde_json::Value) -> Option<Value> {
    Some(match kind {
        Kind::Bool => Value::Bool(v.as_bool().or_else(|| v.as_i64().map(|i| i != 0))?),
        Kind::Int => Value::Int(v.as_i64().or_else(|| v.as_bool().map(i64::from))?),
        Kind::Float => Value::Float(v.as_f64()?),
        Kind::Str => {
            let s = v.as_str()?;
            Value::Str(if s.starts_with("[[") && s.ends_with("]]") {
                String::new()
            } else {
                s.to_owned()
            })
        }
        Kind::Color => match v {
            serde_json::Value::String(s) => Value::Color(u32::from_str_radix(s.trim(), 16).ok()?),
            _ => Value::Color(u32::try_from(v.as_i64()?).ok()?),
        },
        Kind::Gradient => Value::Gradient(parse_gradient(v.as_str()?)?),
        Kind::Gaps => Value::Gaps(parse_gaps(v.as_str()?)?),
        Kind::Vec2 => {
            let a = v.as_array()?;
            if a.len() != 2 {
                return None;
            }
            Value::Vec2([a[0].as_f64()?, a[1].as_f64()?])
        }
    })
}

/// Splits a `[[BATCH]]` answer of `j/getoption` requests into one JSON value
/// per request (None where Hyprland couldn't answer).
pub fn split_batch(out: &str, count: usize) -> Vec<Option<serde_json::Value>> {
    let mut parts: Vec<Option<serde_json::Value>> = out.split("\n\n\n").map(|p| serde_json::from_str(p.trim()).ok()).collect();
    parts.resize(count, None);
    parts
}

/// Options and their current values from `j/descriptions` and the batch
/// answer to `j/getoption <name>` for every described option, in order.
pub fn catalogue(descriptions: &str, getoptions: &[Option<serde_json::Value>]) -> (Vec<OptionInfo>, BTreeMap<String, Value>) {
    let list: Vec<serde_json::Value> = serde_json::from_str(descriptions).unwrap_or_default();
    let mut infos = Vec::with_capacity(list.len());
    let mut current = BTreeMap::new();
    for (i, d) in list.iter().enumerate() {
        let Some(name) = d.get("name").and_then(|n| n.as_str()) else { continue };
        let null = serde_json::Value::Null;
        let default = d.get("default").unwrap_or(&null);
        let got = getoptions.get(i).and_then(|g| g.as_ref()).and_then(getoption_type);
        let kind = kind_from(got.map(|(t, _)| t), default);
        let num = |k: &str| d.get(k).and_then(|v| v.as_f64());
        let mut choices: Vec<(i64, String)> = d
            .get("map")
            .and_then(|m| m.as_array())
            .map(|m| {
                m.iter()
                    .filter_map(|e| e.as_object()?.iter().next().and_then(|(label, v)| Some((v.as_i64()?, label.clone()))))
                    .collect()
            })
            .unwrap_or_default();
        choices.sort();
        if let Some(v) = got.and_then(|(_, v)| value_from_json(kind, v)) {
            current.insert(name.to_owned(), v);
        }
        infos.push(OptionInfo {
            name: name.to_owned(),
            description: d.get("description").and_then(|v| v.as_str()).unwrap_or_default().to_owned(),
            kind,
            min: num("min"),
            max: num("max"),
            choices,
            default: value_from_json(kind, default),
        });
    }
    (infos, current)
}

/// Contents of hyprland.toml: option overrides plus a few remembered UI
/// values (e.g. the edge gap width while edge gaps are switched off).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overrides {
    pub options: BTreeMap<String, Value>,
    /// `hl.animation` per leaf, e.g. `windowsIn`.
    pub animations: BTreeMap<String, crate::hypranim::Anim>,
    /// Full `hl.monitor` rules by output (`desc:…` or connector).
    pub monitors: BTreeMap<String, crate::hyprmon::MonitorRule>,
    /// Shortcuts, switched-off binds, window rules, autostart.
    pub extras: crate::hyprextra::Extras,
    pub remember: BTreeMap<String, i64>,
}

const TOML_HEADER: &str = "# Hyprland settings changed in vela (Settings → Hyprland). Only these options
# are overridden; everything else stays as in your hyprland.lua.";

impl Overrides {
    pub fn from_toml(text: &str) -> anyhow::Result<Overrides> {
        let table: toml::Table = toml::from_str(text)?;
        let mut o = Overrides::default();
        if let Some(opts) = table.get("options").and_then(|v| v.as_table()) {
            for (k, v) in opts {
                match Value::from_toml(v) {
                    Some(val) => {
                        o.options.insert(k.clone(), val);
                    }
                    None => log::warn!("hyprland.toml: ignoring unreadable value for {k}"),
                }
            }
        }
        if let Some(anims) = table.get("animations").and_then(|v| v.as_table()) {
            for (k, v) in anims {
                match crate::hypranim::Anim::from_toml(v) {
                    Some(a) => {
                        o.animations.insert(k.clone(), a);
                    }
                    None => log::warn!("hyprland.toml: ignoring unreadable animation {k}"),
                }
            }
        }
        if let Some(mons) = table.get("monitors").and_then(|v| v.as_table()) {
            for (k, v) in mons {
                match crate::hyprmon::MonitorRule::from_toml(k, v) {
                    Some(r) => {
                        o.monitors.insert(k.clone(), r);
                    }
                    None => log::warn!("hyprland.toml: ignoring unreadable monitor {k}"),
                }
            }
        }
        o.extras = crate::hyprextra::Extras::read(&table);
        if let Some(rem) = table.get("remember").and_then(|v| v.as_table()) {
            for (k, v) in rem {
                if let Some(i) = v.as_integer() {
                    o.remember.insert(k.clone(), i);
                }
            }
        }
        Ok(o)
    }

    pub fn to_toml(&self) -> String {
        let mut table = toml::Table::new();
        let opts: toml::Table = self.options.iter().map(|(k, v)| (k.clone(), v.to_toml())).collect();
        table.insert("options".into(), toml::Value::Table(opts));
        if !self.animations.is_empty() {
            let anims: toml::Table = self.animations.iter().map(|(k, a)| (k.clone(), a.to_toml())).collect();
            table.insert("animations".into(), toml::Value::Table(anims));
        }
        if !self.monitors.is_empty() {
            let mons: toml::Table = self.monitors.iter().map(|(k, r)| (k.clone(), r.to_toml())).collect();
            table.insert("monitors".into(), toml::Value::Table(mons));
        }
        self.extras.write(&mut table);
        if !self.remember.is_empty() {
            let rem: toml::Table = self.remember.iter().map(|(k, v)| (k.clone(), toml::Value::Integer(*v))).collect();
            table.insert("remember".into(), toml::Value::Table(rem));
        }
        format!("{TOML_HEADER}\n\n{}", toml::to_string_pretty(&table).unwrap_or_default())
    }

    /// The Lua `vela.lua` loads. One protected call per option: a key an
    /// older or newer Hyprland doesn't know can't break the rest.
    pub fn to_lua(&self) -> String {
        let mut out = String::from(
            "-- Generated by vela from ~/.config/vela/hyprland.toml; change these in\n\
             -- vela's settings (Hyprland section) instead of editing this file.\n\
             local function try(f, ...)\n    local ok, err = pcall(f, ...)\n    if not ok then print(\"vela: \" .. tostring(err)) end\nend\n\
             local function set(t) try(hl.config, t) end\n",
        );
        // vela's curves exist even without overrides, so they can be picked.
        for (name, def) in crate::hypranim::VELA_CURVES {
            let _ = writeln!(out, "try(hl.curve, {}, {def})", lua_string(name));
        }
        for (name, value) in &self.options {
            let _ = writeln!(out, "set({})", lua_table(name, value));
        }
        for (leaf, anim) in &self.animations {
            let _ = writeln!(out, "try(hl.animation, {})", anim.to_lua(leaf));
        }
        for rule in self.monitors.values() {
            let _ = writeln!(out, "try(hl.monitor, {})", rule.to_lua());
        }
        out.push_str(&self.extras.lua());
        out
    }
}

/// The screen-edge gaps as the settings show them: one width and a switch
/// per edge. `general:gaps_out` holds 0 on a switched-off edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeGaps {
    pub width: i64,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl EdgeGaps {
    /// `remembered` is the width to show while every edge is off.
    pub fn from_gaps(g: Gaps, remembered: i64) -> EdgeGaps {
        let width = if g.max() > 0 { g.max() } else { remembered.max(1) };
        EdgeGaps {
            width,
            top: g.top > 0,
            right: g.right > 0,
            bottom: g.bottom > 0,
            left: g.left > 0,
        }
    }

    pub fn any(&self) -> bool {
        self.top || self.right || self.bottom || self.left
    }

    pub fn to_gaps(self) -> Gaps {
        let side = |on: bool| if on { self.width } else { 0 };
        Gaps {
            top: side(self.top),
            right: side(self.right),
            bottom: side(self.bottom),
            left: side(self.left),
        }
    }
}

/// A colour as people and models write it: `rgba(rrggbbaa)`, `rgb(rrggbb)`,
/// `#rrggbb`, `#rrggbbaa` or `0xAARRGGBB`.
pub fn parse_color(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(c) = parse_rgba(s) {
        return Some(c);
    }
    if let Some(hex) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        return (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten().map(|v| v | 0xff00_0000);
    }
    if let Some(hex) = s.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).ok();
    }
    let hex = s.strip_prefix('#')?;
    let v = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        6 => Some(v | 0xff00_0000),
        8 => Some((v >> 8) | ((v & 0xff) << 24)),
        _ => None,
    }
}

/// A value given as JSON (by the MCP server) for an option of this kind.
pub fn value_from_input(info: &OptionInfo, v: &serde_json::Value) -> Result<Value, String> {
    use serde_json::Value as J;
    let bad = || format!("{} expects {}", info.name, kind_hint(info));
    let num = |v: &J| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()));
    let value = match info.kind {
        Kind::Bool => match v {
            J::Bool(b) => Value::Bool(*b),
            J::Number(n) => Value::Bool(n.as_f64() != Some(0.0)),
            J::String(s) => match s.trim().to_lowercase().as_str() {
                "true" | "on" | "yes" | "1" => Value::Bool(true),
                "false" | "off" | "no" | "0" => Value::Bool(false),
                _ => return Err(bad()),
            },
            _ => return Err(bad()),
        },
        Kind::Int => match v {
            J::String(s) if !info.choices.is_empty() && s.trim().parse::<f64>().is_err() => {
                let s = s.trim().to_lowercase();
                let (n, _) = info.choices.iter().find(|(_, l)| l.to_lowercase() == s).ok_or_else(bad)?;
                Value::Int(*n)
            }
            J::Bool(b) => Value::Int(i64::from(*b)),
            _ => Value::Int(num(v).ok_or_else(bad)?.round() as i64),
        },
        Kind::Float => Value::Float(num(v).ok_or_else(bad)?),
        Kind::Str => Value::Str(v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())),
        Kind::Color => Value::Color(v.as_str().and_then(parse_color).ok_or_else(bad)?),
        Kind::Gradient => match v {
            J::String(s) => Value::Gradient(Gradient {
                colors: vec![parse_color(s).ok_or_else(bad)?],
                angle: 0,
            }),
            J::Array(a) => Value::Gradient(Gradient {
                colors: a
                    .iter()
                    .map(|c| c.as_str().and_then(parse_color))
                    .collect::<Option<Vec<_>>>()
                    .filter(|c| !c.is_empty())
                    .ok_or_else(bad)?,
                angle: 0,
            }),
            J::Object(o) => Value::Gradient(Gradient {
                colors: o
                    .get("colors")
                    .and_then(|c| c.as_array())
                    .and_then(|a| a.iter().map(|c| c.as_str().and_then(parse_color)).collect::<Option<Vec<_>>>())
                    .filter(|c| !c.is_empty())
                    .ok_or_else(bad)?,
                angle: o.get("angle").and_then(num).unwrap_or(0.0).round() as i64,
            }),
            _ => return Err(bad()),
        },
        Kind::Gaps => match v {
            J::Object(o) => {
                let side = |k: &str| o.get(k).and_then(num).map(|f| f.round() as i64);
                let all = side("all").unwrap_or(0);
                Value::Gaps(Gaps {
                    top: side("top").unwrap_or(all),
                    right: side("right").unwrap_or(all),
                    bottom: side("bottom").unwrap_or(all),
                    left: side("left").unwrap_or(all),
                })
            }
            J::String(s) => Value::Gaps(parse_gaps(s).ok_or_else(bad)?),
            _ => Value::Gaps(Gaps::uniform(num(v).ok_or_else(bad)?.round() as i64)),
        },
        Kind::Vec2 => match v.as_array().map(|a| a.iter().map(num).collect::<Option<Vec<_>>>()) {
            Some(Some(a)) if a.len() == 2 => Value::Vec2([a[0], a[1]]),
            _ => return Err(bad()),
        },
    };
    if let (Some(x), Some(lo), Some(hi)) = (value.as_f64(), info.min, info.max)
        && matches!(info.kind, Kind::Int | Kind::Float)
        && !(lo..=hi).contains(&x)
    {
        return Err(format!("{} must be between {lo} and {hi}", info.name));
    }
    Ok(value)
}

/// What to pass for an option, for error messages and the MCP schema text.
pub fn kind_hint(info: &OptionInfo) -> String {
    match info.kind {
        Kind::Bool => "true or false".into(),
        Kind::Int if !info.choices.is_empty() => {
            let c: Vec<String> = info.choices.iter().map(|(n, l)| format!("{n} ({l})")).collect();
            format!("one of {}", c.join(", "))
        }
        Kind::Int => "an integer".into(),
        Kind::Float => "a number".into(),
        Kind::Str => "a string".into(),
        Kind::Color => "a colour like \"#rrggbb\" or \"rgba(rrggbbaa)\"".into(),
        Kind::Gradient => "a colour, a list of colours or {\"colors\": [...], \"angle\": degrees}".into(),
        Kind::Gaps => "pixels for all sides or {\"top\", \"right\", \"bottom\", \"left\"}".into(),
        Kind::Vec2 => "[x, y]".into(),
    }
}

/// JSON form of a value for the MCP server (colours as `#rrggbbaa`).
pub fn value_to_json(v: &Value) -> serde_json::Value {
    let hex = |c: u32| format!("#{:06x}{:02x}", c & 0x00ff_ffff, c >> 24);
    match v {
        Value::Bool(b) => (*b).into(),
        Value::Int(i) => (*i).into(),
        Value::Float(f) => (*f).into(),
        Value::Str(s) => s.clone().into(),
        Value::Color(c) => hex(*c).into(),
        Value::Gradient(g) => serde_json::json!({ "colors": g.colors.iter().map(|c| hex(*c)).collect::<Vec<_>>(), "angle": g.angle }),
        Value::Gaps(g) => serde_json::json!({ "top": g.top, "right": g.right, "bottom": g.bottom, "left": g.left }),
        Value::Vec2([x, y]) => serde_json::json!([x, y]),
    }
}

#[cfg(test)]
mod tests {
    /// Runs `apply_if_loaded(path)` in a real Lua with `dofile` stubbed;
    /// None when no `lua` is installed.
    fn run_apply(path: &str, prelude: &str, state_home: &str) -> Option<String> {
        crate::paths::find_executable("lua")?;
        let code = format!("dofile = function(p) io.write(\"LOADED \" .. p) end {prelude} {}", super::apply_if_loaded(path));
        let out = std::process::Command::new("lua")
            .args(["-e", &code])
            .env("XDG_STATE_HOME", state_home)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    #[test]
    fn applies_the_file_vela_lua_loaded() {
        let Some(out) = run_apply("/repo/vela/hyprland.lua", "vela_settings_file = \"/repo/vela/hyprland.lua\"", "/s") else {
            return;
        };
        assert_eq!(out, "LOADED /repo/vela/hyprland.lua");
    }

    #[test]
    fn leaves_hyprland_alone_for_another_file() {
        let Some(out) = run_apply("/tmp/test/hyprland.lua", "vela_settings_file = \"/repo/vela/hyprland.lua\"", "/s") else {
            return;
        };
        assert_eq!(out, "");
    }

    #[test]
    fn without_a_recorded_file_applies_the_default_state_path() {
        let Some(out) = run_apply("/s/vela/hyprland.lua", "", "/s") else {
            return;
        };
        assert_eq!(out, "LOADED /s/vela/hyprland.lua");
        assert_eq!(run_apply("/repo/vela/hyprland.lua", "", "/s").unwrap(), "");
    }

    use super::*;

    const DESCRIPTIONS: &str = r#"[
        {"name": "general:border_size", "description": "size of the border", "default": 1, "current": 1, "min": 0, "max": 20, "map": null},
        {"name": "general:gaps_out", "description": "gaps to monitor edges", "default": "20 20 20 20", "current": "20 20 20 20", "min": null, "max": null},
        {"name": "general:col.active_border", "description": "border colour", "default": "ffffffff 0deg"},
        {"name": "decoration:blur:noise", "description": "noise", "default": 0.0117, "min": 0, "max": 1},
        {"name": "decoration:active_opacity", "description": "opacity", "default": 1, "min": 0, "max": 1},
        {"name": "input:follow_mouse", "description": "focus follows mouse", "default": 1, "map": [{"separate": 3}, {"detached": 2}, {"follow": 1}, {"disabled": 0}]},
        {"name": "input:kb_variant", "description": "variant", "default": "[[EMPTY]]"},
        {"name": "misc:col.splash", "description": "splash colour", "default": "55ffffff"},
        {"name": "decoration:shadow:offset", "description": "offset", "default": [0, 0]},
        {"name": "group:groupbar:font_weight_active", "description": "weight", "default": "400"}
    ]"#;

    fn getoptions() -> Vec<Option<serde_json::Value>> {
        let batch = [
            r#"{"option": "general:border_size", "int": 2, "set": true }"#,
            r#"{"option": "general:gaps_out", "css": "8 0 8 4", "set": true }"#,
            r#"{"option": "general:col.active_border", "gradient": "b3ffffff 0deg", "set": true }"#,
            r#"{"option": "decoration:blur:noise", "float": 0.015000, "set": true }"#,
            r#"{"option": "decoration:active_opacity", "float": 1.000000, "set": true }"#,
            r#"{"option": "input:follow_mouse", "int": 1, "set": true }"#,
            r#"{"option": "input:kb_variant", "str": "", "set": true }"#,
            r#"{"option": "misc:col.splash", "int": 1442840575, "set": false }"#,
            r#"{"option": "decoration:shadow:offset", "vec2": [0,0], "set": true }"#,
            "invalid type (internal error)",
        ]
        .join("\n\n\n");
        split_batch(&batch, 10)
    }

    #[test]
    fn catalogue_takes_kinds_from_getoption_and_limits_from_descriptions() {
        let (infos, current) = catalogue(DESCRIPTIONS, &getoptions());
        assert_eq!(infos.len(), 10);
        let kinds: Vec<Kind> = infos.iter().map(|i| i.kind).collect();
        {
            use Kind::*;
            assert_eq!(kinds, vec![Int, Gaps, Gradient, Float, Float, Int, Str, Color, Vec2, Str]);
        }
        assert_eq!((infos[0].min, infos[0].max), (Some(0.0), Some(20.0)));
        assert_eq!(infos[0].default, Some(Value::Int(1)));
        assert_eq!(infos[1].default, Some(Value::Gaps(Gaps::uniform(20))));
        assert_eq!(infos[4].default, Some(Value::Float(1.0)));
        assert_eq!(infos[5].choices[0], (0, "disabled".into()));
        assert_eq!(infos[5].choices.len(), 4);
        assert_eq!(infos[6].default, Some(Value::Str(String::new())));
        assert_eq!(infos[7].default, Some(Value::Color(0x55ff_ffff)));
        assert_eq!(infos[3].section(), "decoration:blur");
        assert_eq!(infos[3].key(), "noise");

        assert_eq!(current["general:border_size"], Value::Int(2));
        assert_eq!(
            current["general:gaps_out"],
            Value::Gaps(Gaps {
                top: 8,
                right: 0,
                bottom: 8,
                left: 4
            })
        );
        assert_eq!(
            current["general:col.active_border"],
            Value::Gradient(Gradient {
                colors: vec![0xb3ff_ffff],
                angle: 0
            })
        );
        assert_eq!(current["misc:col.splash"], Value::Color(1442840575));
        assert!(!current.contains_key("group:groupbar:font_weight_active"));
    }

    #[test]
    fn colours_convert_between_hyprland_forms() {
        assert_eq!(rgba(0xb3ff_ffff), "rgba(ffffffb3)");
        assert_eq!(parse_rgba("rgba(ffffffb3)"), Some(0xb3ff_ffff));
        assert_eq!(parse_rgba("rgba(fff)"), None);
        assert_eq!(parse_rgba("#ffffff"), None);
        assert_eq!(
            parse_gradient("ff33ccee ff00ff99 45deg"),
            Some(Gradient {
                colors: vec![0xff33_ccee, 0xff00_ff99],
                angle: 45
            })
        );
        assert_eq!(parse_gradient("0deg"), None);
    }

    #[test]
    fn gaps_parse_like_css() {
        assert_eq!(parse_gaps("5"), Some(Gaps::uniform(5)));
        assert_eq!(
            parse_gaps("1 2 3"),
            Some(Gaps {
                top: 1,
                right: 2,
                bottom: 3,
                left: 2
            })
        );
        assert_eq!(parse_gaps("a b"), None);
    }

    #[test]
    fn lua_sets_nested_keys() {
        assert_eq!(
            eval_code("decoration:blur:size", &Value::Int(10)),
            "hl.config({ decoration = { blur = { size = 10 } } })"
        );
        assert_eq!(
            lua_table(
                "general:col.active_border",
                &Value::Gradient(Gradient {
                    colors: vec![0xb3ff_ffff],
                    angle: 0
                })
            ),
            r#"{ general = { col = { active_border = "rgba(ffffffb3)" } } }"#
        );
        assert_eq!(
            Value::Gradient(Gradient {
                colors: vec![0xff00_00ff, 0x8000_ff00],
                angle: 45
            })
            .to_lua(),
            r#"{ colors = { "rgba(0000ffff)", "rgba(00ff0080)" }, angle = 45 }"#
        );
        assert_eq!(lua_table("input-capture:enabled", &Value::Bool(true)), "{ input_capture = { enabled = true } }");
        assert_eq!(
            lua_table("input:touchpad:tap-to-click", &Value::Bool(true)),
            "{ input = { touchpad = { tap_to_click = true } } }"
        );
        assert_eq!(Value::Float(1.0).to_lua(), "1.0");
        assert_eq!(Value::Float(0.015).to_lua(), "0.015");
        assert_eq!(Value::Str("a\"b\\c\n".into()).to_lua(), r#""a\"b\\c\n""#);
        assert_eq!(Value::Gaps(Gaps::uniform(8)).to_lua(), "{ top = 8, right = 8, bottom = 8, left = 8 }");
        assert_eq!(Value::Vec2([0.0, 2.5]).to_lua(), "{ 0.0, 2.5 }");
    }

    #[test]
    fn overrides_roundtrip_through_toml() {
        let mut o = Overrides::default();
        o.options.insert("general:gaps_out".into(), Value::Gaps(Gaps::uniform(8)));
        o.options.insert("general:border_size".into(), Value::Int(3));
        o.options.insert("decoration:active_opacity".into(), Value::Float(1.0));
        o.options.insert("decoration:blur:enabled".into(), Value::Bool(false));
        o.options.insert("input:kb_layout".into(), Value::Str("de".into()));
        o.options.insert("misc:col.splash".into(), Value::Color(0x55ff_ffff));
        o.options.insert(
            "general:col.active_border".into(),
            Value::Gradient(Gradient {
                colors: vec![0xff33_ccee, 0xff00_ff99],
                angle: 45,
            }),
        );
        o.options.insert("decoration:shadow:offset".into(), Value::Vec2([1.0, 2.0]));
        o.remember.insert("edge_gap".into(), 12);
        o.extras.unbind.push("SUPER + M".into());
        o.extras.autostart.push(crate::hyprextra::Autostart {
            command: "nm-applet".into(),
            enabled: true,
        });
        o.monitors.insert(
            "desc:HP Inc. HP E243i".into(),
            crate::hyprmon::MonitorRule {
                output: "desc:HP Inc. HP E243i".into(),
                mode: "1920x1200@59.95".into(),
                position: (1920, 0),
                scale: 1.0,
                transform: 0,
                vrr: Some(1),
                disabled: false,
            },
        );
        o.animations.insert(
            "windowsIn".into(),
            crate::hypranim::Anim {
                enabled: true,
                speed: 4.1,
                curve: "spring:easy".into(),
                style: "popin 87%".into(),
            },
        );
        let text = o.to_toml();
        assert!(text.starts_with("# Hyprland settings"));
        assert_eq!(Overrides::from_toml(&text).unwrap(), o);
        assert_eq!(Overrides::from_toml("").unwrap(), Overrides::default());
        assert!(Overrides::from_toml("options = [").is_err());
    }

    #[test]
    fn generated_lua_protects_every_option() {
        let mut o = Overrides::default();
        o.options.insert("decoration:rounding".into(), Value::Int(0));
        o.options.insert("general:gaps_in".into(), Value::Gaps(Gaps::uniform(2)));
        o.animations.insert(
            "fade".into(),
            crate::hypranim::Anim {
                enabled: false,
                speed: 1.0,
                curve: String::new(),
                style: String::new(),
            },
        );
        let lua = o.to_lua();
        assert!(lua.contains("pcall(f, ...)"));
        assert!(lua.contains(r#"try(hl.curve, "velaSmooth", { type = "bezier""#));
        assert!(lua.contains(r#"try(hl.animation, { leaf = "fade", enabled = false })"#));
        // Curves before the animations that may use them.
        assert!(lua.find("hl.curve").unwrap() < lua.find("hl.animation").unwrap());
        assert!(lua.contains("set({ decoration = { rounding = 0 } })\n"));
        assert!(lua.contains("set({ general = { gaps_in = { top = 2, right = 2, bottom = 2, left = 2 } } })\n"));
    }

    #[test]
    fn values_coerce_into_the_option_kind() {
        assert_eq!(Value::Int(1).coerce(Kind::Float), Some(Value::Float(1.0)));
        assert_eq!(Value::Int(4).coerce(Kind::Gaps), Some(Value::Gaps(Gaps::uniform(4))));
        assert_eq!(
            Value::Color(1).coerce(Kind::Gradient),
            Some(Value::Gradient(Gradient { colors: vec![1], angle: 0 }))
        );
        assert_eq!(Value::Str("x".into()).coerce(Kind::Int), None);
        assert_eq!(Value::number(Kind::Int, 3.6), Some(Value::Int(4)));
    }

    #[test]
    fn input_values_follow_the_option_kind() {
        let (infos, _) = catalogue(DESCRIPTIONS, &getoptions());
        let by = |n: &str| infos.iter().find(|i| i.name == n).unwrap().clone();
        let j = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
        assert_eq!(value_from_input(&by("general:border_size"), &j("3")), Ok(Value::Int(3)));
        assert!(value_from_input(&by("general:border_size"), &j("99")).unwrap_err().contains("between 0 and 20"));
        assert_eq!(value_from_input(&by("input:follow_mouse"), &j(r#""Detached""#)), Ok(Value::Int(2)));
        assert_eq!(value_from_input(&by("general:gaps_out"), &j("6")), Ok(Value::Gaps(Gaps::uniform(6))));
        assert_eq!(
            value_from_input(&by("general:gaps_out"), &j(r#"{"all": 5, "top": 0}"#)),
            Ok(Value::Gaps(Gaps {
                top: 0,
                right: 5,
                bottom: 5,
                left: 5
            }))
        );
        assert_eq!(
            value_from_input(
                &by("general:col.active_border"),
                &j(r##"{"colors": ["#ff0000", "rgba(00ff0080)"], "angle": 45}"##)
            ),
            Ok(Value::Gradient(Gradient {
                colors: vec![0xffff_0000, 0x8000_ff00],
                angle: 45
            }))
        );
        assert_eq!(value_from_input(&by("misc:col.splash"), &j(r##""#11223344""##)), Ok(Value::Color(0x4411_2233)));
        assert_eq!(value_from_input(&by("decoration:active_opacity"), &j(r#""0.9""#)), Ok(Value::Float(0.9)));
        assert!(value_from_input(&by("decoration:shadow:offset"), &j("[1]")).is_err());
        assert_eq!(value_to_json(&Value::Color(0x4411_2233)), j(r##""#11223344""##));
        assert_eq!(parse_color("rgb(ff0000)"), Some(0xffff_0000));
        assert_eq!(parse_color("0x80ffffff"), Some(0x80ff_ffff));
    }

    #[test]
    fn edge_gaps_switch_single_edges() {
        let g = Gaps {
            top: 8,
            right: 0,
            bottom: 8,
            left: 8,
        };
        let e = EdgeGaps::from_gaps(g, 20);
        assert_eq!(
            e,
            EdgeGaps {
                width: 8,
                top: true,
                right: false,
                bottom: true,
                left: true
            }
        );
        assert_eq!(e.to_gaps(), g);
        let wider = EdgeGaps { width: 14, ..e }.to_gaps();
        assert_eq!((wider.top, wider.right), (14, 0));
        // All off: the remembered width comes back when an edge is switched on.
        let off = EdgeGaps::from_gaps(Gaps::uniform(0), 12);
        assert!(!off.any());
        assert_eq!(EdgeGaps { top: true, ..off }.to_gaps().top, 12);
    }
}
