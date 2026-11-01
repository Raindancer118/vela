//! Wi-Fi Direct for Quick Share without a shared Wi-Fi: vela opens a group
//! as its owner and hands the phone the credentials during the bandwidth
//! upgrade. Opening a group needs root, so it is done by
//! `vela-p2p-helper` ([`helper`]) through polkit; vela only starts it
//! ([`Group::open`]) and keeps it alive while the transfer runs.

pub mod dhcp;
pub mod helper;
pub mod hotspot;

use std::io::{BufRead, BufReader, Write};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, bail};
use serde::Deserialize;

/// Our address in the group, the one Android uses as group owner too.
pub const GO_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 49, 1);
pub const GO_MASK: Ipv4Addr = Ipv4Addr::new(255, 255, 255, 0);
pub const GO_PREFIX: u8 = 24;
pub const POOL_FIRST: Ipv4Addr = Ipv4Addr::new(192, 168, 49, 2);
pub const POOL_LAST: Ipv4Addr = Ipv4Addr::new(192, 168, 49, 20);

/// Where the helper is installed (root-owned, named in the polkit policy).
pub const HELPER_PATHS: [&str; 2] = ["/usr/lib/vela/vela-p2p-helper", "/run/current-system/sw/libexec/vela/vela-p2p-helper"];
/// How long starting the group may take, polkit included.
const OPEN_TIMEOUT: Duration = Duration::from_secs(30);
/// How long the helper gets to close the group after stdin ends.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

/// What the phone needs to join the group.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Credentials {
    pub iface: String,
    pub ssid: String,
    pub passphrase: String,
    pub frequency: u32,
    pub ip: Ipv4Addr,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum HelperLine {
    Up(Credentials),
    Failed { error: String },
}

/// The installed helper, if any.
pub fn helper() -> Option<PathBuf> {
    HELPER_PATHS.iter().map(PathBuf::from).find(|p| p.is_file())
}

/// An open group; closed when dropped.
pub struct Group {
    pub credentials: Credentials,
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Group {
    /// Opens a group whose phone may connect to `port`: runs the helper
    /// through pkexec and waits for its line.
    pub fn open(port: u16) -> anyhow::Result<Group> {
        let helper = helper().context("Wi-Fi Direct is not installed (vela-p2p-helper is missing)")?;
        Group::open_with(Path::new("pkexec"), &[helper.as_os_str().to_str().context("helper path")?], port)
    }

    /// [`Group::open`] with `program args… start PORT` (tests use a fake helper).
    pub fn open_with(program: &Path, args: &[&str], port: u16) -> anyhow::Result<Group> {
        let mut child = Command::new(program)
            .args(args)
            .args(["start", &port.to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("starting {}", program.display()))?;
        let stdout = child.stdout.take().context("helper stdout")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = tx.send(BufReader::new(stdout).read_line(&mut line).map(|_| line));
        });
        let stdin = child.stdin.take();
        let mut group = Group {
            credentials: Credentials {
                iface: String::new(),
                ssid: String::new(),
                passphrase: String::new(),
                frequency: 0,
                ip: GO_IP,
            },
            child,
            stdin,
        };
        let line = match rx.recv_timeout(OPEN_TIMEOUT) {
            Ok(Ok(line)) if !line.trim().is_empty() => line,
            Ok(Ok(_)) => bail!("the Wi-Fi Direct helper ended without a group (polkit refused?)"),
            Ok(Err(e)) => bail!("reading from the Wi-Fi Direct helper: {e}"),
            Err(_) => bail!("the Wi-Fi Direct group did not come up within {OPEN_TIMEOUT:?}"),
        };
        match serde_json::from_str::<HelperLine>(line.trim()).context("the Wi-Fi Direct helper's answer")? {
            HelperLine::Up(c) => group.credentials = c,
            HelperLine::Failed { error } => bail!("Wi-Fi Direct: {error}"),
        }
        Ok(group)
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if let Some(mut stdin) = self.stdin.take() {
            let _ = writeln!(stdin, "stop");
        }
        let deadline = std::time::Instant::now() + CLOSE_TIMEOUT;
        while std::time::Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        log::warn!("quickshare: the Wi-Fi Direct helper did not end in time");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A helper script that prints `line` and then, like the real one,
    /// waits for stdin to end (recording that in `ended`).
    fn fake_helper(dir: &Path, line: &str) -> PathBuf {
        let script = dir.join("helper.sh");
        let ended = dir.join("ended");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n[ \"$1 $2\" = \"start 47392\" ] || exit 9\necho '{line}'\nread _\ntouch '{}'\n",
                ended.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[test]
    fn a_group_comes_up_and_is_closed_when_dropped() {
        let d = tempfile::tempdir().unwrap();
        let line = r#"{"iface":"p2p-wlan0-0","ssid":"DIRECT-ab-Eclipse","passphrase":"x7Yz12ab","frequency":2437,"ip":"192.168.49.1"}"#;
        let helper = fake_helper(d.path(), line);
        let group = Group::open_with(&helper, &[], 47392).unwrap();
        assert_eq!(group.credentials.ssid, "DIRECT-ab-Eclipse");
        assert_eq!(group.credentials.frequency, 2437);
        assert_eq!(group.credentials.ip, GO_IP);
        assert!(!d.path().join("ended").exists());
        drop(group);
        assert!(d.path().join("ended").exists(), "the helper was told to stop");
    }

    #[test]
    fn a_helper_error_is_reported() {
        let d = tempfile::tempdir().unwrap();
        let helper = fake_helper(d.path(), r#"{"error":"no Wi-Fi interface with Wi-Fi Direct (P2P) support"}"#);
        let err = Group::open_with(&helper, &[], 47392).err().unwrap();
        assert!(format!("{err:#}").contains("P2P"), "{err:#}");
    }

    #[test]
    fn a_helper_that_says_nothing_is_reported() {
        let err = Group::open_with(Path::new("/bin/true"), &[], 47392).err().unwrap();
        assert!(format!("{err:#}").contains("without a group"), "{err:#}");
    }
}
