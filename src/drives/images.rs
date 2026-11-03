//! The image folder: which files are images, what system they hold (from
//! the file name, else the ISO's volume label), how they are compressed and
//! how big they are once written.

use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    Xz,
    Gzip,
    Zstd,
    Bzip2,
}

impl Compression {
    /// Magic bytes at the start of the file.
    pub fn sniff(head: &[u8]) -> Option<Compression> {
        if head.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0]) {
            Some(Compression::Xz)
        } else if head.starts_with(&[0x1F, 0x8B]) {
            Some(Compression::Gzip)
        } else if head.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
            Some(Compression::Zstd)
        } else if head.starts_with(b"BZh") {
            Some(Compression::Bzip2)
        } else {
            None
        }
    }

    /// The program that unpacks it to stdout (`<tool> -dc`).
    pub fn tool(self) -> &'static str {
        match self {
            Compression::Xz => "xz",
            Compression::Gzip => "gzip",
            Compression::Zstd => "zstd",
            Compression::Bzip2 => "bzip2",
        }
    }
}

const RAW: [&str; 4] = ["iso", "img", "raw", "bin"];
const PACKED: [&str; 5] = ["xz", "gz", "zst", "bz2", "zstd"];

/// `x.iso`, `x.img`, `x.img.xz`, `x.raw.zst` … (not `notes.txt.gz`).
pub fn is_image_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    let mut parts = lower.rsplit('.');
    let (Some(last), Some(prev)) = (parts.next(), parts.next()) else {
        return false;
    };
    RAW.contains(&last) || PACKED.contains(&last) && RAW.contains(&prev)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Description {
    /// "Arch Linux", else the file name without extension.
    pub system: String,
    pub version: String,
    /// x86_64, arm64 …, "" if not named.
    pub arch: String,
    /// Key for the monogram's colour (arch, ubuntu …), "" = unknown.
    pub family: String,
    /// Windows images don't boot when copied byte for byte.
    pub windows: bool,
}

/// (needle in the lowercased name, system, family). First match wins, so
/// more specific names come first.
const SYSTEMS: &[(&str, &str, &str)] = &[
    // Built on Debian/Ubuntu but naming themselves: before their base.
    ("proxmox-ve", "Proxmox VE", "proxmox"),
    ("proxmox-backup", "Proxmox Backup Server", "proxmox"),
    ("proxmox", "Proxmox", "proxmox"),
    ("truenas", "TrueNAS", "truenas"),
    ("cachyos", "CachyOS", "cachyos"),
    ("endeavouros", "EndeavourOS", "endeavouros"),
    ("manjaro", "Manjaro", "manjaro"),
    ("garuda", "Garuda Linux", "garuda"),
    ("steamos", "SteamOS", "steamos"),
    ("bazzite", "Bazzite", "bazzite"),
    ("nobara", "Nobara", "fedora"),
    ("archlinux", "Arch Linux", "arch"),
    ("arch-linux", "Arch Linux", "arch"),
    ("kubuntu", "Kubuntu", "kubuntu"),
    ("xubuntu", "Xubuntu", "ubuntu"),
    ("lubuntu", "Lubuntu", "ubuntu"),
    ("ubuntu", "Ubuntu", "ubuntu"),
    ("pop-os", "Pop!_OS", "pop"),
    ("pop_os", "Pop!_OS", "pop"),
    ("linuxmint", "Linux Mint", "mint"),
    ("elementaryos", "elementary OS", "elementary"),
    ("zorin", "Zorin OS", "zorin"),
    ("kali", "Kali Linux", "kali"),
    ("tails", "Tails", "tails"),
    ("debian", "Debian", "debian"),
    ("raspios", "Raspberry Pi OS", "raspberry"),
    ("raspberry", "Raspberry Pi OS", "raspberry"),
    ("fedora", "Fedora", "fedora"),
    ("rocky", "Rocky Linux", "rocky"),
    ("almalinux", "AlmaLinux", "alma"),
    ("centos", "CentOS", "centos"),
    ("opensuse", "openSUSE", "suse"),
    ("nixos", "NixOS", "nixos"),
    ("alpine", "Alpine Linux", "alpine"),
    ("void", "Void Linux", "void"),
    ("gentoo", "Gentoo", "gentoo"),
    ("unraid", "Unraid", "unraid"),
    ("haos", "Home Assistant OS", "homeassistant"),
    ("home-assistant", "Home Assistant OS", "homeassistant"),
    ("libreelec", "LibreELEC", "libreelec"),
    ("batocera", "Batocera", "batocera"),
    ("retropie", "RetroPie", "retropie"),
    ("freebsd", "FreeBSD", "freebsd"),
    ("opnsense", "OPNsense", "opnsense"),
    ("pfsense", "pfSense", "pfsense"),
    ("gparted", "GParted Live", "gparted"),
    ("clonezilla", "Clonezilla", "clonezilla"),
    ("memtest", "Memtest86+", "memtest"),
    ("systemrescue", "SystemRescue", "systemrescue"),
    ("ventoy", "Ventoy", "ventoy"),
    ("win11", "Windows 11", "windows"),
    ("win10", "Windows 10", "windows"),
    ("windows", "Windows", "windows"),
    ("red-hat-enterprise", "Red Hat Enterprise Linux", "rhel"),
    ("rhel", "Red Hat Enterprise Linux", "rhel"),
];

