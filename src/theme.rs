//! Theme palettes shared by the launcher CSS and the control center.

use crate::config::Theme;

pub type Rgb = (u8, u8, u8);

pub struct Palette {
    pub bg: Rgb,
    pub fg: Rgb,
    pub dim: Rgb,
    /// Base colour for surfaces and borders (white on dark, black on light).
    pub tint: Rgb,
}

pub fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Dark => Palette {
            bg: (24, 24, 30),
            fg: (236, 236, 244),
            dim: (150, 150, 168),
            tint: (255, 255, 255),
        },
        Theme::Midnight => Palette {
            bg: (11, 14, 28),
            fg: (228, 233, 255),
            dim: (134, 143, 181),
            tint: (190, 205, 255),
        },
        Theme::Graphite => Palette {
            bg: (40, 40, 42),
            fg: (240, 240, 240),
            dim: (160, 160, 160),
            tint: (255, 255, 255),
        },
        Theme::Nord => Palette {
            bg: (46, 52, 64),
            fg: (236, 239, 244),
            dim: (160, 170, 190),
            tint: (216, 222, 233),
        },
        Theme::Light => Palette {
            bg: (248, 248, 250),
            fg: (28, 28, 34),
            dim: (100, 100, 115),
            tint: (0, 0, 0),
        },
    }
}

pub fn is_light(theme: Theme) -> bool {
    theme == Theme::Light
}

pub fn hex((r, g, b): Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// `#rrggbb` (or `#rrggbbaa`, alpha ignored); None if malformed.
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let h = s.strip_prefix('#')?;
    if !matches!(h.len(), 6 | 8) {
        return None;
    }
    let ch = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some((ch(0)?, ch(2)?, ch(4)?))
}

/// Hyprland skips blurring nearly transparent layers.
pub const MIN_BLUR_ALPHA: f64 = 0.05;

/// Opacity of each of `layers` stacked backdrop layers so that together
/// they darken by `dim`. Every layer blurs what is below it again, which is
/// how the backdrop gets stronger than Hyprland's global blur.
pub fn backdrop_layer_alpha(dim: f64, layers: u32) -> f64 {
    let dim = dim.clamp(0.0, 0.95);
    let each = if layers <= 1 { dim } else { 1.0 - (1.0 - dim).powf(1.0 / f64::from(layers)) };
    each.max(MIN_BLUR_ALPHA)
}

/// Stacked backdrop layers: `strength` with blur, else one dimming layer.
pub fn backdrop_layers(strength: u32, blur: bool) -> u32 {
    if blur { strength.clamp(1, 4) } else { 1 }
}

/// Opacity of each backdrop layer (see [`backdrop_layers`]).
pub fn backdrop_alpha(dim: f64, strength: u32, blur: bool) -> f64 {
    if blur {
        backdrop_layer_alpha(dim, backdrop_layers(strength, true))
    } else {
        dim.clamp(0.0, 0.95)
    }
}

pub fn backdrop_visible(dim: f64, blur: bool) -> bool {
    blur || dim > 0.0
}

/// Layer namespace: only `base` has a blur layer rule in vela.lua.
pub fn backdrop_namespace(base: &str, blur: bool) -> String {
    if blur { base.to_owned() } else { format!("{base}-dim") }
}

/// Linear blend: t = 0 → a, t = 1 → b.
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let m = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// Relative luminance (WCAG), 0 = black, 1 = white.
pub fn luminance((r, g, b): Rgb) -> f64 {
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// Text colour that stays readable on `bg`.
pub fn on_color(bg: Rgb) -> Rgb {
    // Contrast against white vs. near-black; pick the larger one.
    let l = luminance(bg);
    let dark: Rgb = (16, 16, 20);
    if (1.05) / (l + 0.05) >= (l + 0.05) / (luminance(dark) + 0.05) {
        (255, 255, 255)
    } else {
        dark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_and_errors() {
        assert_eq!(parse_hex("#7aa2f7"), Some((0x7a, 0xa2, 0xf7)));
        assert_eq!(parse_hex("#7aa2f7ff"), Some((0x7a, 0xa2, 0xf7)));
        assert_eq!(hex((0x7a, 0xa2, 0xf7)), "#7aa2f7");
        assert!(parse_hex("7aa2f7").is_none() && parse_hex("#7aa").is_none() && parse_hex("#zzzzzz").is_none());
    }

    #[test]
    fn stacked_backdrop_keeps_the_total_dimming() {
        let total = |dim: f64, n: u32| 1.0_f64 - (1.0 - backdrop_layer_alpha(dim, n)).powi(n as i32);
        for n in 1..=4 {
            assert!((total(0.4, n) - 0.4_f64).abs() < 1e-9, "n = {n}");
        }
        assert_eq!(backdrop_layer_alpha(0.3, 1), 0.3);
        // Hyprland doesn't blur nearly invisible layers.
        assert_eq!(backdrop_layer_alpha(0.0, 1), MIN_BLUR_ALPHA);
        assert_eq!(backdrop_layer_alpha(0.1, 4), MIN_BLUR_ALPHA);
    }

    #[test]
    fn unblurred_backdrop_is_one_plain_dimming_layer() {
        assert_eq!(backdrop_layers(3, true), 3);
        assert_eq!(backdrop_layers(9, true), 4);
        assert_eq!(backdrop_layers(3, false), 1, "stacking only strengthens blur");
        assert_eq!(backdrop_alpha(0.3, 3, true), backdrop_layer_alpha(0.3, 3));
        assert_eq!(backdrop_alpha(0.3, 3, false), 0.3);
        assert_eq!(backdrop_alpha(0.0, 1, false), 0.0, "no minimum without blur");
        assert!(backdrop_visible(0.0, true));
        assert!(backdrop_visible(0.1, false));
        assert!(!backdrop_visible(0.0, false), "neither blur nor dim: nothing to draw");
        assert_eq!(
            (backdrop_namespace("vela-backdrop", true), backdrop_namespace("vela-backdrop", false)),
            ("vela-backdrop".into(), "vela-backdrop-dim".into())
        );
    }

    #[test]
    fn mixing_and_contrast() {
        assert_eq!(mix((0, 0, 0), (255, 255, 255), 0.5), (128, 128, 128));
        assert_eq!(mix((10, 20, 30), (200, 200, 200), 0.0), (10, 20, 30));
        assert_eq!(on_color((0, 0, 0)), (255, 255, 255));
        assert_eq!(on_color((255, 255, 255)), (16, 16, 20));
    }
}
