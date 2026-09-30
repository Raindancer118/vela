//! File search: plocate where its database covers the configured roots,
//! otherwise a built-in in-memory index that is rebuilt in the background.
//!
//! Query semantics are Spotlight-like: every word must occur somewhere in the
//! path (case-insensitive); matches in the file name rank above matches that
//! only occur in parent directories.

use crate::config::{FileBackend, Search as SearchConfig};
use crate::paths;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Hard cap for the built-in index to bound memory usage.
const MAX_INDEX_ENTRIES: usize = 2_000_000;
/// plocate falls back to a linear scan of the whole database for patterns
/// shorter than a trigram, which takes seconds on large databases.
const PLOCATE_MIN_WORD: usize = 3;
const PLOCATE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq)]
pub struct FileHit {
    pub path: PathBuf,
    pub is_dir: bool,
    pub score: f64,
}

/// Settings relevant for filtering, resolved once per configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct FileFilter {
    pub roots: Vec<PathBuf>,
    pub exclude: Vec<String>,
    pub include_hidden: bool,
    pub include_directories: bool,
}

impl FileFilter {
    pub fn from_config(cfg: &SearchConfig) -> FileFilter {
        let mut roots: Vec<PathBuf> = cfg.file_roots.iter().map(|r| paths::expand_tilde(r.trim())).collect();
        roots.sort();
        roots.dedup();
        FileFilter {
            roots,
            exclude: cfg.exclude.clone(),
            include_hidden: cfg.include_hidden,
            include_directories: cfg.include_directories,
        }
    }

    fn excluded_name(&self, name: &str) -> bool {
        (!self.include_hidden && name.starts_with('.')) || self.exclude.iter().any(|e| e == name)
    }

    /// Root the path lives under, if it passes the hidden/exclude rules.
    fn accepts(&self, path: &str) -> bool {
        let p = Path::new(path);
        let Some(root) = self.roots.iter().find(|r| p.starts_with(r)) else {
            return false;
        };
        let Ok(rel) = p.strip_prefix(root) else { return false };
        if rel.as_os_str().is_empty() {
            return false;
        }
        !rel.components().any(|c| self.excluded_name(&c.as_os_str().to_string_lossy()))
    }
}

/// Lowercased query words.
pub fn query_words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

fn contains_ci(hay: &str, needle_lower: &str) -> bool {
    if hay.is_ascii() && needle_lower.is_ascii() {
        let (h, n) = (hay.as_bytes(), needle_lower.as_bytes());
        n.is_empty() || h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
    } else {
        hay.to_lowercase().contains(needle_lower)
    }
}

fn basename(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

/// Relevance of a path that is known to contain every query word.
pub fn score_path(path: &str, words: &[String]) -> f64 {
    let name = basename(path).to_lowercase();
    let stem = name.rsplit_once('.').map(|(s, _)| s).filter(|s| !s.is_empty()).unwrap_or(&name);
    let name_words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let joined = words.join(" ");

    let mut score = 0.0;
    let in_name = words.iter().filter(|w| name.contains(w.as_str())).count();
    score += 60.0 * in_name as f64 / words.len().max(1) as f64;
    if in_name == words.len() {
        score += 40.0;
    }
    for w in words {
        if name.starts_with(w.as_str()) {
            score += 15.0;
        } else if name_words.iter().any(|nw| nw.starts_with(w.as_str())) {
            score += 10.0;
        }
    }
    // "lecture notes" == "Lecture Notes.pdf" / "lecture_notes.md"
    let normalized_stem: String = stem.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    if normalized_stem.split_whitespace().collect::<Vec<_>>().join(" ") == joined || stem == joined {
        score += 80.0;
    }
    let depth = path.matches('/').count() as f64;
    score - depth.min(20.0)
}

/// Filters candidates, ranks them and verifies the best ones still exist.
pub fn rank(candidates: impl IntoIterator<Item = (String, Option<bool>)>, words: &[String], filter: &FileFilter, limit: usize) -> Vec<FileHit> {
    let mut scored: Vec<(f64, String, Option<bool>)> = candidates
        .into_iter()
        .filter(|(p, _)| filter.accepts(p) && words.iter().all(|w| contains_ci(p, w)))
        .map(|(p, d)| (score_path(&p, words), p, d))
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.len().cmp(&b.1.len())));

    let week_ago = SystemTime::now() - Duration::from_secs(7 * 86_400);
    let mut hits = Vec::with_capacity(limit);
    // Stat only a bounded number of top candidates: plocate results can be
    // stale, and the index may predate a deletion.
    for (score, path, is_dir) in scored.into_iter().take(limit * 4) {
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        let is_dir = is_dir.unwrap_or(meta.is_dir());
        if is_dir && !filter.include_directories {
            continue;
        }
        let recent = meta.modified().is_ok_and(|m| m > week_ago);
        hits.push(FileHit {
            path: PathBuf::from(path),
            is_dir,
            score: score + if recent { 8.0 } else { 0.0 },
        });
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(limit);
    hits
}

