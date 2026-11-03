//! `vela drives serve`: the window's backend. Writes JSON lines to stdout —
//! `hello`, `drives` and `images` whenever they change, `job` while one runs
//! — and takes commands as JSON lines on stdin:
//!
//! - `{"cmd":"write","drive":<id>,"image":<path>,"verify":true}`
//! - `{"cmd":"format","drive":<id>,"fs":"fat32","label":"STICK","erase":false}`
//!   (`"ventoy":true`: install Ventoy instead, a multiboot stick)
//! - on a Ventoy stick `write` copies the image onto it next to the others;
//!   `"images":[…]` (several) installs Ventoy first if needed, then copies all
//!   `{"cmd":"remove","drive":<id>,"path":<relative>}` deletes one from it
//! - `{"cmd":"eject","drive":<id>}`, `{"cmd":"cancel"}`
//! - `{"cmd":"extra","paths":[…]}` images from elsewhere to offer,
//!   `{"cmd":"add","path":…}` copies one into the image folder.
//!
//! One job at a time. Ends when stdin closes.

use super::devices::{self, Drive};
use super::format::Fs;
use super::images::{self, Image};
use super::{udisks, ventoy, write};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

type Job = Box<dyn FnOnce(&mut Reporter) -> anyhow::Result<()> + Send>;

enum Msg {
    Cmd(Value),
    Eof,
    Job(Value),
    JobEnd,
}

/// Loop devices count as sticks (for tests): `VELA_DRIVES_LOOP=1`.
fn allow_loop() -> bool {
    std::env::var_os("VELA_DRIVES_LOOP").is_some_and(|v| v == "1")
}

fn emit(out: &mut impl Write, v: &Value) -> bool {
    serde_json::to_writer(&mut *out, v).is_ok() && out.write_all(b"\n").is_ok() && out.flush().is_ok()
}

/// Reports a job's state; `rate` in bytes/s over the last seconds.
struct Reporter {
    tx: mpsc::Sender<Msg>,
    base: Value,
    last: Instant,
    window: (Instant, u64),
    rate: f64,
}

impl Reporter {
    fn new(tx: mpsc::Sender<Msg>, base: Value) -> Reporter {
        Reporter {
            tx,
            base,
            last: Instant::now() - Duration::from_secs(1),
            window: (Instant::now(), 0),
            rate: 0.0,
        }
    }

    fn send(&mut self, phase: &str, extra: Value) {
        let mut v = self.base.clone();
        v["phase"] = json!(phase);
        if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            m.extend(e.clone());
        }
        let _ = self.tx.send(Msg::Job(v));
    }

    fn progress(&mut self, phase: &str, done: u64, total: u64) {
        let now = Instant::now();
        if now.duration_since(self.window.0) >= Duration::from_secs(2) {
            let secs = now.duration_since(self.window.0).as_secs_f64();
            let fresh = done.saturating_sub(self.window.1) as f64 / secs;
            self.rate = if self.rate == 0.0 { fresh } else { self.rate * 0.5 + fresh * 0.5 };
            self.window = (now, done);
        }
        if now.duration_since(self.last) >= Duration::from_millis(250) || done == total {
            self.last = now;
            self.send(phase, json!({ "done": done, "total": total, "rate": self.rate as u64 }));
        }
    }

    fn reset_rate(&mut self, done: u64) {
        self.window = (Instant::now(), done);
        self.rate = 0.0;
    }

    fn fail(&mut self, err: &anyhow::Error) {
        log::warn!("drives: {err:#}");
        let cancelled = err
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::Interrupted);
        if cancelled {
            self.send("cancelled", json!({}));
        } else {
            let space = err
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::StorageFull);
            let reason = if space {
                "space"
            } else if format!("{err:#}").contains("unpack") {
                "unpack"
            } else {
                udisks::reason(err)
            };
            self.send("failed", json!({ "error": format!("{err:#}"), "reason": reason }));
        }
    }
}

