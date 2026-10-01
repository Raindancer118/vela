//! Hyprland animations (`hl.animation`): what `j/animations` reports, the
//! inheritance between them, vela's own curves and the Lua for overrides.

use crate::hyprconf::lua_string;
use std::collections::BTreeMap;

/// One animation as vela overrides it. `curve` is a bezier name or
/// `spring:<name>`, like Hyprland reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Anim {
    pub enabled: bool,
    /// Hyprland's unit: 1 = 100 ms.
    pub speed: f64,
    pub curve: String,
    pub style: String,
}

impl Anim {
    pub fn to_lua(&self, leaf: &str) -> String {
        let mut out = format!("{{ leaf = {}, enabled = {}", lua_string(leaf), self.enabled);
        if self.enabled {
            out.push_str(&format!(", speed = {:?}", if self.speed.is_finite() { self.speed.max(0.01) } else { 1.0 }));
            match self.curve.strip_prefix("spring:") {
                Some(spring) => out.push_str(&format!(", spring = {}", lua_string(spring))),
                None if !self.curve.is_empty() => out.push_str(&format!(", bezier = {}", lua_string(&self.curve))),
                None => {}
            }
            if !self.style.is_empty() {
                out.push_str(&format!(", style = {}", lua_string(&self.style)));
            }
        }
        out.push_str(" }");
        out
    }

    pub fn eval_code(&self, leaf: &str) -> String {
        format!("hl.animation({})", self.to_lua(leaf))
    }

    pub fn to_toml(&self) -> toml::Value {
        let mut t = toml::Table::new();
        t.insert("enabled".into(), toml::Value::Boolean(self.enabled));
        t.insert("speed".into(), toml::Value::Float(self.speed));
        t.insert("curve".into(), toml::Value::String(self.curve.clone()));
        t.insert("style".into(), toml::Value::String(self.style.clone()));
        toml::Value::Table(t)
    }

    pub fn from_toml(v: &toml::Value) -> Option<Anim> {
        let t = v.as_table()?;
        let s = |k: &str| t.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        Some(Anim {
            enabled: t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
            speed: t
                .get("speed")
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                .unwrap_or(1.0),
            curve: s("curve"),
            style: s("style"),
        })
    }
}

