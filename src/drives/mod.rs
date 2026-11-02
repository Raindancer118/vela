//! Drives: formats USB sticks and SD cards and writes images onto them.
//! Everything goes through udisks2, so nothing runs as root. The window is
//! Quickshell (`shell/drives.qml`), fed by `vela drives serve` (JSON lines);
//! Dolphin's context menu opens it (data/vela-drives-servicemenu.desktop).

pub mod devices;
pub mod format;
pub mod images;
pub mod probe;
pub mod serve;
pub mod udisks;
pub mod ventoy;
pub mod write;

use serde_json::json;
use std::path::Path;

/// What the window should show first, from the paths Dolphin passes: image
/// files to offer, a folder on a stick (→ that stick).
pub fn open_request(paths: &[std::path::PathBuf], tab: &str) -> serde_json::Value {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let mut images = Vec::new();
    let mut drive = String::new();
    for p in paths {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        if p.is_dir() {
            if drive.is_empty()
                && let Some(dev) = devices::device_of_path(&mountinfo, &p.to_string_lossy())
            {
                drive = devices::whole_disk(&dev, Path::new("/sys/class/block"));
            }
        } else if images::is_image_name(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()) {
            images.push(p);
        }
    }
    let tab = if tab.is_empty() && !images.is_empty() { "image" } else { tab };
    json!({ "tab": tab, "drive": drive, "images": images })
}
