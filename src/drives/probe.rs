//! What system an image holds, read from its contents instead of its name:
//! marker files inside ISOs (`.disk/info`, `.treeinfo`, `sources/install.wim`
//! …, read with bsdtar), partition names and labels of disk images. The
//! result is a text of hints that `images::describe` matches like a name,
//! plus a version when the image states one.

use super::images::Compression;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Hints {
    /// Lower-case words that name the system ("ubuntu 24.04.1 lts", "windows").
    pub text: String,
    pub version: String,
}

fn bsdtar(args: &[&str], iso: &Path) -> Option<String> {
    let out = Command::new("bsdtar").args(args).arg(iso).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `key=value` from an INI-like text (`.treeinfo`, `.cd-info`).
fn ini(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim().eq_ignore_ascii_case(key)).then(|| v.trim().trim_matches(['"', '\'']).to_owned())
    })
}

fn first_version(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c == '"' || c == '-' || c == '_' || c == '(')
        .map(|t| t.trim_start_matches('v'))
        // 24.04.1, or a short release number (Mint 22) — not a build date.
        .find(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit() || c == '.') && (t.contains('.') || t.len() <= 3))
        .map(|t| t.trim_end_matches('.').to_owned())
        .unwrap_or_default()
}

/// Hints from an ISO's file list and a few small files in it.
pub fn iso_hints(entries: &[String], read: impl Fn(&str) -> Option<String>) -> Hints {
    let has = |p: &str| entries.iter().any(|e| e.trim_start_matches("./").trim_end_matches('/').eq_ignore_ascii_case(p));
    let has_prefix = |p: &str| entries.iter().any(|e| e.trim_start_matches("./").to_lowercase().starts_with(p));
    let mut h = Hints::default();
    let mut add = |s: &str| {
        h.text.push(' ');
        h.text.push_str(&s.to_lowercase());
    };
    // Debian, Ubuntu and everything built like them.
    if let Some(info) = has(".disk/info").then(|| read(".disk/info")).flatten() {
        add(&info);
        h.version = first_version(&info);
    }
    // Fedora, RHEL, Rocky, Alma, CentOS.
    if let Some(tree) = has(".treeinfo").then(|| read(".treeinfo")).flatten() {
        let name = ini(&tree, "name").or_else(|| ini(&tree, "family")).unwrap_or_default();
        add(&name);
        if h.version.is_empty() {
            h.version = ini(&tree, "version").unwrap_or_default();
        }
    }
    // Proxmox: .cd-info has ISONAME and RELEASE.
    if let Some(cd) = has(".cd-info").then(|| read(".cd-info")).flatten() {
        add(&ini(&cd, "ISONAME").unwrap_or_default());
        if h.version.is_empty() {
            h.version = ini(&cd, "RELEASE").unwrap_or_default();
        }
    }
    if has("sources/install.wim") || has("sources/install.esd") || has("sources/boot.wim") || has("bootmgr") {
        add("windows");
    }
    if has("arch/version") {
        add("archlinux");
        if h.version.is_empty() {
            h.version = read("arch/version").map(|v| v.trim().to_owned()).unwrap_or_default();
        }
    }
    if has("nix-store.squashfs") || has_prefix("nix-store") {
        add("nixos");
        if h.version.is_empty() {
            h.version = read("version.txt").map(|v| v.trim().to_owned()).unwrap_or_default();
        }
    }
    if has("media.1/products") {
        add(&read("media.1/products").unwrap_or_default());
        add("opensuse");
    }
    if has("gparted-live-version") {
        add("gparted");
        h.version = read("GParted-Live-Version").map(|v| first_version(&v)).unwrap_or_default();
    }
    if has("clonezilla-live-version") {
        add("clonezilla");
        h.version = read("Clonezilla-Live-Version").map(|v| first_version(&v)).unwrap_or_default();
    }
    if has_prefix("truenas") || has_prefix("truenas-scale") {
        add("truenas");
    }
    if has_prefix("efi/boot/memtest") || has_prefix("boot/memtest") || has("memtest.bin") {
        add("memtest");
    }
    if has("ventoy") || has_prefix("ventoy/") {
        add("ventoy");
    }
    // FreeBSD's kernel; OPNsense and pfSense carry it too but name themselves.
    if has("boot/kernel/kernel") && !h.text.contains("opnsense") && !h.text.contains("pfsense") {
        h.text.push_str(" freebsd");
    }
    h
}

