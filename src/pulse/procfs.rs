//! Parsers for the text files under /proc. Pure functions on strings; the
//! readers in `sample` feed them.

use std::collections::BTreeMap;

/// One `cpu` line of /proc/stat, in clock ticks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuTimes {
    pub fn total(&self) -> u64 {
        self.user + self.nice + self.system + self.idle + self.iowait + self.irq + self.softirq + self.steal
    }

    pub fn idle_all(&self) -> u64 {
        self.idle + self.iowait
    }
}

/// Shares of the time between two samples (0–1).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CpuUsage {
    pub busy: f64,
    pub user: f64,
    pub system: f64,
    pub iowait: f64,
    pub steal: f64,
}

pub fn cpu_usage(prev: &CpuTimes, now: &CpuTimes) -> CpuUsage {
    let dt = now.total().saturating_sub(prev.total());
    if dt == 0 {
        return CpuUsage::default();
    }
    let d = |a: u64, b: u64| b.saturating_sub(a) as f64 / dt as f64;
    let idle = d(prev.idle_all(), now.idle_all());
    CpuUsage {
        busy: (1.0 - idle).clamp(0.0, 1.0),
        user: d(prev.user + prev.nice, now.user + now.nice),
        system: d(prev.system + prev.irq + prev.softirq, now.system + now.irq + now.softirq),
        iowait: d(prev.iowait, now.iowait),
        steal: d(prev.steal, now.steal),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stat {
    pub total: CpuTimes,
    pub cores: Vec<CpuTimes>,
    pub ctxt: u64,
    pub boot_time: u64,
    pub procs_running: u64,
    pub procs_blocked: u64,
}

fn cpu_line(rest: &str) -> CpuTimes {
    let n: Vec<u64> = rest.split_whitespace().map(|v| v.parse().unwrap_or(0)).collect();
    let g = |i: usize| n.get(i).copied().unwrap_or(0);
    CpuTimes {
        user: g(0),
        nice: g(1),
        system: g(2),
        idle: g(3),
        iowait: g(4),
        irq: g(5),
        softirq: g(6),
        steal: g(7),
    }
}

pub fn parse_stat(text: &str) -> Stat {
    let mut s = Stat::default();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(char::is_whitespace) else { continue };
        let num = || rest.trim().parse().unwrap_or(0);
        match key {
            "cpu" => s.total = cpu_line(rest),
            k if k.starts_with("cpu") => s.cores.push(cpu_line(rest)),
            "ctxt" => s.ctxt = num(),
            "btime" => s.boot_time = num(),
            "procs_running" => s.procs_running = num(),
            "procs_blocked" => s.procs_blocked = num(),
            _ => {}
        }
    }
    s
}

/// `Key: value kB` files (meminfo, status) → key → bytes (or plain numbers
/// for lines without a unit).
pub fn parse_kv(text: &str) -> BTreeMap<String, u64> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            let mut parts = v.split_whitespace();
            let n: u64 = parts.next()?.parse().ok()?;
            let n = if parts.next() == Some("kB") { n * 1024 } else { n };
            Some((k.trim().to_owned(), n))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemInfo {
    pub total: u64,
    pub free: u64,
    pub available: u64,
    pub buffers: u64,
    pub cached: u64,
    pub shmem: u64,
    pub slab_reclaimable: u64,
    pub dirty: u64,
    pub writeback: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    pub zswap: u64,
    pub zswapped: u64,
}

impl MemInfo {
    /// What programs hold: neither free nor reclaimable cache.
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    /// Page cache and buffers the kernel gives back on demand.
    pub fn cache(&self) -> u64 {
        (self.cached + self.buffers + self.slab_reclaimable).saturating_sub(self.shmem)
    }

    pub fn swap_used(&self) -> u64 {
        self.swap_total.saturating_sub(self.swap_free)
    }
}

pub fn parse_meminfo(text: &str) -> MemInfo {
    let kv = parse_kv(text);
    let g = |k: &str| kv.get(k).copied().unwrap_or(0);
    MemInfo {
        total: g("MemTotal"),
        free: g("MemFree"),
        available: kv.get("MemAvailable").copied().unwrap_or_else(|| g("MemFree") + g("Cached")),
        buffers: g("Buffers"),
        cached: g("Cached"),
        shmem: g("Shmem"),
        slab_reclaimable: g("SReclaimable"),
        dirty: g("Dirty"),
        writeback: g("Writeback"),
        swap_total: g("SwapTotal"),
        swap_free: g("SwapFree"),
        zswap: g("Zswap"),
        zswapped: g("Zswapped"),
    }
}

/// Space-separated `name value` lines (/proc/vmstat).
pub fn parse_vmstat(text: &str) -> BTreeMap<String, u64> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(' ')?;
            Some((k.to_owned(), v.trim().parse().ok()?))
        })
        .collect()
}

