//! Minimal, spec-conformant parser for `.desktop` files. Only the
//! `[Desktop Entry]` group and `[Desktop Action …]` groups are interpreted.

use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    MissingMainGroup,
    MissingKey(&'static str),
    NotAnApplication(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::MissingMainGroup => write!(f, "no [Desktop Entry] group"),
            ParseError::MissingKey(k) => write!(f, "required key {k} is missing"),
            ParseError::NotAnApplication(t) => write!(f, "Type={t} is not an application"),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub exec: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesktopEntry {
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub exec: Option<String>,
    pub try_exec: Option<String>,
    pub icon: Option<String>,
    pub working_dir: Option<String>,
    pub no_display: bool,
    pub hidden: bool,
    pub terminal: bool,
    pub dbus_activatable: bool,
    /// StartupWMClass: the window class the app's windows get.
    pub startup_wm_class: Option<String>,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    pub actions: Vec<DesktopAction>,
}

/// Locale lookup keys in order of preference, derived from a POSIX locale
/// such as `de_DE.UTF-8@euro` (spec: "Localized values for keys").
pub fn locale_candidates(locale: &str) -> Vec<String> {
    let locale = locale.trim();
    if locale.is_empty() || locale == "C" || locale == "POSIX" {
        return Vec::new();
    }
    let (rest, modifier) = match locale.split_once('@') {
        Some((r, m)) => (r, Some(m)),
        None => (locale, None),
    };
    let rest = rest.split('.').next().unwrap_or(rest);
    let (lang, country) = match rest.split_once('_') {
        Some((l, c)) => (l, Some(c)),
        None => (rest, None),
    };
    let mut out = Vec::new();
    if let (Some(c), Some(m)) = (country, modifier) {
        out.push(format!("{lang}_{c}@{m}"));
    }
    if let Some(c) = country {
        out.push(format!("{lang}_{c}"));
    }
    if let Some(m) = modifier {
        out.push(format!("{lang}@{m}"));
    }
    out.push(lang.to_owned());
    out
}

/// Reads the effective message locale from the environment.
pub fn current_locale() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var)
            && !v.is_empty()
        {
            return v;
        }
    }
    String::new()
}

/// Unescapes `\s \n \t \r \\` (spec: "Possible value types").
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Splits a `;`-separated list, honouring `\;` escapes.
fn split_list(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(';') => current.push(';'),
                Some(other) => {
                    current.push('\\');
                    current.push(other);
                }
                None => current.push('\\'),
            },
            ';' => items.push(unescape(&std::mem::take(&mut current))),
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        items.push(unescape(&current));
    }
    items.into_iter().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect()
}

fn parse_bool(value: Option<&String>) -> bool {
    value.is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

/// Raw key/value pairs of one group; keys include their `[locale]` suffix.
type Group = HashMap<String, String>;

fn parse_groups(content: &str) -> Vec<(String, Group)> {
    let mut groups: Vec<(String, Group)> = Vec::new();
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    for line in content.lines() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            if let Some(end) = trimmed.find(']') {
                groups.push((trimmed[1..end].to_owned(), Group::new()));
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else { continue };
        if let Some((_, group)) = groups.last_mut() {
            // The first occurrence of a key wins, as in GLib's key file parser.
            group.entry(key.trim().to_owned()).or_insert_with(|| value.trim_start().to_owned());
        }
    }
    groups
}

struct Localized<'a> {
    group: &'a Group,
    locales: &'a [String],
}

impl Localized<'_> {
    fn raw(&self, key: &str) -> Option<&String> {
        for loc in self.locales {
            if let Some(v) = self.group.get(&format!("{key}[{loc}]")) {
                return Some(v);
            }
        }
        self.group.get(key)
    }

    fn string(&self, key: &str) -> Option<String> {
        self.raw(key).map(|v| unescape(v)).filter(|v| !v.trim().is_empty())
    }

    fn plain(&self, key: &str) -> Option<String> {
        self.group.get(key).map(|v| unescape(v)).filter(|v| !v.trim().is_empty())
    }
}

pub fn parse(content: &str, locales: &[String]) -> Result<DesktopEntry, ParseError> {
    let groups = parse_groups(content);
    let main = groups
        .iter()
        .find(|(name, _)| name == "Desktop Entry")
        .map(|(_, g)| g)
        .ok_or(ParseError::MissingMainGroup)?;
    let l = Localized { group: main, locales };

    let entry_type = l.plain("Type").ok_or(ParseError::MissingKey("Type"))?;
    if entry_type != "Application" {
        return Err(ParseError::NotAnApplication(entry_type));
    }
    let name = l.string("Name").ok_or(ParseError::MissingKey("Name"))?;

    let action_ids = main.get("Actions").map(|v| split_list(v)).unwrap_or_default();
    let actions = action_ids
        .into_iter()
        .filter_map(|id| {
            let group = groups.iter().find(|(n, _)| *n == format!("Desktop Action {id}")).map(|(_, g)| g)?;
            let al = Localized { group, locales };
            Some(DesktopAction {
                name: al.string("Name")?,
                exec: al.plain("Exec"),
                icon: al.string("Icon"),
                id,
            })
        })
        .collect();

    Ok(DesktopEntry {
        name,
        generic_name: l.string("GenericName"),
        comment: l.string("Comment"),
        keywords: l.raw("Keywords").map(|v| split_list(v)).unwrap_or_default(),
        categories: main.get("Categories").map(|v| split_list(v)).unwrap_or_default(),
        exec: l.plain("Exec"),
        try_exec: l.plain("TryExec"),
        icon: l.string("Icon"),
        working_dir: l.plain("Path"),
        no_display: parse_bool(main.get("NoDisplay")),
        hidden: parse_bool(main.get("Hidden")),
        terminal: parse_bool(main.get("Terminal")),
        dbus_activatable: parse_bool(main.get("DBusActivatable")),
        startup_wm_class: l.plain("StartupWMClass"),
        only_show_in: main.get("OnlyShowIn").map(|v| split_list(v)).unwrap_or_default(),
        not_show_in: main.get("NotShowIn").map(|v| split_list(v)).unwrap_or_default(),
        actions,
    })
}

