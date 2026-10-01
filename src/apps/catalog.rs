//! Everything the launcher can start: installed applications, their desktop
//! actions, user-defined custom actions and built-in entries. The catalog is
//! immutable and shared between the UI and the search worker via `Arc`.

use super::desktop_entry::DesktopEntry;
use super::index::App;
use crate::config::{Apps as AppsConfig, CustomAction};
use std::collections::HashMap;
use std::path::PathBuf;

pub const SETTINGS_KEY: &str = "vela:settings";
pub const CUSTOM_PREFIX: &str = "custom:";

#[derive(Debug, Clone)]
pub enum Target {
    Desktop { path: PathBuf, entry: DesktopEntry },
    DesktopAction { path: PathBuf, entry: DesktopEntry, action: usize },
    Custom(CustomAction),
    Settings,
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// Stable key used for pinning and history: desktop ID, `custom:<id>`,
    /// `<desktop id>#<action>` or a built-in key.
    pub key: String,
    pub name: String,
    pub subtitle: String,
    pub icon: Option<String>,
    pub keywords: Vec<String>,
    /// Desktop actions are only offered in search, never in the grid.
    pub is_action: bool,
    pub target: Target,
}

#[derive(Debug, Default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    by_key: HashMap<String, usize>,
}

impl Catalog {
    pub fn build(apps: &[App], cfg: &AppsConfig) -> Catalog {
        let mut entries = Vec::with_capacity(apps.len() + cfg.custom.len() + 1);
        for app in apps {
            // vela's own desktop file duplicates the built-in settings entry.
            if cfg.hidden.contains(&app.id) || app.id == "vela.desktop" {
                continue;
            }
            let e = &app.entry;
            entries.push(Entry {
                key: app.id.clone(),
                name: e.name.clone(),
                subtitle: e.generic_name.clone().or_else(|| e.comment.clone()).unwrap_or_default(),
                icon: e.icon.clone(),
                keywords: e.keywords.clone(),
                is_action: false,
                target: Target::Desktop {
                    path: app.path.clone(),
                    entry: e.clone(),
                },
            });
            if cfg.desktop_actions {
                for (i, action) in e.actions.iter().enumerate() {
                    if action.exec.is_none() && !e.dbus_activatable {
                        continue;
                    }
                    entries.push(Entry {
                        key: format!("{}#{}", app.id, action.id),
                        name: action.name.clone(),
                        subtitle: e.name.clone(),
                        icon: action.icon.clone().or_else(|| e.icon.clone()),
                        keywords: vec![e.name.clone()],
                        is_action: true,
                        target: Target::DesktopAction {
                            path: app.path.clone(),
                            entry: e.clone(),
                            action: i,
                        },
                    });
                }
            }
        }
        for custom in &cfg.custom {
            entries.push(Entry {
                key: format!("{CUSTOM_PREFIX}{}", custom.id),
                name: custom.name.clone(),
                subtitle: shlex::try_join(custom.command.iter().map(String::as_str)).unwrap_or_default(),
                icon: Some(custom.icon.clone()).filter(|i| !i.trim().is_empty()),
                keywords: custom.keywords.clone(),
                is_action: false,
                target: Target::Custom(custom.clone()),
            });
        }
        entries.push(Entry {
            key: SETTINGS_KEY.into(),
            name: "Vela Settings".into(),
            subtitle: "Configure the launcher".into(),
            icon: Some("preferences-system".into()),
            keywords: ["preferences", "config", "launcher", "vela", "einstellungen"].map(String::from).to_vec(),
            is_action: false,
            target: Target::Settings,
        });

        let by_key = entries.iter().enumerate().map(|(i, e)| (e.key.clone(), i)).collect();
        Catalog { entries, by_key }
    }

    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.by_key.get(key).map(|&i| &self.entries[i])
    }

    /// Entries for the empty-query grid, in display order.
    pub fn grid(&self, cfg: &AppsConfig) -> Vec<&Entry> {
        use crate::config::GridSource;
        let pinned: Vec<&Entry> = cfg.pinned.iter().filter_map(|k| self.get(k)).collect();
        let all = || self.entries.iter().filter(|e| !e.is_action && e.key != SETTINGS_KEY);
        match cfg.grid {
            GridSource::Pinned if !pinned.is_empty() => pinned,
            GridSource::Pinned | GridSource::All => all().collect(),
            GridSource::PinnedThenAll => {
                let mut out = pinned;
                out.extend(all().filter(|e| !cfg.pinned.contains(&e.key)));
                out
            }
        }
    }

    /// How many of `grid()`'s entries are the pinned ones, when pinned and
    /// other applications are both shown (the grid draws a line between).
    pub fn grid_pinned_count(&self, cfg: &AppsConfig) -> Option<usize> {
        use crate::config::GridSource;
        if cfg.grid != GridSource::PinnedThenAll {
            return None;
        }
        let pinned = cfg.pinned.iter().filter(|k| self.get(k).is_some()).count();
        (pinned > 0 && pinned < self.grid(cfg).len()).then_some(pinned)
    }

    /// Launchable, pinnable entries sorted by name (for the settings UI).
    pub fn pinnable(&self) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self.entries.iter().filter(|e| !e.is_action).collect();
        v.sort_by_cached_key(|e| e.name.to_lowercase());
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::desktop_entry::{DesktopAction, DesktopEntry};
    use crate::config::GridSource;

    fn app(id: &str, name: &str) -> App {
        App {
            id: id.into(),
            path: PathBuf::from(format!("/apps/{id}")),
            entry: DesktopEntry {
                name: name.into(),
                exec: Some(name.to_lowercase()),
                actions: vec![DesktopAction {
                    id: "new".into(),
                    name: "New Window".into(),
                    exec: Some("x --new".into()),
                    icon: None,
                }],
                ..Default::default()
            },
        }
    }

    fn cfg() -> AppsConfig {
        AppsConfig {
            grid: GridSource::Pinned,
            pinned: vec!["b.desktop".into(), "missing.desktop".into(), "custom:term".into()],
            hidden: vec!["h.desktop".into()],
            desktop_actions: true,
            custom: vec![CustomAction {
                id: "term".into(),
                name: "Htop".into(),
                command: vec!["htop".into()],
                terminal: true,
                ..Default::default()
            }],
        }
    }

    fn apps() -> Vec<App> {
        vec![app("a.desktop", "Alpha"), app("b.desktop", "Beta"), app("h.desktop", "Hidden")]
    }

    #[test]
    fn builds_entries_with_actions_custom_and_builtin() {
        let c = Catalog::build(&apps(), &cfg());
        assert!(c.get("h.desktop").is_none());
        assert_eq!(c.get("a.desktop#new").unwrap().subtitle, "Alpha");
        assert!(c.get("custom:term").is_some());
        assert!(c.get(SETTINGS_KEY).is_some());
    }

    #[test]
    fn grid_follows_pins_and_skips_missing() {
        let c = Catalog::build(&apps(), &cfg());
        let keys: Vec<_> = c.grid(&cfg()).iter().map(|e| e.key.clone()).collect();
        assert_eq!(keys, vec!["b.desktop", "custom:term"]);
    }

    #[test]
    fn grid_sources() {
        let mut conf = cfg();
        let c = Catalog::build(&apps(), &conf);
        conf.grid = GridSource::PinnedThenAll;
        let keys: Vec<_> = c.grid(&conf).iter().map(|e| e.key.clone()).collect();
        assert_eq!(keys, vec!["b.desktop", "custom:term", "a.desktop"]);
        conf.grid = GridSource::Pinned;
        conf.pinned.clear();
        let keys: Vec<_> = c.grid(&conf).iter().map(|e| e.key.clone()).collect();
        assert_eq!(keys, vec!["a.desktop", "b.desktop", "custom:term"]);
    }

    #[test]
    fn pinned_part_of_the_grid() {
        let mut conf = cfg();
        let c = Catalog::build(&apps(), &conf);
        assert_eq!(c.grid_pinned_count(&conf), None, "only pinned: no divider");
        conf.grid = GridSource::PinnedThenAll;
        assert_eq!(c.grid_pinned_count(&conf), Some(2), "b + custom:term, then a");
        conf.pinned.clear();
        assert_eq!(c.grid_pinned_count(&conf), None, "nothing pinned: no divider");
        conf.grid = GridSource::All;
        assert_eq!(c.grid_pinned_count(&conf), None);
    }

    #[test]
    fn desktop_actions_can_be_disabled() {
        let mut conf = cfg();
        conf.desktop_actions = false;
        let c = Catalog::build(&apps(), &conf);
        assert!(c.get("a.desktop#new").is_none());
    }
}
