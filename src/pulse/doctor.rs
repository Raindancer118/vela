//! Turns a sample and the app list into findings: what slows the system
//! down or is broken, why, and what would fix it. Findings carry a kind and
//! raw values; the UI words them (and translates), `report` words them in
//! English for Claude.

use super::apps::Kind;
use super::engine::{AppFrame, Crash, FailedUnit};
use super::health::Flag;
use super::sample::Sample;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    /// end, force, restart, resume, efficiency, profile, reboot,
    /// unit-restart, unit-reset, show-app, show-perf, claude
    pub action: String,
    /// App key, unit, profile name or device.
    pub target: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

fn fix(action: &str, target: &str) -> Fix {
    Fix {
        action: action.into(),
        target: target.into(),
        detail: String::new(),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable per problem, so the UI can animate it in and out.
    pub id: String,
    pub kind: String,
    pub severity: Severity,
    pub apps: Vec<String>,
    pub values: Value,
    pub fixes: Vec<Fix>,
    /// Unix ms of the last frame that saw it, once it's over (see `Recent`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gone_since: Option<u64>,
}

pub struct Context<'a> {
    pub throttled_recently: u64,
    pub reboot_needed: bool,
    pub crashes: &'a [Crash],
    pub failed_units: &'a [FailedUnit],
    pub hung: usize,
    pub hung_names: Vec<String>,
    pub zombies: usize,
    /// Processes waiting on I/O right now (state D).
    pub blocked_names: Vec<String>,
}

fn user_app(a: &AppFrame) -> bool {
    matches!(a.kind, Kind::Window | Kind::Background | Kind::Task)
}

fn top(apps: &[AppFrame], by: impl Fn(&AppFrame) -> f64, n: usize) -> Vec<&AppFrame> {
    let mut v: Vec<&AppFrame> = apps.iter().filter(|a| a.kind != Kind::Kernel && by(a) > 0.0).collect();
    v.sort_by(|a, b| by(b).total_cmp(&by(a)));
    v.truncate(n);
    v
}

fn names(apps: &[&AppFrame], value: impl Fn(&AppFrame) -> f64) -> Value {
    apps.iter()
        .map(|a| json!({ "key": a.key, "name": a.name, "icon": a.icon, "value": value(a) }))
        .collect()
}

/// What can be done to the biggest consumer.
fn relief(a: &AppFrame) -> Vec<Fix> {
    let mut f = vec![fix("show-app", &a.key)];
    if user_app(a) {
        if a.kind == Kind::Background || a.kind == Kind::Task {
            f.push(fix("efficiency", &a.key));
        }
        f.push(fix("end", &a.key));
    } else if a.unit.as_deref().is_some_and(|u| u.ends_with(".service")) {
        f.push(fix("unit-restart", &a.key));
    }
    f
}

