//! Launch history used to rank frequently and recently used entries higher.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub count: u32,
    pub last_used: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    pub entries: HashMap<String, Usage>,
}

const MAX_ENTRIES: usize = 500;

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl History {
    pub fn load(path: &Path) -> History {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        crate::config::write_atomic(path, &serde_json::to_string(self)?)
    }

    pub fn record(&mut self, key: &str, now: u64) {
        let u = self.entries.entry(key.to_owned()).or_default();
        u.count = u.count.saturating_add(1);
        u.last_used = now;
        if self.entries.len() > MAX_ENTRIES {
            // Drop the least recently used entries.
            let mut by_age: Vec<_> = self.entries.iter().map(|(k, u)| (u.last_used, k.clone())).collect();
            by_age.sort();
            for (_, k) in by_age.into_iter().take(self.entries.len() - MAX_ENTRIES) {
                self.entries.remove(&k);
            }
        }
    }

    /// Frecency: launch count, decayed by the time since the last use.
    pub fn frecency(&self, key: &str, now: u64) -> f64 {
        let Some(u) = self.entries.get(key) else { return 0.0 };
        let age_days = now.saturating_sub(u.last_used) as f64 / 86_400.0;
        f64::from(u.count) / (1.0 + age_days / 7.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_decays() {
        let mut h = History::default();
        let now = 10_000_000;
        h.record("a", now);
        h.record("a", now);
        h.record("b", now - 30 * 86_400);
        assert_eq!(h.entries["a"].count, 2);
        assert!(h.frecency("a", now) > h.frecency("b", now));
        assert_eq!(h.frecency("missing", now), 0.0);
    }

    #[test]
    fn caps_size() {
        let mut h = History::default();
        for i in 0..(MAX_ENTRIES + 20) {
            h.record(&format!("k{i}"), i as u64);
        }
        assert_eq!(h.entries.len(), MAX_ENTRIES);
        assert!(!h.entries.contains_key("k0"));
    }

    #[test]
    fn persistence_roundtrip_and_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("history.json");
        let mut h = History::default();
        h.record("firefox.desktop", 5);
        h.save(&p).unwrap();
        assert_eq!(History::load(&p), h);
        std::fs::write(&p, "{garbage").unwrap();
        assert_eq!(History::load(&p), History::default());
    }
}
