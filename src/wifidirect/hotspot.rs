//! A Wi-Fi access point of our own for one transfer: hostapd on an extra
//! interface ([`IFACE`]) of the Wi-Fi card, next to the station interface.
//!
//! Why not Wi-Fi Direct with us as group owner: NetworkManager removes every
//! P2P group it did not start itself from the system's wpa_supplicant, and a
//! second wpa_supplicant needs a second station interface, which cards
//! allow only once. An AP interface next to the station one is allowed
//! (on the station's channel), and Quick Share phones join an access point
//! as well (bandwidth upgrade medium WIFI_HOTSPOT).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};

/// Our interface. Fixed, so a leftover from a crash can be found and removed.
pub const IFACE: &str = "vela-ap";
/// hostapd's config and log (root only).
const RUN_DIR: &str = "/run/vela-ap";
/// How long the access point may take to come up.
const START_TIMEOUT: Duration = Duration::from_secs(15);

/// What the phone needs to join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPoint {
    pub iface: String,
    pub ssid: String,
    pub passphrase: String,
    pub frequency: u32,
}

/// The Wi-Fi card of the station interface (`phy0`) and the frequency the
/// station is on, if connected.
pub struct Radio {
    pub phy: String,
    pub frequency: Option<u32>,
}

/// The first Wi-Fi interface that is not ours: its card and frequency.
pub fn radio(iw: &Path) -> anyhow::Result<Radio> {
    let net = Path::new("/sys/class/net");
    let mut stations: Vec<String> = fs::read_dir(net)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n != IFACE && net.join(n).join("phy80211").exists())
        .collect();
    stations.sort();
    let station = stations.first().context("no Wi-Fi card")?;
    let phy = fs::read_to_string(net.join(station).join("phy80211/name"))?.trim().to_owned();
    let info = Command::new(iw).args(["dev", station, "info"]).output().ok();
    let frequency = info.and_then(|o| parse_channel_freq(&String::from_utf8_lossy(&o.stdout)));
    Ok(Radio { phy, frequency })
}

/// The MHz in `iw dev X info`'s `channel 11 (2462 MHz), …` line.
fn parse_channel_freq(info: &str) -> Option<u32> {
    let line = info.lines().find(|l| l.trim_start().starts_with("channel "))?;
    let open = line.find('(')?;
    line[open + 1..].split_whitespace().next()?.parse().ok()
}

/// How an access point is set up: band (`g` 2.4 GHz, `a` 5 GHz), primary
/// channel, width (20, 40, 80 MHz) and whether Wi-Fi 5/6 are switched on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Plan {
    mode: &'static str,
    channel: u32,
    width: u32,
    /// Wi-Fi 6 (802.11ax) on top of Wi-Fi 5 (802.11ac, 5 GHz only).
    ax: bool,
}

/// Channel 149 (5745 MHz): the 5 GHz channel Intel cards let an access
/// point start on by itself (the others need a station on them).
const ALONE_5GHZ: u32 = 149;

/// What to try, fastest first. The card allows an access point only on the
/// channel the station is on: on 5 GHz that is 80 MHz with Wi-Fi 6, on
/// 2.4 GHz 40 MHz. Without a station, 5 GHz channel 149, else 2.4 GHz.
fn plans(station: Option<u32>) -> Vec<Plan> {
    let fast_5ghz = |channel| {
        vec![
            Plan {
                mode: "a",
                channel,
                width: 80,
                ax: true,
            },
            Plan {
                mode: "a",
                channel,
                width: 80,
                ax: false,
            },
            Plan {
                mode: "a",
                channel,
                width: 40,
                ax: false,
            },
        ]
    };
    let ghz24 = |channel| {
        vec![
            Plan {
                mode: "g",
                channel,
                width: 40,
                ax: false,
            },
            Plan {
                mode: "g",
                channel,
                width: 20,
                ax: false,
            },
        ]
    };
    match station {
        Some(f @ 2412..=2472) => ghz24((f - 2407) / 5),
        Some(2484) => vec![Plan {
            mode: "g",
            channel: 14,
            width: 20,
            ax: false,
        }],
        Some(f @ 5150..=5895) => fast_5ghz((f - 5000) / 5),
        _ => {
            let mut all = fast_5ghz(ALONE_5GHZ);
            all.extend(ghz24(6));
            all
        }
    }
}