pub fn diagnose(s: &Sample, apps: &[AppFrame], ctx: &Context) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut add = |id: String, kind: &str, severity: Severity, apps: Vec<String>, values: Value, fixes: Vec<Fix>| {
        out.push(Finding {
            id,
            kind: kind.into(),
            severity,
            apps,
            values,
            fixes,
            gone_since: None,
        });
    };

    // Broken apps first.
    for a in apps.iter().filter(|a| a.flags.contains(&Flag::NotResponding)) {
        add(
            format!("anr:{}", a.key),
            "not-responding",
            Severity::Critical,
            vec![a.key.clone()],
            json!({ "name": a.name, "icon": a.icon }),
            vec![fix("force", &a.key), fix("restart", &a.key)],
        );
    }
    if ctx.hung > 0 {
        let hung_apps: Vec<&AppFrame> = apps.iter().filter(|a| a.flags.contains(&Flag::Hung)).collect();
        add(
            "hung".into(),
            "hung",
            Severity::Warning,
            hung_apps.iter().map(|a| a.key.clone()).collect(),
            json!({ "count": ctx.hung, "names": ctx.hung_names, "apps": names(&hung_apps, |_| 0.0), "ioPressure": s.io.pressure_full }),
            hung_apps.first().map(|a| vec![fix("show-app", &a.key)]).unwrap_or_default(),
        );
    }

    // CPU.
    let cpu_top = top(apps, |a| a.cpu, 3);
    let cpu_sev = if s.cpu.pressure60 >= 60.0 {
        Some(Severity::Critical)
    } else if s.cpu.pressure >= 40.0 || s.cpu.usage >= 90.0 && s.cpu.pressure >= 20.0 {
        Some(Severity::Warning)
    } else {
        None
    };
    if let Some(sev) = cpu_sev {
        add(
            "cpu".into(),
            "cpu-busy",
            sev,
            cpu_top.iter().map(|a| a.key.clone()).collect(),
            json!({ "usage": s.cpu.usage, "pressure": s.cpu.pressure, "top": names(&cpu_top, |a| a.cpu) }),
            cpu_top.first().map(|a| relief(a)).unwrap_or_default(),
        );
    }

    // Memory.
    let mem_top = top(apps, |a| a.mem as f64, 3);
    let avail_pct = if s.memory.total > 0 {
        s.memory.available as f64 / s.memory.total as f64 * 100.0
    } else {
        100.0
    };
    let mem_sev = if s.memory.pressure_full >= 20.0 || avail_pct < 4.0 {
        Some(Severity::Critical)
    } else if s.memory.pressure >= 10.0 || s.memory.pressure_full >= 5.0 || avail_pct < 8.0 {
        Some(Severity::Warning)
    } else {
        None
    };
    if let Some(sev) = mem_sev {
        add(
            "memory".into(),
            "memory-low",
            sev,
            mem_top.iter().map(|a| a.key.clone()).collect(),
            json!({ "available": s.memory.available, "total": s.memory.total, "availablePct": avail_pct, "pressure": s.memory.pressure, "top": names(&mem_top, |a| a.mem as f64) }),
            mem_top.first().map(|a| relief(a)).unwrap_or_default(),
        );
    }
    let swap_rate = s.memory.swap_in_bps + s.memory.swap_out_bps;
    if swap_rate >= 5.0 * 1024.0 * 1024.0 {
        add(
            "swap".into(),
            "swapping",
            if mem_sev.is_some() { Severity::Critical } else { Severity::Warning },
            mem_top.iter().map(|a| a.key.clone()).collect(),
            json!({ "in": s.memory.swap_in_bps, "out": s.memory.swap_out_bps, "used": s.memory.swap_used, "top": names(&mem_top, |a| a.mem as f64) }),
            vec![fix("show-perf", "memory")],
        );
    }

    // Disk I/O.
    let io_top = top(apps, |a| a.read_bps + a.write_bps, 3);
    if s.io.pressure_full >= 10.0 {
        let busiest = s.disks.iter().max_by(|a, b| a.busy.total_cmp(&b.busy));
        add(
            "io".into(),
            "io-busy",
            if s.io.pressure_full >= 30.0 { Severity::Critical } else { Severity::Warning },
            io_top.iter().map(|a| a.key.clone()).collect(),
            json!({ "pressure": s.io.pressure_full, "disk": busiest.map(|d| d.name.clone()), "busy": busiest.map(|d| d.busy), "top": names(&io_top, |a| a.read_bps + a.write_bps), "blocked": ctx.blocked_names, "swapUsed": s.memory.swap_used }),
            io_top.first().map(|a| relief(a)).unwrap_or_else(|| vec![fix("show-perf", "disk")]),
        );
    }

    // Heat.
    if let Some(t) = s.cpu.temp {
        let crit = s.cpu.temp_crit.unwrap_or(100.0);
        let fan = s.fans.iter().map(|f| f.rpm).max();
        if t >= crit - 5.0 || t >= 97.0 {
            add(
                "heat".into(),
                "cpu-hot",
                Severity::Critical,
                cpu_top.iter().map(|a| a.key.clone()).collect(),
                json!({ "temp": t, "crit": crit, "fan": fan }),
                vec![fix("show-perf", "cpu")],
            );
        } else if ctx.throttled_recently > 0 {
            add(
                "throttle".into(),
                "cpu-throttling",
                Severity::Warning,
                cpu_top.iter().map(|a| a.key.clone()).collect(),
                json!({ "temp": t, "events": ctx.throttled_recently, "fan": fan }),
                vec![fix("show-perf", "cpu")],
            );
        }
    }

    // Power.
    let on_battery = !s.power.on_ac && s.power.batteries.iter().any(|b| b.status == "discharging");
    if s.power_profile == "power-saver" && s.power.on_ac && s.cpu.pressure60 >= 10.0 {
        add(
            "profile".into(),
            "power-saver-slow",
            Severity::Info,
            vec![],
            json!({ "profile": s.power_profile }),
            vec![fix("profile", "balanced"), fix("profile", "performance")],
        );
    }
    if on_battery {
        let watts: f64 = s.power.batteries.iter().map(|b| b.watts).sum();
        if watts >= 25.0 {
            add(
                "drain".into(),
                "battery-drain",
                Severity::Warning,
                cpu_top.iter().map(|a| a.key.clone()).collect(),
                json!({ "watts": watts, "top": names(&cpu_top, |a| a.cpu) }),
                if s.power_profile.is_empty() || s.power_profile == "power-saver" {
                    vec![]
                } else {
                    vec![fix("profile", "power-saver")]
                },
            );
        }
        for g in s
            .gpus
            .iter()
            .filter(|g| (g.vendor == "NVIDIA" || g.vendor == "AMD" && s.gpus.len() > 1) && !g.asleep)
        {
            let holders: Vec<&AppFrame> = apps.iter().filter(|a| a.gpus.contains(&g.card) && a.kind != Kind::System).collect();
            add(
                format!("dgpu:{}", g.card),
                "dgpu-awake",
                Severity::Info,
                holders.iter().map(|a| a.key.clone()).collect(),
                json!({ "gpu": g.name, "watts": g.watts, "apps": names(&holders, |_| 0.0) }),
                holders.first().map(|a| vec![fix("show-app", &a.key)]).unwrap_or_default(),
            );
        }
    }
    for b in s
        .power
        .batteries
        .iter()
        .filter(|b| b.design_wh > 0.0 && b.full_wh > 0.0 && b.full_wh / b.design_wh < 0.7)
    {
        add(
            format!("battery:{}", b.name),
            "battery-worn",
            Severity::Info,
            vec![],
            json!({ "health": b.full_wh / b.design_wh * 100.0, "cycles": b.cycles }),
            vec![fix("show-perf", "power")],
        );
    }

    // GPU.
    for g in s.gpus.iter().filter(|g| g.busy.is_some_and(|b| b >= 95.0)) {
        let gpu_top: Vec<&AppFrame> = top(apps, |a| if a.gpus.contains(&g.card) { a.gpu } else { 0.0 }, 3);
        add(
            format!("gpu:{}", g.card),
            "gpu-busy",
            Severity::Info,
            gpu_top.iter().map(|a| a.key.clone()).collect(),
            json!({ "gpu": g.name, "busy": g.busy, "top": names(&gpu_top, |a| a.gpu) }),
            vec![fix("show-perf", &g.card)],
        );
    }

    // Disk space.
    for d in &s.disks {
        for m in &d.mounts {
            if m.total == 0 {
                continue;
            }
            let used = m.used as f64 / m.total as f64;
            let free = m.total.saturating_sub(m.used);
            let small = m.total < 4 * 1024 * 1024 * 1024;
            let sev = if used >= 0.97 || free < 512 * 1024 * 1024 && !small {
                Some(Severity::Critical)
            } else if used >= if small { 0.95 } else { 0.9 } {
                Some(Severity::Warning)
            } else {
                None
            };
            if let Some(sev) = sev {
                add(
                    format!("disk:{}", m.path),
                    "disk-full",
                    sev,
                    vec![],
                    json!({ "path": m.path, "used": m.used, "total": m.total, "free": free }),
                    vec![fix("show-perf", &d.name)],
                );
            }
        }
    }

    // Per-app trouble.
    for a in apps.iter().filter(|a| a.flags.contains(&Flag::Runaway)) {
        add(
            format!("runaway:{}", a.key),
            "runaway",
            Severity::Warning,
            vec![a.key.clone()],
            json!({ "name": a.name, "icon": a.icon, "cores": a.cpu_core / 100.0 }),
            relief(a),
        );
    }
    for a in apps.iter().filter(|a| a.flags.contains(&Flag::Leak)) {
        let l = a.leak.as_ref();
        add(
            format!("leak:{}", a.key),
            "memory-leak",
            Severity::Warning,
            vec![a.key.clone()],
            json!({ "name": a.name, "icon": a.icon, "mem": a.mem, "perMinute": l.map(|l| l.per_minute), "minutes": l.map(|l| l.minutes) }),
            vec![fix("restart", &a.key), fix("show-app", &a.key)],
        );
    }
    for c in ctx.crashes.iter().filter(|c| c.count >= 3) {
        let mut fixes = vec![fix("claude", &format!("crash:{}", c.exe))];
        if let Some(k) = &c.running {
            fixes.insert(0, fix("show-app", k));
        }
        add(
            format!("crashes:{}", c.exe),
            "keeps-crashing",
            Severity::Warning,
            c.running.iter().cloned().collect(),
            json!({ "name": c.name, "icon": c.icon, "count": c.count, "signal": c.signal, "exe": c.exe, "last": c.last }),
            fixes,
        );
    }
    for u in ctx.failed_units.iter().take(6) {
        add(
            format!("failed:{}", u.unit),
            "unit-failed",
            Severity::Warning,
            vec![],
            json!({ "unit": u.unit, "description": u.description, "user": u.user }),
            vec![
                Fix {
                    action: "unit-restart".into(),
                    target: u.unit.clone(),
                    detail: if u.user { "user".into() } else { "system".into() },
                },
                Fix {
                    action: "unit-reset".into(),
                    target: u.unit.clone(),
                    detail: if u.user { "user".into() } else { "system".into() },
                },
                fix("claude", &format!("unit:{}", u.unit)),
            ],
        );
    }
    let restart: Vec<&AppFrame> = apps
        .iter()
        .filter(|a| a.flags.contains(&Flag::NeedsRestart) && a.kind != Kind::Kernel)
        .collect();
    if !restart.is_empty() {
        add(
            "restart".into(),
            "needs-restart",
            Severity::Info,
            restart.iter().map(|a| a.key.clone()).collect(),
            json!({ "apps": names(&restart, |_| 0.0), "count": restart.len() }),
            restart
                .iter()
                .filter(|a| user_app(a) || a.kind == Kind::Service && a.unit.as_deref().is_some_and(|u| u.ends_with(".service")))
                .take(4)
                .map(|a| fix("restart", &a.key))
                .collect(),
        );
    }
    if ctx.reboot_needed {
        add("reboot".into(), "reboot-kernel", Severity::Warning, vec![], json!({}), vec![fix("reboot", "")]);
    }
    let paused: Vec<&AppFrame> = apps.iter().filter(|a| a.flags.contains(&Flag::Paused)).collect();
    if !paused.is_empty() {
        add(
            "paused".into(),
            "paused",
            Severity::Info,
            paused.iter().map(|a| a.key.clone()).collect(),
            json!({ "apps": names(&paused, |_| 0.0) }),
            paused.iter().take(3).map(|a| fix("resume", &a.key)).collect(),
        );
    }
    if ctx.zombies >= 5 {
        let parents: Vec<&AppFrame> = apps.iter().filter(|a| a.flags.contains(&Flag::Zombies)).collect();
        add(
            "zombies".into(),
            "zombies",
            Severity::Info,
            parents.iter().map(|a| a.key.clone()).collect(),
            json!({ "count": ctx.zombies, "apps": names(&parents, |_| 0.0) }),
            parents.first().map(|a| vec![fix("restart", &a.key)]).unwrap_or_default(),
        );
    }
    // The compositor itself working hard (blur, animations, many monitors).
    if let Some(h) = apps
        .iter()
        .find(|a| a.kind == Kind::System && a.name.eq_ignore_ascii_case("hyprland") && a.cpu_core >= 40.0)
    {
        add(
            "compositor".into(),
            "compositor-busy",
            Severity::Info,
            vec![h.key.clone()],
            json!({ "cores": h.cpu_core / 100.0 }),
            vec![fix("show-app", &h.key)],
        );
    }

    for f in &mut out {
        if !f.fixes.iter().any(|x| x.action == "claude") {
            f.fixes.push(fix("claude", &format!("finding:{}", f.id)));
        }
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.severity));
    out
}

