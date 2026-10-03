//! Hardware from sysfs: CPU frequencies and throttling, temperatures and
//! fans (hwmon), batteries, GPUs and block devices. Every reader takes the
//! sysfs root so tests can point it at a fake tree.

use serde::Serialize;
use std::path::{Path, PathBuf};

fn read(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_owned())
}

fn read_u64(p: &Path) -> Option<u64> {
    read(p)?.parse().ok()
}

fn read_i64(p: &Path) -> Option<i64> {
    read(p)?.parse().ok()
}

fn sorted_dir(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(p).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.sort_by_key(|p| natural_key(p));
    v
}

/// "cpu10" after "cpu9".
fn natural_key(p: &Path) -> (String, u64) {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let digits: String = name
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    (name[..name.len() - digits.len()].to_owned(), digits.parse().unwrap_or(0))
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CpuFreq {
    /// Per logical CPU, MHz (0 when unknown).
    pub mhz: Vec<u32>,
    pub max_mhz: u32,
    pub base_mhz: u32,
    pub governor: String,
    /// intel_pstate/amd-pstate energy preference, if any.
    pub epp: String,
    /// Package + core throttle events since boot (Intel), summed.
    pub throttle_events: u64,
    /// Per logical CPU: its core's throttle events since boot (Intel).
    pub core_throttles: Vec<u64>,
}

pub fn cpu_freq(sys: &Path) -> CpuFreq {
    let base = sys.join("devices/system/cpu");
    let mut f = CpuFreq::default();
    for dir in sorted_dir(&base) {
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if !(name.starts_with("cpu") && name[3..].chars().all(|c| c.is_ascii_digit()) && name.len() > 3) {
            continue;
        }
        let freq = dir.join("cpufreq");
        f.mhz.push(read_u64(&freq.join("scaling_cur_freq")).map_or(0, |k| (k / 1000) as u32));
        if f.max_mhz == 0 {
            f.max_mhz = read_u64(&freq.join("cpuinfo_max_freq")).map_or(0, |k| (k / 1000) as u32);
            f.base_mhz = read_u64(&freq.join("base_frequency")).map_or(0, |k| (k / 1000) as u32);
            f.governor = read(&freq.join("scaling_governor")).unwrap_or_default();
            f.epp = read(&freq.join("energy_performance_preference")).unwrap_or_default();
            f.throttle_events = read_u64(&dir.join("thermal_throttle/package_throttle_count")).unwrap_or(0);
        }
        let core = read_u64(&dir.join("thermal_throttle/core_throttle_count")).unwrap_or(0);
        f.throttle_events += core;
        f.core_throttles.push(core);
    }
    f
}

/// Which CPUs the heat slows down right now: their core's counter rose in
/// the last 2 s (a single tick would flicker). `last` keeps when it rose.
pub fn core_throttling(prev: &[u64], now: &[u64], last: &mut Vec<u64>, t_ms: u64) -> Vec<bool> {
    last.resize(now.len(), 0);
    now.iter()
        .enumerate()
        .map(|(i, n)| {
            if prev.get(i).is_some_and(|p| n > p) {
                last[i] = t_ms;
            }
            last[i] > 0 && t_ms.saturating_sub(last[i]) < 2_000
        })
        .collect()
}

/// Model name and cache size from /proc/cpuinfo.
pub fn cpu_model(cpuinfo: &str) -> (String, String) {
    let field = |k: &str| {
        cpuinfo
            .lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().to_owned())
            .unwrap_or_default()
    };
    let model = field("model name");
    let model = if model.is_empty() { field("Hardware") } else { model };
    (model.split_whitespace().collect::<Vec<_>>().join(" "), field("cache size"))
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sensor {
    /// hwmon chip (coretemp, k10temp, nvme, amdgpu, …).
    pub chip: String,
    pub label: String,
    pub celsius: f64,
    /// High / critical threshold if the chip reports one.
    pub high: Option<f64>,
    pub crit: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Fan {
    pub chip: String,
    pub label: String,
    pub rpm: u64,
}

pub fn sensors(sys: &Path) -> (Vec<Sensor>, Vec<Fan>) {
    let mut temps = Vec::new();
    let mut fans = Vec::new();
    for dir in sorted_dir(&sys.join("class/hwmon")) {
        let chip = read(&dir.join("name")).unwrap_or_else(|| "hwmon".into());
        for f in sorted_dir(&dir) {
            let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some(idx) = name.strip_prefix("temp").and_then(|r| r.strip_suffix("_input")) {
                let Some(milli) = read_i64(&f) else { continue };
                let c = milli as f64 / 1000.0;
                // Unplugged probes read as -273, 0 (ThinkPad) or absurd values.
                if !(-40.0..=150.0).contains(&c) || milli == 0 {
                    continue;
                }
                let thr = |s: &str| {
                    read_i64(&dir.join(format!("temp{idx}_{s}")))
                        .map(|v| v as f64 / 1000.0)
                        .filter(|v| *v > 0.0 && *v < 200.0)
                };
                temps.push(Sensor {
                    label: read(&dir.join(format!("temp{idx}_label"))).unwrap_or_else(|| format!("temp{idx}")),
                    chip: chip.clone(),
                    celsius: c,
                    high: thr("max"),
                    crit: thr("crit"),
                });
            } else if let Some(idx) = name.strip_prefix("fan").and_then(|r| r.strip_suffix("_input")) {
                let Some(rpm) = read_u64(&f) else { continue };
                fans.push(Fan {
                    label: read(&dir.join(format!("fan{idx}_label"))).unwrap_or_else(|| format!("Fan {idx}")),
                    chip: chip.clone(),
                    rpm,
                });
            }
        }
    }
    (temps, fans)
}

/// The temperature that best stands for the CPU.
pub fn cpu_temperature(temps: &[Sensor]) -> Option<&Sensor> {
    let rank = |s: &Sensor| -> Option<u8> {
        let l = s.label.to_lowercase();
        match s.chip.as_str() {
            "coretemp" if l.starts_with("package") => Some(0),
            "k10temp" | "zenpower" if l == "tctl" || l == "tdie" => Some(0),
            "coretemp" | "k10temp" | "zenpower" => Some(1),
            "cpu_thermal" | "soc_thermal" => Some(2),
            "thinkpad" if l.contains("cpu") => Some(3),
            "acpitz" => Some(4),
            _ => None,
        }
    };
    temps.iter().filter_map(|s| Some((rank(s)?, s))).min_by_key(|(r, _)| *r).map(|(_, s)| s)
}

/// Per logical CPU: the reading of its physical core (coretemp's
/// "Core <core_id>"). None where there is none — AMD's k10temp only knows
/// the whole package.
pub fn core_temperatures(sys: &Path, temps: &[Sensor], cpus: usize) -> Vec<Option<f64>> {
    let mut by_core = std::collections::HashMap::new();
    for s in temps.iter().filter(|s| s.chip == "coretemp") {
        if let Some(core) = s.label.strip_prefix("Core ").and_then(|c| c.trim().parse::<u32>().ok()) {
            by_core.entry(core).or_insert(s.celsius);
        }
    }
    (0..cpus)
        .map(|i| {
            read(&sys.join(format!("devices/system/cpu/cpu{i}/topology/core_id")))
                .and_then(|c| c.parse::<u32>().ok())
                .and_then(|c| by_core.get(&c).copied())
        })
        .collect()
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub name: String,
    pub percent: f64,
    /// charging, discharging, full, not charging
    pub status: String,
    /// Positive while discharging or charging, watts.
    pub watts: f64,
    pub energy_wh: f64,
    pub full_wh: f64,
    pub design_wh: f64,
    pub cycles: u64,
    /// Until empty (discharging) or full (charging).
    pub seconds_left: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Power {
    pub on_ac: bool,
    pub batteries: Vec<Battery>,
}

pub fn power(sys: &Path) -> Power {
    let mut p = Power::default();
    let mut any_mains = false;
    for dir in sorted_dir(&sys.join("class/power_supply")) {
        let kind = read(&dir.join("type")).unwrap_or_default();
        if kind == "Mains" {
            any_mains = true;
            p.on_ac |= read(&dir.join("online")).as_deref() == Some("1");
            continue;
        }
        if kind != "Battery" || read(&dir.join("scope")).as_deref() == Some("Device") {
            continue;
        }
        let micro = |n: &str| read_u64(&dir.join(n)).map(|v| v as f64 / 1e6);
        let volts = micro("voltage_now");
        // Some batteries only report charge (µAh) and current (µA).
        let wh = |e: &str, c: &str| micro(e).or_else(|| Some(micro(c)? * volts?));
        let watts = micro("power_now").or_else(|| Some(micro("current_now")? * volts?)).unwrap_or(0.0);
        let status = read(&dir.join("status")).unwrap_or_default().to_lowercase();
        let energy = wh("energy_now", "charge_now").unwrap_or(0.0);
        let full = wh("energy_full", "charge_full").unwrap_or(0.0);
        let seconds_left = (watts > 0.5)
            .then(|| match status.as_str() {
                "discharging" => Some(energy / watts * 3600.0),
                "charging" => Some((full - energy).max(0.0) / watts * 3600.0),
                _ => None,
            })
            .flatten()
            .map(|s| s as u64);
        p.batteries.push(Battery {
            name: dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            percent: read_u64(&dir.join("capacity")).map_or_else(|| if full > 0.0 { energy / full * 100.0 } else { 0.0 }, |c| c as f64),
            status,
            watts,
            energy_wh: energy,
            full_wh: full,
            design_wh: wh("energy_full_design", "charge_full_design").unwrap_or(0.0),
            cycles: read_u64(&dir.join("cycle_count")).unwrap_or(0),
            seconds_left,
        });
    }
    // Desktops without a mains supply entry are on mains.
    if !any_mains && p.batteries.is_empty() {
        p.on_ac = true;
    }
    p
}

/// What we know about a GPU before sampling it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GpuDevice {
    /// card0, card1, …
    pub card: String,
    /// PCI address (0000:01:00.0) as in drm-pdev.
    pub pdev: String,
    pub driver: String,
    pub vendor: String,
    pub device_dir: PathBuf,
}

pub fn gpu_devices(sys: &Path) -> Vec<GpuDevice> {
    sorted_dir(&sys.join("class/drm"))
        .into_iter()
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            n.starts_with("card") && !n.contains('-')
        })
        .filter_map(|p| {
            let device_dir = std::fs::canonicalize(p.join("device")).unwrap_or_else(|_| p.join("device"));
            let driver = std::fs::read_link(device_dir.join("driver"))
                .ok()
                .and_then(|d| Some(d.file_name()?.to_string_lossy().into_owned()))
                .unwrap_or_default();
            let vendor = match read(&device_dir.join("vendor")).as_deref() {
                Some("0x10de") => "NVIDIA",
                Some("0x1002") => "AMD",
                Some("0x8086") => "Intel",
                _ => "",
            }
            .to_owned();
            Some(GpuDevice {
                card: p.file_name()?.to_string_lossy().into_owned(),
                pdev: device_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                driver,
                vendor,
                device_dir,
            })
        })
        .collect()
}