fn write_job(drive: &Drive, image: &Image, verify: bool, cancel: &AtomicBool, r: &mut Reporter) -> anyhow::Result<()> {
    let conn = udisks::connect()?;
    r.send("unmount", json!({}));
    udisks::release(&conn, drive)?;
    r.send("auth", json!({}));
    let mut dev = udisks::open(&conn, drive, true)?;
    let mut src = write::Source::open(&image.path, image.compression)?;
    let read = src.read.clone();
    // Packed images of unknown size: progress by what was taken from the file.
    let (packed_total, total) = match (image.compression, image.written) {
        (Some(_), 0) => (true, image.size),
        _ => (false, image.written),
    };
    r.reset_rate(0);
    let (len, hash) = write::copy(&mut src, &mut dev, cancel, |done| {
        let shown = if packed_total { read.load(Ordering::Relaxed) } else { done };
        r.progress("write", shown.min(total), total);
    })?;
    src.finish()?;
    drop(dev);
    if verify {
        let mut dev = udisks::open(&conn, drive, false)?;
        r.reset_rate(0);
        r.progress("verify", 0, len);
        if !write::verify(&mut dev, len, &hash, cancel, |done| r.progress("verify", done, len))? {
            r.send("failed", json!({ "reason": "verify", "error": "the stick reads back different data" }));
            return Ok(());
        }
    }
    r.send("finish", json!({}));
    udisks::rescan(&conn, drive);
    r.send("done", json!({ "written": len, "verified": verify }));
    Ok(())
}