// ---------------------------------------------------------------- built-in

#[derive(Debug, Default)]
pub struct BuiltinIndex {
    /// (path, is_dir)
    entries: Vec<(Box<str>, bool)>,
    pub built_at: Option<Instant>,
    pub filter: Option<FileFilter>,
}

impl BuiltinIndex {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Walks all roots in parallel. Other file systems (network mounts,
    /// FUSE) are not descended into.
    pub fn build(filter: &FileFilter, cancel: &AtomicBool) -> BuiltinIndex {
        let collected: Mutex<Vec<(Box<str>, bool)>> = Mutex::new(Vec::new());
        for root in &filter.roots {
            if !root.is_dir() {
                continue;
            }
            let f = filter.clone();
            let walker = ignore::WalkBuilder::new(root)
                .hidden(!filter.include_hidden)
                .ignore(false)
                .git_ignore(false)
                .git_global(false)
                .git_exclude(false)
                .parents(false)
                .follow_links(false)
                .same_file_system(true)
                .filter_entry(move |e| e.depth() == 0 || !f.exclude.iter().any(|x| e.file_name() == x.as_str()))
                .build_parallel();
            walker.run(|| {
                let mut sink = Sink {
                    local: Vec::with_capacity(4096),
                    all: &collected,
                };
                Box::new(move |res| {
                    if cancel.load(Ordering::Relaxed) {
                        return ignore::WalkState::Quit;
                    }
                    let Ok(entry) = res else { return ignore::WalkState::Continue };
                    if entry.depth() == 0 {
                        return ignore::WalkState::Continue;
                    }
                    let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                    if let Some(p) = entry.path().to_str() {
                        sink.local.push((p.into(), is_dir));
                    }
                    if sink.local.len() >= 4096 && !sink.flush() {
                        return ignore::WalkState::Quit;
                    }
                    ignore::WalkState::Continue
                })
            });
        }
        let mut entries = collected.into_inner().unwrap_or_else(|e| e.into_inner());
        entries.truncate(MAX_INDEX_ENTRIES);
        BuiltinIndex {
            entries,
            built_at: Some(Instant::now()),
            filter: Some(filter.clone()),
        }
    }

    pub fn search(&self, words: &[String], filter: &FileFilter, limit: usize) -> Vec<FileHit> {
        if words.is_empty() {
            return Vec::new();
        }
        let candidates = self
            .entries
            .iter()
            .filter(|(p, _)| words.iter().all(|w| contains_ci(p, w)))
            .take(50_000)
            .map(|(p, d)| (p.to_string(), Some(*d)));
        rank(candidates, words, filter, limit)
    }
}

/// Per-thread buffer of the parallel walk; flushed in chunks and on drop.
struct Sink<'a> {
    local: Vec<(Box<str>, bool)>,
    all: &'a Mutex<Vec<(Box<str>, bool)>>,
}

impl Sink<'_> {
    /// Returns false once the index is full.
    fn flush(&mut self) -> bool {
        let mut all = self.all.lock().unwrap_or_else(|e| e.into_inner());
        all.append(&mut self.local);
        all.len() < MAX_INDEX_ENTRIES
    }
}

impl Drop for Sink<'_> {
    fn drop(&mut self) {
        self.flush();
    }
}

// ----------------------------------------------------------------- plocate

