//! Background recording in the daemon (`[pulse] record`): a light sample
//! every few seconds, kept as graph history and events in a file in the
//! runtime directory. `vela pulse serve` sends it as a backlog, so the
//! window opens with the last hour already drawn and shows what crashed,
//! hung or ran out of memory while it was closed.

use super::engine::{Event, Frame, Monitor};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Backlog {
    /// Milliseconds between points.
    pub interval: u64,
    /// Time of the newest point (ms since the epoch).
    pub t: u64,
    pub series: BTreeMap<String, Vec<f32>>,
    pub events: Vec<Event>,
    /// The long view: one average per `coarse_interval` ms, newest last.
    #[serde(default)]
    pub coarse_interval: u64,
    /// `null` = no data for that minute (off, asleep).
    #[serde(default)]
    pub coarse: BTreeMap<String, Vec<Option<f32>>>,
}

/// The minute averages, kept across reboots: one JSON line per minute
/// (`{"t":…,"v":{key:value}}`), appended, compacted once a day.
pub fn history_file() -> PathBuf {
    crate::paths::state_dir().join("pulse-history.jsonl")
}

#[derive(Serialize, Deserialize)]
struct Minute {
    t: u64,
    v: BTreeMap<String, f32>,
}

/// The long view for the window: `secs` of minute averages from the history
/// file, averaged into at most `max_points` buckets; (bucket ms, series).
pub fn long_view(secs: u64, max_points: usize, now: u64) -> (u64, BTreeMap<String, Vec<Option<f32>>>) {
    let minutes = (secs * 1000 / COARSE_MS).max(1) as usize;
    let text = std::fs::read_to_string(history_file()).unwrap_or_default();
    let series = read_history(&text, now, minutes);
    let bucket = minutes.div_ceil(max_points.max(1)).max(1);
    let out = series
        .into_iter()
        .map(|(k, q)| {
            let v: Vec<Option<f32>> = q.into_iter().collect();
            let start = v.len() % bucket;
            let b: Vec<Option<f32>> = v[start..]
                .chunks(bucket)
                .map(|c| {
                    let s: Vec<f32> = c.iter().flatten().copied().collect();
                    (!s.is_empty()).then(|| s.iter().sum::<f32>() / s.len() as f32)
                })
                .collect();
            (k, b)
        })
        .collect();
    (COARSE_MS * bucket as u64, out)
}

/// Minutes from the history file within `keep` minutes before `now`, as
/// aligned series with `None` where nothing was recorded.
pub fn read_history(text: &str, now: u64, keep: usize) -> BTreeMap<String, VecDeque<Option<f32>>> {
    let from = now.saturating_sub(keep as u64 * COARSE_MS);
    let mut out: BTreeMap<String, VecDeque<Option<f32>>> = BTreeMap::new();
    let mut len = 0usize;
    let mut last: Option<u64> = None;
    let push_gap = |out: &mut BTreeMap<String, VecDeque<Option<f32>>>, len: &mut usize, n: usize| {
        for q in out.values_mut() {
            q.extend(std::iter::repeat_n(None, n));
        }
        *len += n;
    };
    for m in text
        .lines()
        .filter_map(|l| serde_json::from_str::<Minute>(l).ok())
        .filter(|m| m.t >= from && m.t <= now)
    {
        if let Some(prev) = last {
            if m.t <= prev {
                continue;
            }
            let missing = ((m.t - prev) as f64 / COARSE_MS as f64).round() as usize;
            push_gap(&mut out, &mut len, missing.saturating_sub(1).min(keep));
        }
        last = Some(m.t);
        for (k, v) in &m.v {
            out.entry(k.clone())
                .or_insert_with(|| std::iter::repeat_n(None, len).collect())
                .push_back(Some(*v));
        }
        len += 1;
        for q in out.values_mut() {
            if q.len() < len {
                q.push_back(None);
            }
        }
    }
    // Up to now: the time since the last line was not recorded.
    if let Some(prev) = last {
        let missing = (now.saturating_sub(prev) / COARSE_MS) as usize;
        push_gap(&mut out, &mut len, missing.min(keep));
    }
    for q in out.values_mut() {
        while q.len() > keep {
            q.pop_front();
        }
    }
    out
}

/// One point of the long history per minute.
pub const COARSE_MS: u64 = 60_000;