const ARCHES: &[(&str, &str)] = &[
    ("x86_64", "x86_64"),
    ("amd64", "x86_64"),
    ("x64", "x86_64"),
    ("aarch64", "arm64"),
    ("arm64", "arm64"),
    ("armhf", "armhf"),
    ("i686", "x86"),
    ("i386", "x86"),
    ("riscv64", "riscv64"),
];

/// Splits a name into tokens at `-`, `_`, `.`, spaces — but keeps dotted
/// numbers (`24.04.1`) and `x86_64` whole.
fn tokens(stem: &str) -> Vec<String> {
    let lower = stem.to_lowercase().replace("x86_64", "x86\u{1}64");
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = lower.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let dotted_number = c == '.' && i > 0 && chars[i - 1].is_ascii_digit() && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if (c == '-' || c == '_' || c == '.' || c == ' ' || c == '+') && !dotted_number {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur).replace('\u{1}', "_"));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur.replace('\u{1}', "_"));
    }
    out
}

/// The name without `.iso`, `.img.xz` …
pub fn stem(name: &str) -> &str {
    let mut s = name;
    for _ in 0..2 {
        if let Some((head, ext)) = s.rsplit_once('.')
            && (RAW.contains(&ext.to_lowercase().as_str()) || PACKED.contains(&ext.to_lowercase().as_str()))
        {
            s = head;
        }
    }
    s
}

fn is_version(t: &str) -> bool {
    let t = t.strip_prefix('v').unwrap_or(t);
    !t.is_empty() && t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.chars().all(|c| c.is_ascii_digit() || c == '.')
}

pub fn describe(file_name: &str, volume_label: Option<&str>) -> Description {
    describe_with(file_name, volume_label, &super::probe::Hints::default())
}

/// What the contents say (`hints`) wins over the volume label, which wins
/// over the file name.
pub fn describe_with(file_name: &str, volume_label: Option<&str>, hints: &super::probe::Hints) -> Description {
    let stem = stem(file_name);
    let lower = stem.to_lowercase();
    let label = volume_label.unwrap_or("").to_lowercase();
    let content = hints
        .text
        .replace(['_', ' '], "-")
        .replace("linux-mint", "linuxmint")
        .replace("pop!-os", "pop-os")
        .replace("raspberry-pi", "raspios");
    let by_content = SYSTEMS.iter().find(|(n, ..)| !content.trim_matches('-').is_empty() && content.contains(n));
    let found = by_content
        .or_else(|| SYSTEMS.iter().find(|(n, ..)| !label.is_empty() && label.replace('_', "-").contains(n)))
        .or_else(|| SYSTEMS.iter().find(|(n, ..)| lower.contains(n)))
        // Arch's ISO label is ARCH_202610.
        .or_else(|| label.starts_with("arch_").then_some(&("archlinux", "Arch Linux", "arch")));
    let toks = tokens(stem);
    let arch = toks
        .iter()
        .find_map(|t| ARCHES.iter().find(|(n, _)| t == n).map(|(_, a)| (*a).to_owned()))
        .unwrap_or_default();
    // Dated releases (Raspberry Pi OS: 2026-10-22-raspios-…) keep the date whole.
    let date = toks
        .windows(3)
        .find(|w| w[0].len() == 4 && w[1].len() == 2 && w[2].len() == 2 && w.iter().all(|t| t.chars().all(|c| c.is_ascii_digit())));
    // A number in the name only counts when the name names the system too
    // ("download-2.iso" holding Windows is not Windows 2).
    let name_agrees = found.is_none_or(|(n, ..)| lower.contains(n));
    let mut version = match date {
        _ if !name_agrees => String::new(),
        Some(w) => w.join("-"),
        None => toks
            .iter()
            .find(|t| is_version(t))
            .map(|t| t.trim_start_matches('v').to_owned())
            .unwrap_or_default(),
    };
    if !hints.version.is_empty() {
        version = hints.version.clone();
    }
    // ARCH_202610, Fedora-WS-Live-44.
    if version.is_empty()
        && let Some(digits) = tokens(&label).into_iter().find(|t| is_version(t))
    {
        version = digits;
    }
    match found {
        Some((_, system, family)) => Description {
            system: (*system).to_owned(),
            version,
            arch,
            family: (*family).to_owned(),
            windows: *family == "windows",
        },
        None => Description {
            system: stem.to_owned(),
            version: String::new(),
            arch,
            family: String::new(),
            windows: false,
        },
    }
}