pub fn plocate_path() -> Option<PathBuf> {
    paths::find_executable("plocate")
}

/// Runs plocate, killing it when `stale()` turns true or after a timeout.
fn run_plocate(bin: &Path, patterns: &[String], limit: usize, stale: &dyn Fn() -> bool) -> Result<Vec<String>, String> {
    if stale() {
        return Err("superseded".into());
    }
    let mut child = Command::new(bin)
        .args(["-i", "-0", "-l", &limit.to_string(), "--"])
        .args(patterns)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("plocate: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("plocate: no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if stale() || start.elapsed() > PLOCATE_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(if stale() { "superseded".into() } else { "plocate timed out".into() });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(3)),
            Err(e) => return Err(format!("plocate: {e}")),
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let out = reader.join().unwrap_or_default();
    // Exit status 1 just means "no matches".
    if !status.success() && status.code() != Some(1) {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        return Err(format!("plocate failed: {}", err.trim()));
    }
    Ok(out
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect())
}

/// Whether the plocate database actually contains files below `root`
/// (on btrfs, `/home` is often pruned as a bind mount by updatedb).
pub fn plocate_covers(bin: &Path, root: &Path) -> bool {
    let Ok(read) = std::fs::read_dir(root) else { return false };
    let root_str = format!("{}/", root.display());
    let children: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
        .take(4)
        .collect();
    if children.is_empty() {
        return true;
    }
    children.iter().any(|child| {
        let pattern = child.to_string_lossy().into_owned();
        run_plocate(bin, &[pattern], 20, &|| false).is_ok_and(|lines| lines.iter().any(|l| l.starts_with(&root_str)))
    })
}

pub fn plocate_search(bin: &Path, root: &Path, words: &[String], filter: &FileFilter, limit: usize, stale: &dyn Fn() -> bool) -> Result<Vec<FileHit>, String> {
    let mut patterns: Vec<String> = words.iter().filter(|w| w.chars().count() >= PLOCATE_MIN_WORD).cloned().collect();
    if patterns.is_empty() {
        return Ok(Vec::new());
    }
    patterns.push(format!("{}/", root.display()));
    let lines = run_plocate(bin, &patterns, 3000, stale)?;
    Ok(rank(lines.into_iter().map(|l| (l, None)), words, filter, limit))
}

/// Which backend serves a root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootBackend {
    Plocate,
    Builtin,
}

