//! Background search. The UI sends queries tagged with a generation number
//! and receives results over an async channel; nothing here runs on the GTK
//! main thread. Application search answers immediately, file search is
//! debounced and abandons work as soon as a newer query arrives.

use super::apps::AppMatcher;
use super::files::{self, BuiltinIndex, FileFilter, FileHit, RootBackend};
use crate::apps::catalog::Catalog;
use crate::config::Config;
use crate::history::{self, History};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct IndexStatus {
    pub plocate_installed: bool,
    pub roots: Vec<(PathBuf, RootBackend)>,
    pub indexed_entries: usize,
    pub indexing: bool,
    pub last_build_ms: Option<u128>,
}

pub enum Update {
    Apps { generation: u64, catalog: Arc<Catalog>, hits: Vec<usize> },
    Files { generation: u64, hits: Vec<FileHit>, pending: bool },
    Status(IndexStatus),
}

enum AppCmd {
    Catalog(Arc<Catalog>),
    History(Arc<History>),
    Config(Arc<Config>),
    Query(u64, String),
}

#[derive(Default)]
struct FileState {
    pending: Option<(u64, String)>,
    last_query: Option<(u64, String)>,
    config: Option<Arc<Config>>,
    rebuild: bool,
    refresh_older_than: Option<Duration>,
}

struct FileShared {
    state: Mutex<FileState>,
    wake: Condvar,
    latest: AtomicU64,
    index: Mutex<Option<Arc<BuiltinIndex>>>,
    building: AtomicBool,
    cancel_build: AtomicBool,
    updates: async_channel::Sender<Update>,
}

pub struct Engine {
    apps: mpsc::Sender<AppCmd>,
    files: Arc<FileShared>,
}

impl Engine {
    pub fn new(updates: async_channel::Sender<Update>) -> Engine {
        let (tx, rx) = mpsc::channel();
        let app_updates = updates.clone();
        std::thread::Builder::new()
            .name("vela-apps".into())
            .spawn(move || app_worker(rx, app_updates))
            .expect("spawn app search thread");

        let files = Arc::new(FileShared {
            state: Mutex::new(FileState::default()),
            wake: Condvar::new(),
            latest: AtomicU64::new(0),
            index: Mutex::new(None),
            building: AtomicBool::new(false),
            cancel_build: AtomicBool::new(false),
            updates,
        });
        let f = files.clone();
        std::thread::Builder::new()
            .name("vela-files".into())
            .spawn(move || file_worker(f))
            .expect("spawn file search thread");
        Engine { apps: tx, files }
    }

    pub fn set_catalog(&self, catalog: Arc<Catalog>) {
        let _ = self.apps.send(AppCmd::Catalog(catalog));
    }

    pub fn set_history(&self, history: Arc<History>) {
        let _ = self.apps.send(AppCmd::History(history));
    }

    pub fn set_config(&self, config: Arc<Config>) {
        let _ = self.apps.send(AppCmd::Config(config.clone()));
        let mut st = lock(&self.files.state);
        st.config = Some(config);
        self.files.wake.notify_all();
    }

    pub fn query(&self, generation: u64, text: &str) {
        self.files.latest.store(generation, Ordering::SeqCst);
        let _ = self.apps.send(AppCmd::Query(generation, text.to_owned()));
        let mut st = lock(&self.files.state);
        st.pending = Some((generation, text.to_owned()));
        st.last_query = st.pending.clone();
        self.files.wake.notify_all();
    }

    /// Rebuild the built-in index if it is older than `age` (called when the
    /// launcher is shown, so new files appear without waiting for the timer).
    pub fn refresh_index_if_older(&self, age: Duration) {
        let mut st = lock(&self.files.state);
        st.refresh_older_than = Some(age);
        self.files.wake.notify_all();
    }

