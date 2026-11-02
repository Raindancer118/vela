//! Ventoy: a stick that boots any ISO lying on it. Installing it needs
//! root (Ventoy2Disk through pkexec); afterwards images are plain files on
//! its first partition, copied there and deleted like any file.

use super::devices::{Drive, Volume};
use super::images;
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Label of Ventoy's small EFI partition: how a Ventoy stick is told apart.
pub const EFI_LABEL: &str = "VTOYEFI";

/// The partition the images go to, if the drive carries Ventoy.
pub fn data_volume(drive: &Drive) -> Option<&Volume> {
    if !drive.volumes.iter().any(|v| v.label == EFI_LABEL) {
        return None;
    }
    drive
        .volumes
        .iter()
        .filter(|v| v.label != EFI_LABEL && v.id != drive.id && !v.fs.is_empty())
        .min_by(|a, b| a.device.cmp(&b.device))
}

/// `ventoy` (Arch's wrapper that changes into /opt/ventoy) or the script itself.
pub fn tool() -> Option<PathBuf> {
    crate::paths::find_executable("ventoy").or_else(|| Some(PathBuf::from("/opt/ventoy/Ventoy2Disk.sh")).filter(|p| p.exists()))
}

/// argv for a fresh install: forced (the stick may hold an old Ventoy), MBR
/// (boots on old PCs too) unless `gpt`, with secure boot support.
pub fn install_argv(pkexec: &Path, tool: &Path, device: &str, label: &str, gpt: bool) -> Vec<String> {
    let mut argv = vec![pkexec.display().to_string(), tool.display().to_string(), "-I".into()];
    if gpt {
        argv.push("-g".into());
    }
    let label = label.trim();
    if !label.is_empty() {
        argv.extend(["-L".into(), label.into()]);
    }
    argv.push(device.into());
    argv
}

/// Ventoy2Disk exits 0 even when it gave up; only this line means it worked.
pub fn succeeded(output: &str) -> bool {
    output.lines().any(|l| l.contains("Install Ventoy to") && l.contains("successfully finished"))
}

/// The last thing Ventoy complained about (its lines start with `[...]`).
pub fn complaint(output: &str) -> String {
    let clean = |l: &str| {
        // Strip colour codes.
        let mut out = String::new();
        let mut skip = false;
        for c in l.chars() {
            match (skip, c) {
                (_, '\u{1b}') => skip = true,
                (true, 'm') => skip = false,
                (true, _) => {}
                (false, c) => out.push(c),
            }
        }
        out.trim().to_owned()
    };
    output
        .lines()
        .rev()
        .map(clean)
        .find(|l| {
            !l.is_empty()
                && !l.contains("Continue?")
                && !l.starts_with('*')
                && !l.starts_with("Ventoy:")
                && !l.contains("longpanda")
                && !l.contains("www.ventoy.net")
        })
        .unwrap_or_default()
}