pub fn choose_backend(mode: FileBackend, plocate: Option<&Path>, covers: impl FnOnce(&Path) -> bool) -> RootBackend {
    match (mode, plocate) {
        (FileBackend::Builtin, _) | (_, None) => RootBackend::Builtin,
        (FileBackend::Plocate, Some(_)) => RootBackend::Plocate,
        (FileBackend::Auto, Some(bin)) => {
            if covers(bin) {
                RootBackend::Plocate
            } else {
                RootBackend::Builtin
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn filter(root: &Path) -> FileFilter {
        FileFilter {
            roots: vec![root.to_path_buf()],
            exclude: vec!["node_modules".into()],
            include_hidden: false,
            include_directories: true,
        }
    }

    fn tree() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for f in [
            "Uni/Lecture Notes.pdf",
            "Uni/lecture/notes.md",
            "Uni/Crypto/rsa-lecture-notes-week3.pdf",
            "Uni/random.txt",
            "proj/node_modules/lecture-notes/index.js",
            ".hidden/lecture notes.txt",
            "Ünïcode/Übung Notizen.pdf",
        ] {
            let p = d.path().join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        d
    }

    fn rel(d: &Path, hits: &[FileHit]) -> Vec<String> {
        hits.iter().map(|h| h.path.strip_prefix(d).unwrap().to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn scoring_prefers_name_matches() {
        let w = query_words("lecture notes");
        assert!(score_path("/a/Uni/Lecture Notes.pdf", &w) > score_path("/a/Uni/lecture/notes.md", &w));
        assert!(score_path("/a/Uni/lecture_notes.md", &w) > score_path("/a/Uni/Crypto/rsa-lecture-notes-week3.pdf", &w));
    }

    #[test]
    fn builtin_index_finds_ranks_and_filters() {
        let d = tree();
        let f = filter(d.path());
        let idx = BuiltinIndex::build(&f, &AtomicBool::new(false));
        let hits = idx.search(&query_words("lecture notes"), &f, 10);
        let names = rel(d.path(), &hits);
        assert_eq!(names[0], "Uni/Lecture Notes.pdf");
        assert!(names.contains(&"Uni/lecture/notes.md".to_string()));
        assert!(names.iter().all(|n| !n.contains("node_modules") && !n.starts_with(".hidden")));
    }

    #[test]
    fn unicode_and_case_insensitive() {
        let d = tree();
        let f = filter(d.path());
        let idx = BuiltinIndex::build(&f, &AtomicBool::new(false));
        assert_eq!(rel(d.path(), &idx.search(&query_words("übung"), &f, 5)), vec!["Ünïcode/Übung Notizen.pdf"]);
    }

    #[test]
    fn hidden_files_on_request_and_directories_toggle() {
        let d = tree();
        let mut f = filter(d.path());
        f.include_hidden = true;
        let idx = BuiltinIndex::build(&f, &AtomicBool::new(false));
        assert!(rel(d.path(), &idx.search(&query_words("lecture notes"), &f, 20)).contains(&".hidden/lecture notes.txt".to_string()));

        f.include_directories = false;
        let hits = idx.search(&query_words("lecture"), &f, 20);
        assert!(hits.iter().all(|h| !h.is_dir));
        f.include_directories = true;
        let hits = idx.search(&query_words("lecture"), &f, 20);
        assert!(hits.iter().any(|h| h.is_dir));
    }

    #[test]
    fn vanished_files_are_dropped() {
        let d = tree();
        let f = filter(d.path());
        let idx = BuiltinIndex::build(&f, &AtomicBool::new(false));
        fs::remove_file(d.path().join("Uni/random.txt")).unwrap();
        assert!(idx.search(&query_words("random"), &f, 5).is_empty());
    }

    #[test]
    fn paths_outside_roots_are_rejected() {
        let f = filter(Path::new("/home/someone"));
        assert!(!f.accepts("/etc/passwd"));
        assert!(!f.accepts("/home/someone"));
        assert!(!f.accepts("/home/someone/.config/x"));
        assert!(f.accepts("/home/someone/docs/x"));
        assert!(!f.accepts("/home/someoneelse/docs/x"));
    }

    #[test]
    fn cancelled_build_stops_early() {
        let d = tree();
        let idx = BuiltinIndex::build(&filter(d.path()), &AtomicBool::new(true));
        assert!(idx.len() < 3);
    }

    #[test]
    fn backend_choice() {
        let bin = Path::new("/usr/bin/plocate");
        assert_eq!(choose_backend(FileBackend::Auto, None, |_| true), RootBackend::Builtin);
        assert_eq!(choose_backend(FileBackend::Auto, Some(bin), |_| true), RootBackend::Plocate);
        assert_eq!(choose_backend(FileBackend::Auto, Some(bin), |_| false), RootBackend::Builtin);
        assert_eq!(choose_backend(FileBackend::Plocate, None, |_| true), RootBackend::Builtin);
        assert_eq!(choose_backend(FileBackend::Builtin, Some(bin), |_| true), RootBackend::Builtin);
    }

    #[test]
    fn plocate_integration_when_available() {
        let Some(bin) = plocate_path() else { return };
        let root = Path::new("/usr/share/applications");
        if !plocate_covers(&bin, root) {
            return;
        }
        let f = FileFilter {
            roots: vec![root.into()],
            exclude: vec![],
            include_hidden: false,
            include_directories: true,
        };
        let hits = plocate_search(&bin, root, &query_words("desktop"), &f, 5, &|| false).unwrap();
        assert!(hits.iter().all(|h| h.path.starts_with(root)));
        // Short words never reach plocate (they would force a linear scan).
        assert!(plocate_search(&bin, root, &query_words("de"), &f, 5, &|| false).unwrap().is_empty());
        // A stale query is aborted.
        assert!(plocate_search(&bin, root, &query_words("desktop"), &f, 5, &|| true).is_err());
    }
}