/// The center channel of the 80 MHz block `channel` is in (5 GHz).
fn center_80(channel: u32) -> u32 {
    match channel {
        36..=48 => 42,
        52..=64 => 58,
        100..=112 => 106,
        116..=128 => 122,
        132..=144 => 138,
        _ => 155,
    }
}

/// Where the second 20 MHz of a 40 MHz channel lies: above or below.
fn ht40(mode: &str, channel: u32) -> &'static str {
    let above = if mode == "a" { (channel / 4) % 2 == 1 } else { channel <= 7 };
    if above { "[HT40+]" } else { "[HT40-]" }
}

fn random_alnum(n: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    (0..n).map(|_| CHARS[rand::random_range(0..CHARS.len())] as char).collect()
}

/// A locally administered unicast MAC address, so ours differs from the card's.
fn random_mac() -> String {
    let mut b: [u8; 6] = rand::random();
    b[0] = (b[0] & 0xfc) | 0x02;
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(":")
}

/// hostapd's config for `plan`: WPA2 and (with `wpa3`) WPA3 side by side,
/// protected management frames where the phone can. No 802.11b rates:
/// phones fell back to 1 Mbit/s with them on offer.
fn config(ssid: &str, passphrase: &str, plan: &Plan, wpa3: bool) -> String {
    let mut c = format!(
        "interface={IFACE}\n\
         driver=nl80211\n\
         ssid={ssid}\n\
         hw_mode={}\n\
         channel={}\n\
         ieee80211n=1\n\
         wmm_enabled=1\n\
         auth_algs=1\n\
         wpa=2\n\
         rsn_pairwise=CCMP\n\
         wpa_passphrase={passphrase}\n",
        plan.mode, plan.channel
    );
    let mut ht = String::from("[SHORT-GI-20]");
    if plan.width >= 40 {
        ht = format!("{}{ht}[SHORT-GI-40]", ht40(plan.mode, plan.channel));
    }
    c += &format!("ht_capab={ht}\n");
    if plan.mode == "g" {
        c += "supported_rates=60 90 120 180 240 360 480 540\nbasic_rates=60 120 240\n";
    }
    if plan.mode == "a" && plan.width == 80 {
        let center = center_80(plan.channel);
        c += &format!("ieee80211ac=1\nvht_capab=[SHORT-GI-80]\nvht_oper_chwidth=1\nvht_oper_centr_freq_seg0_idx={center}\n");
        if plan.ax {
            c += &format!("ieee80211ax=1\nhe_oper_chwidth=1\nhe_oper_centr_freq_seg0_idx={center}\n");
        }
    }
    if wpa3 {
        c += "wpa_key_mgmt=WPA-PSK SAE\nieee80211w=1\nsae_pwe=2\n";
    } else {
        c += "wpa_key_mgmt=WPA-PSK\n";
    }
    c
}

/// The frequency of `channel` on `mode`.
fn frequency(mode: &str, channel: u32) -> u32 {
    match (mode, channel) {
        ("g", 14) => 2484,
        ("g", c) => 2407 + 5 * c,
        (_, c) => 5000 + 5 * c,
    }
}

/// hostapd on our interface; both gone when dropped.
pub struct Hotspot {
    iw: PathBuf,
    child: Child,
    pub ap: AccessPoint,
}

