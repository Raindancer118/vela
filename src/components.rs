//! Which parts of vela are installed (install.sh's selection).
//!
//! The catalog (data/components.txt) is shared with install.sh, which writes
//! the selection to `<data dir>/vela/components.toml`. Without that file
//! (package install, running from the source tree) everything is installed;
//! on NixOS everything but the pacman-based updates. The Home Manager module
//! writes the file from `programs.vela.components`.

use crate::paths;
use std::path::PathBuf;
use std::sync::OnceLock;

pub const CATALOG: &str = include_str!("../data/components.txt");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Component {
    Launcher,
    Claude,
    Hyprland,
    Panel,
    Idle,
    SharePicker,
    Updates,
}

impl Component {
    pub const ALL: [Component; 7] = [
        Component::Launcher,
        Component::Claude,
        Component::Hyprland,
        Component::Panel,
        Component::Idle,
        Component::SharePicker,
        Component::Updates,
    ];

    /// Id in the catalog and in install.sh (`--with share-picker`).
    pub fn id(self) -> &'static str {
        match self {
            Component::Launcher => "launcher",
            Component::Claude => "claude",
            Component::Hyprland => "hyprland",
            Component::Panel => "panel",
            Component::Idle => "idle",
            Component::SharePicker => "share-picker",
            Component::Updates => "updates",
        }
    }

    /// Key in components.toml (also read by vela.lua, hence no dashes).
    pub fn key(self) -> String {
        self.id().replace('-', "_")
    }

    pub fn from_id(id: &str) -> Option<Component> {
        Component::ALL.into_iter().find(|c| c.id() == id || c.key() == id)
    }

    pub fn description(self) -> &'static str {
        catalog().components.iter().find(|s| s.id == self.id()).map_or("", |s| s.description)
    }

    /// Profiles that contain this component.
    pub fn profiles(self) -> &'static [&'static str] {
        catalog().components.iter().find(|s| s.id == self.id()).map_or(&[], |s| s.profiles.as_slice())
    }
}

#[derive(Debug)]
pub struct Spec {
    pub id: &'static str,
    pub profiles: Vec<&'static str>,
    pub description: &'static str,
}

#[derive(Debug, Default)]
pub struct Catalog {
    /// (name, description) in catalog order.
    pub profiles: Vec<(&'static str, &'static str)>,
    pub components: Vec<Spec>,
}

/// Splits off the first whitespace-separated word.
fn word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    (&s[..end], s[end..].trim())
}

pub fn parse_catalog(text: &'static str) -> Result<Catalog, String> {
    let mut cat = Catalog::default();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || format!("components.txt:{}: {line}", n + 1);
        match word(line) {
            ("profile", rest) => {
                let (name, desc) = word(rest);
                if name.is_empty() || desc.is_empty() {
                    return Err(bad());
                }
                cat.profiles.push((name, desc));
            }
            ("component", rest) => {
                let (id, rest) = word(rest);
                let (profiles, desc) = word(rest);
                if id.is_empty() || profiles.is_empty() || desc.is_empty() {
                    return Err(bad());
                }
                cat.components.push(Spec {
                    id,
                    profiles: profiles.split(',').collect(),
                    description: desc,
                });
            }
            _ => return Err(bad()),
        }
    }
    Ok(cat)
}

pub fn catalog() -> &'static Catalog {
    static CAT: OnceLock<Catalog> = OnceLock::new();
    // A broken catalog fails the tests; at runtime it just lists nothing.
    CAT.get_or_init(|| parse_catalog(CATALOG).unwrap_or_default())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    /// The profile chosen in install.sh ("custom" after picking by hand).
    pub profile: Option<String>,
    on: Vec<Component>,
}

impl Default for Installed {
    fn default() -> Self {
        Installed::all()
    }
}

impl Installed {
    pub fn all() -> Installed {
        Installed {
            profile: None,
            on: Component::ALL.to_vec(),
        }
    }

    pub fn only(components: &[Component]) -> Installed {
        Installed {
            profile: Some("custom".into()),
            on: components.to_vec(),
        }
    }

    /// Without a (readable) components.toml.
    pub fn fallback(nixos: bool) -> Installed {
        let mut i = Installed::all();
        if nixos {
            i.on.retain(|c| *c != Component::Updates);
        }
        i
    }

    pub fn has(&self, c: Component) -> bool {
        self.on.contains(&c)
    }

    pub fn is_full(&self) -> bool {
        Component::ALL.iter().all(|c| self.has(*c))
    }

