//! `vela-hotspot-helper`, run as root through polkit (pkexec): opens an
//! access point for one transfer and closes it again.
//!
//! `vela-hotspot-helper start PORT`: starts the access point ([`ap`]), gives
//! its interface [`AP_IP`], hands out addresses over DHCP, lets PORT (TCP)
//! and DHCP in on that interface only, prints one JSON line with what the
//! phone needs, and then waits. When stdin ends (or
//! says `stop`), on SIGTERM/SIGINT/SIGHUP or after [`MAX_LIFETIME`],
//! everything is undone. Nothing else can be asked of it: no paths, no
//! commands, no other interfaces.

use std::io::{BufRead, Write};
use std::net::{Ipv4Addr, UdpSocket};
use std::os::fd::FromRawFd;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, bail};
use serde_json::json;

use super::ap::{self, Hotspot};
use super::dhcp;
use super::{AP_IP, AP_MASK, AP_PREFIX, POOL_FIRST, POOL_LAST};

/// The access point is closed after this long, whatever vela does.
const MAX_LIFETIME: Duration = Duration::from_secs(2 * 60 * 60);
/// Programs are only taken from here (pkexec's PATH is not trusted).
const BIN_DIRS: [&str; 5] = ["/usr/bin", "/usr/sbin", "/bin", "/sbin", "/run/current-system/sw/bin"];

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

pub fn main(args: &[String]) -> ExitCode {
    let port = match args {
        [cmd, port] if cmd == "start" => match parse_port(port) {
            Some(p) => p,
            None => return usage(),
        },
        _ => return usage(),
    };
    match run(port) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("vela-hotspot-helper: {e:#}");
            println!("{}", json!({ "error": format!("{e:#}") }));
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: vela-hotspot-helper start PORT");
    ExitCode::from(2)
}

/// A TCP port vela may listen on (not a privileged one).
fn parse_port(s: &str) -> Option<u16> {
    s.parse::<u16>().ok().filter(|p| *p >= 1024)
}

fn run(port: u16) -> anyhow::Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        bail!("must run as root (through pkexec)");
    }
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        // SAFETY: the handler only stores to an atomic.
        unsafe { libc::signal(sig, on_signal as *const () as libc::sighandler_t) };
    }
    let tool = |name: &str| find_tool(name).with_context(|| format!("{name} not found"));
    let (iw, ip) = (tool("iw")?, tool("ip")?);
    let hostapd = tool("hostapd").context("Quick Share over a direct Wi-Fi connection needs hostapd")?;
    let radio = ap::radio(&iw)?;
    let hotspot = Hotspot::start(&iw, &ip, &hostapd, &radio, nm_leave_alone)?;
    let ap = hotspot.ap.clone();
    let iface = ap.iface.clone();
    run_tool("ip", &["addr", "add", &format!("{AP_IP}/{AP_PREFIX}"), "dev", &iface])?;
    let _firewall = Firewall::open(&iface, port);
    let _dhcp = DhcpThread::start(&iface)?;

    let line = json!({
        "iface": iface,
        "ssid": ap.ssid,
        "passphrase": ap.passphrase,
        "frequency": ap.frequency,
        "ip": AP_IP.to_string(),
    });
    let mut out = std::io::stdout().lock();
    writeln!(out, "{line}")?;
    out.flush()?;
    drop(out);
    eprintln!("vela-hotspot-helper: access point {} on {iface} ({} MHz) is up", ap.ssid, ap.frequency);

    wait_for_stop();
    eprintln!("vela-hotspot-helper: closing the access point");
    Ok(())
}

/// Until stdin ends or says `stop`, a signal comes or [`MAX_LIFETIME`] passed.
fn wait_for_stop() {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            match line {
                Ok(l) if l.trim() == "stop" => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let _ = tx.send(());
    });
    let started = std::time::Instant::now();
    while !STOP.load(Ordering::SeqCst) && started.elapsed() < MAX_LIFETIME {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            _ => break,
        }
    }
}