impl Hotspot {
    /// Adds [`IFACE`] to the card of `radio` and starts an access point on it.
    pub fn start(iw: &Path, ip: &Path, hostapd: &Path, radio: &Radio, leave_alone: impl Fn(&str)) -> anyhow::Result<Hotspot> {
        // Left over from a crash: start clean.
        let _ = Command::new(iw).args(["dev", IFACE, "del"]).stderr(Stdio::null()).status();
        run(Command::new(iw).args(["phy", &radio.phy, "interface", "add", IFACE, "type", "__ap"]))?;
        let remove = || {
            let _ = Command::new(iw).args(["dev", IFACE, "del"]).status();
        };
        let started = (|| {
            leave_alone(IFACE);
            run(Command::new(ip).args(["link", "set", IFACE, "address", &random_mac()]))?;
            fs::create_dir_all(RUN_DIR)?;
            let ssid = format!("vela-{}", random_alnum(4));
            // 32 characters out of 57: about 187 bits.
            let passphrase = random_alnum(32);
            let conf = Path::new(RUN_DIR).join("hostapd.conf");
            let log_path = Path::new(RUN_DIR).join("hostapd.log");
            // Fastest first; the last try without WPA3 in case hostapd lacks SAE.
            let mut tries: Vec<(Plan, bool)> = plans(radio.frequency).into_iter().map(|p| (p, true)).collect();
            if let Some(&(last, _)) = tries.last() {
                tries.push((last, false));
            }
            let mut failures = Vec::new();
            for (plan, wpa3) in tries {
                fs::write(&conf, config(&ssid, &passphrase, &plan, wpa3))?;
                let log = fs::File::create(&log_path)?;
                let mut child = Command::new(hostapd)
                    .arg(&conf)
                    .stdin(Stdio::null())
                    .stdout(log.try_clone()?)
                    .stderr(log)
                    .spawn()
                    .context("starting hostapd")?;
                match wait_enabled(&mut child, &log_path) {
                    Ok(()) => {
                        eprintln!(
                            "vela-p2p-helper: {} MHz, channel {}, {} MHz wide{}{}",
                            frequency(plan.mode, plan.channel),
                            plan.channel,
                            plan.width,
                            if plan.ax { ", Wi-Fi 6" } else { "" },
                            if wpa3 { ", WPA2/WPA3" } else { ", WPA2" }
                        );
                        return Ok((
                            child,
                            AccessPoint {
                                iface: IFACE.to_owned(),
                                ssid,
                                passphrase,
                                frequency: frequency(plan.mode, plan.channel),
                            },
                        ));
                    }
                    Err(e) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        failures.push(format!("{plan:?}: {e:#}"));
                    }
                }
            }
            bail!("the access point did not come up: {}", failures.join("; "))
        })();
        match started {
            Ok((child, ap)) => Ok(Hotspot { iw: iw.to_owned(), child, ap }),
            Err(e) => {
                remove();
                Err(e)
            }
        }
    }
}