/// The volume label of an ISO 9660 image ("" if it is none).
pub fn iso_label(file: &mut (impl Read + Seek)) -> Option<String> {
    let mut pvd = [0u8; 72];
    file.seek(SeekFrom::Start(0x8000)).ok()?;
    file.read_exact(&mut pvd).ok()?;
    if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
        return None;
    }
    let label = String::from_utf8_lossy(&pvd[40..72]).trim().to_owned();
    (!label.is_empty()).then_some(label)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Image {
    pub path: PathBuf,
    pub file: String,
    /// Size of the file.
    pub size: u64,
    /// Bytes written to the stick; for packed images only if the format says
    /// (xz and zstd do, gzip only below 4 GiB), else 0.
    pub written: u64,
    pub compression: Option<Compression>,
    pub label: String,
    pub modified: u64,
    /// Not in the image folder (opened from Dolphin).
    pub extra: bool,
    #[serde(flatten)]
    pub description: Description,
}

/// Unpacked size from `xz --robot --list` output.
pub fn xz_size(robot: &str) -> Option<u64> {
    robot
        .lines()
        .find(|l| l.starts_with("totals"))
        .and_then(|l| l.split('\t').nth(4))
        .and_then(|v| v.parse().ok())
}

/// Unpacked size from `zstd -lv` output ("Decompressed Size: 2.00 GiB (2147483648 B)").
pub fn zstd_size(verbose: &str) -> Option<u64> {
    let line = verbose.lines().find(|l| l.trim_start().starts_with("Decompressed Size"))?;
    let inner = line.rsplit_once('(')?.1;
    inner.trim_end_matches(')').trim().trim_end_matches('B').trim().parse().ok()
}

fn unpacked_size(path: &Path, compression: Compression, size: u64) -> u64 {
    let run = |args: &[&str]| {
        std::process::Command::new(compression.tool())
            .args(args)
            .arg(path)
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    };
    match compression {
        Compression::Xz => run(&["--robot", "--list", "--"]).as_deref().and_then(xz_size).unwrap_or(0),
        Compression::Zstd => run(&["-lv", "--"]).as_deref().and_then(zstd_size).unwrap_or(0),
        // ISIZE: the size mod 4 GiB, so only believable while it is above the packed size.
        Compression::Gzip => {
            let mut tail = [0u8; 4];
            std::fs::File::open(path)
                .and_then(|mut f| {
                    f.seek(SeekFrom::End(-4))?;
                    f.read_exact(&mut tail)
                })
                .ok()
                .map(|_| u32::from_le_bytes(tail) as u64)
                .filter(|&n| n >= size)
                .unwrap_or(0)
        }
        Compression::Bzip2 => 0,
    }
}

/// Reads what the image is. `extra`: not from the image folder.
pub fn inspect(path: &Path, extra: bool) -> Option<Image> {
    let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    let file = path.file_name()?.to_string_lossy().into_owned();
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 8];
    let n = f.read(&mut head).unwrap_or(0);
    let compression = Compression::sniff(&head[..n]);
    let label = if compression.is_none() {
        iso_label(&mut f).unwrap_or_default()
    } else {
        String::new()
    };
    let modified_at = meta.modified().ok();
    let hints = cached_hints(path, meta.len(), modified_at, || super::probe::probe(path, compression, !label.is_empty()));
    let written = match compression {
        None => meta.len(),
        Some(c) => unpacked_size(path, c, meta.len()),
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    Some(Image {
        path: path.to_owned(),
        description: describe_with(&file, (!label.is_empty()).then_some(label.as_str()), &hints),
        file,
        size: meta.len(),
        written,
        compression,
        label,
        modified,
        extra,
    })
}

