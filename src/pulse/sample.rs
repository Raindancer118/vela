//! Reads the live system once per tick and turns counters into rates.
//! Everything that is expensive (command lines, cgroups, GPU file
//! descriptors, nvidia-smi) is cached or refreshed on a slower clock.

use super::hw::{self, GpuDevice};
use super::procfs::{self, DiskStat, NetStat};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn clk_tck() -> f64 {
    // SAFETY: sysconf has no preconditions.
    let v = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if v > 0 { v as f64 } else { 100.0 }
}

fn page_size() -> u64 {
    // SAFETY: sysconf has no preconditions.
    let v = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if v > 0 { v as u64 } else { 4096 }
}

fn my_uid() -> u32 {
    // SAFETY: getuid cannot fail.
    unsafe { libc::getuid() }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcSample {
    pub pid: i32,
    pub ppid: i32,
    pub comm: String,
    pub state: char,
    /// Percent of one core.
    pub cpu: f64,
    /// Private memory (anonymous + shared memory it created), bytes.
    pub mem: u64,
    pub rss: u64,
    pub swap: u64,
    pub threads: u64,
    pub nice: i64,
    pub read_bps: f64,
    pub write_bps: f64,
    /// GPU busy percent over all engines and GPUs (own processes only).
    pub gpu: f64,
    pub vram: u64,
    /// Cards (card0, …) the process has open.
    pub gpus: Vec<String>,
    /// Unix seconds.
    pub started: u64,
    pub uid: u32,
    pub user: String,
    pub exe: String,
    pub cmdline: Vec<String>,
    pub cgroup: String,
    pub kernel: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuSample {
    pub card: String,
    pub pdev: String,
    pub name: String,
    pub vendor: String,
    pub driver: String,
    /// Percent; None when the GPU can't tell (or is asleep).
    pub busy: Option<f64>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    pub temp: Option<f64>,
    pub watts: Option<f64>,
    pub mhz: Option<u32>,
    pub max_mhz: Option<u32>,
    pub encoder: Option<f64>,
    pub decoder: Option<f64>,
    /// Runtime-suspended (hybrid laptops: the dGPU sleeps).
    pub asleep: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountSample {
    pub path: String,
    pub fstype: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSample {
    pub name: String,
    pub model: String,
    pub size: u64,
    pub rotational: bool,
    pub removable: bool,
    pub read_bps: f64,
    pub write_bps: f64,
    /// Share of time with I/O in flight, percent.
    pub busy: f64,
    pub mounts: Vec<MountSample>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetSample {
    pub iface: String,
    pub kind: String,
    pub up: bool,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub rx_total: u64,
    pub tx_total: u64,
    pub speed_mbps: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuSample {
    pub model: String,
    pub cache: String,
    pub logical: usize,
    /// Percent of all cores.
    pub usage: f64,
    pub user: f64,
    pub system: f64,
    pub iowait: f64,
    pub steal: f64,
    pub cores: Vec<f64>,
    pub mhz: Vec<u32>,
    pub avg_mhz: u32,
    pub max_mhz: u32,
    pub base_mhz: u32,
    pub governor: String,
    pub epp: String,
    pub temp: Option<f64>,
    pub temp_crit: Option<f64>,
    /// Throttle events in the last tick.
    pub throttled: u64,
    pub throttle_total: u64,
    pub load: [f64; 3],
    pub processes: usize,
    pub threads: u64,
    pub running: u64,
    pub blocked: u64,
    pub ctxt_per_sec: f64,
    pub uptime: u64,
    pub pressure: f64,
    pub pressure60: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemSample {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub cache: u64,
    pub free: u64,
    pub shmem: u64,
    pub dirty: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub zswap: u64,
    pub zswapped: u64,
    pub swap_in_bps: f64,
    pub swap_out_bps: f64,
    pub major_faults_per_sec: f64,
    pub pressure: f64,
    pub pressure_full: f64,
    pub pressure60: f64,
    pub oom_kills: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IoSample {
    pub pressure: f64,
    pub pressure_full: f64,
    pub pressure60: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub t: u64,
    /// Seconds since the previous sample (0 for the first).
    pub dt: f64,
    pub cpu: CpuSample,
    pub memory: MemSample,
    pub io: IoSample,
    pub gpus: Vec<GpuSample>,
    pub disks: Vec<DiskSample>,
    pub net: Vec<NetSample>,
    pub power: hw::Power,
    pub power_profile: String,
    pub sensors: Vec<hw::Sensor>,
    pub fans: Vec<hw::Fan>,
    #[serde(skip)]
    pub procs: Vec<ProcSample>,
}

struct PrevProc {
    start: u64,
    ticks: u64,
    read: u64,
    write: u64,
    /// pdev → engine ns.
    gpu_ns: HashMap<String, u64>,
}

struct Static {
    start: u64,
    exe: String,
    cmdline: Vec<String>,
    cgroup: String,
    cgroup_tick: u64,
    uid: u32,
    /// fds that point at /dev/dri nodes, and the cards behind them.
    drm_fds: Vec<(String, String)>,
    drm_tick: u64,
}

/// nvidia-smi runs on its own thread (it takes ~100 ms) and only while the
/// GPU is awake.
#[derive(Default)]
struct NvidiaState {
    data: Vec<hw::NvidiaSmi>,
}

pub struct Sampler {
    proc_root: PathBuf,
    sys_root: PathBuf,
    tick: u64,
    last: Option<Instant>,
    prev_stat: Option<procfs::Stat>,
    prev_vm: HashMap<String, u64>,
    prev_disks: HashMap<String, DiskStat>,
    prev_net: HashMap<String, NetStat>,
    prev_rc6: HashMap<String, u64>,
    prev_throttle: u64,
    prev_procs: HashMap<i32, PrevProc>,
    statics: HashMap<i32, Static>,
    users: HashMap<u32, String>,
    uid: u32,
    clk: f64,
    page: u64,
    model: (String, String),
    gpus: Vec<GpuDevice>,
    /// /dev/dri node → card.
    dri_nodes: HashMap<String, String>,
    blocks: Vec<hw::BlockDevice>,
    nvidia: Arc<Mutex<NvidiaState>>,
    nvidia_started: bool,
    power_profile: String,
    cached_mounts: Vec<(String, MountSample)>,
    /// Background recording: no nvidia-smi, no GPU file descriptors.
    light: bool,
}

impl Default for Sampler {
    fn default() -> Self {
        Sampler::new(Path::new("/proc"), Path::new("/sys"))
    }
}

impl Sampler {
    pub fn new(proc_root: &Path, sys_root: &Path) -> Sampler {
        let users = std::fs::read_to_string("/etc/passwd").map(|t| procfs::parse_passwd(&t)).unwrap_or_default();
        let model = std::fs::read_to_string(proc_root.join("cpuinfo"))
            .map(|t| hw::cpu_model(&t))
            .unwrap_or_default();
        let gpus = hw::gpu_devices(sys_root);
        let mut s = Sampler {
            proc_root: proc_root.into(),
            sys_root: sys_root.into(),
            tick: 0,
            last: None,
            prev_stat: None,
            prev_vm: HashMap::new(),
            prev_disks: HashMap::new(),
            prev_net: HashMap::new(),
            prev_rc6: HashMap::new(),
            prev_throttle: 0,
            prev_procs: HashMap::new(),
            statics: HashMap::new(),
            users: users.into_iter().collect(),
            uid: my_uid(),
            clk: clk_tck(),
            page: page_size(),
            model,
            dri_nodes: HashMap::new(),
            gpus,
            blocks: hw::block_devices(sys_root),
            nvidia: Arc::default(),
            nvidia_started: false,
            power_profile: String::new(),
            cached_mounts: Vec::new(),
            light: false,
        };
        s.dri_nodes = s.map_dri_nodes();
        s
    }

    /// For the background recorder: cheap enough to run all the time.
    pub fn light() -> Sampler {
        Sampler {
            light: true,
            ..Sampler::default()
        }
    }

    fn map_dri_nodes(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Ok(rd) = std::fs::read_dir(self.sys_root.join("class/drm")) else {
            return out;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.contains('-') || !(name.starts_with("card") || name.starts_with("render")) {
                continue;
            }
            let dev = std::fs::canonicalize(e.path().join("device")).unwrap_or_default();
            if let Some(g) = self.gpus.iter().find(|g| g.device_dir == dev) {
                out.insert(format!("/dev/dri/{name}"), g.card.clone());
            }
        }
        out
    }

    fn user_name(&self, uid: u32) -> String {
        self.users.get(&uid).cloned().unwrap_or_else(|| uid.to_string())
    }

    fn start_nvidia_thread(&mut self) {
        if self.nvidia_started {
            return;
        }
        self.nvidia_started = true;
        let Some(smi) = crate::paths::find_executable("nvidia-smi") else { return };
        let dirs: Vec<PathBuf> = self.gpus.iter().filter(|g| g.vendor == "NVIDIA").map(|g| g.device_dir.clone()).collect();
        if dirs.is_empty() {
            return;
        }
        let weak = Arc::downgrade(&self.nvidia);
        std::thread::Builder::new()
            .name("pulse-nvidia".into())
            .spawn(move || {
                loop {
                    let Some(state) = weak.upgrade() else { return };
                    // Asking a sleeping GPU wakes it up: only query awake ones.
                    let awake = dirs.iter().any(|d| hw::runtime_status(d) != "suspended");
                    let data = if awake {
                        std::process::Command::new(&smi)
                            .args([hw::NVIDIA_QUERY, "--format=csv,noheader,nounits"])
                            .stdin(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .output()
                            .ok()
                            .map(|o| hw::parse_nvidia_smi(&String::from_utf8_lossy(&o.stdout)))
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    if let Ok(mut s) = state.lock() {
                        s.data = data;
                    }
                    drop(state);
                    std::thread::sleep(Duration::from_secs(2));
                }
            })
            .ok();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.proc_root.join(rel)).unwrap_or_default()
    }

    pub fn sample(&mut self) -> Sample {
        if !self.light {
            self.start_nvidia_thread();
        }
        let now = Instant::now();
        let dt = self.last.map_or(0.0, |l| now.duration_since(l).as_secs_f64());
        self.last = Some(now);
        self.tick += 1;
        let rate = |delta: u64| if dt > 0.0 { delta as f64 / dt } else { 0.0 };

        // CPU
        let stat = procfs::parse_stat(&self.read("stat"));
        let mut cpu = CpuSample {
            model: self.model.0.clone(),
            cache: self.model.1.clone(),
            logical: stat.cores.len().max(1),
            running: stat.procs_running,
            blocked: stat.procs_blocked,
            load: procfs::parse_loadavg(&self.read("loadavg")),
            ..Default::default()
        };
        if let Some(prev) = &self.prev_stat {
            let u = procfs::cpu_usage(&prev.total, &stat.total);
            cpu.usage = u.busy * 100.0;
            cpu.user = u.user * 100.0;
            cpu.system = u.system * 100.0;
            cpu.iowait = u.iowait * 100.0;
            cpu.steal = u.steal * 100.0;
            cpu.cores = stat
                .cores
                .iter()
                .enumerate()
                .map(|(i, c)| prev.cores.get(i).map_or(0.0, |p| procfs::cpu_usage(p, c).busy * 100.0))
                .collect();
            cpu.ctxt_per_sec = rate(stat.ctxt.saturating_sub(prev.ctxt));
        } else {
            cpu.cores = vec![0.0; stat.cores.len()];
        }
        let freq = hw::cpu_freq(&self.sys_root);
        cpu.avg_mhz = if freq.mhz.is_empty() {
            0
        } else {
            (freq.mhz.iter().map(|m| *m as u64).sum::<u64>() / freq.mhz.len() as u64) as u32
        };
        cpu.max_mhz = freq.max_mhz;
        cpu.base_mhz = freq.base_mhz;
        cpu.governor = freq.governor;
        cpu.epp = freq.epp;
        cpu.mhz = freq.mhz;
        cpu.throttle_total = freq.throttle_events;
        cpu.throttled = if self.prev_throttle > 0 {
            freq.throttle_events.saturating_sub(self.prev_throttle)
        } else {
            0
        };
        self.prev_throttle = freq.throttle_events;
        let uptime: f64 = self.read("uptime").split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        cpu.uptime = uptime as u64;
        let psi_cpu = procfs::parse_pressure(&self.read("pressure/cpu"));
        cpu.pressure = psi_cpu.some10;
        cpu.pressure60 = psi_cpu.some60;
        let (sensors, fans) = hw::sensors(&self.sys_root);
        if let Some(t) = hw::cpu_temperature(&sensors) {
            cpu.temp = Some(t.celsius);
            cpu.temp_crit = t.crit.or(t.high);
        }

        // Memory
        let mi = procfs::parse_meminfo(&self.read("meminfo"));
        let vm = procfs::parse_vmstat(&self.read("vmstat"));
        let dvm = |k: &str| vm.get(k).copied().unwrap_or(0).saturating_sub(self.prev_vm.get(k).copied().unwrap_or(u64::MAX));
        let psi_mem = procfs::parse_pressure(&self.read("pressure/memory"));
        let memory = MemSample {
            total: mi.total,
            used: mi.used(),
            available: mi.available,
            cache: mi.cache(),
            free: mi.free,
            shmem: mi.shmem,
            dirty: mi.dirty + mi.writeback,
            swap_total: mi.swap_total,
            swap_used: mi.swap_used(),
            zswap: mi.zswap,
            zswapped: mi.zswapped,
            swap_in_bps: rate(dvm("pswpin") * self.page),
            swap_out_bps: rate(dvm("pswpout") * self.page),
            major_faults_per_sec: rate(dvm("pgmajfault")),
            pressure: psi_mem.some10,
            pressure_full: psi_mem.full10,
            pressure60: psi_mem.some60,
            oom_kills: vm.get("oom_kill").copied().unwrap_or(0),
        };
        let psi_io = procfs::parse_pressure(&self.read("pressure/io"));
        let io = IoSample {
            pressure: psi_io.some10,
            pressure_full: psi_io.full10,
            pressure60: psi_io.some60,
        };

        // Processes
        let procs = self.sample_procs(dt, stat.boot_time);
        cpu.processes = procs.len();
        cpu.threads = procs.iter().map(|p| p.threads).sum();

        // GPUs
        let gpus = self.sample_gpus(dt, &procs);

        // Disks
        let disk_stats = procfs::parse_diskstats(&self.read("diskstats"));
        if self.tick % 60 == 1 {
            self.blocks = hw::block_devices(&self.sys_root);
        }
        if self.tick % 10 == 1 {
            self.cached_mounts = self.sample_mounts();
        }
        let disks = self
            .blocks
            .iter()
            .map(|b| {
                let now = disk_stats.iter().find(|d| d.name == b.name);
                let prev = self.prev_disks.get(&b.name);
                let (r, w, busy) = match (now, prev) {
                    (Some(n), Some(p)) if dt > 0.0 => (
                        rate(n.read_sectors.saturating_sub(p.read_sectors) * 512),
                        rate(n.write_sectors.saturating_sub(p.write_sectors) * 512),
                        (n.io_ms.saturating_sub(p.io_ms) as f64 / (dt * 1000.0) * 100.0).min(100.0),
                    ),
                    _ => (0.0, 0.0, 0.0),
                };
                DiskSample {
                    name: b.name.clone(),
                    model: b.model.clone(),
                    size: b.size,
                    rotational: b.rotational,
                    removable: b.removable,
                    read_bps: r,
                    write_bps: w,
                    busy,
                    mounts: self.cached_mounts.iter().filter(|(d, _)| *d == b.name).map(|(_, m)| m.clone()).collect(),
                }
            })
            .collect();
        self.prev_disks = disk_stats.into_iter().map(|d| (d.name.clone(), d)).collect();

        // Network
        let net_stats = procfs::parse_net_dev(&self.read("net/dev"));
        let mut net: Vec<NetSample> = net_stats
            .iter()
            .filter(|n| n.iface != "lo")
            .map(|n| {
                let p = self.prev_net.get(&n.iface);
                NetSample {
                    iface: n.iface.clone(),
                    kind: hw::net_kind(&self.sys_root, &n.iface).into(),
                    up: hw::net_up(&self.sys_root, &n.iface),
                    rx_bps: p.map_or(0.0, |p| rate(n.rx.saturating_sub(p.rx))),
                    tx_bps: p.map_or(0.0, |p| rate(n.tx.saturating_sub(p.tx))),
                    rx_total: n.rx,
                    tx_total: n.tx,
                    speed_mbps: hw::net_speed(&self.sys_root, &n.iface),
                }
            })
            .collect();
        let order = |k: &str| match k {
            "wifi" => 0,
            "ethernet" => 1,
            "vpn" => 2,
            _ => 3,
        };
        net.sort_by_key(|n| (!n.up, order(&n.kind), n.iface.clone()));
        self.prev_net = net_stats.into_iter().map(|n| (n.iface.clone(), n)).collect();

        if self.tick % 10 == 1 {
            self.power_profile = power_profile();
        }

        self.prev_stat = Some(stat);
        self.prev_vm = vm.into_iter().collect();
        Sample {
            t: now_ms(),
            dt,
            cpu,
            memory,
            io,
            gpus,
            disks,
            net,
            power: hw::power(&self.sys_root),
            power_profile: self.power_profile.clone(),
            sensors,
            fans,
            procs,
        }
    }

    fn sample_mounts(&self) -> Vec<(String, MountSample)> {
        let mounts = hw::parse_mounts(&self.read("mounts"));
        mounts
            .into_iter()
            .filter_map(|m| {
                let (used, total) = hw::fs_usage(&m.path)?;
                Some((
                    self.disk_of(&m.device),
                    MountSample {
                        path: m.path,
                        fstype: m.fstype,
                        used,
                        total,
                    },
                ))
            })
            .collect()
    }

    /// Whole disk behind a partition, LUKS or LVM volume.
    fn disk_of(&self, device: &str) -> String {
        let real = std::fs::canonicalize(device)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| device.to_owned());
        let mut name = real.trim_start_matches("/dev/").to_owned();
        for _ in 0..4 {
            if !name.starts_with("dm-") {
                break;
            }
            let slaves = self.sys_root.join("block").join(&name).join("slaves");
            match std::fs::read_dir(slaves).ok().and_then(|mut r| r.next()).and_then(|e| e.ok()) {
                Some(e) => name = e.file_name().to_string_lossy().into_owned(),
                None => break,
            }
        }
        hw::parent_disk(&name)
    }

    fn sample_procs(&mut self, dt: f64, boot_time: u64) -> Vec<ProcSample> {
        let Ok(rd) = std::fs::read_dir(&self.proc_root) else { return Vec::new() };
        let mut out = Vec::with_capacity(512);
        let mut seen = HashSet::new();
        let mut next_prev = HashMap::with_capacity(self.prev_procs.len());
        for e in rd.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
                continue;
            };
            let base = e.path();
            let Some(st) = std::fs::read_to_string(base.join("stat")).ok().and_then(|t| procfs::parse_pid_stat(&t)) else {
                continue;
            };
            seen.insert(pid);
            let kernel = st.flags & procfs::PF_KTHREAD != 0 || st.ppid == 2 || pid == 2;
            let status = if kernel {
                String::new()
            } else {
                std::fs::read_to_string(base.join("status")).unwrap_or_default()
            };
            let kv = procfs::parse_kv(&status);
            let g = |k: &str| kv.get(k).copied().unwrap_or(0);

            // Static info, refreshed when the pid was reused or now and then.
            let stale = self.statics.get(&pid).is_none_or(|s| s.start != st.start_ticks);
            if stale {
                let uid = procfs::status_uid(&status).unwrap_or(0);
                self.statics.insert(
                    pid,
                    Static {
                        start: st.start_ticks,
                        exe: std::fs::read_link(base.join("exe"))
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        cmdline: std::fs::read(base.join("cmdline")).map(|b| procfs::parse_cmdline(&b)).unwrap_or_default(),
                        cgroup: procfs::parse_cgroup(&std::fs::read_to_string(base.join("cgroup")).unwrap_or_default()),
                        cgroup_tick: self.tick,
                        uid,
                        drm_fds: Vec::new(),
                        // New processes get their fds looked at on the next tick.
                        drm_tick: 0,
                    },
                );
            }
            let tick = self.tick;
            let own = self.statics.get(&pid).is_some_and(|s| s.uid == self.uid);
            if let Some(s) = self.statics.get_mut(&pid) {
                if tick - s.cgroup_tick >= 10 {
                    s.cgroup = procfs::parse_cgroup(&std::fs::read_to_string(base.join("cgroup")).unwrap_or_default());
                    s.cgroup_tick = tick;
                    // Exec replaces the program.
                    s.cmdline = std::fs::read(base.join("cmdline")).map(|b| procfs::parse_cmdline(&b)).unwrap_or_default();
                    s.exe = std::fs::read_link(base.join("exe"))
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                }
                if own && !kernel && !self.light && (s.drm_tick == 0 && !stale || tick - s.drm_tick >= 10 && s.drm_tick != 0) {
                    s.drm_fds = drm_fds(&base, &self.dri_nodes);
                    s.drm_tick = tick;
                }
            }
            let s = &self.statics[&pid];

            let ticks = st.utime + st.stime;
            let (read, write) = if own {
                procfs::parse_io(&std::fs::read_to_string(base.join("io")).unwrap_or_default())
            } else {
                (0, 0)
            };
            let mut gpu_ns: HashMap<String, u64> = HashMap::new();
            let mut vram = 0;
            let mut seen_clients = HashSet::new();
            for (fd, _) in &s.drm_fds {
                if let Some(c) = std::fs::read_to_string(base.join("fdinfo").join(fd))
                    .ok()
                    .and_then(|t| procfs::parse_drm_fdinfo(&t))
                    && seen_clients.insert((c.pdev.clone(), c.client_id))
                {
                    *gpu_ns.entry(c.pdev.clone()).or_default() += c.engine_ns;
                    vram += c.vram;
                }
            }
            let prev = self.prev_procs.get(&pid).filter(|p| p.start == st.start_ticks);
            let (cpu, rbps, wbps, gpu) = match prev {
                Some(p) if dt > 0.0 => {
                    let gpu: f64 = gpu_ns
                        .iter()
                        .map(|(dev, ns)| ns.saturating_sub(p.gpu_ns.get(dev).copied().unwrap_or(*ns)) as f64 / (dt * 1e9) * 100.0)
                        .fold(0.0, f64::max);
                    (
                        ticks.saturating_sub(p.ticks) as f64 / self.clk / dt * 100.0,
                        read.saturating_sub(p.read) as f64 / dt,
                        write.saturating_sub(p.write) as f64 / dt,
                        gpu.min(100.0),
                    )
                }
                _ => (0.0, 0.0, 0.0, 0.0),
            };
            let mut gpus: Vec<String> = s.drm_fds.iter().map(|(_, c)| c.clone()).collect();
            gpus.sort();
            gpus.dedup();
            out.push(ProcSample {
                pid,
                ppid: st.ppid,
                comm: st.comm.clone(),
                state: st.state,
                cpu,
                mem: g("RssAnon") + g("RssShmem"),
                rss: g("VmRSS").max(st.rss_pages * self.page),
                swap: g("VmSwap"),
                threads: st.threads,
                nice: st.nice,
                read_bps: rbps,
                write_bps: wbps,
                gpu,
                vram,
                gpus,
                started: boot_time + (st.start_ticks as f64 / self.clk) as u64,
                uid: s.uid,
                user: self.user_name(s.uid),
                exe: s.exe.clone(),
                cmdline: s.cmdline.clone(),
                cgroup: s.cgroup.clone(),
                kernel,
            });
            next_prev.insert(
                pid,
                PrevProc {
                    start: st.start_ticks,
                    ticks,
                    read,
                    write,
                    gpu_ns,
                },
            );
        }
        self.prev_procs = next_prev;
        self.statics.retain(|pid, _| seen.contains(pid));
        out
    }

    fn sample_gpus(&mut self, dt: f64, procs: &[ProcSample]) -> Vec<GpuSample> {
        let nvidia = self.nvidia.lock().map(|s| s.data.clone()).unwrap_or_default();
        let mut out = Vec::new();
        for dev in &self.gpus {
            let card_dir = self.sys_root.join("class/drm").join(&dev.card);
            let fs = hw::gpu_sysfs(&card_dir, &dev.device_dir);
            let asleep = hw::runtime_status(&dev.device_dir) == "suspended";
            let mut g = GpuSample {
                card: dev.card.clone(),
                pdev: dev.pdev.clone(),
                name: hw::gpu_name(dev),
                vendor: dev.vendor.clone(),
                driver: dev.driver.clone(),
                busy: fs.busy_percent,
                vram_used: fs.vram_used,
                vram_total: fs.vram_total,
                temp: fs.temp,
                watts: fs.watts,
                mhz: fs.freq_mhz,
                max_mhz: fs.max_freq_mhz,
                asleep,
                ..Default::default()
            };
            if let Some(rc6) = fs.rc6_ms {
                if let Some(prev) = self.prev_rc6.get(&dev.card)
                    && dt > 0.0
                {
                    let idle = rc6.saturating_sub(*prev) as f64 / (dt * 1000.0);
                    g.busy = Some(((1.0 - idle) * 100.0).clamp(0.0, 100.0));
                }
                self.prev_rc6.insert(dev.card.clone(), rc6);
            }
            if let Some(n) = nvidia.iter().find(|n| n.bus == dev.pdev).filter(|_| !asleep) {
                g.name = n.name.clone();
                g.busy = Some(n.busy);
                g.vram_used = Some(n.vram_used);
                g.vram_total = Some(n.vram_total);
                g.temp = n.temp.or(g.temp);
                g.watts = n.watts.or(g.watts);
                g.mhz = n.freq_mhz.or(g.mhz);
                g.encoder = n.encoder;
                g.decoder = n.decoder;
            }
            if asleep {
                g.busy = Some(0.0);
                g.mhz = None;
                g.watts = None;
            } else if g.busy.is_none() {
                // Last resort: what our own processes report.
                let own: f64 = procs.iter().filter(|p| p.gpus.contains(&dev.card)).map(|p| p.gpu).sum();
                g.busy = Some(own.min(100.0));
            }
            out.push(g);
        }
        out
    }
}

/// fds of a process that are DRM nodes, with the card behind each.
fn drm_fds(base: &Path, nodes: &HashMap<String, String>) -> Vec<(String, String)> {
    let Ok(rd) = std::fs::read_dir(base.join("fd")) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let target = std::fs::read_link(e.path()).ok()?;
            let t = target.to_string_lossy();
            if !t.starts_with("/dev/dri/") {
                return None;
            }
            Some((e.file_name().to_string_lossy().into_owned(), nodes.get(t.as_ref())?.clone()))
        })
        .collect()
}

/// power-profiles-daemon's active profile (empty without it).
pub fn power_profile() -> String {
    let Some(ppc) = crate::paths::find_executable("powerprofilesctl") else {
        return String::new();
    };
    std::process::Command::new(ppc)
        .arg("get")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Against the real machine: rates appear on the second sample and the
    /// own process is there with its command line.
    #[test]
    fn samples_this_machine() {
        let mut s = Sampler::default();
        let first = s.sample();
        assert!(first.memory.total > 0);
        assert!(first.cpu.logical >= 1);
        std::thread::sleep(Duration::from_millis(120));
        let second = s.sample();
        assert!(second.dt > 0.0);
        assert!((0.0..=100.0).contains(&second.cpu.usage));
        let me = second.procs.iter().find(|p| p.pid == std::process::id() as i32).expect("own process");
        assert!(!me.cmdline.is_empty());
        assert!(me.mem > 0);
        assert_eq!(me.uid, my_uid());
        // Containers (CI) have their own pid namespace without kernel threads.
        if std::fs::read_to_string("/proc/2/comm").is_ok_and(|c| c.trim() == "kthreadd") {
            assert!(second.procs.iter().any(|p| p.kernel));
        }
    }
}
