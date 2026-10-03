//! Health of apps and processes over time: hung on I/O, not responding,
//! stopped, zombies, runaway CPU, memory that keeps growing, programs that
//! were updated underneath a running process, crashes.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Flag {
    /// Hyprland's "not responding" dialog is open for one of its windows.
    NotResponding,
    /// Stuck in uninterruptible sleep (usually waiting for a disk or a
    /// network filesystem).
    Hung,
    /// Crashed recently (core dump), or keeps crashing.
    Crashed,
    /// Uses a whole core or more for a long time.
    Runaway,
    /// Memory grew steadily for many minutes.
    Leak,
    /// Its program or libraries were updated; a restart loads the new ones.
    NeedsRestart,
    /// Stopped (SIGSTOP / Ctrl+Z) or frozen by systemd.
    Paused,
    /// Has zombie children nobody reaps.
    Zombies,
    /// In efficiency mode (idle CPU weight, or nice 19 with idle I/O).
    Efficiency,
}

impl Flag {
    /// How bad, for the app's dot and sorting: 2 = broken, 1 = worth a look.
    pub fn severity(self) -> u8 {
        match self {
            Flag::NotResponding | Flag::Hung | Flag::Crashed => 2,
            Flag::Runaway | Flag::Leak | Flag::NeedsRestart | Flag::Zombies => 1,
            Flag::Paused | Flag::Efficiency => 0,
        }
    }
}

/// A core dump from `coredumpctl list --json=short`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CoreDump {
    /// Microseconds since the epoch.
    pub time: u64,
    pub pid: i32,
    pub uid: u32,
    pub sig: i32,
    #[serde(default)]
    pub exe: String,
}

pub fn parse_coredumps(json: &str) -> Vec<CoreDump> {
    serde_json::from_str(json.trim()).unwrap_or_default()
}

/// Signal name for crash reports.
pub fn signal_name(sig: i32) -> &'static str {
    match sig {
        4 => "SIGILL",
        5 => "SIGTRAP",
        6 => "SIGABRT",
        7 => "SIGBUS",
        8 => "SIGFPE",
        11 => "SIGSEGV",
        31 => "SIGSYS",
        _ => "signal",
    }
}

/// A unit from `systemctl list-units --output=json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct UnitRow {
    pub unit: String,
    #[serde(default)]
    pub load: String,
    #[serde(default)]
    pub active: String,
    #[serde(default)]
    pub sub: String,
    #[serde(default)]
    pub description: String,
}

pub fn parse_units(json: &str) -> Vec<UnitRow> {
    serde_json::from_str(json.trim()).unwrap_or_default()
}

