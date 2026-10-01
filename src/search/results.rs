//! Combines application, file and Claude results into the list the user
//! sees. Pure logic, kept out of the UI so it can be tested.

use super::files::FileHit;
use crate::apps::catalog::Catalog;
use crate::config::Config;
use crate::paths;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Item {
    App {
        catalog: Arc<Catalog>,
        index: usize,
    },
    File(FileHit),
    /// The input itself is an existing path (`~/Downloads`, `/etc/fstab`).
    Path(PathBuf),
    Claude,
}

impl Item {
    /// Stable identity across result updates.
    pub fn key(&self) -> String {
        match self {
            Item::App { catalog, index } => format!("app:{}", catalog.entries[*index].key),
            Item::File(h) => format!("file:{}", h.path.display()),
            Item::Path(p) => format!("path:{}", p.display()),
            Item::Claude => "claude".into(),
        }
    }

    pub fn section(&self) -> &'static str {
        match self {
            Item::App { .. } => "Applications",
            Item::File(_) | Item::Path(_) => "Files",
            Item::Claude => "Claude",
        }
    }
}

const QUESTION_STARTS: &[&str] = &[
    "what",
    "why",
    "how",
    "who",
    "when",
    "where",
    "which",
    "explain",
    "write",
    "create",
    "help",
    "can",
    "could",
    "should",
    "is",
    "are",
    "does",
    "do",
    "tell",
    "summarize",
    "fix",
    "generate",
    "translate",
    "wie",
    "was",
    "warum",
    "wieso",
    "weshalb",
    "wer",
    "wann",
    "wo",
    "welche",
    "welcher",
    "erkläre",
    "erklär",
    "schreib",
    "schreibe",
    "erstelle",
    "hilf",
    "kannst",
    "übersetze",
    "fasse",
];

/// Heuristic for "this is a prompt, not a search term".
pub fn looks_like_question(query: &str) -> bool {
    let q = query.trim();
    if q.ends_with('?') || q.contains('\n') {
        return true;
    }
    let words: Vec<String> = q.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.len() >= 5 {
        return true;
    }
    words.len() >= 2 && QUESTION_STARTS.contains(&words[0].trim_matches(|c: char| !c.is_alphanumeric()))
}

fn path_item(query: &str) -> Option<PathBuf> {
    let q = query.trim();
    if !(q.starts_with('/') || q.starts_with("~/") || q == "~") {
        return None;
    }
    let p = paths::expand_tilde(q);
    p.exists().then_some(p)
}

pub fn assemble(cfg: &Config, query: &str, catalog: &Arc<Catalog>, apps: &[usize], files: &[FileHit]) -> Vec<Item> {
    let mut items = Vec::new();
    if query.trim().is_empty() {
        return items;
    }
    let max = cfg.general.max_results as usize;

    if cfg.search.files
        && let Some(p) = path_item(query)
    {
        items.push(Item::Path(p));
    }
    if cfg.search.apps {
        items.extend(apps.iter().take(cfg.search.max_app_results as usize).map(|&index| Item::App {
            catalog: catalog.clone(),
            index,
        }));
    }
    if cfg.search.files {
        items.extend(files.iter().take(cfg.search.max_file_results as usize).cloned().map(Item::File));
    }

    let with_claude = cfg.claude_search() && (cfg.claude.always_visible || items.is_empty());
    let claude_first = with_claude && (items.is_empty() || (cfg.claude.prefer_for_questions && looks_like_question(query)));
    let room = if with_claude { max.saturating_sub(1) } else { max };
    items.truncate(room);
    if with_claude {
        if claude_first {
            items.insert(0, Item::Claude);
        } else {
            items.push(Item::Claude);
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::desktop_entry::DesktopEntry;
    use crate::apps::index::App;

    fn catalog() -> Arc<Catalog> {
        let apps: Vec<App> = (0..10)
            .map(|i| App {
                id: format!("a{i}.desktop"),
                path: "/x".into(),
                entry: DesktopEntry {
                    name: format!("App {i}"),
                    exec: Some("x".into()),
                    ..Default::default()
                },
            })
            .collect();
        Arc::new(Catalog::build(&apps, &crate::config::Apps::default()))
    }

    fn file(n: &str) -> FileHit {
        FileHit {
            path: PathBuf::from(n),
            is_dir: false,
            score: 1.0,
        }
    }

    fn kinds(items: &[Item]) -> Vec<&'static str> {
        items
            .iter()
            .map(|i| match i {
                Item::App { .. } => "app",
                Item::File(_) => "file",
                Item::Path(_) => "path",
                Item::Claude => "claude",
            })
            .collect()
    }

    #[test]
    fn questions() {
        assert!(looks_like_question("Explain RSA to me"));
        assert!(looks_like_question("wie funktioniert tls"));
        assert!(looks_like_question("rsa?"));
        assert!(looks_like_question("one two three four five"));
        assert!(!looks_like_question("fire"));
        assert!(!looks_like_question("lecture notes"));
        assert!(!looks_like_question("what"));
    }

    #[test]
    fn order_and_limits() {
        let cfg = Config::default();
        let c = catalog();
        let items = assemble(&cfg, "app", &c, &[0, 1, 2], &[file("/a"), file("/b")]);
        assert_eq!(kinds(&items), vec!["app", "app", "app", "file", "file", "claude"]);

        let mut small = Config::default();
        small.general.max_results = 3;
        let items = assemble(&small, "app", &c, &[0, 1, 2, 3], &[file("/a")]);
        assert_eq!(kinds(&items), vec!["app", "app", "claude"]);
    }

    #[test]
    fn claude_placement() {
        let c = catalog();
        let cfg = Config::default();
        assert_eq!(kinds(&assemble(&cfg, "Explain RSA to me", &c, &[0], &[]))[0], "claude");
        assert_eq!(kinds(&assemble(&cfg, "nothing matches", &c, &[], &[])), vec!["claude"]);

        let mut hidden = Config::default();
        hidden.claude.always_visible = false;
        assert_eq!(kinds(&assemble(&hidden, "app", &c, &[0], &[])), vec!["app"]);
        assert_eq!(kinds(&assemble(&hidden, "zzz", &c, &[], &[])), vec!["claude"]);

        let mut off = Config::default();
        off.search.claude = false;
        assert!(assemble(&off, "zzz", &c, &[], &[]).is_empty());
    }

    #[test]
    fn disabled_sources_disappear() {
        let c = catalog();
        let mut cfg = Config::default();
        cfg.search.files = false;
        assert_eq!(kinds(&assemble(&cfg, "app", &c, &[0], &[file("/a")])), vec!["app", "claude"]);
        cfg.search.apps = false;
        assert_eq!(kinds(&assemble(&cfg, "app", &c, &[0], &[file("/a")])), vec!["claude"]);
    }

    #[test]
    fn existing_paths_are_offered() {
        let c = catalog();
        let items = assemble(&Config::default(), "/etc", &c, &[], &[]);
        assert!(matches!(&items[0], Item::Path(p) if p == &PathBuf::from("/etc")));
        assert!(assemble(&Config::default(), "", &c, &[0], &[]).is_empty());
    }
}