/// How long a finding stays after the last frame that saw it.
pub const HOLD_MS: u64 = 30_000;

/// Fixes that act on an app: dropped once the app is gone.
const APP_ACTIONS: [&str; 6] = ["show-app", "end", "force", "restart", "resume", "efficiency"];

fn actionable(f: &Finding) -> usize {
    f.fixes.iter().filter(|x| x.action != "show-perf" && x.action != "claude").count()
}

/// Keeps findings for `HOLD_MS` after they end, and keeps a finding's apps,
/// buttons and worst severity while single frames know less (the top writer
/// idles for a second, pressure dips) — otherwise the cards change and swap
/// places under the cursor every second.
#[derive(Default)]
pub struct Recent {
    held: Vec<Held>,
}

struct Held {
    finding: Finding,
    seen: u64,
    /// When its buttons were last this rich (unix ms).
    rich_at: u64,
    peak: Severity,
    peak_at: u64,
}

impl Recent {
    pub fn apply(&mut self, now_ms: u64, fresh: Vec<Finding>, apps: &[AppFrame]) -> Vec<Finding> {
        let recent = |t: u64| now_ms.saturating_sub(t) < HOLD_MS;
        let mut next: Vec<Held> = Vec::with_capacity(fresh.len());
        for mut f in fresh {
            let old = self.held.iter().find(|h| h.finding.id == f.id);
            let mut rich_at = now_ms;
            if let Some(h) = old
                && actionable(&f) == 0
                && actionable(&h.finding) > 0
                && recent(h.rich_at)
            {
                f.fixes = h.finding.fixes.clone();
                if f.apps.is_empty() {
                    f.apps = h.finding.apps.clone();
                }
                // Lists like `top` word the text; keep them with the buttons.
                if let (Some(new), Some(old)) = (f.values.as_object_mut(), h.finding.values.as_object()) {
                    for (k, v) in old {
                        let empty = |x: Option<&Value>| x.is_none_or(|x| x.is_null() || x.as_array().is_some_and(|a| a.is_empty()));
                        if !empty(Some(v)) && empty(new.get(k)) {
                            new.insert(k.clone(), v.clone());
                        }
                    }
                }
                rich_at = h.rich_at;
            }
            let (mut peak, mut peak_at) = (f.severity, now_ms);
            if let Some(h) = old
                && h.peak > f.severity
                && recent(h.peak_at)
            {
                (peak, peak_at) = (h.peak, h.peak_at);
                f.severity = peak;
            }
            next.push(Held {
                finding: f,
                seen: now_ms,
                rich_at,
                peak,
                peak_at,
            });
        }
        for mut h in std::mem::take(&mut self.held) {
            if recent(h.seen) && !next.iter().any(|n| n.finding.id == h.finding.id) {
                h.finding.gone_since = Some(h.seen);
                next.push(h);
            }
        }
        for h in &mut next {
            let alive = |key: &str| apps.iter().any(|a| a.key == key);
            h.finding.fixes.retain(|x| !APP_ACTIONS.contains(&x.action.as_str()) || alive(&x.target));
            h.finding.apps.retain(|k| alive(k));
        }
        next.sort_by_key(|h| std::cmp::Reverse(h.finding.severity));
        self.held = next;
        self.held.iter().map(|h| h.finding.clone()).collect()
    }
}