/// In the private runtime directory only; without one (no session) there
/// is no backlog rather than a predictable file in /tmp.
pub fn file() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?);
    dir.is_dir().then(|| dir.join("vela-pulse-backlog.json"))
}

/// The values one frame contributes, under the keys the window uses.
pub fn points(f: &Frame) -> Vec<(String, f32)> {
    let s = &f.sample;
    let mut p: Vec<(String, f32)> = vec![
        ("cpu".into(), s.cpu.usage as f32),
        ("cpu.iowait".into(), s.cpu.iowait as f32),
        ("cpu.mhz".into(), s.cpu.avg_mhz as f32),
        ("cpu.pressure".into(), s.cpu.pressure as f32),
        (
            "mem".into(),
            if s.memory.total > 0 {
                (s.memory.used as f64 / s.memory.total as f64 * 100.0) as f32
            } else {
                0.0
            },
        ),
        (
            "swap".into(),
            if s.memory.swap_total > 0 {
                (s.memory.swap_used as f64 / s.memory.swap_total as f64 * 100.0) as f32
            } else {
                0.0
            },
        ),
        ("mem.pressure".into(), s.memory.pressure as f32),
        ("io.pressure".into(), s.io.pressure_full as f32),
    ];
    if let Some(t) = s.cpu.temp {
        p.push(("cpu.temp".into(), t as f32));
    }
    for (i, c) in s.cpu.cores.iter().enumerate() {
        p.push((format!("core.{i}"), *c as f32));
    }
    for g in &s.gpus {
        p.push((format!("gpu.{}", g.card), g.busy.unwrap_or(0.0) as f32));
        if let (Some(u), Some(t)) = (g.vram_used, g.vram_total)
            && t > 0
        {
            p.push((format!("vram.{}", g.card), (u as f64 / t as f64 * 100.0) as f32));
        }
    }
    for d in &s.disks {
        p.push((format!("disk.r.{}", d.name), d.read_bps as f32));
        p.push((format!("disk.w.{}", d.name), d.write_bps as f32));
        p.push((format!("disk.busy.{}", d.name), d.busy as f32));
    }
    for n in &s.net {
        p.push((format!("net.rx.{}", n.iface), n.rx_bps as f32));
        p.push((format!("net.tx.{}", n.iface), n.tx_bps as f32));
    }
    for t in &s.sensors {
        p.push((format!("sensor.{}/{}", t.chip, t.label), t.celsius as f32));
    }
    for f in &s.fans {
        p.push((format!("fan.{}/{}", f.chip, f.label), f.rpm as f32));
    }
    let watts: f64 = s.power.batteries.iter().filter(|b| b.status == "discharging").map(|b| b.watts).sum();
    p.push(("power.watts".into(), watts as f32));
    p
}

pub struct Recorder {
    keep: usize,
    series: BTreeMap<String, VecDeque<f32>>,
    events: VecDeque<Event>,
    pub interval_ms: u64,
    last_t: u64,
    ticks: u64,
    coarse_keep: usize,
    coarse: BTreeMap<String, VecDeque<Option<f32>>>,
    coarse_len: usize,
    /// Sums and counts of the minute being averaged.
    acc: BTreeMap<String, (f64, u32)>,
    acc_start: u64,
    /// Where the minutes go (None in tests).
    pub history: Option<PathBuf>,
    appended: usize,
}

impl Recorder {
    /// `minutes` of fine history, `hours` of minute averages (0 = none).
    pub fn new(interval_ms: u64, minutes: u32, hours: u32) -> Recorder {
        Recorder {
            keep: ((minutes as u64 * 60_000) / interval_ms.max(1)).max(10) as usize,
            series: BTreeMap::new(),
            events: VecDeque::new(),
            interval_ms,
            last_t: 0,
            ticks: 0,
            coarse_keep: (hours as u64 * 3_600_000 / COARSE_MS) as usize,
            coarse: BTreeMap::new(),
            coarse_len: 0,
            acc: BTreeMap::new(),
            acc_start: 0,
            history: None,
            appended: 0,
        }
    }

    /// Loads the minute averages from the history file (after a reboot too).
    pub fn load_history(&mut self, path: PathBuf, now: u64) {
        if self.coarse_keep > 0 {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            self.coarse = read_history(&text, now, self.coarse_keep);
            self.coarse_len = self.coarse.values().map(VecDeque::len).max().unwrap_or(0);
        }
        self.history = Some(path);
    }

