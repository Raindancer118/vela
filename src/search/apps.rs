//! Fuzzy ranking of catalog entries.
//!
//! nucleo provides the raw fuzzy score; on top of that strong name matches
//! (exact, prefix, word prefix) get large boosts so that "fire" reliably
//! yields Firefox before anything that merely contains the letters.

use crate::apps::catalog::Catalog;
use crate::history::History;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config as MatcherConfig, Matcher, Utf32Str};

pub struct AppMatcher {
    matcher: Matcher,
    buf: Vec<char>,
}

impl Default for AppMatcher {
    fn default() -> Self {
        AppMatcher {
            matcher: Matcher::new(MatcherConfig::DEFAULT),
            buf: Vec::new(),
        }
    }
}

fn fold(s: &str) -> String {
    s.to_lowercase()
}

/// Bonus for how well `query` matches the start of `name` or its words.
fn name_bonus(name: &str, query: &str) -> f64 {
    let name = fold(name);
    if name == query {
        return 1000.0;
    }
    if name.starts_with(query) {
        return 600.0;
    }
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    if words.iter().any(|w| w.starts_with(query)) {
        return 400.0;
    }
    // Multi-word query: every query word starts a word of the name.
    let qwords: Vec<&str> = query.split_whitespace().collect();
    if qwords.len() > 1 && qwords.iter().all(|q| words.iter().any(|w| w.starts_with(q))) {
        return 350.0;
    }
    if name.contains(query) {
        return 150.0;
    }
    0.0
}

/// Minimum nucleo score per query character for a fuzzy-only match.
/// Measured: real matches (incl. acronyms and typos) score ~20–29 per
/// character, letters scattered across a long name ~17.
const MIN_FUZZY_PER_CHAR: f64 = 19.0;

impl AppMatcher {
    /// Fuzzy score of `haystack`, or None if it is only a weak, scattered match.
    fn score(&mut self, pattern: &Pattern, haystack: &str, query: &str) -> Option<f64> {
        let raw = f64::from(pattern.score(Utf32Str::new(haystack, &mut self.buf), &mut self.matcher)?);
        let len = query.chars().filter(|c| !c.is_whitespace()).count();
        let contiguous = name_bonus(haystack, query) > 0.0;
        if contiguous || (len >= 3 && raw >= MIN_FUZZY_PER_CHAR * len as f64) {
            Some(raw)
        } else {
            None
        }
    }

    /// Returns catalog indices, best first.
    pub fn search(&mut self, catalog: &Catalog, history: &History, query: &str, limit: usize, now: u64) -> Vec<usize> {
        let query = fold(query.trim());
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }
        let pattern = Pattern::new(&query, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy);
        let mut scored: Vec<(f64, usize)> = Vec::new();

        for (i, entry) in catalog.entries.iter().enumerate() {
            let mut best: Option<f64> = None;
            let mut consider = |s: Option<f64>| {
                if let Some(s) = s {
                    best = Some(best.map_or(s, |b: f64| b.max(s)));
                }
            };

            if entry.is_action {
                // "firefox private" should find "New Private Window".
                let hay = format!("{} {}", entry.subtitle, entry.name);
                consider(self.score(&pattern, &hay, &query).map(|s| (s + name_bonus(&entry.name, &query) * 0.5) * 0.6));
            } else {
                consider(self.score(&pattern, &entry.name, &query).map(|s| s + name_bonus(&entry.name, &query)));
                // Secondary fields count, but never as much as the name.
                if !entry.subtitle.is_empty() {
                    consider(
                        self.score(&pattern, &entry.subtitle, &query)
                            .map(|s| s * 0.5 + name_bonus(&entry.subtitle, &query) * 0.25),
                    );
                }
                for kw in &entry.keywords {
                    consider(self.score(&pattern, kw, &query).map(|s| s * 0.5 + name_bonus(kw, &query) * 0.3));
                }
                // Desktop IDs help for apps whose display name is localized.
                let id = entry.key.trim_end_matches(".desktop");
                let id = id.rsplit('.').next().unwrap_or(id);
                consider(self.score(&pattern, id, &query).map(|s| s * 0.5 + name_bonus(id, &query) * 0.3));
            }

            if let Some(mut s) = best {
                s += 40.0 * (1.0 + history.frecency(&entry.key, now)).ln();
                scored.push((s, i));
            }
        }

