//! Words of a settings search in German or English: filler words dropped,
//! German stems mapped to the English terms the settings use. Each query
//! word becomes a group of alternatives; a setting matches when every group
//! has one of them in its text.

/// German stem → English words found in setting titles, descriptions and
/// option names. Stems match inflected forms ("fenster" → "Fenstern").
const GERMAN: &[(&str, &[&str])] = &[
    ("abstand", &["gap", "spacing", "distance"]),
    ("abständ", &["gap", "spacing", "distance"]),
    ("lücke", &["gap"]),
    ("rand", &["border", "edge"]),
    ("ränder", &["border", "edge"]),
    ("kante", &["edge", "border"]),
    ("rahmen", &["border"]),
    ("fenster", &["window"]),
    ("unschärf", &["blur"]),
    ("unscharf", &["blur"]),
    ("weichzeichn", &["blur"]),
    ("verschwomm", &["blur"]),
    ("milchglas", &["blur"]),
    ("transparen", &["opacity", "transparen"]),
    ("durchsicht", &["opacity"]),
    ("deckkraft", &["opacity"]),
    ("deckend", &["opacity"]),
    ("ecke", &["corner", "rounding"]),
    ("abgerundet", &["corner", "rounding"]),
    ("rundung", &["corner", "rounding"]),
    ("schatten", &["shadow"]),
    ("leucht", &["glow"]),
    ("glühen", &["glow"]),
    ("animation", &["animation"]),
    ("animier", &["animat"]),
    ("dauer", &["duration"]),
    ("geschwindigkeit", &["speed", "rate"]),
    ("tempo", &["speed"]),
    ("tastenkürzel", &["shortcut", "bind"]),
    ("kürzel", &["shortcut", "bind"]),
    ("tastatur", &["keyboard", "kb"]),
    ("taste", &["key"]),
    ("maus", &["mouse", "pointer"]),
    ("zeiger", &["cursor", "pointer"]),
    ("touchpad", &["touchpad"]),
    ("trackpad", &["touchpad"]),
    ("scroll", &["scroll"]),
    ("natürlich", &["natural"]),
    ("tippen", &["tap"]),
    ("antippen", &["tap"]),
    ("bildschirm", &["monitor", "screen", "display"]),
    ("anzeige", &["monitor", "display", "screen"]),
    ("auflösung", &["resolution"]),
    ("bildwiederhol", &["refresh"]),
    ("frequenz", &["refresh", "rate"]),
    ("hertz", &["refresh"]),
    ("skalier", &["scale"]),
    ("drehung", &["rotation"]),
    ("dreh", &["rotation"]),
    ("arbeitsfläche", &["workspace"]),
    ("arbeitsbereich", &["workspace"]),
    ("farb", &["colour", "color"]),
    ("verlauf", &["gradient", "history"]),
    ("breite", &["width", "size"]),
    ("dicke", &["width", "size", "thickness"]),
    ("stärke", &["strength", "size"]),
    ("größe", &["size"]),
    ("helligkeit", &["brightness"]),
    ("kontrast", &["contrast"]),
    ("rauschen", &["noise"]),
    ("körnung", &["noise"]),
    ("sättigung", &["vibrancy"]),
    ("abdunkel", &["dim"]),
    ("dunkel", &["dim", "dark"]),
    ("dimm", &["dim"]),
    ("fokus", &["focus"]),
    ("benachrichtigung", &["notification"]),
    ("hintergrund", &["background"]),
    ("thema", &["theme"]),
    ("design", &["theme"]),
    ("akzent", &["accent"]),
    ("schrift", &["font"]),
    ("ruhezustand", &["suspend", "idle", "sleep"]),
    ("standby", &["suspend", "sleep"]),
    ("sperr", &["lock"]),
    ("energie", &["power"]),
    ("suche", &["search"]),
    ("anwendung", &["application", "app"]),
    ("programm", &["application", "app"]),
    ("starter", &["launcher"]),
    ("kontrollzentrum", &["panel", "control"]),
    ("leiste", &["panel", "bar"]),
    ("geste", &["gesture", "swipe"]),
    ("wisch", &["swipe"]),
    ("anordnung", &["arrangement", "layout"]),
    ("schweb", &["float"]),
    ("einrast", &["snap"]),
    ("vollbild", &["fullscreen"]),
    ("menü", &["popup", "menu"]),
    ("linkshänd", &["left"]),
    ("beschleunig", &["accel"]),
    ("empfindlich", &["sensitivity", "speed"]),
    ("wiederhol", &["repeat"]),
    ("verzögerung", &["delay"]),
    ("kurve", &["curve"]),
    ("feder", &["spring"]),
];