    fn append(&mut self, m: &Minute) {
        let Some(path) = &self.history else { return };
        use std::io::Write;
        let line = serde_json::to_string(m).unwrap_or_default();
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
        let ok = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| writeln!(f, "{line}"))
            .is_ok();
        if !ok {
            return;
        }
        self.appended += 1;
        // Once a day: drop what is older than the window.
        if self.appended >= 1440 {
            self.appended = 0;
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let from = m.t.saturating_sub(self.coarse_keep as u64 * COARSE_MS);
            let kept: String = text
                .lines()
                .filter(|l| serde_json::from_str::<Minute>(l).is_ok_and(|x| x.t >= from))
                .map(|l| format!("{l}\n"))
                .collect();
            let _ = crate::config::write_atomic(path, &kept);
        }
    }

    /// Continues from a previous daemon's file (same interval only).
    pub fn resume(&mut self, b: Backlog) {
        if b.interval != self.interval_ms {
            self.events = b.events.into();
            return;
        }
        for (k, v) in b.series {
            let mut q: VecDeque<f32> = v.into();
            while q.len() > self.keep {
                q.pop_front();
            }
            self.series.insert(k, q);
        }
        self.events = b.events.into();
        self.last_t = b.t;
    }

    pub fn record(&mut self, f: &Frame) {
        self.ticks += 1;
        // Series end at the newest sample; one that starts later (a new
        // sensor) is just shorter — no made-up zeros before it.
        let pts = points(f);
        let now: std::collections::HashSet<String> = pts.iter().map(|(k, _)| k.clone()).collect();
        self.series.retain(|k, _| now.contains(k));
        for (k, v) in &pts {
            let q = self.series.entry(k.clone()).or_default();
            q.push_back(*v);
            while q.len() > self.keep {
                q.pop_front();
            }
        }
        if self.coarse_keep > 0 {
            let t = f.sample.t;
            if self.acc_start == 0 {
                self.acc_start = t;
            }
            for (k, v) in &pts {
                let e = self.acc.entry(k.clone()).or_default();
                e.0 += *v as f64;
                e.1 += 1;
            }
            if t.saturating_sub(self.acc_start) >= COARSE_MS {
                let minute: BTreeMap<String, f32> = std::mem::take(&mut self.acc)
                    .into_iter()
                    .map(|(k, (sum, n))| (k, (sum / n.max(1) as f64) as f32))
                    .collect();
                let len = self.coarse_len;
                for (k, v) in &minute {
                    self.coarse
                        .entry(k.clone())
                        .or_insert_with(|| std::iter::repeat_n(None, len).collect())
                        .push_back(Some(*v));
                }
                self.coarse_len += 1;
                for q in self.coarse.values_mut() {
                    if q.len() < self.coarse_len {
                        q.push_back(None);
                    }
                    while q.len() > self.coarse_keep {
                        q.pop_front();
                    }
                }
                self.coarse_len = self.coarse_len.min(self.coarse_keep);
                // Keys gone for the whole window disappear.
                self.coarse.retain(|_, q| q.iter().any(Option::is_some));
                self.append(&Minute { t, v: minute });
                self.acc_start = t;
            }
        }
        self.events.extend(f.events.iter().cloned());
        while self.events.len() > super::engine::EVENT_KEEP {
            self.events.pop_front();
        }
        self.last_t = f.sample.t;
    }

    pub fn backlog(&self) -> Backlog {
        Backlog {
            interval: self.interval_ms,
            t: self.last_t,
            series: self.series.iter().map(|(k, v)| (k.clone(), v.iter().copied().collect())).collect(),
            events: self.events.iter().cloned().collect(),
            // The long history stays in its own file (history_file).
            coarse_interval: 0,
            coarse: BTreeMap::new(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = file() else { return Ok(()) };
        crate::config::write_atomic(&path, &String::from_utf8_lossy(&serde_json::to_vec(&self.backlog())?)).map_err(|e| std::io::Error::other(e.to_string()))
    }
}

pub fn load() -> Option<Backlog> {
    serde_json::from_slice(&std::fs::read(file()?).ok()?).ok()
}

/// Runs the recorder on its own thread for as long as the process lives.
pub fn spawn() {
    std::thread::Builder::new()
        .name("pulse-recorder".into())
        .spawn(|| {
            let mut cfg = super::actions::load_config().pulse;
            let mut monitor = Monitor::light();
            let mut rec = Recorder::new(cfg.record_interval_secs as u64 * 1000, cfg.history_minutes, cfg.long_history_hours);
            rec.load_history(history_file(), super::sample::now_ms());
            if let Some(b) = load() {
                rec.resume(b);
            }
            let mut cfg_at = Instant::now();
            loop {
                if cfg_at.elapsed() >= Duration::from_secs(30) {
                    cfg_at = Instant::now();
                    let next = super::actions::load_config().pulse;
                    if next.record_interval_secs != cfg.record_interval_secs
                        || next.history_minutes != cfg.history_minutes
                        || next.long_history_hours != cfg.long_history_hours
                    {
                        let mut fresh = Recorder::new(next.record_interval_secs as u64 * 1000, next.history_minutes, next.long_history_hours);
                        fresh.load_history(history_file(), super::sample::now_ms());
                        fresh.resume(rec.backlog());
                        rec = fresh;
                    }
                    cfg = next;
                }
                if !cfg.record || file().is_none() {
                    if let Some(f) = file() {
                        let _ = std::fs::remove_file(f);
                    }
                    std::thread::sleep(Duration::from_secs(10));
                    continue;
                }
                let started = Instant::now();
                let frame = monitor.tick(false);
                rec.record(&frame);
                // Every ~15 s, and right away when something happened.
                if (rec.ticks * rec.interval_ms) % 15_000 == 0 || !frame.events.is_empty() {
                    if let Err(e) = rec.save() {
                        log::warn!("pulse: cannot write the backlog: {e}");
                    }
                }
                let wait = Duration::from_millis(rec.interval_ms).saturating_sub(started.elapsed());
                std::thread::sleep(wait);
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_lines_become_series_with_gaps() {
        let m = 60_000;
        let text = format!(
            "{{\"t\":{},\"v\":{{\"cpu\":10.0}}}}\n{{\"t\":{},\"v\":{{\"cpu\":20.0,\"gpu\":5.0}}}}\nbroken\n{{\"t\":{},\"v\":{{\"cpu\":30.0}}}}\n",
            100 * m,
            101 * m,
            105 * m
        );
        let s = read_history(&text, 107 * m, 100);
        let cpu: Vec<Option<f32>> = s["cpu"].iter().copied().collect();
        assert_eq!(cpu, vec![Some(10.0), Some(20.0), None, None, None, Some(30.0), None, None]);
        let gpu: Vec<Option<f32>> = s["gpu"].iter().copied().collect();
        assert_eq!(gpu, vec![None, Some(5.0), None, None, None, None, None, None]);
        // Older than the window: left out.
        assert_eq!(read_history(&text, 107 * m, 3)["cpu"].len(), 3);
    }

    #[test]
    fn minutes_are_appended_to_the_history_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("h.jsonl");
        let mut r = Recorder::new(1000, 5, 1);
        r.load_history(path.clone(), 0);
        r.append(&Minute {
            t: 60_000,
            v: BTreeMap::from([("cpu".to_owned(), 1.5)]),
        });
        r.append(&Minute {
            t: 120_000,
            v: BTreeMap::from([("cpu".to_owned(), 2.5)]),
        });
        let s = read_history(&std::fs::read_to_string(&path).unwrap(), 120_000, 60);
        assert_eq!(s["cpu"].iter().copied().collect::<Vec<_>>(), vec![Some(1.5), Some(2.5)]);
    }

    #[test]
    fn keeps_a_window_and_aligns_series() {
        let mut m = Monitor::light();
        let mut r = Recorder::new(1000, 5, 1);
        assert_eq!(r.keep, 300);
        r.keep = 3;
        for _ in 0..5 {
            let f = m.tick(false);
            r.record(&f);
        }
        let b = r.backlog();
        assert!(
            b.series.values().all(|v| v.len() <= 3),
            "{:?}",
            b.series.values().map(Vec::len).collect::<Vec<_>>()
        );
        assert!(b.series.contains_key("cpu") && b.series.contains_key("mem"));
        let json = serde_json::to_string(&b).unwrap();
        let back: Backlog = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
        let mut r2 = Recorder::new(1000, 5, 1);
        r2.resume(back);
        assert_eq!(r2.series["cpu"].len(), 3);
        // A different interval keeps the events but not the series.
        let mut r3 = Recorder::new(3000, 5, 1);
        r3.resume(b);
        assert!(r3.series.is_empty());
    }
}