        scored.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| catalog.entries[a.1].name.len().cmp(&catalog.entries[b.1].name.len()))
        });
        // Cut the long tail of weak, scattered matches.
        if let Some(&(top, _)) = scored.first() {
            let floor = top * 0.25;
            scored.retain(|(s, _)| *s >= floor);
        }
        scored.into_iter().take(limit).map(|(_, i)| i).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::desktop_entry::{DesktopAction, DesktopEntry};
    use crate::apps::index::App;
    use crate::config::Apps as AppsConfig;
    use std::path::PathBuf;

    fn app(id: &str, name: &str, generic: &str, keywords: &[&str]) -> App {
        App {
            id: id.into(),
            path: PathBuf::from(format!("/a/{id}")),
            entry: DesktopEntry {
                name: name.into(),
                generic_name: Some(generic.into()).filter(|g: &String| !g.is_empty()),
                keywords: keywords.iter().map(|s| s.to_string()).collect(),
                exec: Some("x".into()),
                actions: if id == "firefox.desktop" {
                    vec![DesktopAction {
                        id: "private".into(),
                        name: "New Private Window".into(),
                        exec: Some("firefox --private-window".into()),
                        icon: None,
                    }]
                } else {
                    vec![]
                },
                ..Default::default()
            },
        }
    }

    fn catalog() -> Catalog {
        let apps = vec![
            app("firefox.desktop", "Firefox", "Web Browser", &["Internet", "WWW"]),
            app("org.kde.filelight.desktop", "Filelight", "Disk Usage Statistics", &[]),
            app("fr.handbrake.ghb.desktop", "HandBrake", "Video Transcoder", &["rip", "dvd"]),
            app("kitty.desktop", "kitty", "Terminal emulator", &["term", "shell"]),
            app("org.kde.konsole.desktop", "Konsole", "Terminal", &["terminal", "command line"]),
            app("code.desktop", "Visual Studio Code", "Text Editor", &["vscode"]),
            app("org.gnome.clocks.desktop", "Uhren", "", &[]),
            app("firewall-config.desktop", "Firewall", "Firewall Configuration", &[]),
            app("qtcreator.desktop", "Qt Creator", "IDE", &[]),
        ];
        let cfg = AppsConfig {
            pinned: vec![],
            ..AppsConfig::default()
        };
        Catalog::build(&apps, &cfg)
    }

    fn names(q: &str, history: &History) -> Vec<String> {
        let c = catalog();
        AppMatcher::default()
            .search(&c, history, q, 8, 1_000_000)
            .into_iter()
            .map(|i| c.entries[i].name.clone())
            .collect()
    }

    #[test]
    fn prefix_of_name_ranks_first() {
        let n = names("fire", &History::default());
        assert_eq!(n[0], "Firefox");
        assert!(n.contains(&"Firewall".to_string()));
    }

    #[test]
    fn exact_name_beats_prefix() {
        assert_eq!(names("firewall", &History::default())[0], "Firewall");
    }

    #[test]
    fn word_prefix_and_keywords() {
        assert_eq!(names("code", &History::default())[0], "Visual Studio Code");
        assert_eq!(names("vscode", &History::default())[0], "Visual Studio Code");
        let term = names("terminal", &History::default());
        assert!(term[..2].contains(&"Konsole".to_string()) && term[..2].contains(&"kitty".to_string()));
    }

    #[test]
    fn matches_generic_name_and_desktop_id() {
        assert_eq!(names("browser", &History::default())[0], "Firefox");
        assert_eq!(names("clocks", &History::default())[0], "Uhren");
    }

    #[test]
    fn desktop_actions_are_found_by_app_and_action() {
        assert_eq!(names("firefox private", &History::default())[0], "New Private Window");
        // The app itself stays above its actions.
        assert_eq!(names("firefox", &History::default())[0], "Firefox");
    }

    #[test]
    fn history_breaks_ties() {
        let mut h = History::default();
        for _ in 0..5 {
            h.record("org.kde.konsole.desktop", 1_000_000);
        }
        assert_eq!(names("k", &History::default())[0], "kitty");
        assert_eq!(names("k", &h)[0], "Konsole");
    }

    #[test]
    fn scattered_fuzzy_matches_are_rejected() {
        let c = Catalog::build(
            &[
                app("libreoffice-impress.desktop", "LibreOffice Impress", "Presentation", &[]),
                app("brave-browser.desktop", "Brave Web Browser", "Web Browser", &[]),
            ],
            &AppsConfig {
                pinned: vec![],
                ..AppsConfig::default()
            },
        );
        let names = |q: &str| -> Vec<String> {
            AppMatcher::default()
                .search(&c, &History::default(), q, 8, 0)
                .into_iter()
                .map(|i| c.entries[i].name.clone())
                .collect()
        };
        assert!(names("fire").is_empty());
        assert_eq!(names("brw"), vec!["Brave Web Browser"]);
        assert_eq!(names("imp"), vec!["LibreOffice Impress"]);
        // Short queries need a real substring.
        assert!(names("lp").is_empty());
    }

    #[test]
    fn typos_still_match() {
        assert_eq!(names("firefx", &History::default())[0], "Firefox");
    }

    #[test]
    fn empty_and_garbage_queries() {
        assert!(names("", &History::default()).is_empty());
        assert!(names("zzzzqqq", &History::default()).is_empty());
        // Characters with meaning in fzf syntax are matched literally.
        assert!(names("^$!'", &History::default()).is_empty());
    }

    #[test]
    fn respects_limit() {
        let c = catalog();
        assert!(AppMatcher::default().search(&c, &History::default(), "e", 3, 0).len() <= 3);
    }
}