impl DesktopEntry {
    /// OnlyShowIn/NotShowIn evaluation against `XDG_CURRENT_DESKTOP`.
    pub fn shown_in(&self, desktops: &[String]) -> bool {
        if !self.only_show_in.is_empty() && !self.only_show_in.iter().any(|d| desktops.iter().any(|c| c.eq_ignore_ascii_case(d))) {
            return false;
        }
        !self.not_show_in.iter().any(|d| desktops.iter().any(|c| c.eq_ignore_ascii_case(d)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = r#"
# comment
[Desktop Entry]
Version=1.0
Name=Firefox
Name[de]=Firefox Webbrowser
GenericName=Web Browser
GenericName[de]=Webbrowser
Comment=Browse the Web
Keywords=Internet;WWW;Browser;Web\;Explorer;
Keywords[de]=Internet;Netz;
Exec=/usr/lib/firefox/firefox %u
Icon=firefox
Terminal=false
Type=Application
Categories=Network;WebBrowser;
Actions=new-window;new-private-window;missing;

[Desktop Action new-window]
Name=New Window
Name[de]=Neues Fenster
Exec=/usr/lib/firefox/firefox --new-window %u

[Desktop Action new-private-window]
Name=New Private Window
Exec=/usr/lib/firefox/firefox --private-window %u
"#;

    fn locales(l: &str) -> Vec<String> {
        locale_candidates(l)
    }

    #[test]
    fn locale_candidates_follow_spec_order() {
        assert_eq!(locales("de_DE.UTF-8@euro"), vec!["de_DE@euro", "de_DE", "de@euro", "de"]);
        assert_eq!(locales("en_US.UTF-8"), vec!["en_US", "en"]);
        assert!(locales("C").is_empty());
    }

    #[test]
    fn parses_basic_fields() {
        let e = parse(FIREFOX, &[]).unwrap();
        assert_eq!(e.name, "Firefox");
        assert_eq!(e.generic_name.as_deref(), Some("Web Browser"));
        assert_eq!(e.keywords, vec!["Internet", "WWW", "Browser", "Web;Explorer"]);
        assert_eq!(e.exec.as_deref(), Some("/usr/lib/firefox/firefox %u"));
        assert_eq!(e.icon.as_deref(), Some("firefox"));
        assert!(!e.terminal && !e.no_display && !e.hidden);
        assert_eq!(e.categories, vec!["Network", "WebBrowser"]);
    }

    #[test]
    fn uses_localized_values() {
        let e = parse(FIREFOX, &locales("de_DE.UTF-8")).unwrap();
        assert_eq!(e.name, "Firefox Webbrowser");
        assert_eq!(e.generic_name.as_deref(), Some("Webbrowser"));
        assert_eq!(e.keywords, vec!["Internet", "Netz"]);
        assert_eq!(e.actions[0].name, "Neues Fenster");
    }

    #[test]
    fn parses_only_declared_existing_actions() {
        let e = parse(FIREFOX, &[]).unwrap();
        let ids: Vec<_> = e.actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["new-window", "new-private-window"]);
    }

    #[test]
    fn unescapes_values() {
        let e = parse("[Desktop Entry]\nType=Application\nName=A\\sB\\\\C\nExec=app\n", &[]).unwrap();
        assert_eq!(e.name, "A B\\C");
    }

    #[test]
    fn booleans_and_visibility() {
        let e = parse("[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\nHidden=True\nTerminal=true\n", &[]).unwrap();
        assert!(e.no_display && e.hidden && e.terminal);
    }

    #[test]
    fn malformed_entries_are_errors() {
        assert_eq!(parse("Name=x", &[]), Err(ParseError::MissingMainGroup));
        assert_eq!(parse("[Desktop Entry]\nName=x", &[]), Err(ParseError::MissingKey("Type")));
        assert_eq!(parse("[Desktop Entry]\nType=Application\n", &[]), Err(ParseError::MissingKey("Name")));
        assert_eq!(
            parse("[Desktop Entry]\nType=Link\nName=x", &[]),
            Err(ParseError::NotAnApplication("Link".into()))
        );
        // Garbage lines are skipped instead of failing the whole file.
        assert!(parse("[Desktop Entry]\n\u{0}garbage\nType=Application\nName=ok", &[]).is_ok());
    }

    #[test]
    fn show_in_rules() {
        let e = parse("[Desktop Entry]\nType=Application\nName=X\nOnlyShowIn=KDE;GNOME;\n", &[]).unwrap();
        assert!(e.shown_in(&["kde".into()]));
        assert!(!e.shown_in(&["Hyprland".into()]));
        let e = parse("[Desktop Entry]\nType=Application\nName=X\nNotShowIn=Hyprland;\n", &[]).unwrap();
        assert!(!e.shown_in(&["Hyprland".into()]));
        assert!(e.shown_in(&[]));
    }

    #[test]
    fn keys_in_other_groups_do_not_leak() {
        let e = parse("[Other]\nName=Wrong\n[Desktop Entry]\nType=Application\nName=Right\n", &[]).unwrap();
        assert_eq!(e.name, "Right");
    }
}
