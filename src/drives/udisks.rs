//! udisks2 over D-Bus: lists drives, unmounts, formats, and hands out the
//! device for writing and reading back. No root of our own: udisks asks
//! polkit (writing raw asks for the password once, formatting a removable
//! drive of the active session doesn't).

use super::devices::{self, Drive, Objects};
use super::format::Fs;
use anyhow::{Context, Result};
use serde_json::{Map, Value as Json};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const UDISKS: &str = "org.freedesktop.UDisks2";
const ROOT: &str = "/org/freedesktop/UDisks2";
const MANAGER: &str = "/org/freedesktop/UDisks2/Manager";
const JOB: &str = "org.freedesktop.UDisks2.Job";
const ENCRYPTED: &str = "org.freedesktop.UDisks2.Encrypted";
const PARTITION_TABLE: &str = "org.freedesktop.UDisks2.PartitionTable";

pub fn connect() -> Result<Connection> {
    Connection::system().context("the system D-Bus is not available")
}

fn json(v: &Value<'_>) -> Json {
    match v {
        Value::U8(n) => (*n).into(),
        Value::Bool(b) => (*b).into(),
        Value::I16(n) => (*n).into(),
        Value::U16(n) => (*n).into(),
        Value::I32(n) => (*n).into(),
        Value::U32(n) => (*n).into(),
        Value::I64(n) => (*n).into(),
        Value::U64(n) => (*n).into(),
        Value::F64(n) => (*n).into(),
        Value::Str(s) => s.as_str().into(),
        Value::Signature(s) => s.to_string().into(),
        Value::ObjectPath(p) => p.as_str().into(),
        Value::Value(inner) => json(inner),
        Value::Array(a) => Json::Array(a.inner().iter().map(json).collect()),
        Value::Dict(d) => Json::Object(
            d.iter()
                .map(|(k, v)| {
                    let key = match json(k) {
                        Json::String(s) => s,
                        other => other.to_string(),
                    };
                    (key, json(v))
                })
                .collect(),
        ),
        Value::Structure(s) => Json::Array(s.fields().iter().map(json).collect()),
        _ => Json::Null,
    }
}

pub fn objects(conn: &Connection) -> Result<Objects> {
    type Raw = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
    let reply = conn
        .call_method(Some(UDISKS), ROOT, Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &())
        .context("udisks2 is not available")?;
    let raw: Raw = reply.body().deserialize().context("reading udisks2's objects")?;
    Ok(raw
        .into_iter()
        .map(|(path, ifaces)| {
            let ifaces: BTreeMap<String, Map<String, Json>> = ifaces
                .into_iter()
                .map(|(i, props)| (i, props.iter().map(|(k, v)| (k.clone(), json(v))).collect()))
                .collect();
            (path.as_str().to_owned(), ifaces)
        })
        .collect())
}

type Options<'a> = HashMap<&'a str, Value<'a>>;

fn call<B>(conn: &Connection, path: &str, iface: &str, method: &str, body: &B) -> zbus::Result<zbus::message::Message>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    conn.call_method(Some(UDISKS), path, Some(iface), method, body)
}

/// Whether udisks can make this filesystem; else the missing program.
pub fn can_format(conn: &Connection, fs: Fs) -> (bool, String) {
    call(conn, MANAGER, "org.freedesktop.UDisks2.Manager", "CanFormat", &(fs.udisks_type(),))
        .ok()
        .and_then(|r| r.body().deserialize::<(bool, String)>().ok())
        .unwrap_or((false, String::new()))
}

/// Unmounts everything on the drive and locks unlocked LUKS volumes.
pub fn release(conn: &Connection, drive: &Drive) -> Result<()> {
    let objs = objects(conn)?;
    let own: Vec<&str> = std::iter::once(drive.id.as_str()).chain(drive.volumes.iter().map(|v| v.id.as_str())).collect();
    // Unlocked volumes first: their filesystems hold the backing device.
    let unlocked: Vec<(&String, &str)> = objs
        .iter()
        .filter_map(|(p, i)| {
            let backing = i.get(devices::BLOCK)?.get("CryptoBackingDevice")?.as_str()?;
            own.contains(&backing).then_some((p, backing))
        })
        .collect();
    let mounted = |p: &str| {
        objs.get(p)
            .and_then(|i| i.get(devices::FILESYSTEM))
            .and_then(|f| f.get("MountPoints"))
            .and_then(Json::as_array)
            .is_some_and(|a| !a.is_empty())
    };
    let unmount = |p: &str| -> Result<()> {
        if mounted(p) {
            call(conn, p, devices::FILESYSTEM, "Unmount", &(Options::new(),)).with_context(|| format!("unmounting {p}"))?;
        }
        Ok(())
    };
    for (clear, backing) in &unlocked {
        unmount(clear)?;
        call(conn, backing, ENCRYPTED, "Lock", &(Options::new(),)).with_context(|| format!("locking {backing}"))?;
    }
    for p in &own {
        unmount(p)?;
    }
    Ok(())
}