/// Runtime power state of a PCI device; "suspended" means asleep — asking
/// it anything (nvidia-smi!) would wake it and cost battery.
pub fn runtime_status(device_dir: &Path) -> String {
    read(&device_dir.join("power/runtime_status")).unwrap_or_else(|| "active".into())
}

/// Counters a GPU exposes directly in sysfs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuSysfs {
    /// amdgpu: busy percent right now.
    pub busy_percent: Option<f64>,
    /// i915: total ms in RC6 (idle); busy = 1 − Δrc6 / Δt.
    pub rc6_ms: Option<u64>,
    pub freq_mhz: Option<u32>,
    pub max_freq_mhz: Option<u32>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    pub temp: Option<f64>,
    pub watts: Option<f64>,
}

pub fn gpu_sysfs(card_dir: &Path, device_dir: &Path) -> GpuSysfs {
    let gt = card_dir.join("gt/gt0");
    let hwmon = sorted_dir(&device_dir.join("hwmon")).into_iter().next();
    let hw = |n: &str| hwmon.as_ref().and_then(|h| read_u64(&h.join(n)));
    GpuSysfs {
        busy_percent: read_u64(&device_dir.join("gpu_busy_percent")).map(|v| v as f64),
        rc6_ms: read_u64(&gt.join("rc6_residency_ms")),
        freq_mhz: read_u64(&gt.join("rps_act_freq_mhz"))
            .or_else(|| read_u64(&card_dir.join("gt_act_freq_mhz")))
            .map(|v| v as u32)
            .or_else(|| hw("freq1_input").map(|hz| (hz / 1_000_000) as u32)),
        max_freq_mhz: read_u64(&gt.join("rps_max_freq_mhz"))
            .or_else(|| read_u64(&card_dir.join("gt_max_freq_mhz")))
            .map(|v| v as u32),
        vram_used: read_u64(&device_dir.join("mem_info_vram_used")),
        vram_total: read_u64(&device_dir.join("mem_info_vram_total")),
        temp: hw("temp1_input").map(|m| m as f64 / 1000.0),
        watts: hw("power1_average").or_else(|| hw("power1_input")).map(|u| u as f64 / 1e6),
    }
}

