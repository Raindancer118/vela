//! Tiny client for Hyprland's request socket, used to put the launcher on
//! the focused monitor. Everything degrades to `None` outside Hyprland.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

fn socket_path() -> Option<PathBuf> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().filter(|s| !s.is_empty())?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok().filter(|s| !s.is_empty())?;
    Some(PathBuf::from(runtime).join("hypr").join(sig).join(".socket.sock"))
}

fn request(cmd: &str) -> Option<String> {
    let mut stream = UnixStream::connect(socket_path()?).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(250))).ok()?;
    stream.set_write_timeout(Some(Duration::from_millis(250))).ok()?;
    stream.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    stream.read_to_string(&mut out).ok()?;
    Some(out)
}

/// Name of the focused monitor from `j/monitors` output.
pub fn parse_focused_monitor(json: &str) -> Option<String> {
    let monitors: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    monitors
        .iter()
        .find(|m| m.get("focused").and_then(|f| f.as_bool()) == Some(true))?
        .get("name")?
        .as_str()
        .map(str::to_owned)
}

pub fn focused_monitor() -> Option<String> {
    parse_focused_monitor(&request("j/monitors")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_focused_monitor() {
        let json = r#"[{"id":0,"name":"eDP-1","focused":false},{"id":1,"name":"DP-6","focused":true}]"#;
        assert_eq!(parse_focused_monitor(json).as_deref(), Some("DP-6"));
        assert_eq!(parse_focused_monitor("[]"), None);
        assert_eq!(parse_focused_monitor("not json"), None);
    }
}
