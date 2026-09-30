//! Line-based control protocol over a Unix domain socket
//! (`$XDG_RUNTIME_DIR/vela.sock`). The client side never touches GTK, so
//! `vela toggle` stays a few milliseconds even though the daemon is a GTK app.

use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

/// Pages of the settings window (`vela settings <page>`).
pub const SETTINGS_PAGES: [&str; 6] = ["general", "appearance", "apps", "search", "claude", "panel"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Show,
    Hide,
    /// Optional page, one of `SETTINGS_PAGES`.
    Settings(Option<&'static str>),
    Reload,
    Quit,
    Ping,
}

impl Command {
    pub fn to_line(self) -> String {
        match self {
            Command::Settings(Some(page)) => format!("settings {page}"),
            other => other.as_str().to_owned(),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Command::Toggle => "toggle",
            Command::Show => "show",
            Command::Hide => "hide",
            Command::Settings(_) => "settings",
            Command::Reload => "reload",
            Command::Quit => "quit",
            Command::Ping => "ping",
        }
    }

    pub fn parse(s: &str) -> Option<Command> {
        let s = s.trim();
        if let Some(page) = s.strip_prefix("settings ") {
            return SETTINGS_PAGES.iter().find(|p| **p == page.trim()).map(|p| Command::Settings(Some(p)));
        }
        Some(match s {
            "toggle" => Command::Toggle,
            "show" => Command::Show,
            "hide" => Command::Hide,
            "settings" => Command::Settings(None),
            "reload" => Command::Reload,
            "quit" => Command::Quit,
            "ping" => Command::Ping,
            _ => return None,
        })
    }
}

#[derive(Debug)]
pub enum SendError {
    /// Nobody listens on the socket.
    NotRunning,
    Other(anyhow::Error),
}

pub fn send(socket: &Path, cmd: Command) -> Result<String, SendError> {
    let mut stream = match UnixStream::connect(socket) {
        Ok(s) => s,
        Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused) => return Err(SendError::NotRunning),
        Err(e) => return Err(SendError::Other(e.into())),
    };
    let io = |e: std::io::Error| SendError::Other(e.into());
    stream.set_read_timeout(Some(Duration::from_secs(2))).map_err(io)?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(io)?;
    stream.write_all(format!("{}\n", cmd.to_line()).as_bytes()).map_err(io)?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).map_err(io)?;
    Ok(reply.trim().to_owned())
}

/// Binds the socket; fails if another instance is alive, replaces stale ones.
pub fn bind(socket: &Path) -> Result<UnixListener> {
    if socket.exists() {
        if UnixStream::connect(socket).is_ok() {
            bail!("another vela instance is already running ({})", socket.display());
        }
        std::fs::remove_file(socket).with_context(|| format!("removing stale socket {}", socket.display()))?;
    }
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = UnixListener::bind(socket).with_context(|| format!("binding {}", socket.display()))?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Accepts connections on a background thread and forwards commands.
pub fn serve(listener: UnixListener, commands: async_channel::Sender<Command>) {
    std::thread::Builder::new()
        .name("vela-ipc".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                let mut line = String::new();
                let mut reader = BufReader::new(&stream);
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let reply = match Command::parse(&line) {
                    Some(cmd) => {
                        if commands.send_blocking(cmd).is_err() {
                            return;
                        }
                        "ok"
                    }
                    None => "error: unknown command",
                };
                let _ = (&stream).write_all(format!("{reply}\n").as_bytes());
            }
        })
        .expect("spawn ipc thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_stale_socket() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("vela.sock");
        assert!(matches!(send(&sock, Command::Ping), Err(SendError::NotRunning)));

        // A leftover socket file without a listener is replaced.
        drop(UnixListener::bind(&sock).unwrap());
        let listener = bind(&sock).unwrap();
        // A second instance is refused while the first one is alive.
        assert!(bind(&sock).is_err());

        let (tx, rx) = async_channel::unbounded();
        serve(listener, tx);
        assert_eq!(send(&sock, Command::Toggle).unwrap(), "ok");
        assert_eq!(rx.recv_blocking().unwrap(), Command::Toggle);
        assert_eq!(send(&sock, Command::Settings(Some("claude"))).unwrap(), "ok");
        assert_eq!(rx.recv_blocking().unwrap(), Command::Settings(Some("claude")));

        let mut raw = UnixStream::connect(&sock).unwrap();
        raw.write_all(b"rm -rf /\n").unwrap();
        let mut reply = String::new();
        BufReader::new(raw).read_line(&mut reply).unwrap();
        assert!(reply.starts_with("error"));
    }

    #[test]
    fn parse_all_commands() {
        for c in [
            Command::Toggle,
            Command::Show,
            Command::Hide,
            Command::Settings(None),
            Command::Settings(Some("apps")),
            Command::Reload,
            Command::Quit,
            Command::Ping,
        ] {
            assert_eq!(Command::parse(&c.to_line()), Some(c));
        }
        assert_eq!(Command::parse("settings nope"), None);
        assert_eq!(Command::parse("nope"), None);
    }
}