/// One line of `nvidia-smi --query-gpu=pci.bus_id,name,utilization.gpu,
/// memory.used,memory.total,temperature.gpu,power.draw,clocks.gr,
/// utilization.encoder,utilization.decoder --format=csv,noheader,nounits`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NvidiaSmi {
    pub bus: String,
    pub name: String,
    pub busy: f64,
    pub vram_used: u64,
    pub vram_total: u64,
    pub temp: Option<f64>,
    pub watts: Option<f64>,
    pub freq_mhz: Option<u32>,
    pub encoder: Option<f64>,
    pub decoder: Option<f64>,
}

pub const NVIDIA_QUERY: &str =
    "--query-gpu=pci.bus_id,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,clocks.gr,utilization.encoder,utilization.decoder";

pub fn parse_nvidia_smi(out: &str) -> Vec<NvidiaSmi> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            if f.len() < 8 {
                return None;
            }
            let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
            const MIB: u64 = 1024 * 1024;
            Some(NvidiaSmi {
                // 00000000:01:00.0 → 0000:01:00.0 like sysfs.
                bus: match f[0].to_lowercase().split_once(':') {
                    Some((domain, rest)) => format!("{}:{rest}", &domain[domain.len().saturating_sub(4)..]),
                    None => f[0].to_lowercase(),
                },
                name: f[1].to_owned(),
                busy: num(2).unwrap_or(0.0),
                vram_used: num(3).map_or(0, |m| m as u64 * MIB),
                vram_total: num(4).map_or(0, |m| m as u64 * MIB),
                temp: num(5),
                watts: num(6),
                freq_mhz: num(7).map(|v| v as u32),
                encoder: num(8),
                decoder: num(9),
            })
        })
        .collect()
}