    pub fn rebuild_index(&self) {
        let mut st = lock(&self.files.state);
        st.rebuild = true;
        self.files.wake.notify_all();
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn app_worker(rx: mpsc::Receiver<AppCmd>, updates: async_channel::Sender<Update>) {
    let mut matcher = AppMatcher::default();
    let mut catalog = Arc::new(Catalog::default());
    let mut history = Arc::new(History::default());
    let mut config = Arc::new(Config::default());
    while let Ok(first) = rx.recv() {
        // Coalesce: only the newest query of a burst is worth computing.
        let mut query = None;
        for cmd in std::iter::once(first).chain(rx.try_iter()) {
            match cmd {
                AppCmd::Catalog(c) => catalog = c,
                AppCmd::History(h) => history = h,
                AppCmd::Config(c) => config = c,
                AppCmd::Query(g, q) => query = Some((g, q)),
            }
        }
        let Some((generation, text)) = query else { continue };
        let hits = if config.search.apps {
            matcher.search(&catalog, &history, &text, config.search.max_app_results as usize, history::now_secs())
        } else {
            Vec::new()
        };
        if updates
            .send_blocking(Update::Apps {
                generation,
                catalog: catalog.clone(),
                hits,
            })
            .is_err()
        {
            return;
        }
    }
}

struct Coverage {
    checked: Instant,
    filter: FileFilter,
    mode: crate::config::FileBackend,
    roots: Vec<(PathBuf, RootBackend)>,
}

const COVERAGE_TTL: Duration = Duration::from_secs(600);

fn file_worker(shared: Arc<FileShared>) {
    let mut coverage: Option<Coverage> = None;
    loop {
        // Wait for work.
        let (job, config, rebuild, refresh) = {
            let mut st = lock(&shared.state);
            loop {
                if st.config.is_some() && (st.pending.is_some() || st.rebuild || st.refresh_older_than.is_some()) {
                    break;
                }
                st = shared
                    .wake
                    .wait_timeout(st, Duration::from_secs(60))
                    .map(|(g, _)| g)
                    .unwrap_or_else(|e| e.into_inner().0);
                if st.config.is_some() && st.pending.is_none() {
                    // Periodic wake-up: keep the index fresh.
                    let interval = Duration::from_secs(u64::from(st.config.as_ref().map_or(30, |c| c.search.index_interval_minutes)) * 60);
                    st.refresh_older_than = Some(interval);
                }
            }
            (
                st.pending.take(),
                st.config.clone().expect("checked above"),
                std::mem::take(&mut st.rebuild),
                st.refresh_older_than.take(),
            )
        };

        let filter = FileFilter::from_config(&config.search);
        let plocate = files::plocate_path();
        let stale_cov = coverage
            .as_ref()
            .is_none_or(|c| c.checked.elapsed() > COVERAGE_TTL || c.filter != filter || c.mode != config.search.file_backend);
        if stale_cov {
            let roots = filter
                .roots
                .iter()
                .map(|r| {
                    (
                        r.clone(),
                        files::choose_backend(config.search.file_backend, plocate.as_deref(), |bin| files::plocate_covers(bin, r)),
                    )
                })
                .collect();
            coverage = Some(Coverage {
                checked: Instant::now(),
                filter: filter.clone(),
                mode: config.search.file_backend,
                roots,
            });
        }
        let roots = coverage.as_ref().map(|c| c.roots.clone()).unwrap_or_default();
        let needs_index = config.search.files && roots.iter().any(|(_, b)| *b == RootBackend::Builtin);

        if needs_index {
            let current = lock(&shared.index).clone();
            let outdated = match &current {
                None => true,
                Some(idx) => idx.filter.as_ref() != Some(&filter) || rebuild || refresh.is_some_and(|age| idx.built_at.is_none_or(|t| t.elapsed() > age)),
            };
            if outdated {
                start_index_build(&shared, filter.clone(), roots.clone(), plocate.is_some());
            }
        } else if stale_cov || rebuild {
            publish_status(&shared, &roots, plocate.is_some());
        }

        let Some((generation, text)) = job else { continue };

        // Debounce: wait, and drop this query if a newer one arrives.
        let debounce = Duration::from_millis(u64::from(config.search.debounce_ms));
        if !debounce.is_zero() {
            let st = lock(&shared.state);
            let (st, _) = shared
                .wake
                .wait_timeout_while(st, debounce, |s| s.pending.is_none())
                .unwrap_or_else(|e| e.into_inner());
            if st.pending.is_some() {
                continue;
            }
        }

        let words = files::query_words(&text);
        let long_enough = text.trim().chars().count() >= config.search.min_file_query_len as usize;
        let mut pending = false;
        let mut hits: Vec<FileHit> = Vec::new();
        if config.search.files && long_enough && !words.is_empty() {
            let limit = config.search.max_file_results as usize;
            let stale = || shared.latest.load(Ordering::SeqCst) != generation;
            let index = lock(&shared.index).clone();
            for (root, backend) in &roots {
                let root_filter = FileFilter {
                    roots: vec![root.clone()],
                    ..filter.clone()
                };
                match backend {
                    RootBackend::Plocate => match plocate
                        .as_deref()
                        .map(|bin| files::plocate_search(bin, root, &words, &root_filter, limit, &stale))
                    {
                        Some(Ok(h)) => hits.extend(h),
                        Some(Err(e)) if e != "superseded" => log::warn!("{e}"),
                        _ => {}
                    },
                    RootBackend::Builtin => match &index {
                        Some(idx) if idx.filter.as_ref() == Some(&filter) => hits.extend(idx.search(&words, &root_filter, limit)),
                        _ => pending = true,
                    },
                }
                if stale() {
                    break;
                }
            }
            if stale() {
                continue;
            }
            hits.sort_by(|a, b| b.score.total_cmp(&a.score));
            hits.dedup_by(|a, b| a.path == b.path);
            hits.truncate(limit);
        }
        if shared.updates.send_blocking(Update::Files { generation, hits, pending }).is_err() {
            return;
        }
    }
}

fn publish_status(shared: &FileShared, roots: &[(PathBuf, RootBackend)], plocate_installed: bool) {
    let index = lock(&shared.index).clone();
    let _ = shared.updates.try_send(Update::Status(IndexStatus {
        plocate_installed,
        roots: roots.to_vec(),
        indexed_entries: index.as_ref().map_or(0, |i| i.len()),
        indexing: shared.building.load(Ordering::SeqCst),
        last_build_ms: None,
    }));
}

fn start_index_build(shared: &Arc<FileShared>, filter: FileFilter, roots: Vec<(PathBuf, RootBackend)>, plocate_installed: bool) {
    if shared.building.swap(true, Ordering::SeqCst) {
        return;
    }
    shared.cancel_build.store(false, Ordering::SeqCst);
    publish_status(shared, &roots, plocate_installed);
    let shared = shared.clone();
    std::thread::Builder::new()
        .name("vela-index".into())
        .spawn(move || {
            let builtin_filter = FileFilter {
                roots: roots.iter().filter(|(_, b)| *b == RootBackend::Builtin).map(|(r, _)| r.clone()).collect(),
                ..filter.clone()
            };
            let start = Instant::now();
            let mut index = BuiltinIndex::build(&builtin_filter, &shared.cancel_build);
            // Remember the full filter so config comparisons stay simple.
            index.filter = Some(filter);
            let took = start.elapsed();
            log::info!("indexed {} paths in {} ms", index.len(), took.as_millis());
            let entries = index.len();
            *lock(&shared.index) = Some(Arc::new(index));
            shared.building.store(false, Ordering::SeqCst);
            let _ = shared.updates.try_send(Update::Status(IndexStatus {
                plocate_installed,
                roots,
                indexed_entries: entries,
                indexing: false,
                last_build_ms: Some(took.as_millis()),
            }));
            // Re-run the latest query so results appear without typing again.
            let mut st = lock(&shared.state);
            if st.pending.is_none() {
                st.pending = st.last_query.clone().filter(|(g, _)| *g == shared.latest.load(Ordering::SeqCst));
            }
            shared.wake.notify_all();
        })
        .expect("spawn index thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::desktop_entry::DesktopEntry;
    use crate::apps::index::App;

    fn recv_until<F: Fn(&Update) -> bool>(rx: &async_channel::Receiver<Update>, pred: F) -> Update {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "timed out waiting for update");
            match rx.try_recv() {
                Ok(u) if pred(&u) => return u,
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    #[test]
    fn end_to_end_app_and_file_search() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Uni")).unwrap();
        std::fs::write(dir.path().join("Uni/Lecture Notes.pdf"), "x").unwrap();

        let (tx, rx) = async_channel::unbounded();
        let engine = Engine::new(tx);
        let mut cfg = Config::default();
        cfg.search.file_backend = crate::config::FileBackend::Builtin;
        cfg.search.file_roots = vec![dir.path().to_string_lossy().into()];
        cfg.search.debounce_ms = 10;
        engine.set_config(Arc::new(cfg));
        let apps = vec![App {
            id: "firefox.desktop".into(),
            path: "/x/firefox.desktop".into(),
            entry: DesktopEntry {
                name: "Firefox".into(),
                exec: Some("firefox".into()),
                ..Default::default()
            },
        }];
        engine.set_catalog(Arc::new(Catalog::build(&apps, &crate::config::Apps::default())));

        engine.query(1, "fire");
        let Update::Apps { generation, catalog, hits } = recv_until(&rx, |u| matches!(u, Update::Apps { .. })) else {
            unreachable!()
        };
        assert_eq!(generation, 1);
        assert_eq!(catalog.entries[hits[0]].name, "Firefox");

        // Wait for the index, then search files.
        recv_until(&rx, |u| matches!(u, Update::Status(s) if !s.indexing && s.indexed_entries > 0));
        engine.query(2, "lecture notes");
        let Update::Files { hits, .. } = recv_until(&rx, |u| matches!(u, Update::Files { generation: 2, .. })) else {
            unreachable!()
        };
        assert_eq!(hits[0].path, dir.path().join("Uni/Lecture Notes.pdf"));
    }

    #[test]
    fn superseded_queries_are_skipped() {
        let (tx, rx) = async_channel::unbounded();
        let engine = Engine::new(tx);
        let mut cfg = Config::default();
        cfg.search.file_backend = crate::config::FileBackend::Builtin;
        cfg.search.file_roots = vec!["/nonexistent-vela".into()];
        cfg.search.debounce_ms = 200;
        engine.set_config(Arc::new(cfg));
        for g in 1..=5 {
            engine.query(g, &format!("query{g}"));
        }
        let Update::Files { generation, .. } = recv_until(&rx, |u| matches!(u, Update::Files { .. })) else {
            unreachable!()
        };
        assert_eq!(generation, 5);
    }
}
