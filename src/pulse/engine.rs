//! Ties sampling, app grouping, health tracking, events and the doctor
//! together into one `Frame` per tick.

use super::apps::{self, DesktopApp, Group, Kind, ProcInfo, WindowInfo};
use super::doctor::{self, Finding};
use super::health::{self, AppTrack, CoreDump, DTracker, Flag, LeakInfo};
use super::procfs;
use super::sample::{self, ProcSample, Sample, Sampler};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppFrame {
    pub key: String,
    pub kind: Kind,
    pub name: String,
    pub icon: String,
    pub desktop_id: Option<String>,
    pub unit: Option<String>,
    pub user_unit: bool,
    pub main_pid: i32,
    pub pids: Vec<i32>,
    /// Start time (unix s) of each pid, to not hit a reused pid later.
    #[serde(skip)]
    pub starts: Vec<u64>,
    pub windows: Vec<WindowInfo>,
    /// Percent of the whole CPU (all cores).
    pub cpu: f64,
    /// Percent of one core.
    pub cpu_core: f64,
    pub mem: u64,
    pub swap: u64,
    pub gpu: f64,
    pub vram: u64,
    pub read_bps: f64,
    pub write_bps: f64,
    pub threads: u64,
    pub started: u64,
    pub user: String,
    pub uid: u32,
    pub flags: Vec<Flag>,
    /// 0 fine, 1 worth a look, 2 broken.
    pub health: u8,
    pub leak: Option<LeakView>,
    pub gpus: Vec<String>,
    /// The main process's command, for "run again" and the details.
    pub command: Vec<String>,
    pub exe: String,
    pub restart_reasons: Vec<String>,
    pub focused: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeakView {
    pub per_minute: f64,
    pub grown: f64,
    pub minutes: f64,
}

impl From<LeakInfo> for LeakView {
    fn from(l: LeakInfo) -> Self {
        LeakView {
            per_minute: l.per_minute,
            grown: l.grown,
            minutes: l.minutes,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcFrame {
    #[serde(flatten)]
    pub p: ProcSample,
    pub app: String,
    pub flags: Vec<Flag>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub t: u64,
    /// started, closed, crashed, oom, failed, not-responding, responding,
    /// ended, restarted, action-failed, info
    pub kind: String,
    pub name: String,
    pub icon: String,
    pub detail: String,
    pub key: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Crash {
    pub exe: String,
    pub name: String,
    pub icon: String,
    pub count: usize,
    pub last: u64,
    pub signal: String,
    /// Key of the app if it is running again.
    pub running: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedUnit {
    pub unit: String,
    pub description: String,
    pub user: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(flatten)]
    pub sample: Sample,
    pub apps: Vec<AppFrame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub procs: Option<Vec<ProcFrame>>,
    pub findings: Vec<Finding>,
    pub score: u32,
    pub events: Vec<Event>,
    pub crashes: Vec<Crash>,
    pub failed_units: Vec<FailedUnit>,
    pub reboot_needed: bool,
    pub zombies: usize,
}

#[derive(Default)]
struct Background {
    coredumps: Vec<CoreDump>,
    failed: Vec<FailedUnit>,
    updated: bool,
    /// Check again now (after an action on a unit or app).
    again: bool,
}

pub struct Monitor {
    sampler: Sampler,
    desktop: Vec<DesktopApp>,
    desktop_paths: HashMap<String, PathBuf>,
    desktop_at: Option<Instant>,
    tracks: HashMap<String, AppTrack>,
    dstate: DTracker,
    restart: HashMap<i32, (u64, Vec<String>)>,
    restart_at: Option<Instant>,
    restart_stamp: Option<SystemTime>,
    bg: Arc<Mutex<Background>>,
    bg_started: bool,
    events: VecDeque<Event>,
    pending: Vec<Event>,
    known: HashMap<String, Known>,
    first: bool,
    seen_dumps: HashSet<(u64, i32)>,
    dumps_primed: bool,
    seen_failed: HashSet<String>,
    failed_primed: bool,
    anr: HashSet<String>,
    throttle: VecDeque<u64>,
    oom: Option<u64>,
    start: Instant,
}

pub const EVENT_KEEP: usize = 300;

/// What the last tick knew of an app: name, icon, kind, pids.
type Known = (String, String, Kind, Vec<i32>);

impl Default for Monitor {
    fn default() -> Self {
        Monitor::new()
    }
}

impl Monitor {
    pub fn new() -> Monitor {
        Monitor::with(Sampler::default())
    }

    /// The daemon's background recorder.
    pub fn light() -> Monitor {
        Monitor::with(Sampler::light())
    }

    fn with(sampler: Sampler) -> Monitor {
        Monitor {
            sampler,
            desktop: Vec::new(),
            desktop_paths: HashMap::new(),
            desktop_at: None,
            tracks: HashMap::new(),
            dstate: DTracker::default(),
            restart: HashMap::new(),
            restart_at: None,
            restart_stamp: None,
            bg: Arc::default(),
            bg_started: false,
            events: VecDeque::new(),
            pending: Vec::new(),
            known: HashMap::new(),
            first: true,
            seen_dumps: HashSet::new(),
            dumps_primed: false,
            seen_failed: HashSet::new(),
            failed_primed: false,
            anr: HashSet::new(),
            throttle: VecDeque::new(),
            oom: None,
            start: Instant::now(),
        }
    }

    /// Re-reads failed units and core dumps right away.
    pub fn recheck(&self) {
        if let Ok(mut b) = self.bg.lock() {
            b.again = true;
        }
    }

    pub fn events(&self) -> Vec<Event> {
        self.events.iter().cloned().collect()
    }

    pub fn desktop_path(&self, id: &str) -> Option<&PathBuf> {
        self.desktop_paths.get(id)
    }

    /// Records an event (also from actions) for the next frame.
    pub fn push_event(&mut self, kind: &str, name: &str, icon: &str, detail: &str, key: &str) {
        let e = Event {
            t: sample::now_ms(),
            kind: kind.into(),
            name: name.into(),
            icon: icon.into(),
            detail: detail.into(),
            key: key.into(),
        };
        self.pending.push(e.clone());
        self.events.push_back(e);
        while self.events.len() > EVENT_KEEP {
            self.events.pop_front();
        }
    }

    fn refresh_desktop(&mut self) {
        if self.desktop_at.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
            return;
        }
        self.desktop_at = Some(Instant::now());
        let report = crate::apps::index::scan(&crate::paths::data_dirs(), &crate::apps::index::ScanOptions::from_env());
        self.desktop = report
            .apps
            .iter()
            .map(|a| DesktopApp {
                id: a.id.trim_end_matches(".desktop").to_owned(),
                name: a.entry.name.clone(),
                icon: a.entry.icon.clone().unwrap_or_default(),
                wm_class: a.entry.startup_wm_class.clone().unwrap_or_default(),
                exec_bin: a.entry.exec.as_deref().map(apps::exec_bin).unwrap_or_default(),
            })
            .collect();
        self.desktop_paths = report
            .apps
            .iter()
            .map(|a| (a.id.trim_end_matches(".desktop").to_owned(), a.path.clone()))
            .collect();
    }

    fn start_background(&mut self) {
        if self.bg_started {
            return;
        }
        self.bg_started = true;
        let weak = Arc::downgrade(&self.bg);
        std::thread::Builder::new()
            .name("pulse-checks".into())
            .spawn(move || {
                loop {
                    let dumps = run_text("coredumpctl", &["list", "--json=short", "--no-pager", "--since=-24h"]).map(|t| health::parse_coredumps(&t));
                    let mut failed = Vec::new();
                    for user in [true, false] {
                        let mut args = vec!["list-units", "--failed", "--output=json", "--no-pager", "--plain"];
                        if user {
                            args.insert(0, "--user");
                        }
                        for u in run_text("systemctl", &args).map(|t| health::parse_units(&t)).unwrap_or_default() {
                            failed.push(FailedUnit {
                                unit: u.unit,
                                description: u.description,
                                user,
                            });
                        }
                    }
                    let Some(state) = weak.upgrade() else { return };
                    if let Ok(mut b) = state.lock() {
                        if let Some(d) = dumps {
                            b.coredumps = d;
                        }
                        b.failed = failed;
                        b.updated = true;
                    }
                    drop(state);
                    for _ in 0..30 {
                        std::thread::sleep(Duration::from_millis(500));
                        let Some(state) = weak.upgrade() else { return };
                        let again = state.lock().map(|mut b| std::mem::take(&mut b.again)).unwrap_or(false);
                        if again {
                            break;
                        }
                    }
                }
            })
            .ok();
    }

    /// Re-checks which processes run replaced code: after a package
    /// transaction, otherwise every 10 minutes.
    fn refresh_restart(&mut self, procs: &[ProcSample]) {
        let stamp = std::fs::metadata(if crate::nixos::running_nixos() {
            "/run/current-system"
        } else {
            "/var/log/pacman.log"
        })
        .and_then(|m| m.modified())
        .ok();
        let due = self.restart_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(600)) || stamp != self.restart_stamp;
        if !due {
            return;
        }
        self.restart_at = Some(Instant::now());
        self.restart_stamp = stamp;
        let uid = unsafe { libc::getuid() };
        let nixos = crate::nixos::running_nixos();
        let profiles = nix_profiles();
        let mut next = HashMap::new();
        for p in procs.iter().filter(|p| !p.kernel) {
            let mut reasons = Vec::new();
            // The store never changes in place: compare with what the current
            // system or profile would start now.
            let store_exe = if p.exe.is_empty() {
                p.cmdline.first().cloned().unwrap_or_default()
            } else {
                p.exe.clone()
            };
            if nixos && store_exe.starts_with("/nix/store/") {
                let prog = store_exe
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_start_matches('.')
                    .trim_end_matches("-wrapped")
                    .to_owned();
                let current = profiles.iter().find_map(|d| std::fs::canonicalize(d.join(&prog)).ok());
                if let Some(cur) = current
                    && nix_outdated(&store_exe, &cur.to_string_lossy())
                {
                    reasons.push(store_exe.clone());
                }
            } else if p.uid == uid {
                if p.exe.ends_with(" (deleted)") {
                    reasons.push(p.exe.trim_end_matches(" (deleted)").to_owned());
                }
                if let Ok(maps) = std::fs::read_to_string(format!("/proc/{}/maps", p.pid)) {
                    reasons.extend(procfs::deleted_mappings(&maps));
                }
            } else if let Some(path) = p.cmdline.first().filter(|a| a.starts_with('/')) {
                let replaced = std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .is_some_and(|m| m.as_secs() > p.started + 2);
                if replaced {
                    reasons.push(path.clone());
                }
            }
            reasons.sort();
            reasons.dedup();
            if !reasons.is_empty() {
                next.insert(p.pid, (p.started, reasons));
            }
        }
        self.restart = next;
    }

    pub fn tick(&mut self, with_procs: bool) -> Frame {
        self.start_background();
        self.refresh_desktop();
        let now_s = self.start.elapsed().as_secs_f64();
        let mut sample = self.sampler.sample();
        let procs = std::mem::take(&mut sample.procs);
        self.refresh_restart(&procs);

        let windows = hypr_windows();
        let infos: Vec<ProcInfo> = procs
            .iter()
            .map(|p| ProcInfo {
                pid: p.pid,
                ppid: p.ppid,
                uid: p.uid,
                comm: p.comm.clone(),
                exe: p.exe.clone(),
                cmdline: p.cmdline.clone(),
                cgroup: p.cgroup.clone(),
                kernel: p.kernel,
            })
            .collect();
        let groups = apps::group(&infos, &windows, &self.desktop);
        let by_pid: HashMap<i32, &ProcSample> = procs.iter().map(|p| (p.pid, p)).collect();

        // Per-process conditions.
        let hung: HashMap<i32, f64> = self
            .dstate
            .update(now_s, &procs.iter().map(|p| (p.pid, p.started, p.state)).collect::<Vec<_>>(), 15.0)
            .into_iter()
            .collect();
        let zombie_ppids: Vec<i32> = procs.iter().filter(|p| p.state == 'Z').map(|p| p.ppid).collect();
        let zombie_parents: HashSet<i32> = zombie_ppids.iter().copied().collect();
        let dialogs: Vec<Vec<String>> = procs
            .iter()
            .filter(|p| p.comm == "hyprland-dialo" || p.comm == "hyprland-dialog")
            .map(|p| p.cmdline.clone())
            .collect();
        let win_keys: Vec<(String, String)> = windows.iter().map(|w| (w.class.clone(), w.title.clone())).collect();
        let anr_windows: HashSet<i32> = health::not_responding(&dialogs, &win_keys).into_iter().map(|i| windows[i].pid).collect();

        let bg = self.bg.lock().map(|b| (b.coredumps.clone(), b.failed.clone(), b.updated)).unwrap_or_default();
        let (dumps, failed, bg_ready) = bg;
        let now_us = sample.t * 1000;
        let recent_dump_pids: HashMap<i32, &CoreDump> = dumps
            .iter()
            .filter(|d| now_us.saturating_sub(d.time) < 3_600_000_000)
            .map(|d| (d.pid, d))
            .collect();

        let ncpu = sample.cpu.logical.max(1) as f64;
        let mut out_apps = Vec::with_capacity(groups.len());
        let mut app_of: HashMap<i32, String> = HashMap::new();
        for g in &groups {
            let members: Vec<&ProcSample> = g.pids.iter().filter_map(|p| by_pid.get(p).copied()).collect();
            let cpu_core: f64 = members.iter().map(|p| p.cpu).sum();
            let mem: u64 = members.iter().map(|p| p.mem).sum();
            let track = self.tracks.entry(g.key.clone()).or_default();
            track.push(now_s, cpu_core, mem);
            let mut flags = Vec::new();
            if g.windows.iter().any(|w| anr_windows.contains(&w.pid)) {
                flags.push(Flag::NotResponding);
            }
            if members.iter().any(|p| hung.contains_key(&p.pid)) {
                flags.push(Flag::Hung);
            }
            let main = by_pid.get(&g.main_pid).copied();
            let exe_name = main.map(|m| exe_base(&m.exe, &m.cmdline)).unwrap_or_default();
            if dumps
                .iter()
                .any(|d| now_us.saturating_sub(d.time) < 3_600_000_000 && !exe_name.is_empty() && exe_base(&d.exe, &[]) == exe_name)
            {
                flags.push(Flag::Crashed);
            }
            let focused = g.windows.iter().any(|w| w.focused);
            let watch_runaway = matches!(g.kind, Kind::Background | Kind::Task | Kind::Service) || g.kind == Kind::Window && !focused;
            if watch_runaway && health::runaway(&track.cpu, 60, 50.0) {
                flags.push(Flag::Runaway);
            }
            let leak = if g.kind == Kind::Kernel { None } else { track.leak() };
            if leak.is_some() {
                flags.push(Flag::Leak);
            }
            let mut restart_reasons: Vec<String> = members
                .iter()
                .filter_map(|p| self.restart.get(&p.pid).filter(|(s, _)| *s == p.started).map(|(_, r)| r.clone()))
                .flatten()
                .collect();
            restart_reasons.sort();
            restart_reasons.dedup();
            if !restart_reasons.is_empty() && g.kind != Kind::Kernel {
                flags.push(Flag::NeedsRestart);
            }
            let live: Vec<&&ProcSample> = members.iter().filter(|p| p.state != 'Z').collect();
            if !live.is_empty() && live.iter().all(|p| p.state == 'T' || p.state == 't') || unit_frozen(g) {
                flags.push(Flag::Paused);
            }
            if members.iter().any(|p| zombie_parents.contains(&p.pid)) && zombie_ppids.iter().filter(|pp| g.pids.contains(pp)).count() >= 3 {
                flags.push(Flag::Zombies);
            }
            if g.kind != Kind::Kernel && main.is_some_and(|m| super::actions::in_efficiency_mode(m.pid, m.nice)) {
                flags.push(Flag::Efficiency);
            }
            flags.sort();
            flags.dedup();
            let health = flags.iter().map(|f| f.severity()).max().unwrap_or(0);
            for p in &g.pids {
                app_of.insert(*p, g.key.clone());
            }
            let mut gpus: Vec<String> = members.iter().flat_map(|p| p.gpus.iter().cloned()).collect();
            gpus.sort();
            gpus.dedup();
            out_apps.push(AppFrame {
                key: g.key.clone(),
                kind: g.kind,
                name: g.name.clone(),
                icon: g.icon.clone(),
                desktop_id: g.desktop_id.clone(),
                unit: g.unit.clone(),
                user_unit: g.user_unit,
                main_pid: g.main_pid,
                pids: g.pids.clone(),
                starts: g.pids.iter().map(|p| by_pid.get(p).map_or(0, |s| s.started)).collect(),
                windows: g.windows.clone(),
                cpu: cpu_core / ncpu,
                cpu_core,
                mem,
                swap: members.iter().map(|p| p.swap).sum(),
                gpu: members.iter().map(|p| p.gpu).sum::<f64>().min(100.0),
                vram: members.iter().map(|p| p.vram).sum(),
                read_bps: members.iter().map(|p| p.read_bps).sum(),
                write_bps: members.iter().map(|p| p.write_bps).sum(),
                threads: members.iter().map(|p| p.threads).sum(),
                started: members.iter().map(|p| p.started).min().unwrap_or(0),
                user: main.map(|m| m.user.clone()).unwrap_or_default(),
                uid: main.map_or(0, |m| m.uid),
                flags,
                health,
                leak: leak.map(LeakView::from),
                gpus,
                command: main.map(|m| m.cmdline.clone()).unwrap_or_default(),
                exe: main.map(|m| m.exe.trim_end_matches(" (deleted)").to_owned()).unwrap_or_default(),
                restart_reasons,
                focused,
            });
        }
        let live_keys: HashSet<&String> = out_apps.iter().map(|a| &a.key).collect();
        self.tracks.retain(|k, _| live_keys.contains(k));

        self.lifecycle_events(&groups, &recent_dump_pids);
        if bg_ready {
            self.crash_and_failure_events(&dumps, &failed, &out_apps);
        }
        self.anr_events(&out_apps);
        self.oom_event(sample.memory.oom_kills);

        // Throttle events in the last 30 s, for the doctor.
        self.throttle.push_back(sample.cpu.throttled);
        while self.throttle.len() > 30 {
            self.throttle.pop_front();
        }
        let crashes = crash_groups(&dumps, &out_apps, now_us);
        let failed_units = failed.clone();
        let reboot_needed = kernel_updated();
        let ctx = doctor::Context {
            throttled_recently: self.throttle.iter().sum(),
            reboot_needed,
            crashes: &crashes,
            failed_units: &failed_units,
            hung: hung.len(),
            hung_names: hung.keys().filter_map(|pid| by_pid.get(pid)).map(|p| p.comm.clone()).collect(),
            zombies: zombie_ppids.len(),
            blocked_names: {
                let mut b: Vec<String> = procs.iter().filter(|p| p.state == 'D').map(|p| p.comm.clone()).collect();
                b.sort();
                b.dedup();
                b
            },
        };
        let findings = doctor::diagnose(&sample, &out_apps, &ctx);
        let score = doctor::score(&findings);

        let procs_out = with_procs.then(|| {
            procs
                .into_iter()
                .map(|mut p| {
                    let mut flags = Vec::new();
                    if hung.contains_key(&p.pid) {
                        flags.push(Flag::Hung);
                    }
                    if p.state == 'T' {
                        flags.push(Flag::Paused);
                    }
                    if self.restart.get(&p.pid).is_some_and(|(s, _)| *s == p.started) {
                        flags.push(Flag::NeedsRestart);
                    }
                    if anr_windows.contains(&p.pid) {
                        flags.push(Flag::NotResponding);
                    }
                    if p.nice == 19 && super::actions::in_efficiency_mode(p.pid, p.nice) {
                        flags.push(Flag::Efficiency);
                    }
                    p.cmdline.truncate(64);
                    for a in &mut p.cmdline {
                        if a.len() > 400 {
                            let mut cut = 400;
                            while !a.is_char_boundary(cut) {
                                cut -= 1;
                            }
                            a.truncate(cut);
                        }
                    }
                    ProcFrame {
                        app: app_of.get(&p.pid).cloned().unwrap_or_default(),
                        flags,
                        p,
                    }
                })
                .collect()
        });

        Frame {
            kind: "frame",
            sample,
            apps: out_apps,
            procs: procs_out,
            findings,
            score,
            events: std::mem::take(&mut self.pending),
            crashes,
            failed_units,
            reboot_needed,
            zombies: zombie_ppids.len(),
        }
    }

    fn lifecycle_events(&mut self, groups: &[Group], dumps: &HashMap<i32, &CoreDump>) {
        let watched = |k: Kind| matches!(k, Kind::Window | Kind::Background);
        let now: HashMap<String, Known> = groups
            .iter()
            .filter(|g| watched(g.kind))
            .map(|g| (g.key.clone(), (g.name.clone(), g.icon.clone(), g.kind, g.pids.clone())))
            .collect();
        if self.first {
            self.first = false;
            self.known = now;
            return;
        }
        let names_now: HashSet<&String> = now.values().map(|v| &v.0).collect();
        let gone: Vec<(String, Known)> = self
            .known
            .iter()
            .filter(|(k, _)| !now.contains_key(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, (name, icon, _, pids)) in gone {
            // Same app under a new key (a second scope merged/split): no event.
            if names_now.contains(&name) {
                continue;
            }
            match pids.iter().find_map(|p| dumps.get(p)) {
                Some(d) => {
                    self.seen_dumps.insert((d.time, d.pid));
                    let detail = health::signal_name(d.sig).to_owned();
                    self.push_event("crashed", &name, &icon, &detail, &key);
                }
                None => self.push_event("closed", &name, &icon, "", &key),
            }
        }
        let names_before: HashSet<String> = self.known.values().map(|v| v.0.clone()).collect();
        let new: Vec<(String, String, String)> = now
            .iter()
            .filter(|(k, v)| !self.known.contains_key(*k) && !names_before.contains(&v.0))
            .map(|(k, v)| (k.clone(), v.0.clone(), v.1.clone()))
            .collect();
        for (key, name, icon) in new {
            self.push_event("started", &name, &icon, "", &key);
        }
        self.known = now;
    }

    fn crash_and_failure_events(&mut self, dumps: &[CoreDump], failed: &[FailedUnit], apps: &[AppFrame]) {
        if !self.dumps_primed {
            self.dumps_primed = true;
            self.seen_dumps.extend(dumps.iter().map(|d| (d.time, d.pid)));
        }
        for d in dumps {
            if self.seen_dumps.insert((d.time, d.pid)) {
                let base = exe_base(&d.exe, &[]);
                let app = apps.iter().find(|a| exe_base(&a.exe, &a.command) == base);
                let (name, icon, key) = app.map_or((base.clone(), base.to_lowercase(), String::new()), |a| {
                    (a.name.clone(), a.icon.clone(), a.key.clone())
                });
                self.push_event("crashed", &name, &icon, health::signal_name(d.sig), &key);
            }
        }
        let now: HashSet<String> = failed.iter().map(|f| f.unit.clone()).collect();
        if !self.failed_primed {
            self.failed_primed = true;
            self.seen_failed = now;
            return;
        }
        let fresh: Vec<&FailedUnit> = failed.iter().filter(|f| !self.seen_failed.contains(&f.unit)).collect();
        for f in fresh {
            let name = if f.description.is_empty() { f.unit.clone() } else { f.description.clone() };
            self.push_event("failed", &name, "", &f.unit, "");
        }
        self.seen_failed = now;
    }

    fn anr_events(&mut self, apps: &[AppFrame]) {
        let now: HashSet<String> = apps.iter().filter(|a| a.flags.contains(&Flag::NotResponding)).map(|a| a.key.clone()).collect();
        let fresh: Vec<&AppFrame> = apps.iter().filter(|a| now.contains(&a.key) && !self.anr.contains(&a.key)).collect();
        for a in fresh {
            self.push_event("not-responding", &a.name, &a.icon, "", &a.key);
        }
        let back: Vec<&AppFrame> = apps.iter().filter(|a| self.anr.contains(&a.key) && !now.contains(&a.key)).collect();
        let back: Vec<(String, String, String)> = back.iter().map(|a| (a.name.clone(), a.icon.clone(), a.key.clone())).collect();
        for (name, icon, key) in back {
            self.push_event("responding", &name, &icon, "", &key);
        }
        self.anr = now;
    }

    fn oom_event(&mut self, kills: u64) {
        if let Some(prev) = self.oom
            && kills > prev
        {
            let victim = run_text("journalctl", &["-k", "-o", "cat", "--no-pager", "--since=-2min", "-g", "Killed process"])
                .and_then(|t| t.lines().last().map(oom_victim))
                .unwrap_or_default();
            let n = (kills - prev).to_string();
            self.push_event("oom", &victim, "", &n, "");
        }
        self.oom = Some(kills);
    }
}

/// Where NixOS looks for programs (system, Home Manager, nix profile).
fn nix_profiles() -> Vec<PathBuf> {
    let home = crate::paths::home_dir();
    let user = std::env::var("USER").unwrap_or_default();
    vec![
        PathBuf::from(format!("/etc/profiles/per-user/{user}/bin")),
        home.join(".nix-profile/bin"),
        home.join(".local/state/nix/profile/bin"),
        PathBuf::from("/run/current-system/sw/bin"),
    ]
}

/// `/nix/store/<hash>-firefox-130.0/…` → ("<hash>", "firefox").
fn store_package(path: &str) -> Option<(&str, &str)> {
    let dir = path.strip_prefix("/nix/store/")?.split('/').next()?;
    let (hash, name) = dir.split_once('-')?;
    // The name ends where the version starts (first "-<digit>").
    let base = name
        .match_indices('-')
        .find(|(i, _)| name[i + 1..].starts_with(|c: char| c.is_ascii_digit()))
        .map_or(name, |(i, _)| &name[..i]);
    Some((hash, base))
}

/// A running store program is outdated when the profile now resolves the
/// same package (by name, any version) to another store path.
pub fn nix_outdated(running: &str, current: &str) -> bool {
    match (store_package(running), store_package(current)) {
        (Some((h1, n1)), Some((h2, n2))) => n1 == n2 && h1 != h2,
        _ => false,
    }
}

/// `Out of memory: Killed process 1234 (firefox) total-vm:…` → firefox.
pub fn oom_victim(line: &str) -> String {
    line.split_once('(')
        .and_then(|(_, r)| r.split_once(')'))
        .map(|(n, _)| n.to_owned())
        .unwrap_or_default()
}

fn exe_base(exe: &str, cmdline: &[String]) -> String {
    let e = exe.trim_end_matches(" (deleted)");
    let src = if e.is_empty() { cmdline.first().map(String::as_str).unwrap_or("") } else { e };
    src.rsplit('/').next().unwrap_or("").to_owned()
}

fn crash_groups(dumps: &[CoreDump], apps: &[AppFrame], now_us: u64) -> Vec<Crash> {
    let mut by_exe: HashMap<String, Crash> = HashMap::new();
    for d in dumps.iter().filter(|d| now_us.saturating_sub(d.time) < 24 * 3_600_000_000) {
        let base = exe_base(&d.exe, &[]);
        let c = by_exe.entry(d.exe.clone()).or_insert_with(|| {
            let app = apps.iter().find(|a| exe_base(&a.exe, &a.command) == base);
            Crash {
                exe: d.exe.clone(),
                name: app.map_or(base.clone(), |a| a.name.clone()),
                icon: app.map_or(String::new(), |a| a.icon.clone()),
                running: app.map(|a| a.key.clone()),
                ..Default::default()
            }
        });
        c.count += 1;
        if d.time / 1000 >= c.last {
            c.last = d.time / 1000;
            c.signal = health::signal_name(d.sig).into();
        }
    }
    let mut v: Vec<Crash> = by_exe.into_values().collect();
    v.sort_by_key(|c| std::cmp::Reverse(c.last));
    v
}

fn unit_frozen(g: &Group) -> bool {
    let Some(unit) = &g.unit else { return false };
    if !unit.starts_with("app-") {
        return false;
    }
    // cgroup.freeze of the unit's cgroup (systemctl freeze).
    let path = format!("/proc/{}/cgroup", g.main_pid);
    let cg = std::fs::read_to_string(path).map(|t| procfs::parse_cgroup(&t)).unwrap_or_default();
    std::fs::read_to_string(format!("/sys/fs/cgroup{cg}/cgroup.freeze")).is_ok_and(|v| v.trim() == "1")
}

/// The running kernel's modules are gone: an update installed a new kernel.
pub fn kernel_updated() -> bool {
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let release = release.trim();
    if release.is_empty() {
        return false;
    }
    if crate::nixos::running_nixos() {
        let booted = std::fs::canonicalize("/run/booted-system/kernel").ok();
        let current = std::fs::canonicalize("/run/current-system/kernel").ok();
        return booted.is_some() && current.is_some() && booted != current;
    }
    let modules = std::path::Path::new("/usr/lib/modules");
    modules.is_dir() && !modules.join(release).exists()
}

pub fn run_text(cmd: &str, args: &[&str]) -> Option<String> {
    let exe = crate::paths::find_executable(cmd)?;
    let out = std::process::Command::new(exe)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .env("LC_ALL", "C.UTF-8")
        .env("SYSTEMD_COLORS", "0")
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn parse_clients(json: &str) -> Vec<WindowInfo> {
    let list: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    list.iter()
        .filter(|c| c.get("mapped").and_then(|v| v.as_bool()).unwrap_or(true))
        .filter_map(|c| {
            Some(WindowInfo {
                pid: c.get("pid")?.as_i64()? as i32,
                address: c.get("address")?.as_str()?.to_owned(),
                class: c.get("class").and_then(|v| v.as_str()).unwrap_or("").to_owned(),
                title: c.get("title").and_then(|v| v.as_str()).unwrap_or("").to_owned(),
                workspace: c.pointer("/workspace/name").and_then(|v| v.as_str()).unwrap_or("").to_owned(),
                focused: c.get("focusHistoryID").and_then(|v| v.as_i64()) == Some(0),
            })
        })
        .filter(|w| w.pid > 0)
        .collect()
}

fn hypr_windows() -> Vec<WindowInfo> {
    crate::hyprland::clients_json().map(|j| parse_clients(&j)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nix_store_programs_are_outdated_after_a_switch() {
        let run = "/nix/store/aaaa-firefox-130.0/lib/firefox/firefox";
        assert!(nix_outdated(run, "/nix/store/bbbb-firefox-131.0/bin/firefox"));
        assert!(!nix_outdated(run, "/nix/store/aaaa-firefox-130.0/bin/firefox"));
        // A wrapper of another package isn't an update of this one.
        assert!(!nix_outdated(run, "/nix/store/cccc-firefox-wrapper/bin/firefox"));
        assert!(!nix_outdated("/usr/bin/firefox", "/nix/store/bbbb-firefox-131.0/bin/firefox"));
        assert_eq!(store_package("/nix/store/h-python3-3.13.1-env/bin/python"), Some(("h", "python3")));
    }

    #[test]
    fn clients_and_oom_lines() {
        let w = parse_clients(
            r#"[{"address":"0x1","mapped":true,"pid":10,"class":"firefox","title":"T","workspace":{"id":1,"name":"1"},"focusHistoryID":0},
                {"address":"0x2","mapped":false,"pid":11,"class":"x","title":"","workspace":{"id":1,"name":"1"},"focusHistoryID":1},
                {"address":"0x3","pid":-1,"class":"y"}]"#,
        );
        assert_eq!(w.len(), 1);
        assert!(w[0].focused);
        assert_eq!(w[0].workspace, "1");
        assert_eq!(oom_victim("Out of memory: Killed process 1234 (firefox) total-vm:1kB"), "firefox");
    }

    /// One real tick on this machine (needs /proc; Hyprland optional).
    #[test]
    fn ticks_on_this_machine() {
        let mut m = Monitor::new();
        let _ = m.tick(true);
        std::thread::sleep(Duration::from_millis(100));
        let f = m.tick(true);
        assert!(!f.apps.is_empty());
        if std::fs::read_to_string("/proc/2/comm").is_ok_and(|c| c.trim() == "kthreadd") {
            assert!(f.apps.iter().any(|a| a.kind == Kind::Kernel));
        }
        let procs = f.procs.expect("procs");
        assert!(procs.iter().any(|p| p.p.pid == std::process::id() as i32 && !p.app.is_empty()));
        assert!(f.score <= 100);
        let json = serde_json::to_string(&Frame { procs: None, ..f }).unwrap();
        assert!(json.contains("\"type\":\"frame\"") && json.contains("\"apps\""));
    }
}