/// Runs the install; the stick must be released already. pkexec shows the
/// password dialog; the two "Continue? (y/n)" are answered here.
pub fn install(device: &str, label: &str) -> anyhow::Result<()> {
    let tool = tool().ok_or_else(|| anyhow::anyhow!("Ventoy is not installed (package ventoy-bin)"))?;
    let pkexec = crate::paths::find_executable("pkexec").ok_or_else(|| anyhow::anyhow!("pkexec is not installed"))?;
    let argv = install_argv(&pkexec, &tool, device, label, false);
    let mut child = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let _ = child.stdin.take().expect("piped").write_all(b"y\ny\n");
    let mut err = child.stderr.take().expect("piped");
    let errs = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let mut out = String::new();
    child.stdout.take().expect("piped").read_to_string(&mut out)?;
    let status = child.wait()?;
    let all = out + &errs.join().unwrap_or_default();
    // pkexec: 126 = the dialog was dismissed, 127 = not authorised.
    match status.code() {
        Some(126) | Some(127) => anyhow::bail!("NotAuthorized: {}", complaint(&all)),
        _ if succeeded(&all) => Ok(()),
        _ => anyhow::bail!("Ventoy: {}", complaint(&all)),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StickFile {
    pub file: String,
    /// Relative to the partition's root.
    pub path: String,
    pub size: u64,
    pub system: String,
    pub version: String,
    pub family: String,
}

/// The images on a mounted Ventoy partition (root and one folder deep, the
/// way people sort them), by name.
pub fn files(mount: &Path) -> Vec<StickFile> {
    let mut out = Vec::new();
    let scan = |dir: &Path, prefix: &str, out: &mut Vec<StickFile>, depth: u8| {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut dirs = Vec::new();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() && depth == 0 && !name.starts_with('.') && name != "ventoy" && name != "System Volume Information" {
                dirs.push((e.path(), format!("{prefix}{name}/")));
            } else if meta.is_file() && images::is_image_name(&name) {
                let d = images::describe(&name, None);
                out.push(StickFile {
                    path: format!("{prefix}{name}"),
                    file: name,
                    size: meta.len(),
                    system: d.system,
                    version: d.version,
                    family: d.family,
                });
            }
        }
        dirs
    };
    for (dir, prefix) in scan(mount, "", &mut out, 0) {
        scan(&dir, &prefix, &mut out, 1);
    }
    out.sort_by_key(|a| a.path.to_lowercase());
    out
}

/// Free bytes on the filesystem at `path`.
pub fn free_space(path: &Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return 0;
    };
    // SAFETY: statvfs fills the struct we hand it.
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) == 0 {
            s.f_bavail as u64 * s.f_frsize as u64
        } else {
            0
        }
    }
}

