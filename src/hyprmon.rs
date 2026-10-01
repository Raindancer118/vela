//! Monitors for the settings: what `j/monitors all` reports, full monitor
//! rules (`hl.monitor`; a partial rule would reset the rest), valid scales
//! and the arrangement maths for dragging monitors around.

use crate::hyprconf::lua_string;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub width: i32,
    pub height: i32,
    pub refresh: f64,
}

impl Mode {
    /// `1920x1200@59.95` as Hyprland takes it.
    pub fn spec(&self) -> String {
        let r = format!("{:.2}", self.refresh);
        let r = r.trim_end_matches('0').trim_end_matches('.');
        format!("{}x{}@{r}", self.width, self.height)
    }

    /// `1920x1200@59.95Hz` from availableModes.
    pub fn parse(s: &str) -> Option<Mode> {
        let (size, rate) = s.trim().trim_end_matches("Hz").split_once('@')?;
        let (w, h) = size.split_once('x')?;
        Some(Mode {
            width: w.parse().ok()?,
            height: h.parse().ok()?,
            refresh: rate.parse().ok()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    pub name: String,
    pub description: String,
    pub make: String,
    pub model: String,
    pub mode: Mode,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub transform: i64,
    pub vrr: bool,
    pub disabled: bool,
    pub modes: Vec<Mode>,
}

impl MonitorInfo {
    /// Rule target that survives a different connector (dock, adapter).
    pub fn output(&self) -> String {
        if self.description.is_empty() {
            self.name.clone()
        } else {
            format!("desc:{}", self.description)
        }
    }

    pub fn title(&self) -> String {
        let model = format!("{} {}", self.make, self.model).trim().to_owned();
        if model.is_empty() {
            self.name.clone()
        } else {
            format!("{model} ({})", self.name)
        }
    }

    /// Size on the layout: scale and rotation applied.
    pub fn logical_size(&self) -> (i32, i32) {
        logical_size(self.mode.width, self.mode.height, self.scale, self.transform)
    }

    /// The rule that keeps everything as it is now.
    pub fn current_rule(&self) -> MonitorRule {
        MonitorRule {
            output: self.output(),
            mode: self.mode.spec(),
            position: (self.x, self.y),
            scale: self.scale,
            transform: self.transform,
            vrr: None,
            disabled: self.disabled,
        }
    }
}

/// The rule to change for a monitor: vela's saved one if any, else what it
/// does now; the position always as Hyprland placed it.
pub fn rule_for(saved: Option<&MonitorRule>, m: &MonitorInfo) -> MonitorRule {
    let mut r = saved.cloned().unwrap_or_else(|| m.current_rule());
    if !m.disabled {
        r.position = (m.x, m.y);
    }
    r
}

pub fn logical_size(w: i32, h: i32, scale: f64, transform: i64) -> (i32, i32) {
    let s = if scale > 0.0 { scale } else { 1.0 };
    let (w, h) = ((f64::from(w) / s).round() as i32, (f64::from(h) / s).round() as i32);
    if transform % 2 == 1 { (h, w) } else { (w, h) }
}

pub fn parse_monitors(json: &str) -> Vec<MonitorInfo> {
    let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|m| {
            let s = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or_default().trim().to_owned();
            let int = |k: &str| m.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
            let mode = Mode {
                width: int("width") as i32,
                height: int("height") as i32,
                refresh: m.get("refreshRate").and_then(|v| v.as_f64()).unwrap_or(60.0),
            };
            Some(MonitorInfo {
                name: m.get("name")?.as_str()?.to_owned(),
                description: s("description"),
                make: s("make"),
                model: s("model"),
                mode,
                x: int("x") as i32,
                y: int("y") as i32,
                scale: m.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0),
                transform: int("transform"),
                vrr: m.get("vrr").and_then(|v| v.as_bool()).unwrap_or(false),
                disabled: m.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false),
                modes: m
                    .get("availableModes")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| Mode::parse(v.as_str()?)).collect())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// A complete `hl.monitor` rule.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorRule {
    pub output: String,
    pub mode: String,
    pub position: (i32, i32),
    pub scale: f64,
    pub transform: i64,
    /// None: the global `misc:vrr` decides.
    pub vrr: Option<i64>,
    pub disabled: bool,
}

impl MonitorRule {
    pub fn to_lua(&self) -> String {
        if self.disabled {
            return format!("{{ output = {}, disabled = true }}", lua_string(&self.output));
        }
        let mut out = format!(
            "{{ output = {}, mode = {}, position = {}, scale = {}, transform = {}",
            lua_string(&self.output),
            lua_string(&self.mode),
            lua_string(&format!("{}x{}", self.position.0, self.position.1)),
            fmt_scale(self.scale),
            self.transform
        );
        if let Some(v) = self.vrr {
            out.push_str(&format!(", vrr = {v}"));
        }
        out.push_str(" }");
        out
    }

    pub fn eval_code(&self) -> String {
        format!("hl.monitor({})", self.to_lua())
    }

    pub fn to_toml(&self) -> toml::Value {
        let mut t = toml::Table::new();
        t.insert("mode".into(), toml::Value::String(self.mode.clone()));
        t.insert("position".into(), toml::Value::Array(vec![self.position.0.into(), self.position.1.into()]));
        t.insert("scale".into(), toml::Value::Float(self.scale));
        t.insert("transform".into(), toml::Value::Integer(self.transform));
        if let Some(v) = self.vrr {
            t.insert("vrr".into(), toml::Value::Integer(v));
        }
        t.insert("disabled".into(), toml::Value::Boolean(self.disabled));
        toml::Value::Table(t)
    }

    pub fn from_toml(output: &str, v: &toml::Value) -> Option<MonitorRule> {
        let t = v.as_table()?;
        let pos = t.get("position").and_then(|p| p.as_array());
        let coord = |i: usize| pos.and_then(|p| p.get(i)).and_then(|v| v.as_integer()).unwrap_or(0) as i32;
        Some(MonitorRule {
            output: output.to_owned(),
            mode: t.get("mode")?.as_str()?.to_owned(),
            position: (coord(0), coord(1)),
            scale: t
                .get("scale")
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                .unwrap_or(1.0),
            transform: t.get("transform").and_then(|v| v.as_integer()).unwrap_or(0),
            vrr: t.get("vrr").and_then(|v| v.as_integer()),
            disabled: t.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false),
        })
    }
}

fn fmt_scale(s: f64) -> String {
    let v = format!("{s:.6}");
    let v = v.trim_end_matches('0');
    if v.ends_with('.') { format!("{v}0") } else { v.to_owned() }
}

/// Scales that give a whole-pixel layout size for this mode (Hyprland
/// corrects others and complains), plus the current one.
pub fn valid_scales(width: i32, height: i32, current: f64) -> Vec<f64> {
    const CANDIDATES: [f64; 16] = [
        1.0, 1.066_667, 1.2, 1.25, 1.333_333, 1.5, 1.6, 1.666_667, 1.75, 1.8, 2.0, 2.25, 2.4, 2.5, 2.666_667, 3.0,
    ];
    let whole = |v: f64| (v - v.round()).abs() < 0.01;
    let mut out: Vec<f64> = CANDIDATES
        .into_iter()
        .filter(|s| whole(f64::from(width) / s) && whole(f64::from(height) / s))
        .collect();
    if !out.iter().any(|s| (s - current).abs() < 0.001) {
        out.push(current);
        out.sort_by(f64::total_cmp);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

/// Where a dragged monitor lands: its edges snap to the edges of the others
/// within `threshold` layout pixels, and it never overlaps one.
pub fn snap(rects: &[Rect], moving: usize, x: i32, y: i32, threshold: i32) -> (i32, i32) {
    let me = rects[moving];
    let others: Vec<&Rect> = rects.iter().enumerate().filter(|(i, _)| *i != moving).map(|(_, r)| r).collect();
    let best = |pos: i32, size: i32, edges: &mut dyn Iterator<Item = i32>| {
        edges
            .flat_map(|e| [e, e - size])
            .filter(|c| (c - pos).abs() <= threshold)
            .min_by_key(|c| (c - pos).abs())
            .unwrap_or(pos)
    };
    let sx = best(x, me.w, &mut others.iter().flat_map(|o| [o.x, o.x + o.w]));
    let sy = best(y, me.h, &mut others.iter().flat_map(|o| [o.y, o.y + o.h]));
    let placed = Rect { x: sx, y: sy, ..me };
    if !others.iter().any(|o| o.overlaps(&placed)) {
        return (sx, sy);
    }
    // Overlapping: push it out on the side that needs the shortest move.
    let mut candidates = Vec::new();
    for o in &others {
        candidates.push((o.x - me.w, sy));
        candidates.push((o.x + o.w, sy));
        candidates.push((sx, o.y - me.h));
        candidates.push((sx, o.y + o.h));
    }
    candidates
        .into_iter()
        .filter(|(cx, cy)| !others.iter().any(|o| o.overlaps(&Rect { x: *cx, y: *cy, ..me })))
        .min_by_key(|(cx, cy)| (cx - x).abs() + (cy - y).abs())
        .unwrap_or((me.x, me.y))
}

/// Shifts the layout so its top-left corner is at 0,0.
pub fn normalize(rects: &mut [Rect]) {
    let (Some(min_x), Some(min_y)) = (rects.iter().map(|r| r.x).min(), rects.iter().map(|r| r.y).min()) else {
        return;
    };
    for r in rects {
        r.x -= min_x;
        r.y -= min_y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"[
        {"id":0,"name":"DP-6","description":"HP Inc. HP E243i 6CM8430N46","make":"HP Inc.","model":"HP E243i","width":1920,"height":1200,"refreshRate":59.95,"x":0,"y":0,"scale":1.0,"transform":0,"vrr":false,"disabled":false,
         "availableModes":["1920x1200@59.95Hz","1920x1080@60.00Hz","1280x720@50.00Hz"]},
        {"id":2,"name":"eDP-1","description":"Chimei Innolux Corporation 0x150C","make":"Chimei Innolux Corporation","model":"0x150C","width":1920,"height":1080,"refreshRate":60.001,"x":3840,"y":0,"scale":1.25,"transform":0,"vrr":false,"disabled":false,
         "availableModes":["1920x1080@60.00Hz"]}
    ]"#;

    #[test]
    fn parses_monitors_with_modes() {
        let m = parse_monitors(JSON);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].modes.len(), 3);
        assert_eq!(
            m[0].modes[1],
            Mode {
                width: 1920,
                height: 1080,
                refresh: 60.0
            }
        );
        assert_eq!(m[0].output(), "desc:HP Inc. HP E243i 6CM8430N46");
        assert_eq!(m[0].title(), "HP Inc. HP E243i (DP-6)");
        assert_eq!(m[1].logical_size(), (1536, 864));
        assert!(parse_monitors("nope").is_empty());
    }

    #[test]
    fn mode_specs_are_short() {
        assert_eq!(
            Mode {
                width: 1920,
                height: 1200,
                refresh: 59.95
            }
            .spec(),
            "1920x1200@59.95"
        );
        assert_eq!(
            Mode {
                width: 2560,
                height: 1440,
                refresh: 144.0
            }
            .spec(),
            "2560x1440@144"
        );
        assert_eq!(
            Mode {
                width: 1920,
                height: 1080,
                refresh: 60.001
            }
            .spec(),
            "1920x1080@60"
        );
        assert_eq!(
            Mode::parse("1680x1050@59.95Hz"),
            Some(Mode {
                width: 1680,
                height: 1050,
                refresh: 59.95
            })
        );
        assert_eq!(Mode::parse("junk"), None);
    }

    #[test]
    fn rules_are_complete_and_roundtrip() {
        let m = &parse_monitors(JSON)[1];
        let mut r = m.current_rule();
        assert_eq!(
            r.to_lua(),
            r#"{ output = "desc:Chimei Innolux Corporation 0x150C", mode = "1920x1080@60", position = "3840x0", scale = 1.25, transform = 0 }"#
        );
        r.vrr = Some(2);
        assert!(r.eval_code().ends_with(", vrr = 2 })"));
        assert_eq!(MonitorRule::from_toml(&r.output, &r.to_toml()), Some(r.clone()));
        r.disabled = true;
        assert_eq!(r.to_lua(), r#"{ output = "desc:Chimei Innolux Corporation 0x150C", disabled = true }"#);
        assert_eq!(fmt_scale(1.0), "1.0");
        assert_eq!(fmt_scale(1.333333), "1.333333");
    }

    #[test]
    fn scales_give_whole_pixels() {
        let s = valid_scales(1920, 1080, 1.25);
        for want in [1.0, 1.2, 1.25, 1.5, 2.0] {
            assert!(s.contains(&want), "{want} missing in {s:?}");
        }
        assert!(!s.contains(&1.75), "1920/1.75 isn't whole");
        // An odd current scale stays selectable.
        assert!(valid_scales(1920, 1080, 1.1).contains(&1.1));
    }

    #[test]
    fn dragged_monitors_snap_to_edges_and_never_overlap() {
        let rects = [
            Rect { x: 0, y: 0, w: 1920, h: 1200 },
            Rect {
                x: 1920,
                y: 0,
                w: 1536,
                h: 864,
            },
        ];
        // Dropped a little off the right edge: snaps flush, top aligned.
        assert_eq!(snap(&rects, 1, 1950, 30, 60), (1920, 0));
        // Dropped onto the other monitor: pushed out.
        let (x, y) = snap(&rects, 1, 1000, 100, 60);
        let placed = Rect { x, y, w: 1536, h: 864 };
        assert!(!placed.overlaps(&rects[0]), "{placed:?}");
        // Bottom edges align too.
        assert_eq!(snap(&rects, 1, 1920, 1200 - 864 - 20, 60), (1920, 1200 - 864));
        let mut r = vec![Rect { x: -100, y: 50, w: 10, h: 10 }, Rect { x: 0, y: 0, w: 10, h: 10 }];
        normalize(&mut r);
        assert_eq!((r[0].x, r[0].y, r[1].x, r[1].y), (0, 50, 100, 0));
    }
}