fn format_job(drive: &Drive, fs: Fs, label: &str, erase: bool, r: &mut Reporter) -> anyhow::Result<()> {
    let conn = udisks::connect()?;
    r.send("unmount", json!({}));
    udisks::release(&conn, drive)?;
    r.send(if erase { "erase" } else { "format" }, json!({ "done": 0, "total": drive.size }));
    // Format blocks until udisks is done; meanwhile its job shows the erase.
    let watching = Arc::new(AtomicBool::new(erase));
    let watcher = {
        let watching = watching.clone();
        let tx = r.tx.clone();
        let base = r.base.clone();
        let (id, size) = (drive.id.clone(), drive.size);
        std::thread::spawn(move || {
            let Ok(conn) = udisks::connect() else { return };
            let mut rep = Reporter::new(tx, base);
            while watching.load(Ordering::Relaxed) {
                if let Some(p) = udisks::objects(&conn).ok().and_then(|o| udisks::job_progress(&o, &id)) {
                    rep.progress("erase", (p * size as f64) as u64, size);
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        })
    };
    let part = udisks::format(&conn, drive, fs, label, erase);
    watching.store(false, Ordering::Relaxed);
    let _ = watcher.join();
    let part = part?;
    r.send("mount", json!({}));
    let mount = udisks::mount(&conn, &part).unwrap_or_default();
    r.send("done", json!({ "mount": mount, "label": fs.label(label) }));
    Ok(())
}

/// Mounts the Ventoy partition if needed; its mount point.
fn ventoy_mount(conn: &zbus::blocking::Connection, drive: &Drive) -> anyhow::Result<PathBuf> {
    let vol = ventoy::data_volume(drive).ok_or_else(|| anyhow::anyhow!("no Ventoy partition on {}", drive.device))?;
    Ok(PathBuf::from(match vol.mount_points.first() {
        Some(m) => m.clone(),
        None => udisks::mount(conn, &vol.id)?,
    }))
}

fn ventoy_install_job(drive: &Drive, label: &str, r: &mut Reporter) -> anyhow::Result<()> {
    let mount = ventoy_install(drive, label, r)?;
    r.send("done", json!({ "mount": mount, "ventoy": true }));
    Ok(())
}

/// Installs Ventoy and mounts its partition; the mount point ("" if that failed).
fn ventoy_install(drive: &Drive, label: &str, r: &mut Reporter) -> anyhow::Result<String> {
    let conn = udisks::connect()?;
    r.send("unmount", json!({}));
    udisks::release(&conn, drive)?;
    r.send("ventoy", json!({}));
    ventoy::install(&drive.device, label)?;
    r.send("mount", json!({}));
    udisks::rescan(&conn, drive);
    // The new partitions show up in udisks a moment later.
    let mut mount = String::new();
    for _ in 0..40 {
        let fresh = udisks::objects(&conn).ok().map(|o| devices::removable(&o, allow_loop())).unwrap_or_default();
        if let Some(d) = fresh.iter().find(|d| d.id == drive.id)
            && let Some(vol) = ventoy::data_volume(d)
        {
            mount = vol
                .mount_points
                .first()
                .cloned()
                .or_else(|| udisks::mount(&conn, &vol.id).ok())
                .unwrap_or_default();
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(mount)
}

/// Several images onto one stick: Ventoy first unless it is there, then each
/// copied next to the others. Progress counts all of them together.
fn ventoy_batch_job(drive: &Drive, images: &[Image], cancel: &AtomicBool, r: &mut Reporter) -> anyhow::Result<()> {
    let total: u64 = images.iter().map(|i| i.size).sum();
    let mount = if ventoy::data_volume(drive).is_some() {
        r.send("mount", json!({}));
        ventoy_mount(&udisks::connect()?, drive)?
    } else {
        if total > ventoy::room(drive.size) {
            return Err(std::io::Error::new(std::io::ErrorKind::StorageFull, "the images don't fit on the stick together").into());
        }
        let m = ventoy_install(drive, "", r)?;
        anyhow::ensure!(!m.is_empty(), "Ventoy is installed, but its partition did not mount");
        PathBuf::from(m)
    };
    if total > ventoy::free_space(&mount) {
        return Err(std::io::Error::new(std::io::ErrorKind::StorageFull, "the images don't fit on the stick together").into());
    }
    r.reset_rate(0);
    let mut before = 0u64;
    for (n, image) in images.iter().enumerate() {
        r.base["item"] = json!(n + 1);
        r.base["items"] = json!(images.len());
        r.base["name"] = json!(image.file);
        ventoy::copy_to(&image.path, &mount, cancel, |done, _| r.progress("copy", before + done, total))?;
        before += image.size;
    }
    r.send("done", json!({ "ventoy": true, "count": images.len(), "mount": mount }));
    Ok(())
}

fn ventoy_copy_job(drive: &Drive, image: &Image, cancel: &AtomicBool, r: &mut Reporter) -> anyhow::Result<()> {
    let conn = udisks::connect()?;
    r.send("mount", json!({}));
    let mount = ventoy_mount(&conn, drive)?;
    r.reset_rate(0);
    let target = ventoy::copy_to(&image.path, &mount, cancel, |done, total| r.progress("copy", done, total))?;
    r.send("done", json!({ "path": target, "ventoy": true }));
    Ok(())
}

fn copy_job(src: &Path, dir: &Path, cancel: &AtomicBool, r: &mut Reporter) -> anyhow::Result<()> {
    use anyhow::Context;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let name = src.file_name().context("no file name")?;
    let target = dir.join(name);
    if target.exists() {
        r.send("done", json!({ "path": target }));
        return Ok(());
    }
    let part = dir.join(format!(".{}.part", name.to_string_lossy()));
    let total = std::fs::metadata(src)?.len();
    let result = (|| -> anyhow::Result<()> {
        let mut from = std::fs::File::open(src)?;
        let mut to = std::fs::File::create(&part)?;
        let mut buf = vec![0u8; 4 << 20];
        let mut done = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(write::cancelled().into());
            }
            let n = std::io::Read::read(&mut from, &mut buf)?;
            if n == 0 {
                break;
            }
            to.write_all(&buf[..n])?;
            done += n as u64;
            r.progress("copy", done, total);
        }
        to.sync_all()?;
        std::fs::rename(&part, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result?;
    r.send("done", json!({ "path": target }));
    Ok(())
}

struct State {
    dir: PathBuf,
    drives: Vec<Drive>,
    shown: Value,
    images: Vec<Image>,
    extra: Vec<PathBuf>,
    fingerprint: Vec<(String, u64, u64)>,
    job: Option<Arc<AtomicBool>>,
}

impl State {
    fn image_list(&self) -> Vec<Image> {
        let mut out: Vec<Image> = self
            .extra
            .iter()
            .filter(|p| !self.images.iter().any(|i| &i.path == *p))
            .filter_map(|p| images::inspect(p, true))
            .collect();
        out.extend(self.images.iter().cloned());
        out
    }
}

/// The drives as the window sees them: Ventoy sticks with their images.
fn drives_json(drives: &[Drive]) -> Value {
    Value::Array(
        drives
            .iter()
            .map(|d| {
                let mut v = serde_json::to_value(d).unwrap_or_default();
                if let Some(vol) = ventoy::data_volume(d) {
                    let mount = vol.mount_points.first().cloned().unwrap_or_default();
                    let (files, free) = if mount.is_empty() {
                        (Vec::new(), 0)
                    } else {
                        (ventoy::files(Path::new(&mount)), ventoy::free_space(Path::new(&mount)))
                    };
                    v["ventoy"] = json!({ "volume": vol.id, "label": vol.label, "mount": mount, "files": files, "free": free, "size": vol.size });
                }
                v
            })
            .collect(),
    )
}

fn hello(dir: &Path, cfg: &crate::config::Drives) -> Value {
    let formats: serde_json::Map<String, Value> = match udisks::connect() {
        Ok(conn) => Fs::ALL
            .iter()
            .map(|fs| {
                let (ok, missing) = udisks::can_format(&conn, *fs);
                (fs.id().to_owned(), json!({ "ok": ok, "missing": missing }))
            })
            .collect(),
        Err(_) => serde_json::Map::new(),
    };
    json!({
        "type": "hello",
        "version": env!("CARGO_PKG_VERSION"),
        "imageDir": dir,
        "imageDirShown": crate::paths::display_path(dir),
        "verify": cfg.verify,
        "formats": formats,
        "ventoy": ventoy::tool().is_some(),
    })
}

pub fn run() -> anyhow::Result<()> {
    let cfg = crate::config::Config::load().drives;
    let dir = cfg.image_dir();
    let (tx, rx) = mpsc::channel::<Msg>();
    let reader = tx.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str(&line) {
                Ok(v) => {
                    if reader.send(Msg::Cmd(v)).is_err() {
                        return;
                    }
                }
                Err(e) => log::warn!("drives: bad command {line:?}: {e}"),
            }
        }
        let _ = reader.send(Msg::Eof);
    });

    let mut out = std::io::stdout().lock();
    if !emit(&mut out, &hello(&dir, &cfg)) {
        return Ok(());
    }
    let conn = udisks::connect().ok();
    let mut state = State {
        dir,
        drives: Vec::new(),
        shown: Value::Null,
        images: Vec::new(),
        extra: Vec::new(),
        fingerprint: Vec::new(),
        job: None,
    };
    let mut first = true;
    let mut next_poll = Instant::now();
    loop {
        if Instant::now() >= next_poll {
            next_poll = Instant::now() + Duration::from_millis(1000);
            let drives = conn
                .as_ref()
                .and_then(|c| udisks::objects(c).ok())
                .map(|o| devices::removable(&o, allow_loop()))
                .unwrap_or_default();
            let shown = drives_json(&drives);
            state.drives = drives;
            if shown != state.shown || first {
                state.shown = shown;
                if !emit(&mut out, &json!({ "type": "drives", "drives": state.shown })) {
                    return Ok(());
                }
            }
            let fp = images::fingerprint(&state.dir);
            if fp != state.fingerprint || first {
                state.fingerprint = fp;
                state.images = images::list(&state.dir);
                if !emit(&mut out, &json!({ "type": "images", "images": state.image_list() })) {
                    return Ok(());
                }
            }
            first = false;
        }
        let msg = match rx.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
            Ok(m) => m,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        };
        match msg {
            Msg::Eof => {
                // Don't leave a half-written stick behind a closed window.
                if let Some(c) = &state.job {
                    c.store(true, Ordering::Relaxed);
                }
                return Ok(());
            }
            Msg::Job(v) => {
                if !emit(&mut out, &json!({ "type": "job", "job": v })) {
                    return Ok(());
                }
            }
            Msg::JobEnd => {
                state.job = None;
                next_poll = Instant::now();
            }
            Msg::Cmd(cmd) => {
                let name = cmd.get("cmd").and_then(Value::as_str).unwrap_or("");
                let s = |k: &str| cmd.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
                match name {
                    "cancel" => {
                        if let Some(c) = &state.job {
                            c.store(true, Ordering::Relaxed);
                        }
                    }
                    "extra" => {
                        let paths: Vec<PathBuf> = cmd
                            .get("paths")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(PathBuf::from)
                            .collect();
                        for p in paths {
                            if !state.extra.contains(&p) {
                                state.extra.insert(0, p);
                            }
                        }
                        if !emit(&mut out, &json!({ "type": "images", "images": state.image_list() })) {
                            return Ok(());
                        }
                    }
                    "write" | "format" | "eject" | "add" | "remove" => {
                        let id = cmd.get("id").cloned().unwrap_or(Value::Null);
                        if state.job.is_some() {
                            let _ = emit(
                                &mut out,
                                &json!({ "type": "job", "job": { "id": id, "kind": name, "phase": "failed", "reason": "running" } }),
                            );
                            continue;
                        }
                        let drive = state.drives.iter().find(|d| d.id == s("drive") || d.device == s("drive")).cloned();
                        let base = json!({ "id": id, "kind": name, "drive": drive.as_ref().map(|d| d.id.clone()) });
                        let mut rep = Reporter::new(tx.clone(), base);
                        let cancel = Arc::new(AtomicBool::new(false));
                        let job: Job = match name {
                            "add" => {
                                let (src, dir, cancel) = (PathBuf::from(s("path")), state.dir.clone(), cancel.clone());
                                Box::new(move |r| copy_job(&src, &dir, &cancel, r))
                            }
                            _ if drive.is_none() => {
                                rep.send("failed", json!({ "reason": "gone", "error": "the drive is gone" }));
                                continue;
                            }
                            "eject" => {
                                let d = drive.expect("checked");
                                Box::new(move |r| {
                                    udisks::eject(&udisks::connect()?, &d)?;
                                    r.send("done", json!({}));
                                    Ok(())
                                })
                            }
                            "remove" => {
                                let (d, rel) = (drive.expect("checked"), s("path"));
                                Box::new(move |r| {
                                    let mount = ventoy_mount(&udisks::connect()?, &d)?;
                                    let ok = !rel.split('/').any(|c| c == ".." || c.is_empty()) && super::images::is_image_name(&rel);
                                    anyhow::ensure!(ok, "not an image on the stick: {rel}");
                                    std::fs::remove_file(mount.join(&rel))?;
                                    r.send("done", json!({}));
                                    Ok(())
                                })
                            }
                            "format" if cmd.get("ventoy").and_then(Value::as_bool) == Some(true) => {
                                let (d, label) = (drive.expect("checked"), s("label"));
                                Box::new(move |r| ventoy_install_job(&d, &label, r))
                            }
                            "format" => {
                                let Some(fs) = Fs::from_id(&s("fs")) else {
                                    rep.send("failed", json!({ "reason": "other", "error": "unknown filesystem" }));
                                    continue;
                                };
                                let (d, label, erase) = (drive.expect("checked"), s("label"), cmd.get("erase").and_then(Value::as_bool).unwrap_or(false));
                                Box::new(move |r| format_job(&d, fs, &label, erase, r))
                            }
                            _ if cmd.get("images").and_then(Value::as_array).is_some_and(|a| a.len() > 1) => {
                                let wanted: Vec<&str> = cmd["images"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                                let list = state.image_list();
                                let picked: Vec<Image> = wanted.iter().filter_map(|w| list.iter().find(|i| i.path == Path::new(w)).cloned()).collect();
                                let d = drive.expect("checked");
                                let refuse = if picked.len() != wanted.len() {
                                    Some(("other", "an image is gone"))
                                } else if d.read_only {
                                    Some(("readonly", "the drive is write-protected"))
                                } else if ventoy::data_volume(&d).is_none() && ventoy::tool().is_none() {
                                    Some(("noventoy", "Ventoy is not installed"))
                                } else {
                                    None
                                };
                                if let Some((reason, error)) = refuse {
                                    rep.send("failed", json!({ "reason": reason, "error": error }));
                                    continue;
                                }
                                let cancel = cancel.clone();
                                Box::new(move |r| ventoy_batch_job(&d, &picked, &cancel, r))
                            }
                            _ => {
                                let Some(image) = state.image_list().into_iter().find(|i| i.path == Path::new(&s("image"))) else {
                                    rep.send("failed", json!({ "reason": "other", "error": "the image is gone" }));
                                    continue;
                                };
                                let d = drive.expect("checked");
                                if d.read_only {
                                    rep.send("failed", json!({ "reason": "readonly", "error": "the drive is write-protected" }));
                                    continue;
                                }
                                let verify = cmd.get("verify").and_then(Value::as_bool).unwrap_or(true);
                                let cancel = cancel.clone();
                                // Onto a Ventoy stick next to the others, unless asked to overwrite it.
                                if ventoy::data_volume(&d).is_some() && s("mode") != "raw" {
                                    Box::new(move |r| ventoy_copy_job(&d, &image, &cancel, r))
                                } else {
                                    Box::new(move |r| write_job(&d, &image, verify, &cancel, r))
                                }
                            }
                        };
                        state.job = Some(cancel);
                        let done = tx.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = job(&mut rep) {
                                rep.fail(&e);
                            }
                            let _ = done.send(Msg::JobEnd);
                        });
                    }
                    _ => log::warn!("drives: unknown command {name:?}"),
                }
            }
        }
    }
}