/// Copies `src` onto the stick at `mount` (as `.part` first, so a broken
/// copy never shows up as an image), flushing as it goes.
pub fn copy_to(src: &Path, mount: &Path, cancel: &AtomicBool, mut progress: impl FnMut(u64, u64)) -> anyhow::Result<PathBuf> {
    let name = src.file_name().ok_or_else(|| anyhow::anyhow!("no file name"))?;
    let target = mount.join(name);
    let part = mount.join(format!(".{}.part", name.to_string_lossy()));
    let total = std::fs::metadata(src)?.len();
    if total > free_space(mount) + std::fs::metadata(&target).map_or(0, |m| m.len()) {
        return Err(std::io::Error::new(std::io::ErrorKind::StorageFull, "the image is bigger than the free space on the stick").into());
    }
    let result = (|| -> anyhow::Result<()> {
        let mut from = std::fs::File::open(src)?;
        let mut to = std::fs::File::create(&part)?;
        let mut buf = vec![0u8; 4 << 20];
        let (mut done, mut synced) = (0u64, 0u64);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(super::write::cancelled().into());
            }
            let n = from.read(&mut buf)?;
            if n == 0 {
                break;
            }
            to.write_all(&buf[..n])?;
            done += n as u64;
            if done - synced >= 64 << 20 {
                to.sync_data()?;
                synced = done;
            }
            progress(done, total);
        }
        to.sync_all()?;
        std::fs::rename(&part, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vol(id: &str, label: &str, fs: &str) -> Volume {
        Volume {
            id: id.into(),
            device: id.replace("/b/", "/dev/"),
            label: label.into(),
            fs: fs.into(),
            size: 1,
            mount_points: vec![],
        }
    }

    fn drive(volumes: Vec<Volume>) -> Drive {
        Drive {
            id: "/b/sda".into(),
            device: "/dev/sda".into(),
            drive: "/d/x".into(),
            name: "x".into(),
            size: 1,
            kind: "usb".into(),
            read_only: false,
            can_power_off: true,
            ejectable: true,
            volumes,
            backing: String::new(),
        }
    }

    #[test]
    fn ventoy_sticks_are_recognised_by_their_efi_partition() {
        let v = drive(vec![vol("/b/sda1", "Ventoy", "exfat"), vol("/b/sda2", "VTOYEFI", "vfat")]);
        assert_eq!(data_volume(&v).map(|v| v.id.as_str()), Some("/b/sda1"));
        // Renamed data partition still counts.
        let v = drive(vec![vol("/b/sda1", "Meine ISOs", "exfat"), vol("/b/sda2", "VTOYEFI", "vfat")]);
        assert_eq!(data_volume(&v).map(|v| v.label.as_str()), Some("Meine ISOs"));
        assert!(data_volume(&drive(vec![vol("/b/sda1", "STICK", "vfat")])).is_none());
        assert!(data_volume(&drive(vec![vol("/b/sda", "PVE", "iso9660")])).is_none());
    }

    #[test]
    fn install_command_and_result() {
        let argv = install_argv(Path::new("/usr/bin/pkexec"), Path::new("/usr/bin/ventoy"), "/dev/sda", " Multiboot ", false);
        assert_eq!(argv, ["/usr/bin/pkexec", "/usr/bin/ventoy", "-I", "-L", "Multiboot", "/dev/sda"]);
        let argv = install_argv(Path::new("pkexec"), Path::new("v"), "/dev/sdb", "", true);
        assert_eq!(argv, ["pkexec", "v", "-I", "-g", "/dev/sdb"]);
        let ok = "\u{1b}[32m[INFO] Install Ventoy to /dev/sda successfully finished.\u{1b}[0m\n";
        assert!(succeeded(ok));
        let bad = "**********************************************\n      Ventoy: 1.1.18  x86_64\n\u{1b}[31m[ERR] /dev/sda is mounted, please umount it first!\u{1b}[0m\nContinue? (y/n) ";
        assert!(!succeeded(bad));
        assert_eq!(complaint(bad), "[ERR] /dev/sda is mounted, please umount it first!");
    }

    #[test]
    fn files_on_the_stick_and_copying_onto_it() {
        let tmp = tempfile::tempdir().unwrap();
        let stick = tmp.path().join("stick");
        std::fs::create_dir_all(stick.join("Linux")).unwrap();
        std::fs::create_dir_all(stick.join("ventoy")).unwrap();
        std::fs::write(stick.join("Linux/ubuntu-24.04-desktop-amd64.iso"), b"u").unwrap();
        std::fs::write(stick.join("ventoy/ventoy.json"), b"{}").unwrap();
        std::fs::write(stick.join("notes.txt"), b"n").unwrap();
        let src = tmp.path().join("archlinux-2026.10.01-x86_64.iso");
        std::fs::write(&src, vec![3u8; 5 << 20]).unwrap();
        let mut seen = 0;
        let target = copy_to(&src, &stick, &AtomicBool::new(false), |d, t| {
            assert_eq!(t, 5 << 20);
            seen = d;
        })
        .unwrap();
        assert_eq!((seen, target.clone()), (5 << 20, stick.join("archlinux-2026.10.01-x86_64.iso")));
        assert_eq!(std::fs::read(&target).unwrap(), std::fs::read(&src).unwrap());
        let listed: Vec<_> = files(&stick).into_iter().map(|f| (f.path, f.system)).collect();
        assert_eq!(
            listed,
            [
                ("archlinux-2026.10.01-x86_64.iso".to_owned(), "Arch Linux".to_owned()),
                ("Linux/ubuntu-24.04-desktop-amd64.iso".to_owned(), "Ubuntu".to_owned())
            ]
        );
        // Cancelled: nothing half-copied stays behind.
        let other = tmp.path().join("debian-13.iso");
        std::fs::write(&other, b"d").unwrap();
        assert!(copy_to(&other, &stick, &AtomicBool::new(true), |_, _| {}).is_err());
        assert!(!stick.join("debian-13.iso").exists() && !stick.join(".debian-13.iso.part").exists());
        assert!(free_space(&stick) > 0);
    }
}