/// lspci-ish name for a GPU without nvidia-smi.
pub fn gpu_name(dev: &GpuDevice) -> String {
    let product = read(&dev.device_dir.join("product_name")).filter(|s| !s.is_empty());
    product.unwrap_or_else(|| match (dev.vendor.as_str(), dev.driver.as_str()) {
        ("Intel", _) => "Intel Graphics".into(),
        ("AMD", _) => "AMD Radeon".into(),
        ("NVIDIA", _) => "NVIDIA GPU".into(),
        (_, d) if !d.is_empty() => d.to_owned(),
        _ => dev.card.clone(),
    })
}

/// Whole disks worth showing (no partitions, loop, ram or device-mapper
/// volumes without a name).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockDevice {
    pub name: String,
    pub model: String,
    pub rotational: bool,
    pub removable: bool,
    pub size: u64,
}

pub fn block_devices(sys: &Path) -> Vec<BlockDevice> {
    sorted_dir(&sys.join("block"))
        .into_iter()
        .filter_map(|d| {
            let name = d.file_name()?.to_string_lossy().into_owned();
            if ["loop", "ram", "zram", "dm-", "md", "sr", "fd"].iter().any(|p| name.starts_with(p)) {
                return None;
            }
            let size = read_u64(&d.join("size")).unwrap_or(0) * 512;
            if size == 0 {
                return None;
            }
            Some(BlockDevice {
                model: read(&d.join("device/model")).unwrap_or_default(),
                rotational: read(&d.join("queue/rotational")).as_deref() == Some("1"),
                removable: read(&d.join("removable")).as_deref() == Some("1"),
                size,
                name,
            })
        })
        .collect()
}