/// Probing reads into the image; once per file version is enough.
fn cached_hints(path: &Path, size: u64, modified: Option<std::time::SystemTime>, probe: impl FnOnce() -> super::probe::Hints) -> super::probe::Hints {
    type Key = (PathBuf, u64, Option<std::time::SystemTime>);
    static CACHE: std::sync::Mutex<Option<std::collections::HashMap<Key, super::probe::Hints>>> = std::sync::Mutex::new(None);
    let key = (path.to_owned(), size, modified);
    if let Some(h) = CACHE.lock().ok().and_then(|c| c.as_ref().and_then(|m| m.get(&key).cloned())) {
        return h;
    }
    let h = probe();
    if let Ok(mut c) = CACHE.lock() {
        c.get_or_insert_with(Default::default).insert(key, h.clone());
    }
    h
}

/// The images in `dir` (not recursive), newest first.
pub fn list(dir: &Path) -> Vec<Image> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Image> = entries
        .flatten()
        .filter(|e| is_image_name(&e.file_name().to_string_lossy()))
        .filter_map(|e| inspect(&e.path(), false))
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.file.cmp(&b.file)));
    out
}

/// What changes the listing: names, sizes and times.
pub fn fingerprint(dir: &Path) -> Vec<(String, u64, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<(String, u64, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            Some((e.file_name().to_string_lossy().into_owned(), m.len(), t))
        })
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_names() {
        for yes in [
            "archlinux-2026.10.01-x86_64.iso",
            "x.IMG",
            "pi.img.xz",
            "a.raw.zst",
            "b.iso.gz",
            "c.img.bz2",
            "fw.bin",
        ] {
            assert!(is_image_name(yes), "{yes}");
        }
        for no in ["notes.txt", "backup.tar.gz", "photo.jpg", "iso", "x.xz"] {
            assert!(!is_image_name(no), "{no}");
        }
        assert_eq!(stem("2026-10-22-raspios-bookworm-arm64.img.xz"), "2026-10-22-raspios-bookworm-arm64");
        assert_eq!(stem("ubuntu-24.04.1-desktop-amd64.iso"), "ubuntu-24.04.1-desktop-amd64");
    }

    fn d(name: &str) -> (String, String, String, String) {
        let d = describe(name, None);
        (d.system, d.version, d.arch, d.family)
    }

    #[test]
    fn systems_from_file_names() {
        let s = |a: &str, b: &str, c: &str, e: &str| (a.to_owned(), b.to_owned(), c.to_owned(), e.to_owned());
        assert_eq!(d("archlinux-2026.10.01-x86_64.iso"), s("Arch Linux", "2026.10.01", "x86_64", "arch"));
        assert_eq!(d("ubuntu-24.04.1-desktop-amd64.iso"), s("Ubuntu", "24.04.1", "x86_64", "ubuntu"));
        assert_eq!(d("kubuntu-24.10-desktop-amd64.iso"), s("Kubuntu", "24.10", "x86_64", "kubuntu"));
        assert_eq!(d("proxmox-ve_9.0-1.iso"), s("Proxmox VE", "9.0", "", "proxmox"));
        assert_eq!(d("cachyos-desktop-linux-260915.iso"), s("CachyOS", "260915", "", "cachyos"));
        assert_eq!(d("2026-10-22-raspios-bookworm-arm64.img.xz").0, "Raspberry Pi OS");
        assert_eq!(d("2026-10-22-raspios-bookworm-arm64.img.xz").2, "arm64");
        assert_eq!(d("2026-10-22-raspios-bookworm-arm64.img.xz").1, "2026-10-22");
        assert_eq!(d("Fedora-Workstation-Live-x86_64-41-1.4.iso"), s("Fedora", "41", "x86_64", "fedora"));
        assert_eq!(d("haos_rpi5-64-14.2.img.xz").0, "Home Assistant OS");
        assert_eq!(d("my-own-thing.img"), s("my-own-thing", "", "", ""));
        let w = describe("Win11_24H2_German_x64.iso", None);
        assert!(w.windows && w.system == "Windows 11" && w.arch == "x86_64");
    }

    #[test]
    fn systems_from_iso_labels() {
        let a = describe("download.iso", Some("ARCH_202610"));
        assert_eq!((a.system.as_str(), a.version.as_str()), ("Arch Linux", "202610"));
        assert_eq!(describe("x.iso", Some("PVE")).system, "x");
        assert_eq!(describe("x.iso", Some("Ubuntu 24.04.1 LTS amd64")).system, "Ubuntu");
        // Proxmox's ISO (as on a real stick): Debian inside, Proxmox named.
        let h = super::super::probe::Hints {
            text: " debian proxmox-ve".into(),
            version: "9.2".into(),
        };
        let p = describe_with("download.iso", Some("PVE"), &h);
        assert_eq!((p.system.as_str(), p.version.as_str()), ("Proxmox VE", "9.2"));
        let w = describe_with(
            "download-2.iso",
            None,
            &super::super::probe::Hints {
                text: " windows".into(),
                version: String::new(),
            },
        );
        assert_eq!((w.system.as_str(), w.version.as_str()), ("Windows", ""));
        // Known by its label only (Fedora Live has no .treeinfo): the label's number, not the name's.
        let f = describe("download-4.iso", Some("Fedora-WS-Live-44"));
        assert_eq!((f.system.as_str(), f.version.as_str()), ("Fedora", "44"));
        assert_eq!(describe("Fedora-Workstation-Live-44-1.7.x86_64.iso", Some("Fedora-WS-Live-44")).version, "44");
    }

    #[test]
    fn iso_label_is_read_from_the_primary_volume_descriptor() {
        let mut iso = vec![0u8; 0x8000 + 2048];
        iso[0x8000] = 1;
        iso[0x8001..0x8006].copy_from_slice(b"CD001");
        let label = b"ARCH_202610";
        iso[0x8028..0x8028 + 32].fill(b' ');
        iso[0x8028..0x8028 + label.len()].copy_from_slice(label);
        assert_eq!(iso_label(&mut std::io::Cursor::new(&iso)).as_deref(), Some("ARCH_202610"));
        assert_eq!(iso_label(&mut std::io::Cursor::new(vec![0u8; 0x9000])), None);
        assert_eq!(iso_label(&mut std::io::Cursor::new(vec![0u8; 10])), None);
    }

    #[test]
    fn compression_sniffing_and_sizes() {
        assert_eq!(Compression::sniff(&[0xFD, b'7', b'z', b'X', b'Z', 0, 0]), Some(Compression::Xz));
        assert_eq!(Compression::sniff(&[0x1F, 0x8B, 8]), Some(Compression::Gzip));
        assert_eq!(Compression::sniff(&[0x28, 0xB5, 0x2F, 0xFD]), Some(Compression::Zstd));
        assert_eq!(Compression::sniff(b"BZh91AY"), Some(Compression::Bzip2));
        assert_eq!(Compression::sniff(b"\x33\xc0"), None);
        let robot = "name\tx.img.xz\nfile\t1\t1\t1000\t5000\t0.200\tCRC64\t\ntotals\t1\t1\t1000\t5000\t0.200\tCRC64\t0\t1\n";
        assert_eq!(xz_size(robot), Some(5000));
        let zstd =
            "*** Zstandard CLI ***\nx.img.zst\n# Zstandard Frames: 1\nCompressed Size: 1.00 MiB (1048576 B)\nDecompressed Size: 2.00 GiB (2147483648 B)\n";
        assert_eq!(zstd_size(zstd), Some(2147483648));
    }

    #[test]
    fn listing_reads_real_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("readme.txt"), "x").unwrap();
        std::fs::write(tmp.path().join("debian-13.1.0-amd64-netinst.iso"), vec![0u8; 4096]).unwrap();
        let raw = vec![7u8; 300_000];
        let packed = tmp.path().join("pi.img.xz");
        let ok = std::process::Command::new("xz")
            .arg("-zc")
            .stdin(std::process::Stdio::piped())
            .stdout(std::fs::File::create(&packed).unwrap())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                c.stdin.take().unwrap().write_all(&raw)?;
                c.wait()
            });
        let images = list(tmp.path());
        let debian = images.iter().find(|i| i.file.starts_with("debian")).unwrap();
        assert_eq!((debian.description.system.as_str(), debian.written, debian.compression), ("Debian", 4096, None));
        if ok.is_ok_and(|s| s.success()) {
            let pi = images.iter().find(|i| i.file == "pi.img.xz").unwrap();
            assert_eq!((pi.compression, pi.written), (Some(Compression::Xz), 300_000));
        }
        assert!(images.iter().all(|i| i.file != "readme.txt"));
    }
}
