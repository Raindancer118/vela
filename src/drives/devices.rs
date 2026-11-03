//! Which drives may be formatted or written: the removable ones from
//! udisks2's objects (USB sticks, SD cards), never a disk that holds the
//! running system. Works on the objects as JSON (see `udisks::objects`), so
//! the rules are tested without a bus.

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const BLOCK: &str = "org.freedesktop.UDisks2.Block";
pub const PARTITION: &str = "org.freedesktop.UDisks2.Partition";
pub const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
pub const LOOP: &str = "org.freedesktop.UDisks2.Loop";
pub const SWAP: &str = "org.freedesktop.UDisks2.Swapspace";
pub const DRIVE: &str = "org.freedesktop.UDisks2.Drive";

/// object path → interface → property → value.
pub type Objects = BTreeMap<String, BTreeMap<String, Map<String, Value>>>;

/// Mount points that mean "this disk runs the system".
const SYSTEM_MOUNTS: [&str; 8] = ["/", "/boot", "/boot/efi", "/efi", "/home", "/usr", "/var", "/nix"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Drive {
    /// udisks object of the whole disk's block device.
    pub id: String,
    /// `/dev/sda`.
    pub device: String,
    /// udisks drive object ("" for loop devices).
    pub drive: String,
    /// "Vendor Model", may be empty.
    pub name: String,
    pub size: u64,
    /// usb, sd or loop.
    pub kind: String,
    pub read_only: bool,
    pub can_power_off: bool,
    pub ejectable: bool,
    pub volumes: Vec<Volume>,
    /// Loop devices: the file behind them.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub backing: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Volume {
    pub id: String,
    pub device: String,
    pub label: String,
    /// vfat, exfat, ntfs, ext4, iso9660, crypto_LUKS … ("" = unknown).
    pub fs: String,
    pub size: u64,
    pub mount_points: Vec<String>,
}

/// `ay` as udisks sends paths: bytes with a trailing NUL.
pub fn bytes_str(v: Option<&Value>) -> String {
    match v {
        Some(Value::Array(a)) => {
            let bytes: Vec<u8> = a.iter().filter_map(Value::as_u64).map(|b| b as u8).take_while(|b| *b != 0).collect();
            String::from_utf8_lossy(&bytes).into_owned()
        }
        Some(Value::String(s)) => s.trim_end_matches('\0').to_owned(),
        _ => String::new(),
    }
}

fn str_of(props: Option<&Map<String, Value>>, key: &str) -> String {
    props.and_then(|p| p.get(key)).and_then(Value::as_str).unwrap_or("").trim().to_owned()
}

fn bool_of(props: Option<&Map<String, Value>>, key: &str) -> bool {
    props.and_then(|p| p.get(key)).and_then(Value::as_bool).unwrap_or(false)
}

fn u64_of(props: Option<&Map<String, Value>>, key: &str) -> u64 {
    props.and_then(|p| p.get(key)).and_then(Value::as_u64).unwrap_or(0)
}

fn mount_points(ifaces: &BTreeMap<String, Map<String, Value>>) -> Vec<String> {
    match ifaces.get(FILESYSTEM).and_then(|f| f.get("MountPoints")) {
        Some(Value::Array(points)) => points.iter().map(|p| bytes_str(Some(p))).filter(|p| !p.is_empty()).collect(),
        _ => Vec::new(),
    }
}

fn volume(path: &str, ifaces: &BTreeMap<String, Map<String, Value>>) -> Volume {
    let block = ifaces.get(BLOCK);
    Volume {
        id: path.to_owned(),
        device: bytes_str(block.and_then(|b| b.get("Device"))),
        label: str_of(block, "IdLabel"),
        fs: str_of(block, "IdType"),
        size: u64_of(block, "Size"),
        mount_points: mount_points(ifaces),
    }
}

/// The removable drives that can be written, sorted by device. Loop devices
/// only with `allow_loop` (tests).
pub fn removable(objects: &Objects, allow_loop: bool) -> Vec<Drive> {
    let mut out: Vec<Drive> = objects
        .iter()
        .filter_map(|(path, ifaces)| {
            let block = ifaces.get(BLOCK)?;
            if ifaces.contains_key(PARTITION) || bool_of(Some(block), "HintIgnore") {
                return None;
            }
            let size = u64_of(Some(block), "Size");
            if size == 0 {
                return None;
            }
            let drive_path = block.get("Drive").and_then(Value::as_str).filter(|d| *d != "/").unwrap_or("");
            let drive = objects.get(drive_path).and_then(|d| d.get(DRIVE));
            let kind = if ifaces.contains_key(LOOP) {
                if !allow_loop || bytes_str(ifaces.get(LOOP).and_then(|l| l.get("BackingFile"))).is_empty() {
                    return None;
                }
                "loop"
            } else {
                let d = drive?;
                let bus = str_of(Some(d), "ConnectionBus");
                let removable = bool_of(Some(d), "Removable") || bool_of(Some(d), "MediaRemovable") || bus == "usb" || bus == "sdio";
                if !removable || bool_of(Some(d), "Optical") || bool_of(Some(block), "HintSystem") && bus != "usb" {
                    return None;
                }
                let media = str_of(Some(d), "Media");
                if bus == "sdio" || media.starts_with("flash_sd") || media.starts_with("flash_mmc") {
                    "sd"
                } else {
                    "usb"
                }
            };
            let volumes: Vec<Volume> = std::iter::once((path.as_str(), ifaces))
                .filter(|(_, i)| i.contains_key(FILESYSTEM) || !str_of(i.get(BLOCK), "IdType").is_empty())
                .chain(
                    objects
                        .iter()
                        .filter(|(_, i)| i.get(PARTITION).and_then(|p| p.get("Table")).and_then(Value::as_str) == Some(path))
                        .map(|(p, i)| (p.as_str(), i)),
                )
                .map(|(p, i)| volume(p, i))
                .collect();
            // Unlocked LUKS volumes and partitions: what's mounted from them counts too.
            let own: Vec<&str> = std::iter::once(path.as_str()).chain(volumes.iter().map(|v| v.id.as_str())).collect();
            let runs_system = objects.iter().any(|(p, i)| {
                let ours = own.contains(&p.as_str())
                    || i.get(BLOCK)
                        .and_then(|b| b.get("CryptoBackingDevice"))
                        .and_then(Value::as_str)
                        .is_some_and(|b| own.contains(&b));
                ours && (bool_of(i.get(SWAP), "Active") || mount_points(i).iter().any(|m| SYSTEM_MOUNTS.contains(&m.as_str())))
            });
            if runs_system {
                return None;
            }
            let name = format!("{} {}", str_of(drive, "Vendor"), str_of(drive, "Model")).trim().to_owned();
            Some(Drive {
                id: path.clone(),
                device: bytes_str(block.get("Device")),
                drive: if kind == "loop" { String::new() } else { drive_path.to_owned() },
                name,
                size,
                kind: kind.to_owned(),
                read_only: bool_of(Some(block), "ReadOnly"),
                can_power_off: bool_of(drive, "CanPowerOff"),
                ejectable: bool_of(drive, "Ejectable"),
                volumes,
                backing: bytes_str(ifaces.get(LOOP).and_then(|l| l.get("BackingFile"))),
            })
        })
        .collect();
    out.sort_by(|a, b| a.device.cmp(&b.device));
    out
}

/// Mount point → source device from /proc/self/mountinfo (longest mount
/// point that contains `path`).
pub fn device_of_path(mountinfo: &str, path: &str) -> Option<String> {
    let unescape = |s: &str| s.replace("\\040", " ").replace("\\011", "\t").replace("\\012", "\n").replace("\\134", "\\");
    mountinfo
        .lines()
        .filter_map(|l| {
            let mut fields = l.split(' ');
            let mount = unescape(fields.nth(4)?);
            let (_, rest) = l.split_once(" - ")?;
            let source = rest.split(' ').nth(1)?.to_owned();
            Some((mount, source))
        })
        .filter(|(m, _)| path == m || m == "/" || path.starts_with(&format!("{m}/")))
        .max_by_key(|(m, _)| m.len())
        .map(|(_, s)| s)
        .filter(|s| s.starts_with("/dev/"))
}

/// `/dev/sda1` → `/dev/sda` (via sysfs: a partition's parent is its disk).
pub fn whole_disk(device: &str, sys_class_block: &std::path::Path) -> String {
    let Some(name) = device.strip_prefix("/dev/") else { return device.to_owned() };
    let entry = sys_class_block.join(name);
    if entry.join("partition").exists()
        && let Ok(real) = std::fs::canonicalize(&entry)
        && let Some(parent) = real.parent().and_then(|p| p.file_name())
    {
        return format!("/dev/{}", parent.to_string_lossy());
    }
    device.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dev(s: &str) -> Value {
        let mut b: Vec<Value> = s.bytes().map(Value::from).collect();
        b.push(0.into());
        Value::Array(b)
    }

    fn obj(v: Value) -> BTreeMap<String, Map<String, Value>> {
        serde_json::from_value(v).unwrap()
    }

    fn system() -> Objects {
        let mut o = Objects::new();
        let d = "/org/freedesktop/UDisks2/drives/";
        let b = "/org/freedesktop/UDisks2/block_devices/";
        o.insert(
            format!("{d}Samsung_SSD"),
            obj(json!({ DRIVE: { "Vendor": "", "Model": "Samsung SSD", "ConnectionBus": "", "Removable": false } })),
        );
        o.insert(
            format!("{b}nvme0n1"),
            obj(json!({ BLOCK: { "Device": dev("/dev/nvme0n1"), "Drive": format!("{d}Samsung_SSD"), "Size": 512_u64 << 30, "HintSystem": true } })),
        );
        o.insert(
            format!("{b}nvme0n1p2"),
            obj(json!({ BLOCK: { "Device": dev("/dev/nvme0n1p2"), "Size": 500_u64 << 30, "IdType": "btrfs" }, PARTITION: { "Table": format!("{b}nvme0n1") }, FILESYSTEM: { "MountPoints": [dev("/"), dev("/home")] } })),
        );
        o.insert(format!("{d}SanDisk_Cruzer"), obj(json!({ DRIVE: { "Vendor": "SanDisk", "Model": "Cruzer Blade", "ConnectionBus": "usb", "Removable": true, "MediaRemovable": true, "CanPowerOff": true, "Ejectable": true } })));
        o.insert(format!("{b}sda"), obj(json!({ BLOCK: { "Device": dev("/dev/sda"), "Drive": format!("{d}SanDisk_Cruzer"), "Size": 16_u64 << 30, "IdType": "iso9660", "IdLabel": "PVE", "HintSystem": false }, FILESYSTEM: { "MountPoints": [dev("/run/media/tom/PVE")] } })));
        o.insert(
            format!("{b}sda2"),
            obj(json!({ BLOCK: { "Device": dev("/dev/sda2"), "Size": 8_u64 << 20, "IdType": "vfat" }, PARTITION: { "Table": format!("{b}sda") } })),
        );
        o
    }

    #[test]
    fn only_the_usb_stick_is_offered() {
        let drives = removable(&system(), false);
        assert_eq!(drives.len(), 1);
        let d = &drives[0];
        assert_eq!(
            (d.device.as_str(), d.name.as_str(), d.kind.as_str()),
            ("/dev/sda", "SanDisk Cruzer Blade", "usb")
        );
        assert!(d.can_power_off && d.ejectable && !d.read_only);
        // The ISO's own filesystem on the whole disk and its EFI partition.
        let vols: Vec<_> = d.volumes.iter().map(|v| (v.device.as_str(), v.fs.as_str(), v.label.as_str())).collect();
        assert_eq!(vols, [("/dev/sda", "iso9660", "PVE"), ("/dev/sda2", "vfat", "")]);
        assert_eq!(d.volumes[0].mount_points, ["/run/media/tom/PVE"]);
    }

    #[test]
    fn a_removable_disk_running_the_system_is_never_offered() {
        let mut o = system();
        let b = "/org/freedesktop/UDisks2/block_devices/";
        // A system booted from a stick: its partition is /.
        o.get_mut(&format!("{b}sda2"))
            .unwrap()
            .insert(FILESYSTEM.into(), serde_json::from_value(json!({ "MountPoints": [dev("/")] })).unwrap());
        assert!(removable(&o, false).is_empty());
        // Or through LUKS: the unlocked volume holds /home.
        let mut o = system();
        o.insert(format!("{b}dm_2d0"), obj(json!({ BLOCK: { "Device": dev("/dev/dm-0"), "Size": 1, "CryptoBackingDevice": format!("{b}sda2") }, FILESYSTEM: { "MountPoints": [dev("/home")] } })));
        assert!(removable(&o, false).is_empty());
        // Swap on the stick.
        let mut o = system();
        o.get_mut(&format!("{b}sda2"))
            .unwrap()
            .insert(SWAP.into(), serde_json::from_value(json!({ "Active": true })).unwrap());
        assert!(removable(&o, false).is_empty());
    }

    #[test]
    fn card_readers_without_a_card_and_optical_drives_are_skipped_sd_cards_named() {
        let mut o = Objects::new();
        let d = "/org/freedesktop/UDisks2/drives/";
        let b = "/org/freedesktop/UDisks2/block_devices/";
        o.insert(
            format!("{d}reader"),
            obj(json!({ DRIVE: { "Model": "Card Reader", "ConnectionBus": "usb", "MediaRemovable": true } })),
        );
        o.insert(
            format!("{b}sdb"),
            obj(json!({ BLOCK: { "Device": dev("/dev/sdb"), "Drive": format!("{d}reader"), "Size": 0 } })),
        );
        o.insert(
            format!("{d}dvd"),
            obj(json!({ DRIVE: { "ConnectionBus": "usb", "Removable": true, "Optical": true } })),
        );
        o.insert(
            format!("{b}sr0"),
            obj(json!({ BLOCK: { "Device": dev("/dev/sr0"), "Drive": format!("{d}dvd"), "Size": 4_u64 << 30 } })),
        );
        o.insert(
            format!("{d}mmc"),
            obj(json!({ DRIVE: { "Model": "SD64G", "ConnectionBus": "sdio", "Media": "flash_sd", "MediaRemovable": true } })),
        );
        o.insert(
            format!("{b}mmcblk0"),
            obj(json!({ BLOCK: { "Device": dev("/dev/mmcblk0"), "Drive": format!("{d}mmc"), "Size": 64_u64 << 30 } })),
        );
        let drives = removable(&o, false);
        assert_eq!(
            drives.iter().map(|d| (d.device.as_str(), d.kind.as_str())).collect::<Vec<_>>(),
            [("/dev/mmcblk0", "sd")]
        );
        assert!(drives[0].volumes.is_empty());
    }

    #[test]
    fn loop_devices_only_for_tests() {
        let mut o = system();
        let b = "/org/freedesktop/UDisks2/block_devices/";
        o.insert(
            format!("{b}loop0"),
            obj(json!({ BLOCK: { "Device": dev("/dev/loop0"), "Drive": "/", "Size": 64_u64 << 20 }, LOOP: { "BackingFile": dev("/tmp/stick.img") } })),
        );
        assert_eq!(removable(&o, false).len(), 1);
        let drives = removable(&o, true);
        assert_eq!(drives.iter().map(|d| d.kind.as_str()).collect::<Vec<_>>(), ["loop", "usb"]);
        assert_eq!((drives[0].drive.as_str(), drives[0].backing.as_str()), ("", "/tmp/stick.img"));
    }

    #[test]
    fn folder_to_device() {
        let info = "22 1 0:21 / / rw,relatime shared:1 - btrfs /dev/nvme0n1p2 rw\n\
                    90 22 8:0 / /run/media/tom/My\\040Stick rw,nosuid shared:50 - vfat /dev/sda1 rw\n\
                    91 22 0:5 / /proc rw - proc proc rw\n";
        assert_eq!(device_of_path(info, "/run/media/tom/My Stick").as_deref(), Some("/dev/sda1"));
        assert_eq!(device_of_path(info, "/run/media/tom/My Stick/sub/dir").as_deref(), Some("/dev/sda1"));
        assert_eq!(device_of_path(info, "/run/media/tom/My Sticky").as_deref(), Some("/dev/nvme0n1p2"));
        assert_eq!(device_of_path(info, "/proc/1"), None);
    }

    #[test]
    fn partition_to_disk_through_sysfs() {
        let tmp = tempfile::tempdir().unwrap();
        let devices = tmp.path().join("devices/sda");
        std::fs::create_dir_all(devices.join("sda1")).unwrap();
        std::fs::write(devices.join("sda1/partition"), "1").unwrap();
        let class = tmp.path().join("class");
        std::fs::create_dir_all(&class).unwrap();
        std::os::unix::fs::symlink(devices.join("sda1"), class.join("sda1")).unwrap();
        std::os::unix::fs::symlink(&devices, class.join("sda")).unwrap();
        assert_eq!(whole_disk("/dev/sda1", &class), "/dev/sda");
        assert_eq!(whole_disk("/dev/sda", &class), "/dev/sda");
    }
}