/// Parent of an animation in Hyprland's tree; settings it doesn't set itself
/// come from there.
pub fn parent(leaf: &str) -> Option<&'static str> {
    Some(match leaf {
        "global" => return None,
        "windowsIn" | "windowsOut" | "windowsMove" => "windows",
        "layersIn" | "layersOut" => "layers",
        "fadeLayersIn" | "fadeLayersOut" => "fadeLayers",
        "fadePopupsIn" | "fadePopupsOut" => "fadePopups",
        "fadeIn" | "fadeOut" | "fadeSwitch" | "fadeShadow" | "fadeDim" | "fadeLayers" | "fadePopups" | "fadeDpms" | "fadeGlow" => "fade",
        "workspacesIn" | "workspacesOut" | "specialWorkspace" => "workspaces",
        "specialWorkspaceIn" | "specialWorkspaceOut" => "specialWorkspace",
        _ => "global",
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimState {
    /// Set on this leaf (in hyprland.lua or by vela), not inherited.
    pub overridden: bool,
    pub anim: Anim,
}

/// The animations and curve names from `j/animations`: a list of
/// animations and a list of beziers. Springs only show up as
/// `spring:<name>` on animations that use them.
pub fn parse(json: &str) -> (BTreeMap<String, AnimState>, Vec<String>) {
    let mut anims = BTreeMap::new();
    let mut curves = Vec::new();
    let Ok(serde_json::Value::Array(parts)) = serde_json::from_str::<serde_json::Value>(json) else {
        return (anims, curves);
    };
    for a in parts.first().and_then(|a| a.as_array()).into_iter().flatten() {
        let Some(name) = a.get("name").and_then(|n| n.as_str()) else { continue };
        let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        let curve = s("bezier");
        if curve.starts_with("spring:") && !curves.contains(&curve) {
            curves.push(curve.clone());
        }
        anims.insert(
            name.to_owned(),
            AnimState {
                overridden: a.get("overridden").and_then(|v| v.as_bool()).unwrap_or(false),
                anim: Anim {
                    enabled: a.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
                    speed: a.get("speed").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    curve,
                    style: s("style"),
                },
            },
        );
    }
    for b in parts.get(1).and_then(|b| b.as_array()).into_iter().flatten() {
        if let Some(name) = b.get("name").and_then(|n| n.as_str())
            && !curves.iter().any(|c| c == name)
        {
            curves.push(name.to_owned());
        }
    }
    curves.sort_by_key(|c| (c.starts_with("spring:"), c.to_lowercase()));
    (anims, curves)
}

/// What a leaf actually does: its own settings if it has any, else the
/// nearest ancestor's.
pub fn effective(anims: &BTreeMap<String, AnimState>, leaf: &str) -> Option<Anim> {
    let mut cur = leaf;
    loop {
        if let Some(s) = anims.get(cur)
            && (s.overridden || cur == "global")
        {
            return Some(s.anim.clone());
        }
        cur = parent(cur)?;
    }
}

/// Curves vela always defines, so they can be picked in the settings.
pub const VELA_CURVES: [(&str, &str); 5] = [
    ("velaSmooth", r#"{ type = "bezier", points = { {0.25, 0.1}, {0.25, 1} } }"#),
    ("velaSnappy", r#"{ type = "bezier", points = { {0.05, 0.9}, {0.1, 1.0} } }"#),
    ("velaOvershoot", r#"{ type = "bezier", points = { {0.34, 1.56}, {0.64, 1} } }"#),
    ("velaSpring", r#"{ type = "spring", mass = 1, stiffness = 210, dampening = 17 }"#),
    ("velaSoftSpring", r#"{ type = "spring", mass = 1, stiffness = 120, dampening = 16 }"#),
];

/// Lua lines that define vela's curves (protected like every override).
pub fn curves_lua() -> Vec<String> {
    VELA_CURVES.iter().map(|(name, def)| format!("hl.curve({}, {def})", lua_string(name))).collect()
}

/// How vela's curves appear in the curve list.
pub fn vela_curve_names() -> Vec<String> {
    VELA_CURVES
        .iter()
        .map(|(name, def)| {
            if def.contains("spring") {
                format!("spring:{name}")
            } else {
                (*name).to_owned()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"[[
        {"name": "global", "overridden": true, "bezier": "default", "enabled": true, "speed": 10.0, "style": ""},
        {"name": "windows", "overridden": true, "bezier": "spring:easy", "enabled": true, "speed": 4.79, "style": ""},
        {"name": "windowsIn", "overridden": true, "bezier": "spring:easy", "enabled": true, "speed": 4.1, "style": "popin 87%"},
        {"name": "windowsMove", "overridden": false, "bezier": "", "enabled": true, "speed": 0.0, "style": ""},
        {"name": "fadeLayersIn", "overridden": false, "bezier": "", "enabled": true, "speed": 0.0, "style": ""},
        {"name": "fade", "overridden": true, "bezier": "quick", "enabled": false, "speed": 3.03, "style": ""}
    ], [
        {"name": "quick", "X0": 0.15, "Y0": 0.0, "X1": 0.1, "Y1": 1.0},
        {"name": "default", "X0": 0.0, "Y0": 0.75, "X1": 0.15, "Y1": 1.0}
    ]]"#;

    #[test]
    fn parses_animations_and_curves() {
        let (anims, curves) = parse(JSON);
        assert_eq!(anims.len(), 6);
        assert_eq!(anims["windowsIn"].anim.style, "popin 87%");
        assert!(!anims["windowsMove"].overridden);
        assert_eq!(curves, vec!["default", "quick", "spring:easy"]);
        assert_eq!(parse("nonsense"), (BTreeMap::new(), Vec::new()));
    }

    #[test]
    fn unset_leaves_inherit_from_their_parents() {
        let (anims, _) = parse(JSON);
        assert_eq!(effective(&anims, "windowsMove").unwrap().speed, 4.79);
        // fadeLayersIn → fadeLayers (unknown) → fade
        let f = effective(&anims, "fadeLayersIn").unwrap();
        assert!(!f.enabled);
        assert_eq!(f.curve, "quick");
        // Unknown leaves fall back to global.
        assert_eq!(effective(&anims, "zoomFactor").unwrap().speed, 10.0);
    }

    #[test]
    fn lua_uses_spring_or_bezier() {
        let a = Anim {
            enabled: true,
            speed: 4.1,
            curve: "spring:easy".into(),
            style: "popin 87%".into(),
        };
        assert_eq!(
            a.to_lua("windowsIn"),
            r#"{ leaf = "windowsIn", enabled = true, speed = 4.1, spring = "easy", style = "popin 87%" }"#
        );
        let b = Anim {
            curve: "quick".into(),
            style: String::new(),
            speed: 3.0,
            ..a.clone()
        };
        assert_eq!(
            b.eval_code("fade"),
            r#"hl.animation({ leaf = "fade", enabled = true, speed = 3.0, bezier = "quick" })"#
        );
        let off = Anim { enabled: false, ..a.clone() };
        assert_eq!(off.to_lua("fade"), r#"{ leaf = "fade", enabled = false }"#);
        assert_eq!(Anim::from_toml(&a.to_toml()), Some(a));
    }

    #[test]
    fn vela_curves_are_named_like_hyprland_reports_them() {
        let names = vela_curve_names();
        assert!(names.contains(&"velaSmooth".to_owned()));
        assert!(names.contains(&"spring:velaSpring".to_owned()));
        assert!(curves_lua()[0].starts_with(r#"hl.curve("velaSmooth", { type = "bezier""#));
    }
}
