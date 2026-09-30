//! CSS for the launcher, generated from the configuration so appearance
//! changes apply instantly by reloading a single CssProvider. All selectors
//! are scoped below `.vela-launcher` so the settings window keeps the
//! regular libadwaita look.

use crate::config::Config;
use crate::theme::palette;

fn rgba((r, g, b): (u8, u8, u8), a: f64) -> String {
    format!("rgba({r},{g},{b},{a:.3})")
}

pub fn launcher_css(cfg: &Config) -> String {
    let a = &cfg.appearance;
    let p = palette(a.theme);
    let accent = &a.accent;
    let radius = a.border_radius;
    let inner_radius = (f64::from(radius) * 0.6).round() as u32;
    let surface = a.surface_opacity;
    let bg = rgba(p.bg, cfg.general.opacity);
    let border = rgba(p.tint, 0.10);
    let fg = rgba(p.fg, 1.0);
    let dim = rgba(p.dim, 1.0);
    let tile_bg = rgba(p.tint, surface);
    let tile_hover = rgba(p.tint, (surface + 0.06).min(1.0));
    let chip_bg = rgba(p.tint, 0.07);
    let border_base = rgba(p.tint, 1.0);
    let claude = CLAUDE_ORANGE;
    let font = a.font_scale;
    let spacing = a.spacing;
    let backdrop_dim = a.backdrop_dim;

    format!(
        r#"
window.vela-launcher, window.vela-launcher.background {{ background: transparent; box-shadow: none; }}
window.vela-backdrop, window.vela-backdrop.background {{ background: rgba(0, 0, 0, {backdrop_dim}); box-shadow: none; }}
.vela-launcher .vela-panel {{
  background-color: {bg};
  color: {fg};
  border-radius: {radius}px;
  border: 1px solid {border};
  box-shadow: 0 10px 28px rgba(0,0,0,0.28);
  margin: 18px;
  font-size: {font:.3}em;
}}
.vela-launcher .vela-search {{ padding: 14px 16px 12px 14px; }}
.vela-launcher .vela-search-chip {{
  min-width: 38px; min-height: 38px; border-radius: 12px;
  background: {chip_bg}; color: {dim};
}}
.vela-launcher .vela-search.active .vela-search-chip {{ background: alpha({accent}, 0.16); color: {accent}; }}
.vela-launcher .vela-search.claude .vela-search-chip {{
  background: alpha({claude}, 0.18); color: {claude};
  box-shadow: inset 0 0 0 1px alpha({claude}, 0.40), 0 0 18px alpha({claude}, 0.28);
}}
.vela-launcher .vela-search-chip image {{ color: inherit; }}
.vela-launcher .vela-search.claude entry.vela-entry {{ caret-color: {claude}; }}
.vela-launcher .vela-mode-badge {{
  color: {claude}; background: alpha({claude}, 0.14); border-radius: 99px;
  padding: 3px 10px; margin-right: 6px; font-size: 0.8em; font-weight: 700; letter-spacing: 0.02em;
}}
.vela-launcher entry.vela-entry, .vela-launcher entry.vela-entry:focus-within {{
  background: transparent; border: none; box-shadow: none; outline: none;
  color: {fg}; font-size: 1.45em; min-height: 36px; padding: 0 4px; caret-color: {accent};
}}
.vela-launcher entry.vela-entry > text > placeholder {{ color: {dim}; }}
.vela-launcher .vela-divider {{ min-height: 1px; margin: 0 14px; background-image: linear-gradient(to right, alpha({border_base}, 0), {border} 18%, {border} 82%, alpha({border_base}, 0)); }}
.vela-launcher .vela-search.active + .vela-divider {{ background-image: linear-gradient(to right, alpha({accent}, 0), alpha({accent}, 0.45) 50%, alpha({accent}, 0)); }}
.vela-launcher .vela-search.claude + .vela-divider {{ background-image: linear-gradient(to right, alpha({claude}, 0), alpha({claude}, 0.55) 50%, alpha({claude}, 0)); }}
.vela-launcher .vela-content {{ padding: 4px 2px 6px 2px; }}
.vela-launcher list.vela-results {{ margin: 6px 10px; }}
.vela-launcher scrolledwindow, .vela-launcher scrolledwindow > viewport {{ background: transparent; }}
.vela-launcher button.vela-tile {{
  background: {tile_bg}; border: none; box-shadow: none; outline: none;
  border-radius: {inner_radius}px; padding: 6px; color: {fg};
  transition: background 90ms ease-out;
}}
.vela-launcher button.vela-tile:hover {{ background: {tile_hover}; }}
.vela-launcher button.vela-tile.selected {{ background: alpha({accent}, 0.26); box-shadow: inset 0 0 0 1px alpha({accent}, 0.55); }}
.vela-launcher .vela-tile-label {{ font-size: 0.86em; font-weight: 500; }}
.vela-launcher grid.vela-grid {{ margin: 8px 10px 12px 10px; }}
.vela-launcher list.vela-results {{ background: transparent; }}
.vela-launcher list.vela-results > row {{
  border-radius: {inner_radius}px; padding: 7px 10px; margin: 1px 0; color: {fg}; background: transparent; outline: none;
}}
.vela-launcher list.vela-results > row:hover {{ background: {tile_bg}; }}
.vela-launcher list.vela-results > row:selected {{ background: alpha({accent}, 0.26); box-shadow: inset 0 0 0 1px alpha({accent}, 0.45); }}
.vela-launcher .vela-row-title {{ font-weight: 600; }}
.vela-launcher .vela-row-subtitle {{ color: {dim}; font-size: 0.86em; }}
.vela-launcher .vela-row-badge {{ color: {dim}; font-size: 0.8em; padding: 2px 8px; border-radius: 99px; background: {tile_bg}; }}
.vela-launcher row:selected .vela-row-badge {{ color: {fg}; background: alpha({accent}, 0.35); }}
.vela-launcher .vela-section {{ color: {dim}; font-size: 0.78em; font-weight: 700; letter-spacing: 0.06em; margin: 10px 10px 4px 10px; }}
.vela-launcher .vela-claude-icon {{ color: {claude}; }}
.vela-launcher .vela-error {{ color: #ff8a80; font-size: 0.88em; padding: 0 18px 10px 18px; }}
.vela-launcher .vela-empty {{ color: {dim}; padding: 24px; }}
.vela-launcher button.vela-gear {{ min-width: 30px; min-height: 30px; padding: 0; border-radius: 99px; background: transparent; color: {dim}; box-shadow: none; }}
.vela-launcher button.vela-gear:hover {{ background: {tile_bg}; color: {fg}; }}
.vela-launcher .vela-spacing {{ border-spacing: {spacing}px; }}
"#
    ) + &motion_css(cfg)
}

/// Upper bound for staggered entrance delays (`vela-d0` … `vela-dN`).
pub const MAX_STAGGER: usize = 24;

/// Durations in milliseconds, scaled by the configured speed.
pub struct Motion {
    pub open: u32,
    pub close: u32,
    pub tile: u32,
    pub row: u32,
    pub step: u32,
    pub crossfade: u32,
}

pub fn motion(cfg: &Config) -> Option<Motion> {
    let a = &cfg.appearance;
    if !a.animations {
        return None;
    }
    let ms = |base: f64| (base / a.animation_speed).round() as u32;
    Some(Motion {
        open: ms(240.0),
        close: ms(120.0),
        tile: ms(300.0),
        row: ms(190.0),
        step: ms(14.0),
        crossfade: ms(140.0),
    })
}

/// Claude brand orange, used for everything Claude-related.
const CLAUDE_ORANGE: &str = "#d97757";

const EASE_OUT: &str = "cubic-bezier(0.22, 1, 0.36, 1)";

fn motion_css(cfg: &Config) -> String {
    let Some(m) = motion(cfg) else {
        return ".vela-launcher * { transition: none; animation: none; }\n".into();
    };
    let accent = &cfg.appearance.accent;
    let mut css = String::new();
    // Two identical keyframe sets: switching between them on every show
    // restarts the animations (GTK only restarts when the name changes).
    for v in ["a", "b"] {
        css += &format!(
            r#"
@keyframes vela-in-{v} {{ from {{ opacity: 0; transform: translateY(-12px) scale(0.965); }} to {{ opacity: 1; transform: none; }} }}
@keyframes vela-tile-{v} {{ from {{ opacity: 0; transform: translateY(10px) scale(0.92); }} to {{ opacity: 1; transform: none; }} }}
.vela-launcher.anim-{v} .vela-panel {{ animation: vela-in-{v} {open}ms {EASE_OUT} backwards; }}
.vela-launcher.anim-{v} button.vela-tile {{ animation: vela-tile-{v} {tile}ms {EASE_OUT} backwards; }}
"#,
            open = m.open,
            tile = m.tile,
        );
    }
    css += &format!(
        r#"
@keyframes vela-out {{ from {{ opacity: 1; transform: none; }} to {{ opacity: 0; transform: translateY(-6px) scale(0.975); }} }}
@keyframes vela-row-in {{ from {{ opacity: 0; transform: translateX(-8px); }} to {{ opacity: 1; transform: none; }} }}
@keyframes vela-glow {{ 0% {{ opacity: 0.75; transform: scale(1) rotate(0deg); }} 50% {{ opacity: 1; transform: scale(1.14) rotate(10deg); }} 100% {{ opacity: 0.75; transform: scale(1) rotate(0deg); }} }}
@keyframes vela-shake {{ 0% {{ transform: translateX(0); }} 25% {{ transform: translateX(-4px); }} 50% {{ transform: translateX(4px); }} 75% {{ transform: translateX(-2px); }} 100% {{ transform: translateX(0); }} }}
.vela-launcher.closing .vela-panel {{ animation: vela-out {close}ms cubic-bezier(0.4, 0, 1, 1) forwards; }}
.vela-launcher list.vela-results > row.new {{ animation: vela-row-in {row}ms {EASE_OUT} backwards; }}
.vela-launcher button.vela-tile {{ transition: background 120ms ease-out, box-shadow 160ms ease-out, transform 220ms {EASE_OUT}; }}
.vela-launcher button.vela-tile:hover {{ transform: translateY(-1px); }}
.vela-launcher button.vela-tile.selected {{ transform: translateY(-2px) scale(1.02); box-shadow: inset 0 0 0 1px alpha({accent}, 0.55), 0 6px 18px alpha({accent}, 0.18); }}
.vela-launcher list.vela-results > row {{ transition: background 120ms ease-out, box-shadow 160ms ease-out; }}
.vela-launcher row:selected .vela-claude-icon {{ animation: vela-glow 2400ms ease-in-out infinite; }}
@keyframes vela-pop {{ 0% {{ transform: scale(0.55) rotate(-40deg); opacity: 0.2; }} 70% {{ transform: scale(1.12) rotate(6deg); opacity: 1; }} 100% {{ transform: none; }} }}
@keyframes vela-badge-in {{ from {{ opacity: 0; transform: translateX(6px); }} to {{ opacity: 1; transform: none; }} }}
.vela-launcher .vela-search-chip {{ transition: background 240ms ease-out, box-shadow 320ms ease-out, color 240ms ease-out; }}
.vela-launcher .vela-search.claude image.vela-mode-claude {{ animation: vela-pop {pop}ms {EASE_OUT}, vela-glow 2600ms ease-in-out {pop}ms infinite; }}
.vela-launcher .vela-search.claude .vela-mode-badge {{ animation: vela-badge-in {pop}ms {EASE_OUT}; }}
.vela-launcher .vela-error.shake {{ animation: vela-shake 280ms ease-out; }}
"#,
        close = m.close,
        row = m.row,
        pop = m.open + 40,
    );
    for d in 0..=MAX_STAGGER {
        let delay = d as u32 * m.step;
        css += &format!(
            ".vela-launcher.anim-a button.vela-tile.vela-d{d}, .vela-launcher.anim-b button.vela-tile.vela-d{d}, .vela-launcher list.vela-results > row.new.vela-d{d} {{ animation-delay: {delay}ms; }}\n"
        );
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Theme;

    #[test]
    fn css_reflects_config() {
        let mut cfg = Config::default();
        cfg.general.opacity = 0.5;
        cfg.appearance.border_radius = 30;
        cfg.appearance.accent = "#ff0000".into();
        let css = launcher_css(&cfg);
        assert!(css.contains("rgba(24,24,30,0.500)"));
        assert!(css.contains("border-radius: 30px"));
        assert!(css.contains("alpha(#ff0000, 0.26)"));
    }

    #[test]
    fn motion_can_be_disabled_and_scaled() {
        let mut cfg = Config::default();
        assert!(launcher_css(&cfg).contains("@keyframes vela-in-a"));
        cfg.appearance.animation_speed = 2.0;
        assert_eq!(motion(&cfg).unwrap().open, 120);
        cfg.appearance.animations = false;
        let css = launcher_css(&cfg);
        assert!(!css.contains("@keyframes") && css.contains("animation: none"));
    }

    #[test]
    fn every_theme_produces_css() {
        for t in Theme::ALL {
            let mut cfg = Config::default();
            cfg.appearance.theme = t;
            assert!(launcher_css(&cfg).contains(".vela-panel"));
        }
    }
}