/// A new MBR table with one partition over the whole drive, formatted with
/// `fs`. `erase`: zero the drive first (slow). Returns the partition.
pub fn format(conn: &Connection, drive: &Drive, fs: Fs, label: &str, erase: bool) -> Result<String> {
    let mut table = Options::new();
    if erase {
        table.insert("erase", Value::from("zero"));
    }
    call(conn, &drive.id, devices::BLOCK, "Format", &("dos", table)).context("writing a new partition table")?;
    let mut fs_opts = Options::new();
    let label = fs.label(label);
    if !label.is_empty() {
        fs_opts.insert("label", Value::from(label));
    }
    if fs == Fs::Ext4 {
        // The user owns the root of the new filesystem, not root.
        fs_opts.insert("take-ownership", Value::from(true));
    }
    let reply = call(
        conn,
        &drive.id,
        PARTITION_TABLE,
        "CreatePartitionAndFormat",
        &(1u64 << 20, 0u64, fs.mbr_type(), "", Options::new(), fs.udisks_type(), fs_opts),
    )
    .context("creating the partition")?;
    let part: OwnedObjectPath = reply.body().deserialize()?;
    Ok(part.as_str().to_owned())
}

pub fn mount(conn: &Connection, block: &str) -> Result<String> {
    let reply = call(conn, block, devices::FILESYSTEM, "Mount", &(Options::new(),))?;
    Ok(reply.body().deserialize::<String>()?)
}

/// The whole drive for writing (`OpenForRestore`) or reading (`OpenForBackup`).
pub fn open(conn: &Connection, drive: &Drive, write: bool) -> Result<File> {
    // Test loop devices (VELA_DRIVES_LOOP): their own file, no password.
    if drive.kind == "loop" && !drive.backing.is_empty() {
        return std::fs::OpenOptions::new()
            .read(true)
            .write(write)
            .open(&drive.backing)
            .with_context(|| format!("opening {}", drive.backing));
    }
    let method = if write { "OpenForRestore" } else { "OpenForBackup" };
    let reply = call(conn, &drive.id, devices::BLOCK, method, &(Options::new(),)).with_context(|| format!("opening {}", drive.device))?;
    let fd: zbus::zvariant::OwnedFd = reply.body().deserialize()?;
    Ok(File::from(std::os::fd::OwnedFd::from(fd)))
}

/// Has the kernel read the new partition table.
pub fn rescan(conn: &Connection, drive: &Drive) {
    if let Err(e) = call(conn, &drive.id, devices::BLOCK, "Rescan", &(Options::new(),)) {
        log::warn!("drives: rescan of {}: {e}", drive.device);
    }
}

/// Unmounts and powers the drive off (USB), or ejects the medium (cards).
pub fn eject(conn: &Connection, drive: &Drive) -> Result<()> {
    release(conn, drive)?;
    if drive.drive.is_empty() {
        return Ok(());
    }
    if drive.can_power_off {
        call(conn, &drive.drive, devices::DRIVE, "PowerOff", &(Options::new(),)).context("powering the drive off")?;
    } else if drive.ejectable {
        call(conn, &drive.drive, devices::DRIVE, "Eject", &(Options::new(),)).context("ejecting")?;
    }
    Ok(())
}

/// Progress (0..1) of udisks' own job on `block` (erasing), if one runs.
pub fn job_progress(objects: &Objects, block: &str) -> Option<f64> {
    objects.values().find_map(|i| {
        let job = i.get(JOB)?;
        let on_block = job
            .get("Objects")
            .and_then(Json::as_array)
            .is_some_and(|o| o.iter().any(|p| p.as_str() == Some(block)));
        (on_block && job.get("ProgressValid").and_then(Json::as_bool) == Some(true)).then(|| job.get("Progress").and_then(Json::as_f64).unwrap_or(0.0))
    })
}

/// What went wrong, as a word the window translates: auth (polkit said no or
/// the dialog was cancelled), busy (something still uses the drive), gone
/// (unplugged), or other.
pub fn reason(err: &anyhow::Error) -> &'static str {
    let text = format!("{err:#}").to_lowercase();
    if text.contains("notauthorized") || text.contains("not authorized") || text.contains("dismissed") {
        "auth"
    } else if text.contains("busy") || text.contains("target is busy") {
        "busy"
    } else if text.contains("no such") || text.contains("unknownobject") || text.contains("no medium") {
        "gone"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn values_become_json() {
        let v = Value::from(vec![b'/', b'd', 0u8]);
        assert_eq!(json(&v), json!([47, 100, 0]));
        assert_eq!(json(&Value::Value(Box::new(Value::from(7u64)))), json!(7));
        assert_eq!(json(&Value::from("x")), json!("x"));
    }

    #[test]
    fn erase_job_progress() {
        let mut o = Objects::new();
        o.insert(
            "/org/freedesktop/UDisks2/jobs/3".into(),
            serde_json::from_value(json!({ JOB: { "Objects": ["/b/sda"], "Progress": 0.25, "ProgressValid": true } })).unwrap(),
        );
        assert_eq!(job_progress(&o, "/b/sda"), Some(0.25));
        assert_eq!(job_progress(&o, "/b/sdb"), None);
    }

    #[test]
    fn error_reasons() {
        assert_eq!(
            reason(&anyhow::anyhow!("org.freedesktop.UDisks2.Error.NotAuthorizedDismissed: Not authorized")),
            "auth"
        );
        assert_eq!(reason(&anyhow::anyhow!("Error unmounting /dev/sda1: target is busy")), "busy");
        assert_eq!(reason(&anyhow::anyhow!("weird")), "other");
    }
}