/// GPT partition names (UTF-16LE at LBA 2) of a disk image's start.
pub fn gpt_names(disk: &[u8]) -> Vec<String> {
    if disk.len() < 1024 || &disk[512..520] != b"EFI PART" {
        return Vec::new();
    }
    let at = |o: usize, n: usize| disk.get(o..o + n);
    let lba = |o: usize| at(512 + o, 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()) as usize);
    let count = at(512 + 80, 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()) as usize).min(128);
    let size = at(512 + 84, 4).map_or(128, |b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let start = lba(72).unwrap_or(2) * 512;
    (0..count)
        .filter_map(|i| {
            let e = at(start + i * size, size.max(128))?;
            let units: Vec<u16> = e[56..128].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
            let name = String::from_utf16_lossy(&units);
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

/// Labels of FAT filesystems in the MBR partitions of a disk image's start.
pub fn mbr_fat_labels(disk: &[u8]) -> Vec<String> {
    if disk.len() < 512 || disk[510..512] != [0x55, 0xAA] {
        return Vec::new();
    }
    (0..4)
        .filter_map(|i| {
            let e = &disk[446 + i * 16..446 + (i + 1) * 16];
            let lba = u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize;
            let boot = disk.get(lba * 512..lba * 512 + 512)?;
            if lba == 0 || boot[510..512] != [0x55, 0xAA] {
                return None;
            }
            // FAT32 keeps the label at 0x47, FAT12/16 at 0x2B; the type string follows it.
            let label = if &boot[0x52..0x57] == b"FAT32" {
                &boot[0x47..0x52]
            } else if &boot[0x36..0x39] == b"FAT" {
                &boot[0x2B..0x36]
            } else {
                return None;
            };
            let l = String::from_utf8_lossy(label).trim().to_owned();
            (!l.is_empty() && l != "NO NAME").then_some(l)
        })
        .collect()
}

/// Hints from the first bytes of a disk image.
pub fn disk_hints(start: &[u8]) -> Hints {
    let names: Vec<String> = gpt_names(start).into_iter().chain(mbr_fat_labels(start)).collect();
    let mut text = names.join(" ").to_lowercase();
    // Raspberry Pi OS calls its boot partition bootfs (before: boot) and has no other name.
    if names.iter().any(|n| n == "bootfs") {
        text.push_str(" raspios");
    }
    if text.contains("hassos") {
        text.push_str(" haos");
    }
    Hints { text, version: String::new() }
}

/// The first `n` bytes of the image as written (unpacked if needed).
fn head(path: &Path, compression: Option<Compression>, n: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(n);
    match compression {
        None => {
            let _ = std::fs::File::open(path).and_then(|f| f.take(n as u64).read_to_end(&mut buf));
        }
        Some(c) => {
            if let Ok(mut child) = Command::new(c.tool())
                .args(["-dc", "--"])
                .arg(path)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(out) = child.stdout.take() {
                    let _ = out.take(n as u64).read_to_end(&mut buf);
                }
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    buf
}

/// Everything the image says about itself.
pub fn probe(path: &Path, compression: Option<Compression>, is_iso: bool) -> Hints {
    if is_iso {
        let Some(list) = bsdtar(&["-tf"], path) else { return Hints::default() };
        let entries: Vec<String> = list.lines().map(str::to_owned).collect();
        // Members as listed: with or without a leading "./".
        let dotted = entries.iter().any(|e| e.starts_with("./"));
        return iso_hints(&entries, |member| {
            let name = if dotted { format!("./{member}") } else { member.to_owned() };
            let out = Command::new("bsdtar").arg("-xOf").arg(path).arg(&name).stderr(Stdio::null()).output().ok()?;
            (out.status.success() && !out.stdout.is_empty()).then(|| String::from_utf8_lossy(&out.stdout).chars().take(4096).collect())
        });
    }
    disk_hints(&head(path, compression, 4 << 20))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_from(files: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |p| files.iter().find(|(n, _)| n.eq_ignore_ascii_case(p)).map(|(_, t)| (*t).to_owned())
    }

    fn list(files: &[(&str, &str)], extra: &[&str]) -> Vec<String> {
        files
            .iter()
            .map(|(n, _)| format!("./{n}"))
            .chain(extra.iter().map(|e| format!("./{e}")))
            .collect()
    }

    #[test]
    fn isos_say_what_they_are() {
        const UBUNTU: &[(&str, &str)] = &[(".disk/info", "Ubuntu 24.04.1 LTS \"Noble Numbat\" - Release amd64 (20240827)")];
        let h = iso_hints(&list(UBUNTU, &["casper/vmlinuz"]), read_from(UBUNTU));
        assert!(h.text.contains("ubuntu") && h.version == "24.04.1", "{h:?}");

        const FEDORA: &[(&str, &str)] = &[(
            ".treeinfo",
            "[general]\nfamily = Fedora\nversion = 41\n[release]\nname = Fedora\nversion = 41\n",
        )];
        let h = iso_hints(&list(FEDORA, &[]), read_from(FEDORA));
        assert_eq!((h.text.trim(), h.version.as_str()), ("fedora", "41"));

        const PVE: &[(&str, &str)] = &[(".cd-info", "PRODUCT='pve'\nISONAME='proxmox-ve'\nRELEASE='9.0'\n")];
        let h = iso_hints(&list(PVE, &["proxmox/packages"]), read_from(PVE));
        assert!(h.text.contains("proxmox-ve") && h.version == "9.0", "{h:?}");

        const ARCH: &[(&str, &str)] = &[("arch/version", "2026.10.01\n")];
        let h = iso_hints(&list(ARCH, &["arch/boot/x86_64/vmlinuz-linux"]), read_from(ARCH));
        assert_eq!((h.text.trim(), h.version.as_str()), ("archlinux", "2026.10.01"));

        let h = iso_hints(&list(&[], &["sources/install.wim", "bootmgr", "setup.exe"]), read_from(&[]));
        assert!(h.text.contains("windows"));
        assert_eq!(iso_hints(&list(&[], &["readme.txt"]), read_from(&[])), Hints::default());
    }

    fn gpt_disk(names: &[&str]) -> Vec<u8> {
        let mut d = vec![0u8; 512 * 40];
        d[510] = 0x55;
        d[511] = 0xAA;
        d[512..520].copy_from_slice(b"EFI PART");
        d[512 + 72..512 + 80].copy_from_slice(&2u64.to_le_bytes());
        d[512 + 80..512 + 84].copy_from_slice(&(names.len() as u32).to_le_bytes());
        d[512 + 84..512 + 88].copy_from_slice(&128u32.to_le_bytes());
        for (i, n) in names.iter().enumerate() {
            let at = 1024 + i * 128 + 56;
            for (k, u) in n.encode_utf16().enumerate() {
                d[at + 2 * k..at + 2 * k + 2].copy_from_slice(&u.to_le_bytes());
            }
        }
        d
    }

    #[test]
    fn disk_images_by_their_partitions() {
        let h = disk_hints(&gpt_disk(&["hassos-boot", "hassos-kernel0", "hassos-system0"]));
        assert!(h.text.contains("haos"));
        // A Raspberry Pi OS image: MBR, FAT32 boot partition "bootfs" at sector 8.
        let mut d = vec![0u8; 512 * 16];
        d[510] = 0x55;
        d[511] = 0xAA;
        d[446 + 8..446 + 12].copy_from_slice(&8u32.to_le_bytes());
        let b = 8 * 512;
        d[b + 0x47..b + 0x52].copy_from_slice(b"bootfs     ");
        d[b + 0x52..b + 0x57].copy_from_slice(b"FAT32");
        d[b + 510] = 0x55;
        d[b + 511] = 0xAA;
        assert_eq!(mbr_fat_labels(&d), ["bootfs"]);
        assert!(disk_hints(&d).text.contains("raspios"));
        assert!(disk_hints(&[0u8; 2048]).text.is_empty());
    }

    #[test]
    fn real_iso_read_with_bsdtar() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        std::fs::create_dir_all(root.join(".disk")).unwrap();
        std::fs::write(root.join(".disk/info"), "Linux Mint 22 \"Wilma\" - Release amd64 20240724").unwrap();
        let iso = tmp.path().join("download (3).iso");
        let ok = Command::new("bsdtar")
            .args(["--format", "iso9660", "--options", "iso9660:volume-id=MINT", "-cf"])
            .arg(&iso)
            .arg("-C")
            .arg(&root)
            .arg(".")
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            return; // no bsdtar here
        }
        let h = probe(&iso, None, true);
        assert!(h.text.contains("linux mint") && h.version == "22", "{h:?}");
        // Named by the file "download (3).iso", known by its contents.
        let img = super::super::images::inspect(&iso, false).unwrap();
        assert_eq!(
            (
                img.description.system.as_str(),
                img.description.version.as_str(),
                img.description.family.as_str()
            ),
            ("Linux Mint", "22", "mint")
        );
    }
}