/// Words that say what to do, not what to find.
const FILLER: &[&str] = &[
    "der",
    "die",
    "das",
    "den",
    "dem",
    "des",
    "ein",
    "eine",
    "einen",
    "einem",
    "einer",
    "und",
    "oder",
    "zu",
    "zum",
    "zur",
    "bei",
    "mit",
    "von",
    "vom",
    "im",
    "in",
    "am",
    "an",
    "auf",
    "aus",
    "für",
    "bitte",
    "mehr",
    "weniger",
    "etwas",
    "ganz",
    "sehr",
    "nicht",
    "kein",
    "keine",
    "zwischen",
    "um",
    "als",
    "wie",
    "so",
    "ich",
    "will",
    "möchte",
    "mein",
    "meine",
    "meinen",
    "alle",
    "mach",
    "mache",
    "machen",
    "setz",
    "setze",
    "setzen",
    "stell",
    "stelle",
    "stellen",
    "ändere",
    "ändern",
    "verkleinere",
    "verkleinern",
    "vergrößere",
    "vergrößern",
    "erhöhe",
    "erhöhen",
    "verringere",
    "verringern",
    "reduziere",
    "reduzieren",
    "schalte",
    "schalten",
    "aktiviere",
    "aktivieren",
    "deaktiviere",
    "deaktivieren",
    "kleiner",
    "größer",
    "höher",
    "niedriger",
    "schneller",
    "langsamer",
    "the",
    "a",
    "of",
    "to",
    "and",
    "or",
    "make",
    "set",
    "change",
    "please",
    "more",
    "less",
    "between",
    "increase",
    "decrease",
    "bigger",
    "smaller",
    "turn",
    "enable",
    "disable",
    "on",
    "off",
    "my",
];

/// `unschaerfe` → `unschärfe`, for people typing without umlauts.
fn with_umlauts(w: &str) -> String {
    w.replace("ae", "ä").replace("oe", "ö").replace("ue", "ü")
}

/// One group of alternatives per meaningful query word.
pub fn groups(query: &str) -> Vec<Vec<String>> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !FILLER.contains(w))
        .map(|w| {
            let mut alts = vec![w.to_owned()];
            for form in [w.to_owned(), with_umlauts(w)] {
                for (stem, english) in GERMAN {
                    // Inflected ("fenstern"), still being typed ("unschä") or
                    // part of a compound ("mauszeiger"; short stems would
                    // match too much inside words).
                    let partial = form.chars().count() >= 4 && stem.starts_with(form.as_str());
                    let compound = stem.chars().count() >= 5 && form.contains(stem);
                    if form.starts_with(stem) || partial || compound {
                        alts.extend(english.iter().map(|e| (*e).to_owned()));
                    }
                }
            }
            alts.dedup();
            alts
        })
        .collect()
}

/// True when every group has an alternative in `text` (lowercase).
pub fn matches(groups: &[Vec<String>], text: &str) -> bool {
    !groups.is_empty() && groups.iter().all(|g| g.iter().any(|a| text.contains(a.as_str())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_sentences_find_english_settings() {
        let g = groups("Verkleinere den Abstand zwischen Fenstern");
        assert_eq!(g.len(), 2, "{g:?}");
        assert!(matches(&g, "gaps between windows space between tiled windows"));
        assert!(!matches(&g, "corner radius rounded window corners"));
        assert!(matches(&groups("Unschärfe"), "blurriness radius of the blur"));
        assert!(matches(&groups("unschaerfe"), "blurriness"));
        assert!(matches(&groups("Ränder"), "border width"));
        assert!(matches(&groups("Mauszeiger"), "hide the cursor when idle"));
        assert!(matches(&groups("Bildschirm Auflösung"), "resolution monitors"));
    }

    #[test]
    fn english_and_partial_words_still_work() {
        assert!(matches(&groups("gaps windows"), "gaps between windows"));
        assert!(matches(&groups("unsch"), "blur"), "typing a German word shows results early");
        assert!(matches(&groups("blu"), "blur"));
        assert!(groups("der die das").is_empty());
        assert!(!matches(&[], "anything"));
    }
}