/// Tells NetworkManager (if it runs) to keep its hands off our interface;
/// it may take a moment to notice the interface first. The interface is
/// removed at the end, so nothing needs undoing.
fn nm_leave_alone(iface: &str) {
    if find_tool("nmcli").is_none() {
        return;
    }
    for _ in 0..20 {
        if run_tool("nmcli", &["device", "set", iface, "managed", "no"]).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("vela-hotspot-helper: NetworkManager does not know {iface}, leaving it as is");
}

fn find_tool(name: &str) -> Option<PathBuf> {
    BIN_DIRS.iter().map(|d| PathBuf::from(d).join(name)).find(|p| p.is_file())
}

fn run_tool(name: &str, args: &[&str]) -> anyhow::Result<()> {
    let tool = find_tool(name).with_context(|| format!("{name} not found"))?;
    let out = Command::new(&tool)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("running {name}"))?;
    if !out.status.success() {
        bail!("{name} {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

/// Lets the phone in on our interface only, until dropped: through
/// firewalld (runtime only, trusted zone) if it runs, else iptables rules
/// (ufw and the NixOS firewall use iptables).
enum Firewall {
    Firewalld(String),
    Iptables(Vec<Vec<String>>),
    Nothing,
}

impl Firewall {
    fn open(iface: &str, port: u16) -> Firewall {
        if find_tool("firewall-cmd").is_some() && run_tool("firewall-cmd", &["--state"]).is_ok() {
            match run_tool("firewall-cmd", &["--zone=trusted", &format!("--add-interface={iface}")]) {
                Ok(()) => return Firewall::Firewalld(iface.to_owned()),
                Err(e) => eprintln!("vela-hotspot-helper: firewalld: {e:#}"),
            }
        }
        if find_tool("iptables").is_none() {
            return Firewall::Nothing;
        }
        let mut added = Vec::new();
        for rule in iptables_rules(iface, port) {
            let mut args = vec!["-I".to_owned(), "INPUT".to_owned(), "1".to_owned()];
            args.extend(rule.iter().cloned());
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            match run_tool("iptables", &refs) {
                Ok(()) => added.push(rule),
                Err(e) => eprintln!("vela-hotspot-helper: {e:#}"),
            }
        }
        Firewall::Iptables(added)
    }
}

impl Drop for Firewall {
    fn drop(&mut self) {
        let result = match self {
            Firewall::Firewalld(iface) => run_tool("firewall-cmd", &["--zone=trusted", &format!("--remove-interface={iface}")]),
            Firewall::Iptables(rules) => rules.iter().try_for_each(|rule| {
                let mut args = vec!["-D", "INPUT"];
                args.extend(rule.iter().map(String::as_str));
                run_tool("iptables", &args)
            }),
            Firewall::Nothing => Ok(()),
        };
        if let Err(e) = result {
            eprintln!("vela-hotspot-helper: {e:#}");
        }
    }
}

/// The two rules: the transfer port (TCP) and DHCP (UDP 67).
fn iptables_rules(iface: &str, port: u16) -> Vec<Vec<String>> {
    let rule = |proto: &str, port: u16| -> Vec<String> {
        ["-i", iface, "-p", proto, "--dport", &port.to_string(), "-j", "ACCEPT"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    };
    vec![rule("tcp", port), rule("udp", 67)]
}

/// The DHCP server on the access point, until dropped.
struct DhcpThread {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl DhcpThread {
    fn start(iface: &str) -> anyhow::Result<DhcpThread> {
        let socket = dhcp_socket(iface).context("opening the DHCP port")?;
        socket.set_read_timeout(Some(Duration::from_millis(250)))?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::Builder::new().name("dhcp".into()).spawn(move || {
            let mut server = dhcp::Server::new(AP_IP, AP_MASK, POOL_FIRST, POOL_LAST);
            let mut buf = [0u8; 1500];
            while !stopped.load(Ordering::SeqCst) {
                let Ok((n, _)) = socket.recv_from(&mut buf) else { continue };
                if let Some(reply) = server.handle(&buf[..n]) {
                    // The client has no address yet: broadcast on the interface.
                    let _ = socket.send_to(&reply, (Ipv4Addr::BROADCAST, 68));
                }
            }
        })?;
        Ok(DhcpThread { stop, thread: Some(thread) })
    }
}

impl Drop for DhcpThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// UDP port 67 on `iface` only (SO_BINDTODEVICE), with broadcast allowed;
/// SO_REUSEADDR so another DHCP server bound to other interfaces does not
/// keep it from starting.
fn dhcp_socket(iface: &str) -> std::io::Result<UdpSocket> {
    let check = |r: libc::c_int| if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(r) };
    // SAFETY: plain socket calls on a fresh descriptor, owned by the UdpSocket on success.
    unsafe {
        let fd = check(libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0))?;
        let socket = UdpSocket::from_raw_fd(fd);
        let one: libc::c_int = 1;
        let opt = |level, name, value: *const libc::c_void, len| check(libc::setsockopt(fd, level, name, value, len as libc::socklen_t));
        let int = std::mem::size_of::<libc::c_int>();
        opt(libc::SOL_SOCKET, libc::SO_REUSEADDR, (&raw const one).cast(), int)?;
        opt(libc::SOL_SOCKET, libc::SO_BROADCAST, (&raw const one).cast(), int)?;
        opt(libc::SOL_SOCKET, libc::SO_BINDTODEVICE, iface.as_ptr().cast(), iface.len())?;
        let addr = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: 67u16.to_be(),
            sin_addr: libc::in_addr { s_addr: 0 },
            sin_zero: [0; 8],
        };
        check(libc::bind(
            fd,
            (&raw const addr).cast(),
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        ))?;
        Ok(socket)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unprivileged_ports_are_taken() {
        assert_eq!(parse_port("47392"), Some(47392));
        assert_eq!(parse_port("22"), None);
        assert_eq!(parse_port("70000"), None);
        assert_eq!(parse_port("-1"), None);
        assert_eq!(parse_port("4739; rm"), None);
    }

    #[test]
    fn the_firewall_lets_in_the_port_and_dhcp_on_our_interface_only() {
        let rules = iptables_rules("p2p-wlan0-0", 47392);
        assert_eq!(rules[0].join(" "), "-i p2p-wlan0-0 -p tcp --dport 47392 -j ACCEPT");
        assert_eq!(rules[1].join(" "), "-i p2p-wlan0-0 -p udp --dport 67 -j ACCEPT");
    }

    #[test]
    fn unknown_tools_are_not_found() {
        assert!(find_tool("vela-no-such-tool").is_none());
    }
}