/// /proc/pressure/<res>: share of time (0–100) some / all tasks stalled.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pressure {
    pub some10: f64,
    pub some60: f64,
    pub full10: f64,
    pub full60: f64,
}

pub fn parse_pressure(text: &str) -> Pressure {
    let mut p = Pressure::default();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let kind = parts.next();
        let mut avg10 = 0.0;
        let mut avg60 = 0.0;
        for f in parts {
            if let Some(v) = f.strip_prefix("avg10=") {
                avg10 = v.parse().unwrap_or(0.0);
            } else if let Some(v) = f.strip_prefix("avg60=") {
                avg60 = v.parse().unwrap_or(0.0);
            }
        }
        match kind {
            Some("some") => (p.some10, p.some60) = (avg10, avg60),
            Some("full") => (p.full10, p.full60) = (avg10, avg60),
            _ => {}
        }
    }
    p
}

pub fn parse_loadavg(text: &str) -> [f64; 3] {
    let n: Vec<f64> = text.split_whitespace().take(3).map(|v| v.parse().unwrap_or(0.0)).collect();
    [
        n.first().copied().unwrap_or(0.0),
        n.get(1).copied().unwrap_or(0.0),
        n.get(2).copied().unwrap_or(0.0),
    ]
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskStat {
    pub name: String,
    pub read_sectors: u64,
    pub write_sectors: u64,
    /// Milliseconds the device had I/O in flight.
    pub io_ms: u64,
}

pub fn parse_diskstats(text: &str) -> Vec<DiskStat> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 13 {
                return None;
            }
            let n = |i: usize| f[i].parse::<u64>().unwrap_or(0);
            Some(DiskStat {
                name: f[2].to_owned(),
                read_sectors: n(5),
                write_sectors: n(9),
                io_ms: n(12),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetStat {
    pub iface: String,
    pub rx: u64,
    pub tx: u64,
}

pub fn parse_net_dev(text: &str) -> Vec<NetStat> {
    text.lines()
        .skip(2)
        .filter_map(|l| {
            let (iface, rest) = l.split_once(':')?;
            let f: Vec<u64> = rest.split_whitespace().map(|v| v.parse().unwrap_or(0)).collect();
            Some(NetStat {
                iface: iface.trim().to_owned(),
                rx: *f.first()?,
                tx: *f.get(8)?,
            })
        })
        .collect()
}

/// The fields of /proc/<pid>/stat we use.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PidStat {
    pub pid: i32,
    pub comm: String,
    pub state: char,
    pub ppid: i32,
    pub utime: u64,
    pub stime: u64,
    pub nice: i64,
    pub threads: u64,
    /// Clock ticks after boot.
    pub start_ticks: u64,
    pub rss_pages: u64,
    pub flags: u64,
}

/// Kernel threads have PF_KTHREAD in their flags.
pub const PF_KTHREAD: u64 = 0x0020_0000;

pub fn parse_pid_stat(text: &str) -> Option<PidStat> {
    // comm may contain spaces and parentheses: it ends at the last ')'.
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let pid = text[..open].trim().parse().ok()?;
    let comm = text[open + 1..close].to_owned();
    let f: Vec<&str> = text[close + 1..].split_whitespace().collect();
    // f[0] is field 3 (state).
    let n = |i: usize| f.get(i).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    Some(PidStat {
        pid,
        comm,
        state: f.first()?.chars().next()?,
        ppid: f.get(1)?.parse().ok()?,
        flags: n(6),
        utime: n(11),
        stime: n(12),
        nice: f.get(16).and_then(|v| v.parse().ok()).unwrap_or(0),
        threads: n(17),
        start_ticks: n(19),
        rss_pages: n(21),
    })
}

/// The unified (v2) cgroup path from /proc/<pid>/cgroup.
pub fn parse_cgroup(text: &str) -> String {
    text.lines().find_map(|l| l.strip_prefix("0::")).unwrap_or("").trim().to_owned()
}

/// NUL-separated /proc/<pid>/cmdline.
pub fn parse_cmdline(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// (read_bytes, write_bytes) of /proc/<pid>/io: what reached the disk.
pub fn parse_io(text: &str) -> (u64, u64) {
    let kv = parse_kv(text);
    (kv.get("read_bytes").copied().unwrap_or(0), kv.get("write_bytes").copied().unwrap_or(0))
}

/// A DRM client from /proc/<pid>/fdinfo/<fd>. Several fds can share a
/// client (dup, fork), so callers dedupe by (pdev, client id).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrmClient {
    pub driver: String,
    pub pdev: String,
    pub client_id: u64,
    /// Busy time over all engines, nanoseconds.
    pub engine_ns: u64,
    /// Resident VRAM (or local memory) in bytes.
    pub vram: u64,
}

pub fn parse_drm_fdinfo(text: &str) -> Option<DrmClient> {
    let mut c = DrmClient::default();
    let mut seen_driver = false;
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim();
        match k {
            "drm-driver" => {
                c.driver = v.to_owned();
                seen_driver = true;
            }
            "drm-pdev" => c.pdev = v.to_owned(),
            "drm-client-id" => c.client_id = v.parse().unwrap_or(0),
            _ if k.starts_with("drm-engine-") && !k.starts_with("drm-engine-capacity") => {
                c.engine_ns += v.trim_end_matches("ns").trim().parse::<u64>().unwrap_or(0);
            }
            // amdgpu: drm-memory-vram, i915/xe: drm-resident-local0.
            "drm-memory-vram" | "drm-resident-vram" | "drm-resident-local0" => {
                let mut p = v.split_whitespace();
                let n: u64 = p.next().and_then(|n| n.parse().ok()).unwrap_or(0);
                c.vram += match p.next() {
                    Some("KiB") => n * 1024,
                    Some("MiB") => n * 1024 * 1024,
                    Some("GiB") => n * 1024 * 1024 * 1024,
                    _ => n,
                };
            }
            _ => {}
        }
    }
    seen_driver.then_some(c)
}

/// Files a process still maps although they were replaced or deleted on
/// disk — typically shared libraries after an update. Only real files
/// count: no memfd, shared memory, or files under /tmp.
pub fn deleted_mappings(maps: &str) -> Vec<String> {
    let mut out: Vec<String> = maps
        .lines()
        .filter_map(|l| l.strip_suffix(" (deleted)"))
        .filter_map(|l| {
            let path = &l[l.find('/')?..];
            let ignored = ["/memfd:", "/dev/", "/tmp/", "/run/", "/var/tmp/", "/SYSV", "/home/", "/proc/"];
            let is_code = path.contains(".so") || path.starts_with("/usr/bin/") || path.starts_with("/usr/lib/");
            (is_code && !ignored.iter().any(|p| path.starts_with(p))).then(|| path.to_owned())
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// uid → name from /etc/passwd.
pub fn parse_passwd(text: &str) -> BTreeMap<u32, String> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split(':');
            let name = f.next()?;
            let uid = f.nth(1)?.parse().ok()?;
            Some((uid, name.to_owned()))
        })
        .collect()
}

/// Real uid from a status file's `Uid:` line.
pub fn status_uid(status: &str) -> Option<u32> {
    status.lines().find_map(|l| l.strip_prefix("Uid:"))?.split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_and_cpu_usage() {
        let a = parse_stat("cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 50 0 25 400 25 0 0 0\ncpu1 50 0 25 400 25 0 0 0\nctxt 9\nbtime 1700000000\nprocs_running 3\n");
        assert_eq!(a.cores.len(), 2);
        assert_eq!(a.total.total(), 1000);
        assert_eq!((a.ctxt, a.boot_time, a.procs_running), (9, 1_700_000_000, 3));
        let b = parse_stat("cpu  200 0 100 1600 100 0 0 0 0 0\n");
        let u = cpu_usage(&a.total, &b.total);
        // 1000 ticks passed: 100 user, 50 system, 800 idle, 50 iowait (counts as idle).
        assert!((u.busy - 0.15).abs() < 1e-9, "{u:?}");
        assert!((u.user - 0.1).abs() < 1e-9);
        assert!((u.iowait - 0.05).abs() < 1e-9);
        assert_eq!(cpu_usage(&b.total, &b.total), CpuUsage::default());
    }

    #[test]
    fn meminfo() {
        let m = parse_meminfo(
            "MemTotal:       16000 kB\nMemFree:  1000 kB\nMemAvailable: 6000 kB\nBuffers: 500 kB\nCached: 4000 kB\nShmem: 1000 kB\nSReclaimable: 500 kB\nSwapTotal: 8000 kB\nSwapFree: 6000 kB\n",
        );
        assert_eq!(m.total, 16000 * 1024);
        assert_eq!(m.used(), 10000 * 1024);
        assert_eq!(m.cache(), 4000 * 1024);
        assert_eq!(m.swap_used(), 2000 * 1024);
        // Very old kernels: no MemAvailable.
        assert_eq!(parse_meminfo("MemTotal: 10 kB\nMemFree: 2 kB\nCached: 3 kB\n").available, 5 * 1024);
    }

    #[test]
    fn pressure_and_load() {
        let p = parse_pressure("some avg10=12.50 avg60=3.00 avg300=1.00 total=1\nfull avg10=2.25 avg60=0.50 avg300=0.00 total=0\n");
        assert_eq!(
            p,
            Pressure {
                some10: 12.5,
                some60: 3.0,
                full10: 2.25,
                full60: 0.5
            }
        );
        assert_eq!(parse_loadavg("0.52 1.10 2.00 3/900 1234\n"), [0.52, 1.1, 2.0]);
    }

    #[test]
    fn disks_and_net() {
        let d = parse_diskstats(" 259       0 nvme0n1 2566434 465669 246811863 1670380 13396448 375891 616357400 28455395 0 2999252 30324663 1 2 3 4 5 6\n");
        assert_eq!(
            d,
            vec![DiskStat {
                name: "nvme0n1".into(),
                read_sectors: 246811863,
                write_sectors: 616357400,
                io_ms: 2999252
            }]
        );
        let n = parse_net_dev("h1\nh2\n    lo: 10 1 0 0 0 0 0 0 20 2 0 0 0 0 0 0\nwlan0: 300 3 0 0 0 0 0 0 400 4 0 0 0 0 0 0\n");
        assert_eq!(
            n[1],
            NetStat {
                iface: "wlan0".into(),
                rx: 300,
                tx: 400
            }
        );
    }

    #[test]
    fn pid_stat_with_odd_comm() {
        let s = parse_pid_stat("42 (Web Content) (x)) S 7 42 42 0 -1 4194304 1 0 0 0 150 30 0 0 20 5 12 0 99999 7077888 562 18446744073709551615").unwrap();
        assert_eq!(s.pid, 42);
        assert_eq!(s.comm, "Web Content) (x)");
        assert_eq!((s.state, s.ppid), ('S', 7));
        assert_eq!((s.utime, s.stime, s.nice, s.threads, s.start_ticks, s.rss_pages), (150, 30, 5, 12, 99999, 562));
        assert_eq!(s.flags, 4194304);
        assert!(parse_pid_stat("garbage").is_none());
    }

    #[test]
    fn cgroup_cmdline_io() {
        assert_eq!(
            parse_cgroup("0::/user.slice/app.slice/app-vela-spotify-1.scope\n"),
            "/user.slice/app.slice/app-vela-spotify-1.scope"
        );
        assert_eq!(parse_cmdline(b"/usr/bin/foo\0--bar\0\0"), vec!["/usr/bin/foo", "--bar"]);
        assert_eq!(parse_io("rchar: 1\nread_bytes: 4096\nwrite_bytes: 8192\n"), (4096, 8192));
    }

    #[test]
    fn drm_fdinfo() {
        let c = parse_drm_fdinfo(
            "pos:\t0\ndrm-driver:\ti915\ndrm-client-id:\t8039\ndrm-pdev:\t0000:00:02.0\ndrm-resident-local0:\t2 MiB\ndrm-engine-render:\t1000 ns\ndrm-engine-copy:\t24 ns\ndrm-engine-capacity-video:\t2\n",
        )
        .unwrap();
        assert_eq!((c.driver.as_str(), c.client_id, c.engine_ns, c.vram), ("i915", 8039, 1024, 2 * 1024 * 1024));
        assert!(parse_drm_fdinfo("pos:\t0\nflags:\t02\n").is_none());
    }

    #[test]
    fn deleted_libraries() {
        let maps = "7f00-7f01 r-xp 0 00:1 1 /usr/lib/libssl.so.3 (deleted)\n\
                    7f00-7f01 r-xp 0 00:1 1 /usr/lib/libssl.so.3 (deleted)\n\
                    7f00-7f01 rw-s 0 00:1 1 /memfd:wayland-cursor (deleted)\n\
                    7f00-7f01 rw-s 0 00:1 1 /dev/shm/x (deleted)\n\
                    7f00-7f01 r-xp 0 00:1 1 /usr/lib/libc.so.6\n\
                    5500-5501 r-xp 0 00:1 1 /usr/bin/firefox (deleted)\n";
        assert_eq!(deleted_mappings(maps), vec!["/usr/bin/firefox", "/usr/lib/libssl.so.3"]);
    }

    #[test]
    fn passwd_and_uid() {
        let p = parse_passwd("root:x:0:0::/root:/bin/bash\ntom:x:1000:1000::/home/tom:/bin/fish\nbad\n");
        assert_eq!(p.get(&1000).map(String::as_str), Some("tom"));
        assert_eq!(status_uid("Name:\tx\nUid:\t1000\t1000\t1000\t1000\n"), Some(1000));
    }
}
