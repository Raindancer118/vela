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
}

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
}

impl Recorder {
    pub fn new(interval_ms: u64, minutes: u32) -> Recorder {
        Recorder {
            keep: ((minutes as u64 * 60_000) / interval_ms.max(1)).max(10) as usize,
            series: BTreeMap::new(),
            events: VecDeque::new(),
            interval_ms,
            last_t: 0,
            ticks: 0,
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
        let pts = points(f);
        // Keys that disappeared (unplugged disk) still advance.
        let len = self.series.values().map(VecDeque::len).max().unwrap_or(0);
        for (k, v) in pts {
            let q = self.series.entry(k).or_insert_with(|| std::iter::repeat_n(0.0, len).collect());
            q.push_back(v);
        }
        for q in self.series.values_mut() {
            if q.len() < len + 1 {
                q.push_back(0.0);
            }
            while q.len() > self.keep {
                q.pop_front();
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
            let mut rec = Recorder::new(cfg.record_interval_secs as u64 * 1000, cfg.history_minutes);
            if let Some(b) = load() {
                rec.resume(b);
            }
            let mut cfg_at = Instant::now();
            loop {
                if cfg_at.elapsed() >= Duration::from_secs(30) {
                    cfg_at = Instant::now();
                    let next = super::actions::load_config().pulse;
                    if next.record_interval_secs != cfg.record_interval_secs || next.history_minutes != cfg.history_minutes {
                        rec = Recorder::new(next.record_interval_secs as u64 * 1000, next.history_minutes);
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
    fn keeps_a_window_and_aligns_series() {
        let mut m = Monitor::light();
        let mut r = Recorder::new(1000, 5);
        assert_eq!(r.keep, 300);
        r.keep = 3;
        for _ in 0..5 {
            let f = m.tick(false);
            r.record(&f);
        }
        let b = r.backlog();
        assert!(
            b.series.values().all(|v| v.len() == 3),
            "{:?}",
            b.series.values().map(Vec::len).collect::<Vec<_>>()
        );
        assert!(b.series.contains_key("cpu") && b.series.contains_key("mem"));
        let json = serde_json::to_string(&b).unwrap();
        let back: Backlog = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
        let mut r2 = Recorder::new(1000, 5);
        r2.resume(back);
        assert_eq!(r2.series["cpu"].len(), 3);
        // A different interval keeps the events but not the series.
        let mut r3 = Recorder::new(3000, 5);
        r3.resume(b);
        assert!(r3.series.is_empty());
    }
}