/// Window keys (class, title) that Hyprland's ANR dialogs name. The
/// dialog text is "An application <title> - <class> is not responding"
/// (translated), so match on the parts rather than the sentence.
pub fn not_responding(dialog_cmdlines: &[Vec<String>], windows: &[(String, String)]) -> Vec<usize> {
    let texts: Vec<String> = dialog_cmdlines.iter().map(|c| c.join(" ")).collect();
    windows
        .iter()
        .enumerate()
        .filter(|(_, (class, title))| {
            !class.is_empty()
                && texts
                    .iter()
                    .any(|t| t.contains(&format!(" - {class}")) && (title.is_empty() || t.contains(title.as_str())))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Least-squares line through (t, v): (slope per second, r²).
pub fn trend(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    if points.len() < 3 {
        return None;
    }
    let mx = points.iter().map(|p| p.0).sum::<f64>() / n;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let syy: f64 = points.iter().map(|p| (p.1 - my).powi(2)).sum();
    if sxx == 0.0 {
        return None;
    }
    let slope = sxy / sxx;
    let r2 = if syy == 0.0 { 0.0 } else { (sxy * sxy) / (sxx * syy) };
    Some((slope, r2))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeakInfo {
    /// Bytes per minute.
    pub per_minute: f64,
    pub grown: f64,
    pub minutes: f64,
}

/// Memory that grows steadily: ≥ 8 minutes of points, a straight-line fit
/// (r² ≥ 0.9), ≥ 256 MiB and ≥ 20 % more than at the start.
pub fn leak(points: &[(f64, f64)]) -> Option<LeakInfo> {
    let (first, last) = (points.first()?, points.last()?);
    let minutes = (last.0 - first.0) / 60.0;
    if minutes < 8.0 {
        return None;
    }
    let (slope, r2) = trend(points)?;
    let grown = last.1 - first.1;
    (r2 >= 0.9 && slope > 0.0 && grown >= 256.0 * 1024.0 * 1024.0 && grown >= first.1 * 0.2).then_some(LeakInfo {
        per_minute: slope * 60.0,
        grown,
        minutes,
    })
}

/// Runaway: the last `window` seconds of CPU (percent of one core) all at
/// or above `floor` and on average ≥ 90 %.
pub fn runaway(history: &VecDeque<f64>, window: usize, floor: f64) -> bool {
    if history.len() < window {
        return false;
    }
    let tail: Vec<f64> = history.iter().rev().take(window).copied().collect();
    let mean = tail.iter().sum::<f64>() / tail.len() as f64;
    mean >= 90.0 && tail.iter().all(|v| *v >= floor)
}

/// Per-app history kept between ticks.
#[derive(Debug, Default)]
pub struct AppTrack {
    pub cpu: VecDeque<f64>,
    /// (seconds, bytes), one point per `MEM_EVERY` seconds.
    pub mem: VecDeque<(f64, f64)>,
    pub last_mem_point: f64,
    pub seen: u64,
}

pub const CPU_KEEP: usize = 120;
pub const MEM_EVERY: f64 = 15.0;
pub const MEM_KEEP: usize = 80;

impl AppTrack {
    pub fn push(&mut self, now_s: f64, cpu: f64, mem: u64) {
        self.cpu.push_back(cpu);
        while self.cpu.len() > CPU_KEEP {
            self.cpu.pop_front();
        }
        if now_s - self.last_mem_point >= MEM_EVERY {
            self.mem.push_back((now_s, mem as f64));
            self.last_mem_point = now_s;
            while self.mem.len() > MEM_KEEP {
                self.mem.pop_front();
            }
        }
        self.seen += 1;
    }

    pub fn leak(&self) -> Option<LeakInfo> {
        leak(&self.mem.iter().copied().collect::<Vec<_>>())
    }
}

/// When each pid entered uninterruptible sleep (D).
#[derive(Debug, Default)]
pub struct DTracker {
    since: HashMap<i32, (u64, f64)>,
}

impl DTracker {
    /// `procs`: (pid, start, state). Returns pids in D for ≥ `secs`.
    pub fn update(&mut self, now_s: f64, procs: &[(i32, u64, char)], secs: f64) -> Vec<(i32, f64)> {
        let mut next = HashMap::new();
        let mut hung = Vec::new();
        for &(pid, start, state) in procs {
            if state != 'D' {
                continue;
            }
            let since = match self.since.get(&pid) {
                Some(&(s, t)) if s == start => t,
                _ => now_s,
            };
            next.insert(pid, (start, since));
            if now_s - since >= secs {
                hung.push((pid, now_s - since));
            }
        }
        self.since = next;
        hung
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coredumps_and_units() {
        let c = parse_coredumps(
            r#"[{"time":1790797827139385,"pid":353584,"uid":1000,"gid":1000,"sig":11,"corefile":"present","exe":"/usr/lib/xdg-desktop-portal-hyprland","size":164098}]"#,
        );
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].pid, c[0].sig, signal_name(c[0].sig)), (353584, 11, "SIGSEGV"));
        assert!(parse_coredumps("No coredumps found.").is_empty());
        let u = parse_units(r#"[{"unit":"app-x.service","load":"loaded","active":"failed","sub":"failed","description":"X"}]"#);
        assert_eq!(u[0].active, "failed");
    }

    #[test]
    fn anr_dialogs_match_windows() {
        let dialogs = vec![vec![
            "hyprland-dialog".into(),
            "--title".into(),
            "Application Not Responding".into(),
            "--text".into(),
            "An application Untitled - Gedit - org.gnome.gedit is not responding.".into(),
        ]];
        let windows = vec![
            ("org.gnome.gedit".to_owned(), "Untitled - Gedit".to_owned()),
            ("firefox".to_owned(), "Mozilla".to_owned()),
            ("org.gnome.gedit".to_owned(), "Other doc".to_owned()),
        ];
        assert_eq!(not_responding(&dialogs, &windows), vec![0]);
        assert!(not_responding(&[], &windows).is_empty());
    }

    #[test]
    fn leak_needs_a_steady_climb() {
        let mib = 1024.0 * 1024.0;
        let climb: Vec<(f64, f64)> = (0..40).map(|i| (i as f64 * 15.0, 500.0 * mib + i as f64 * 10.0 * mib)).collect();
        let l = leak(&climb).expect("leak");
        assert!((l.per_minute / mib - 40.0).abs() < 0.5, "{l:?}");
        // Too short.
        assert!(leak(&climb[..20]).is_none());
        // Noisy up and down.
        let saw: Vec<(f64, f64)> = (0..40)
            .map(|i| (i as f64 * 15.0, 500.0 * mib + if i % 2 == 0 { 0.0 } else { 400.0 * mib }))
            .collect();
        assert!(leak(&saw).is_none());
        // Small growth on a big process.
        let small: Vec<(f64, f64)> = (0..40).map(|i| (i as f64 * 15.0, 8000.0 * mib + i as f64 * 8.0 * mib)).collect();
        assert!(leak(&small).is_none());
    }

    #[test]
    fn runaway_cpu() {
        let mut h: VecDeque<f64> = std::iter::repeat_n(99.0, 60).collect();
        assert!(runaway(&h, 60, 50.0));
        h.push_back(10.0);
        assert!(!runaway(&h, 60, 50.0));
        assert!(!runaway(&VecDeque::from(vec![100.0; 10]), 60, 50.0));
    }

    #[test]
    fn d_state_needs_time() {
        let mut d = DTracker::default();
        assert!(d.update(0.0, &[(1, 7, 'D'), (2, 7, 'S')], 15.0).is_empty());
        assert!(d.update(10.0, &[(1, 7, 'D')], 15.0).is_empty());
        assert_eq!(d.update(20.0, &[(1, 7, 'D')], 15.0), vec![(1, 20.0)]);
        // pid reused: starts over.
        assert!(d.update(21.0, &[(1, 9, 'D')], 15.0).is_empty());
        // Woke up in between: starts over.
        d.update(22.0, &[(1, 9, 'S')], 15.0);
        assert!(d.update(40.0, &[(1, 9, 'D')], 15.0).is_empty());
    }
}