/// Until hostapd says `AP-ENABLED`; its last lines if it ends or fails.
fn wait_enabled(child: &mut Child, log: &Path) -> anyhow::Result<()> {
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        let text = fs::read_to_string(log).unwrap_or_default();
        if text.contains("AP-ENABLED") {
            return Ok(());
        }
        let tail = || text.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" / ");
        if let Ok(Some(status)) = child.try_wait() {
            bail!("hostapd ended ({status}): {}", tail());
        }
        if text.contains("AP-DISABLED") || Instant::now() >= deadline {
            bail!("the access point did not come up: {}", tail());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

impl Drop for Hotspot {
    fn drop(&mut self) {
        // SIGTERM lets hostapd deauthenticate the phone and clean up.
        // SAFETY: plain kill(2) on our own child.
        unsafe { libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !matches!(self.child.try_wait(), Ok(Some(_))) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = Command::new(&self.iw).args(["dev", IFACE, "del"]).status();
    }
}

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    let out = cmd.stdin(Stdio::null()).output().with_context(|| format!("running {cmd:?}"))?;
    if !out.status.success() {
        bail!("{cmd:?}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frequency_comes_from_iw() {
        let info = "Interface wlan0\n\tifindex 3\n\ttype managed\n\tchannel 11 (2462 MHz), width: 20 MHz, center1: 2462 MHz\n";
        assert_eq!(parse_channel_freq(info), Some(2462));
        assert_eq!(parse_channel_freq("Interface wlan0\n\ttype managed\n"), None, "not connected");
    }

    #[test]
    fn the_fastest_way_comes_first() {
        let p = plans(Some(2462));
        assert_eq!(
            p[0],
            Plan {
                mode: "g",
                channel: 11,
                width: 40,
                ax: false
            }
        );
        assert_eq!(p[1].width, 20);
        let p = plans(Some(5180));
        assert_eq!(
            p[0],
            Plan {
                mode: "a",
                channel: 36,
                width: 80,
                ax: true
            }
        );
        assert!(p.iter().all(|p| p.channel == 36), "only the station's channel");
        let p = plans(None);
        assert_eq!(
            p[0],
            Plan {
                mode: "a",
                channel: 149,
                width: 80,
                ax: true
            }
        );
        assert_eq!(
            p.last().unwrap(),
            &Plan {
                mode: "g",
                channel: 6,
                width: 20,
                ax: false
            }
        );
        for (f, plan) in [(2462, plans(Some(2462))[0]), (5180, plans(Some(5180))[0]), (2484, plans(Some(2484))[0])] {
            assert_eq!(frequency(plan.mode, plan.channel), f);
        }
    }

    #[test]
    fn wide_channels_are_set_up_right() {
        assert_eq!((ht40("g", 1), ht40("g", 11)), ("[HT40+]", "[HT40-]"));
        assert_eq!(
            (ht40("a", 36), ht40("a", 40), ht40("a", 149), ht40("a", 153)),
            ("[HT40+]", "[HT40-]", "[HT40+]", "[HT40-]")
        );
        assert_eq!((center_80(36), center_80(48), center_80(149), center_80(161)), (42, 42, 155, 155));
        let c = config(
            "vela-x",
            "p",
            &Plan {
                mode: "a",
                channel: 44,
                width: 80,
                ax: true,
            },
            true,
        );
        for line in [
            "ht_capab=[HT40+][SHORT-GI-20][SHORT-GI-40]",
            "vht_oper_centr_freq_seg0_idx=42",
            "he_oper_centr_freq_seg0_idx=42",
            "ieee80211ax=1",
        ] {
            assert!(c.lines().any(|l| l == line), "{line} missing in\n{c}");
        }
        assert!(!c.contains("basic_rates"), "5 GHz has no 802.11b rates anyway");
        let c = config(
            "vela-x",
            "p",
            &Plan {
                mode: "g",
                channel: 11,
                width: 20,
                ax: false,
            },
            false,
        );
        assert!(c.lines().any(|l| l == "ht_capab=[SHORT-GI-20]"));
        assert!(!c.contains("ieee80211ac") && !c.contains("SAE"));
    }

    #[test]
    fn the_config_is_wpa2_and_wpa3_on_our_interface() {
        let c = config("vela-AbCd", "p4ssw0rdp4ssw0rd", &plans(Some(2462))[0], true);
        for line in [
            "interface=vela-ap",
            "ssid=vela-AbCd",
            "hw_mode=g",
            "channel=11",
            "wpa=2",
            "wpa_key_mgmt=WPA-PSK SAE",
            "ieee80211w=1",
            "rsn_pairwise=CCMP",
            "wpa_passphrase=p4ssw0rdp4ssw0rd",
            "basic_rates=60 120 240",
            "ht_capab=[HT40-][SHORT-GI-20][SHORT-GI-40]",
        ] {
            assert!(c.lines().any(|l| l == line), "{line} missing in\n{c}");
        }
    }

    #[test]
    fn secrets_are_random_and_long_enough() {
        let (a, b) = (random_alnum(32), random_alnum(32));
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn our_mac_is_local_and_unicast() {
        for _ in 0..50 {
            let first = u8::from_str_radix(&random_mac()[..2], 16).unwrap();
            assert_eq!(first & 0x03, 0x02);
        }
    }
}