/// A mounted filesystem from /proc/mounts worth showing (real devices only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub device: String,
    pub path: String,
    pub fstype: String,
}

pub fn parse_mounts(text: &str) -> Vec<Mount> {
    const REAL: [&str; 11] = ["ext4", "ext3", "ext2", "btrfs", "xfs", "f2fs", "vfat", "exfat", "ntfs3", "ntfs", "bcachefs"];
    let mut out: Vec<Mount> = Vec::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() < 3 || !REAL.contains(&f[2]) || !f[0].starts_with("/dev/") {
            continue;
        }
        let path = f[1].replace("\\040", " ");
        // btrfs subvolumes of one device: keep the shortest mount point.
        if let Some(prev) = out.iter_mut().find(|m| m.device == f[0]) {
            if path.len() < prev.path.len() {
                prev.path = path;
            }
            continue;
        }
        out.push(Mount {
            device: f[0].to_owned(),
            path,
            fstype: f[2].to_owned(),
        });
    }
    out
}

/// (used, total) bytes of a mounted filesystem.
pub fn fs_usage(path: &str) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: valid C string and an out-pointer to a zeroed struct.
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let total = s.f_blocks * s.f_frsize;
    let avail = s.f_bavail * s.f_frsize;
    let free = s.f_bfree * s.f_frsize;
    Some((total.saturating_sub(free), (total - free) + avail))
}

/// The whole disk a partition (nvme0n1p2, sda1, mmcblk0p1) belongs to.
pub fn parent_disk(dev: &str) -> String {
    let name = dev.trim_start_matches("/dev/");
    if name.starts_with("nvme") || name.starts_with("mmcblk") {
        if let Some(i) = name.rfind('p')
            && name[i + 1..].chars().all(|c| c.is_ascii_digit())
            && !name[i + 1..].is_empty()
        {
            return name[..i].to_owned();
        }
        return name.to_owned();
    }
    name.trim_end_matches(|c: char| c.is_ascii_digit()).to_owned()
}

/// Network interface kind for the icon and the order.
pub fn net_kind(sys: &Path, iface: &str) -> &'static str {
    let dir = sys.join("class/net").join(iface);
    if dir.join("wireless").exists() || dir.join("phy80211").exists() {
        "wifi"
    } else if iface.starts_with("docker") || iface.starts_with("br-") || iface.starts_with("veth") || iface.starts_with("virbr") {
        "virtual"
    } else if iface.starts_with("wg") || iface.starts_with("tun") || iface.starts_with("tailscale") {
        "vpn"
    } else if dir.join("device").exists() {
        "ethernet"
    } else {
        "virtual"
    }
}

pub fn net_up(sys: &Path, iface: &str) -> bool {
    let dir = sys.join("class/net").join(iface);
    matches!(read(&dir.join("operstate")).as_deref(), Some("up") | Some("unknown")) && read(&dir.join("carrier")).as_deref() != Some("0")
}