    /// Reads components.toml. A component the file doesn't mention (added in
    /// a later version) is installed if the chosen profile contains it.
    pub fn parse(text: &str) -> Result<Installed, String> {
        let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.message().to_string())?;
        let profile = table.get("profile").and_then(|v| v.as_str()).map(str::to_string);
        let on = Component::ALL
            .into_iter()
            .filter(|c| match table.get(&c.key()).and_then(|v| v.as_bool()) {
                Some(v) => v,
                None => profile.as_deref().is_some_and(|p| c.profiles().contains(&p)),
            })
            .collect();
        Ok(Installed { profile, on })
    }
}

/// components.toml of the installation in use: `$VELA_COMPONENTS`, else the
/// first one in the data directories.
pub fn manifest_file() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("VELA_COMPONENTS").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    paths::data_dirs().into_iter().map(|d| d.join("vela/components.toml")).find(|p| p.is_file())
}

pub fn load() -> Installed {
    let fallback = || Installed::fallback(crate::nixos::running_nixos());
    let Some(path) = manifest_file() else { return fallback() };
    match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| Installed::parse(&t)) {
        Ok(i) => i,
        Err(e) => {
            log::warn!("{}: {e}; using the default components", path.display());
            fallback()
        }
    }
}

/// The selection of this installation, read once.
pub fn installed() -> &'static Installed {
    static INSTALLED: OnceLock<Installed> = OnceLock::new();
    // Unit tests must not depend on what is installed on the machine.
    INSTALLED.get_or_init(if cfg!(test) { Installed::all } else { load })
}

pub fn has(c: Component) -> bool {
    installed().has(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_matches_the_components() {
        let cat = parse_catalog(CATALOG).expect("catalog parses");
        let ids: Vec<&str> = cat.components.iter().map(|s| s.id).collect();
        assert_eq!(ids, Component::ALL.map(Component::id));
        let profiles: Vec<&str> = cat.profiles.iter().map(|p| p.0).collect();
        assert_eq!(profiles, ["full", "minimal", "launcher", "panel"]);
        for spec in &cat.components {
            assert!(spec.profiles.contains(&"full"), "{} missing from full", spec.id);
            for p in &spec.profiles {
                assert!(profiles.contains(p), "{}: unknown profile {p}", spec.id);
            }
        }
        for c in Component::ALL {
            assert!(!c.description().is_empty());
            assert_eq!(Component::from_id(c.id()), Some(c));
            assert_eq!(Component::from_id(&c.key()), Some(c));
        }
        assert_eq!(Component::SharePicker.key(), "share_picker");
    }

    #[test]
    fn profiles_select_their_components() {
        let minimal = Installed::parse("profile = \"minimal\"").unwrap();
        assert!(minimal.has(Component::Launcher));
        assert!(!minimal.has(Component::Panel) && !minimal.has(Component::Claude) && !minimal.has(Component::Updates));
        let panel = Installed::parse("profile = \"panel\"").unwrap();
        assert!(panel.has(Component::Panel) && panel.has(Component::Idle) && panel.has(Component::SharePicker));
        assert!(!panel.has(Component::Launcher));
        assert!(Installed::parse("profile = \"full\"").unwrap().is_full());
    }

    #[test]
    fn explicit_keys_win_over_the_profile() {
        let i = Installed::parse("profile = \"full\"\nclaude = false\nshare_picker = false\nunknown = true\n").unwrap();
        assert!(!i.has(Component::Claude) && !i.has(Component::SharePicker));
        assert!(i.has(Component::Launcher) && i.has(Component::Panel));
        assert_eq!(i.profile.as_deref(), Some("full"));
        // custom: only what is listed.
        let c = Installed::parse("profile = \"custom\"\nlauncher = true\nidle = true\n").unwrap();
        assert_eq!(c, Installed::only(&[Component::Launcher, Component::Idle]));
    }

    #[test]
    fn broken_files_are_errors() {
        assert!(Installed::parse("launcher = ").is_err());
        assert!(parse_catalog("component x\n").is_err());
        assert!(parse_catalog("nonsense here\n").is_err());
    }

    #[test]
    fn no_manifest_means_everything() {
        assert!(Installed::all().is_full());
        assert_eq!(Installed::default(), Installed::all());
        assert_eq!(Installed::fallback(false), Installed::all());
    }

    #[test]
    fn no_manifest_on_nixos_means_everything_but_updates() {
        // Updates run pacman; NixOS updates through nixos-rebuild.
        let nixos = Installed::fallback(true);
        assert!(!nixos.has(Component::Updates));
        assert!(Component::ALL.iter().filter(|c| **c != Component::Updates).all(|c| nixos.has(*c)));
        assert_eq!(nixos.profile, None);
    }
}