/// 100 = nothing to say.
pub fn score(findings: &[Finding]) -> u32 {
    let lost: u32 = findings
        .iter()
        .map(|f| match f.severity {
            Severity::Critical => 25,
            Severity::Warning => 10,
            Severity::Info => 2,
        })
        .sum();
    100u32.saturating_sub(lost)
}

fn gib(b: f64) -> String {
    format!("{:.1} GiB", b / 1024.0 / 1024.0 / 1024.0)
}

/// Plain-English report of the current state, for "Ask Claude".
pub fn report(s: &Sample, apps: &[AppFrame], findings: &[Finding], score: u32) -> String {
    let mut r = String::new();
    use std::fmt::Write;
    let _ = writeln!(r, "System health score: {score}/100");
    let _ = writeln!(
        r,
        "CPU: {} ({} threads), {:.0}% busy, pressure {:.1}% (avg10), load {:.2} {:.2} {:.2}{}",
        s.cpu.model,
        s.cpu.logical,
        s.cpu.usage,
        s.cpu.pressure,
        s.cpu.load[0],
        s.cpu.load[1],
        s.cpu.load[2],
        s.cpu.temp.map(|t| format!(", {t:.0} °C")).unwrap_or_default()
    );
    let _ = writeln!(
        r,
        "Memory: {} of {} used, {} available, swap {} of {}, memory pressure {:.1}%",
        gib(s.memory.used as f64),
        gib(s.memory.total as f64),
        gib(s.memory.available as f64),
        gib(s.memory.swap_used as f64),
        gib(s.memory.swap_total as f64),
        s.memory.pressure
    );
    for g in &s.gpus {
        let _ = writeln!(
            r,
            "GPU {} ({}): {}",
            g.name,
            g.driver,
            if g.asleep {
                "asleep".to_owned()
            } else {
                format!("{:.0}% busy", g.busy.unwrap_or(0.0))
            }
        );
    }
    for d in &s.disks {
        for m in &d.mounts {
            let _ = writeln!(r, "Disk {} {}: {} of {} used", d.name, m.path, gib(m.used as f64), gib(m.total as f64));
        }
    }
    if !s.power_profile.is_empty() {
        let _ = writeln!(r, "Power profile: {}, {}", s.power_profile, if s.power.on_ac { "on AC" } else { "on battery" });
    }
    let _ = writeln!(r, "\nTop apps by CPU:");
    for a in top(apps, |a| a.cpu, 8) {
        let _ = writeln!(
            r,
            "- {} ({:?}, pid {}): {:.1}% CPU, {} memory",
            a.name,
            a.kind,
            a.main_pid,
            a.cpu,
            gib(a.mem as f64)
        );
    }
    let _ = writeln!(r, "\nTop apps by memory:");
    for a in top(apps, |a| a.mem as f64, 8) {
        let _ = writeln!(r, "- {} ({:?}, pid {}): {}", a.name, a.kind, a.main_pid, gib(a.mem as f64));
    }
    let _ = writeln!(r, "\nFindings:");
    if findings.is_empty() {
        let _ = writeln!(r, "- none");
    }
    for f in findings {
        let over = if f.gone_since.is_some() { " (over, seen in the last 30 s)" } else { "" };
        let _ = writeln!(r, "- [{:?}] {}{over}: {}", f.severity, f.kind, f.values);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pulse::sample::{DiskSample, MountSample};

    fn app(key: &str, kind: Kind, cpu: f64, mem: u64) -> AppFrame {
        AppFrame {
            key: key.into(),
            kind,
            name: key.into(),
            icon: String::new(),
            desktop_id: None,
            unit: None,
            user_unit: false,
            main_pid: 1,
            pids: vec![1],
            starts: vec![],
            windows: vec![],
            cpu,
            cpu_core: cpu * 16.0,
            mem,
            swap: 0,
            gpu: 0.0,
            vram: 0,
            read_bps: 0.0,
            write_bps: 0.0,
            threads: 1,
            started: 0,
            user: "tom".into(),
            uid: 1000,
            flags: vec![],
            health: 0,
            leak: None,
            gpus: vec![],
            command: vec![],
            exe: String::new(),
            restart_reasons: vec![],
            focused: false,
        }
    }

    fn ctx<'a>() -> Context<'a> {
        Context {
            throttled_recently: 0,
            reboot_needed: false,
            crashes: &[],
            failed_units: &[],
            hung: 0,
            hung_names: vec![],
            zombies: 0,
            blocked_names: vec![],
        }
    }

    fn calm() -> Sample {
        let mut s = Sample::default();
        s.memory.total = 16 << 30;
        s.memory.available = 10 << 30;
        s.power.on_ac = true;
        s
    }

    #[test]
    fn calm_system_has_nothing_to_say() {
        let f = diagnose(&calm(), &[app("a", Kind::Window, 5.0, 1 << 30)], &ctx());
        assert!(f.is_empty(), "{f:?}");
        assert_eq!(score(&f), 100);
    }

    #[test]
    fn busy_cpu_names_the_top_app_and_offers_relief() {
        let mut s = calm();
        s.cpu.usage = 97.0;
        s.cpu.pressure = 45.0;
        let apps = [
            app("idle", Kind::Window, 1.0, 1),
            app("hog", Kind::Background, 80.0, 1),
            app("mid", Kind::Service, 10.0, 1),
        ];
        let f = diagnose(&s, &apps, &ctx());
        assert_eq!(f[0].kind, "cpu-busy");
        assert_eq!(f[0].severity, Severity::Warning);
        assert_eq!(f[0].apps, vec!["hog", "mid", "idle"]);
        let actions: Vec<&str> = f[0].fixes.iter().map(|x| x.action.as_str()).collect();
        assert_eq!(actions, vec!["show-app", "efficiency", "end", "claude"]);
    }

    #[test]
    fn low_memory_and_swapping() {
        let mut s = calm();
        s.memory.available = 400 << 20;
        s.memory.swap_out_bps = 20.0 * 1024.0 * 1024.0;
        let f = diagnose(&s, &[app("big", Kind::Window, 1.0, 9 << 30)], &ctx());
        let kinds: Vec<&str> = f.iter().map(|x| x.kind.as_str()).collect();
        assert_eq!(kinds, vec!["memory-low", "swapping"]);
        assert_eq!(f[0].severity, Severity::Critical);
        assert_eq!(f[1].severity, Severity::Critical);
        assert!(score(&f) <= 50);
    }

    #[test]
    fn no_restart_button_for_the_session_or_the_compositor() {
        let mut session = app("session", Kind::System, 0.0, 1);
        session.unit = Some("session-5.scope".into());
        let mut hypr = app("hyprland", Kind::System, 0.0, 1);
        let mut svc = app("pipewire", Kind::Service, 0.0, 1);
        svc.unit = Some("pipewire.service".into());
        let mut win = app("firefox", Kind::Window, 0.0, 1);
        for a in [&mut session, &mut hypr, &mut svc, &mut win] {
            a.flags = vec![Flag::NeedsRestart];
        }
        let f = diagnose(&calm(), &[session, hypr, svc, win], &ctx());
        let r = f.iter().find(|x| x.kind == "needs-restart").unwrap();
        let targets: Vec<&str> = r.fixes.iter().filter(|x| x.action == "restart").map(|x| x.target.as_str()).collect();
        assert_eq!(targets, vec!["pipewire", "firefox"]);
    }

    #[test]
    fn full_disk_thresholds_depend_on_size() {
        let mut s = calm();
        s.disks.push(DiskSample {
            name: "nvme0n1".into(),
            mounts: vec![
                MountSample {
                    path: "/".into(),
                    fstype: "btrfs".into(),
                    used: 92 << 30,
                    total: 100 << 30,
                },
                MountSample {
                    path: "/boot".into(),
                    fstype: "vfat".into(),
                    used: 920 << 20,
                    total: 1 << 30,
                },
            ],
            ..Default::default()
        });
        let f = diagnose(&s, &[], &ctx());
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].id, "disk:/");
    }

    #[test]
    fn broken_apps_come_first() {
        let mut hung = app("frozen", Kind::Window, 0.0, 1);
        hung.flags = vec![Flag::NotResponding];
        let mut leaky = app("leaky", Kind::Background, 0.0, 1);
        leaky.flags = vec![Flag::Leak, Flag::NeedsRestart];
        let c = Context { reboot_needed: true, ..ctx() };
        let f = diagnose(&calm(), &[leaky, hung], &c);
        assert_eq!(f[0].kind, "not-responding");
        assert_eq!(f[0].fixes[0].action, "force");
        let kinds: Vec<&str> = f.iter().map(|x| x.kind.as_str()).collect();
        assert!(kinds.contains(&"memory-leak") && kinds.contains(&"needs-restart") && kinds.contains(&"reboot-kernel"));
        assert!(report(&calm(), &[], &f, score(&f)).contains("not-responding"));
    }

    fn io_finding(apps: &[AppFrame]) -> Vec<Finding> {
        let mut s = calm();
        s.io.pressure_full = 40.0;
        diagnose(&s, apps, &ctx())
    }

    fn disk_writer() -> AppFrame {
        let mut a = app("claude", Kind::Task, 1.0, 1);
        a.write_bps = 50e6;
        a
    }

    #[test]
    fn every_finding_can_go_to_claude_once() {
        let f = io_finding(&[]);
        assert_eq!(f[0].fixes.last(), Some(&fix("claude", "finding:io")));
        let crash = Crash {
            exe: "/usr/bin/x".into(),
            name: "x".into(),
            icon: String::new(),
            count: 3,
            signal: "SIGSEGV".into(),
            last: 0,
            running: None,
        };
        let c = Context {
            crashes: std::slice::from_ref(&crash),
            ..ctx()
        };
        let f = diagnose(&calm(), &[], &c);
        let claude: Vec<&Fix> = f[0].fixes.iter().filter(|x| x.action == "claude").collect();
        assert_eq!(claude, vec![&fix("claude", "crash:/usr/bin/x")]);
    }

    #[test]
    fn findings_stay_for_30_seconds_after_they_end() {
        let mut r = Recent::default();
        let f = r.apply(1_000, io_finding(&[]), &[]);
        assert_eq!(f[0].gone_since, None);
        let f = r.apply(20_000, vec![], &[]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].gone_since, Some(1_000));
        assert!(r.apply(31_001, vec![], &[]).is_empty());
    }

    #[test]
    fn a_returning_finding_is_active_again() {
        let mut r = Recent::default();
        r.apply(0, io_finding(&[]), &[]);
        r.apply(5_000, vec![], &[]);
        let f = r.apply(6_000, io_finding(&[]), &[]);
        assert_eq!(f[0].gone_since, None);
        assert_eq!(r.apply(35_000, vec![], &[])[0].gone_since, Some(6_000));
    }

    #[test]
    fn buttons_dont_flicker_when_a_frame_knows_less() {
        let w = disk_writer();
        let mut r = Recent::default();
        let rich = r.apply(0, io_finding(std::slice::from_ref(&w)), std::slice::from_ref(&w));
        let actions = |f: &Finding| f.fixes.iter().map(|x| x.action.clone()).collect::<Vec<_>>();
        assert_eq!(actions(&rich[0]), vec!["show-app", "efficiency", "end", "claude"]);
        // Next second the writer is quiet: same buttons, apps and text.
        let f = r.apply(1_000, io_finding(&[]), std::slice::from_ref(&w));
        assert_eq!(actions(&f[0]), actions(&rich[0]));
        assert_eq!(f[0].apps, vec!["claude"]);
        assert_eq!(f[0].values["top"], rich[0].values["top"]);
        // Long enough without it: the frame's own buttons.
        let f = r.apply(31_000, io_finding(&[]), std::slice::from_ref(&w));
        assert_eq!(actions(&f[0]), vec!["show-perf", "claude"]);
    }

    #[test]
    fn another_top_app_replaces_the_held_buttons() {
        let w = disk_writer();
        let mut win = app("firefox", Kind::Window, 1.0, 1);
        win.write_bps = 80e6;
        let both = [w.clone(), win.clone()];
        let mut r = Recent::default();
        r.apply(0, io_finding(std::slice::from_ref(&w)), &both);
        let f = r.apply(1_000, io_finding(std::slice::from_ref(&win)), &both);
        assert_eq!(f[0].apps, vec!["firefox"]);
        assert!(f[0].fixes.iter().all(|x| x.target == "firefox" || x.action == "claude"), "{:?}", f[0].fixes);
    }

    #[test]
    fn severity_holds_its_peak_so_the_order_stays() {
        let mut s = calm();
        s.io.pressure_full = 40.0;
        let mut r = Recent::default();
        assert_eq!(r.apply(0, diagnose(&s, &[], &ctx()), &[])[0].severity, Severity::Critical);
        s.io.pressure_full = 15.0;
        assert_eq!(r.apply(1_000, diagnose(&s, &[], &ctx()), &[])[0].severity, Severity::Critical);
        assert_eq!(r.apply(31_000, diagnose(&s, &[], &ctx()), &[])[0].severity, Severity::Warning);
    }

    #[test]
    fn held_buttons_drop_apps_that_are_gone() {
        let w = disk_writer();
        let mut r = Recent::default();
        r.apply(0, io_finding(std::slice::from_ref(&w)), std::slice::from_ref(&w));
        let f = r.apply(1_000, vec![], &[]);
        assert!(f[0].apps.is_empty());
        let actions: Vec<&str> = f[0].fixes.iter().map(|x| x.action.as_str()).collect();
        assert_eq!(actions, vec!["claude"]);
    }
}