/// Link speed in Mbit/s (Ethernet only; Wi-Fi reports -1).
pub fn net_speed(sys: &Path, iface: &str) -> Option<u64> {
    read_i64(&sys.join("class/net").join(iface).join("speed")).filter(|v| *v > 0).map(|v| v as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn put(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    #[test]
    fn cpu_frequencies_in_natural_order() {
        let t = tempfile::tempdir().unwrap();
        for (i, khz) in [(0, 800000), (1, 4600000), (10, 1200000), (2, 2000000)] {
            put(t.path(), &format!("devices/system/cpu/cpu{i}/cpufreq/scaling_cur_freq"), &khz.to_string());
            put(t.path(), &format!("devices/system/cpu/cpu{i}/thermal_throttle/core_throttle_count"), "2");
        }
        put(t.path(), "devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq", "5100000");
        put(t.path(), "devices/system/cpu/cpu0/cpufreq/scaling_governor", "powersave");
        put(t.path(), "devices/system/cpu/cpu0/thermal_throttle/package_throttle_count", "5");
        put(t.path(), "devices/system/cpu/cpufreq/x", "");
        let f = cpu_freq(t.path());
        assert_eq!(f.mhz, vec![800, 4600, 2000, 1200]);
        assert_eq!((f.max_mhz, f.governor.as_str(), f.throttle_events), (5100, "powersave", 13));
        assert_eq!(f.core_throttles, vec![2, 2, 2, 2]);
    }

    #[test]
    fn a_core_counts_as_throttled_for_two_seconds() {
        let mut last = Vec::new();
        // First sample: nothing to compare with.
        assert_eq!(core_throttling(&[], &[5, 5], &mut last, 1_000), vec![false, false]);
        assert_eq!(core_throttling(&[5, 5], &[6, 5], &mut last, 2_000), vec![true, false]);
        assert_eq!(core_throttling(&[6, 5], &[6, 5], &mut last, 3_500), vec![true, false]);
        assert_eq!(core_throttling(&[6, 5], &[6, 5], &mut last, 4_100), vec![false, false]);
    }

    #[test]
    fn cpu_model_from_cpuinfo() {
        let (m, c) = cpu_model("processor\t: 0\nmodel name\t: Intel(R) Core(TM)   i7-10875H CPU @ 2.30GHz\ncache size\t: 16384 KB\n");
        assert_eq!(m, "Intel(R) Core(TM) i7-10875H CPU @ 2.30GHz");
        assert_eq!(c, "16384 KB");
    }

    #[test]
    fn core_temperatures_follow_the_topology() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), "class/hwmon/hwmon6/name", "coretemp");
        put(t.path(), "class/hwmon/hwmon6/temp1_label", "Package id 0");
        put(t.path(), "class/hwmon/hwmon6/temp1_input", "80000");
        put(t.path(), "class/hwmon/hwmon6/temp2_label", "Core 0");
        put(t.path(), "class/hwmon/hwmon6/temp2_input", "70000");
        put(t.path(), "class/hwmon/hwmon6/temp3_label", "Core 4");
        put(t.path(), "class/hwmon/hwmon6/temp3_input", "75000");
        // Two threads per core; cpu3's core has no sensor.
        for (cpu, core) in [(0, 0), (1, 4), (2, 0), (3, 9)] {
            put(t.path(), &format!("devices/system/cpu/cpu{cpu}/topology/core_id"), &core.to_string());
        }
        let (temps, _) = sensors(t.path());
        assert_eq!(core_temperatures(t.path(), &temps, 4), vec![Some(70.0), Some(75.0), Some(70.0), None]);
        // No per-core sensors (k10temp): nothing per core.
        assert_eq!(core_temperatures(t.path(), &[], 2), vec![None, None]);
    }

    #[test]
    fn sensors_and_cpu_temperature() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), "class/hwmon/hwmon1/name", "acpitz");
        put(t.path(), "class/hwmon/hwmon1/temp1_input", "50000");
        put(t.path(), "class/hwmon/hwmon6/name", "coretemp");
        put(t.path(), "class/hwmon/hwmon6/temp1_input", "76000");
        put(t.path(), "class/hwmon/hwmon6/temp1_label", "Package id 0");
        put(t.path(), "class/hwmon/hwmon6/temp1_crit", "100000");
        put(t.path(), "class/hwmon/hwmon6/temp2_input", "-273000");
        put(t.path(), "class/hwmon/hwmon4/name", "thinkpad");
        put(t.path(), "class/hwmon/hwmon4/fan1_input", "2900");
        let (temps, fans) = sensors(t.path());
        assert_eq!(temps.len(), 2);
        let cpu = cpu_temperature(&temps).unwrap();
        assert_eq!((cpu.label.as_str(), cpu.celsius, cpu.crit), ("Package id 0", 76.0, Some(100.0)));
        assert_eq!(
            fans,
            vec![Fan {
                chip: "thinkpad".into(),
                label: "Fan 1".into(),
                rpm: 2900
            }]
        );
    }

    #[test]
    fn battery_watts_and_time_left() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), "class/power_supply/AC/type", "Mains");
        put(t.path(), "class/power_supply/AC/online", "0");
        let b = "class/power_supply/BAT0/";
        for (k, v) in [
            ("type", "Battery"),
            ("status", "Discharging"),
            ("capacity", "50"),
            ("power_now", "20000000"),
            ("energy_now", "40000000"),
            ("energy_full", "80000000"),
            ("energy_full_design", "90000000"),
            ("cycle_count", "106"),
        ] {
            put(t.path(), &format!("{b}{k}"), v);
        }
        put(t.path(), "class/power_supply/hidpp_battery_0/type", "Battery");
        put(t.path(), "class/power_supply/hidpp_battery_0/scope", "Device");
        let p = power(t.path());
        assert!(!p.on_ac);
        assert_eq!(p.batteries.len(), 1);
        let bat = &p.batteries[0];
        assert_eq!((bat.watts, bat.energy_wh, bat.design_wh, bat.cycles), (20.0, 40.0, 90.0, 106));
        assert_eq!(bat.seconds_left, Some(7200));
        // Charge-only battery: µAh × V.
        let t2 = tempfile::tempdir().unwrap();
        for (k, v) in [
            ("type", "Battery"),
            ("status", "Charging"),
            ("charge_now", "1000000"),
            ("charge_full", "2000000"),
            ("current_now", "1000000"),
            ("voltage_now", "10000000"),
        ] {
            put(t2.path(), &format!("class/power_supply/BAT1/{k}"), v);
        }
        let bat = &power(t2.path()).batteries[0];
        assert_eq!((bat.energy_wh, bat.full_wh, bat.watts, bat.percent), (10.0, 20.0, 10.0, 50.0));
        assert_eq!(bat.seconds_left, Some(3600));
        assert!(power(tempfile::tempdir().unwrap().path()).on_ac);
    }

    #[test]
    fn nvidia_smi_lines() {
        let g = parse_nvidia_smi("00000000:01:00.0, Quadro T1000, 34, 902, 4096, 51, 9.70, 345, 0, [N/A]\n");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].bus, "0000:01:00.0");
        assert_eq!(
            (g[0].busy, g[0].vram_used, g[0].temp, g[0].watts),
            (34.0, 902 * 1024 * 1024, Some(51.0), Some(9.7))
        );
        assert_eq!(g[0].decoder, None);
        assert!(parse_nvidia_smi("No devices were found").is_empty());
    }

    #[test]
    fn mounts_and_disks() {
        let m = parse_mounts(
            "/dev/nvme0n1p2 / btrfs rw 0 0\n/dev/nvme0n1p2 /home btrfs rw 0 0\nproc /proc proc rw 0 0\n/dev/nvme0n1p1 /boot vfat rw 0 0\n/dev/sdb1 /run/media/tom/My\\040Disk exfat rw 0 0\n",
        );
        assert_eq!(
            m.iter().map(|m| m.path.as_str()).collect::<Vec<_>>(),
            vec!["/", "/boot", "/run/media/tom/My Disk"]
        );
        assert_eq!(parent_disk("/dev/nvme0n1p2"), "nvme0n1");
        assert_eq!(parent_disk("sda12"), "sda");
        assert_eq!(parent_disk("mmcblk0p1"), "mmcblk0");
        assert_eq!(parent_disk("nvme0n1"), "nvme0n1");
        assert!(fs_usage("/").is_some_and(|(u, t)| t > 0 && u <= t));
    }
}
