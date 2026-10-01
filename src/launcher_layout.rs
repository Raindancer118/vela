//! Where the launcher and its clock go on a monitor (logical pixels).

use crate::config::HorizontalPosition;

/// Distance kept between the clock and the launcher or the screen edge.
pub const CLOCK_GAP: i32 = 24;

/// Left edge of the launcher surface.
pub fn launcher_x(screen_w: i32, width: i32, pos: HorizontalPosition, margin: i32) -> i32 {
    match pos {
        HorizontalPosition::Center => (screen_w - width) / 2,
        HorizontalPosition::Left => margin,
        HorizontalPosition::Right => screen_w - margin - width,
    }
}

/// Top-left corner of the clock: above a centred launcher, otherwise in the
/// middle of the free side next to it.
pub fn clock_origin(screen: (i32, i32), launcher: (i32, i32, i32), clock: (i32, i32), pos: HorizontalPosition) -> (i32, i32) {
    let (w, h) = screen;
    let (lx, lw, ltop) = launcher;
    let (cw, ch) = clock;
    match pos {
        HorizontalPosition::Center => ((w - cw) / 2, (ltop - ch - CLOCK_GAP).max(CLOCK_GAP)),
        HorizontalPosition::Left => {
            let start = lx + lw;
            (start + (w - start - cw) / 2, (h - ch) / 2)
        }
        HorizontalPosition::Right => ((lx - cw) / 2, (h - ch) / 2),
    }
}

/// strftime formats (GLib) for time and date, after LC_ALL/LC_TIME/LANG
/// like the control center.
pub fn clock_formats(locale: &str) -> (&'static str, &'static str) {
    let lang = locale.split(['_', '.', '@']).next().unwrap_or("");
    match (lang, locale.split(['.', '@']).next().unwrap_or("")) {
        ("de", _) => ("%H:%M", "%A, %-d. %B"),
        ("en", "en_US") | ("C", _) | ("POSIX", _) | ("", _) => ("%-I:%M %p", "%A, %B %-d"),
        ("en", _) => ("%H:%M", "%A %-d %B"),
        _ => ("%H:%M", "%A, %-d %B"),
    }
}

/// The time locale from the environment (gettext precedence).
pub fn time_locale(get: impl Fn(&str) -> Option<String>) -> String {
    ["LC_ALL", "LC_TIME", "LANG"]
        .iter()
        .filter_map(|k| get(k))
        .find(|v| !v.is_empty())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use HorizontalPosition::*;

    #[test]
    fn launcher_sits_centred_or_at_an_edge() {
        assert_eq!(launcher_x(1920, 720, Center, 48), 600);
        assert_eq!(launcher_x(1920, 720, Left, 48), 48);
        assert_eq!(launcher_x(1920, 720, Right, 48), 1152);
    }

    #[test]
    fn clock_goes_above_or_beside_the_launcher() {
        assert_eq!(clock_origin((1920, 1200), (600, 720, 300), (400, 160), Center), (760, 116));
        assert_eq!(
            clock_origin((1920, 1200), (600, 720, 100), (400, 160), Center),
            (760, CLOCK_GAP),
            "no room: stays on screen"
        );
        // Launcher on the left: the clock in the middle of the right part.
        assert_eq!(clock_origin((1920, 1200), (48, 720, 200), (400, 160), Left), (1144, 520));
        assert_eq!(clock_origin((1920, 1200), (1152, 720, 200), (400, 160), Right), (376, 520));
    }

    #[test]
    fn clock_formats_follow_the_locale() {
        assert_eq!(clock_formats("de_DE.UTF-8"), ("%H:%M", "%A, %-d. %B"));
        assert_eq!(clock_formats("en_US.UTF-8"), ("%-I:%M %p", "%A, %B %-d"));
        assert_eq!(clock_formats("en_GB.UTF-8"), ("%H:%M", "%A %-d %B"));
        assert_eq!(clock_formats(""), ("%-I:%M %p", "%A, %B %-d"));
        assert_eq!(clock_formats("fr_FR.UTF-8"), ("%H:%M", "%A, %-d %B"));
    }

    #[test]
    fn time_locale_takes_the_first_set_variable() {
        let env = |k: &str| match k {
            "LC_ALL" => Some(String::new()),
            "LC_TIME" => Some("de_DE.UTF-8".into()),
            _ => Some("en_US.UTF-8".into()),
        };
        assert_eq!(time_locale(env), "de_DE.UTF-8");
        assert_eq!(time_locale(|_| None), "");
    }
}
