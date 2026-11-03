//! Copying an image onto a device and reading it back. Works on any `File`
//! (the device fd from udisks, a temp file in tests).

use super::images::Compression;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const CHUNK: usize = 4 << 20;
/// Flushed this often, so the progress shows what reached the stick and not
/// what sits in the page cache (and a cancel doesn't wait for gigabytes).
const SYNC_EVERY: u64 = 64 << 20;

pub fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// Copies `src` to the start of `dst`, flushing as it goes. `progress` gets
/// the bytes written so far. Returns their count and SHA-256.
pub fn copy(src: &mut impl Read, dst: &mut File, cancel: &AtomicBool, mut progress: impl FnMut(u64)) -> io::Result<(u64, [u8; 32])> {
    let mut buf = vec![0u8; CHUNK];
    let mut hash = Sha256::new();
    let mut done = 0u64;
    let mut synced = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let n = read_full(src, &mut buf)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).map_err(|e| match e.raw_os_error() {
            Some(libc::ENOSPC) => io::Error::new(io::ErrorKind::StorageFull, "the image is bigger than the drive"),
            _ => e,
        })?;
        hash.update(&buf[..n]);
        done += n as u64;
        if done - synced >= SYNC_EVERY {
            dst.sync_data()?;
            synced = done;
        }
        progress(done);
    }
    dst.sync_data()?;
    progress(done);
    Ok((done, hash.finalize().into()))
}

/// Reads `len` bytes from the start of `dev` past the page cache and
/// compares their SHA-256. `Ok(false)` = they differ.
pub fn verify(dev: &mut File, len: u64, expected: &[u8; 32], cancel: &AtomicBool, mut progress: impl FnMut(u64)) -> io::Result<bool> {
    // What we just wrote is still cached; reading that would prove nothing.
    // SAFETY: plain syscall on an fd we own.
    unsafe {
        libc::posix_fadvise(dev.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
    }
    let mut buf = vec![0u8; CHUNK];
    let mut hash = Sha256::new();
    let mut done = 0u64;
    while done < len {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let want = ((len - done) as usize).min(CHUNK);
        let n = read_full(dev, &mut buf[..want])?;
        if n == 0 {
            return Ok(false);
        }
        hash.update(&buf[..n]);
        done += n as u64;
        progress(done);
    }
    Ok(<[u8; 32]>::from(hash.finalize()) == *expected)
}

/// The image's bytes as written to the stick: the file itself, or the
/// output of its unpacker. `read` counts the bytes taken from the file (for
/// progress of packed images, whose unpacked size may be unknown).
pub struct Source {
    reader: Box<dyn Read + Send>,
    child: Option<std::process::Child>,
    feeder: Option<std::thread::JoinHandle<io::Result<()>>>,
    pub read: Arc<AtomicU64>,
}

impl Source {
    pub fn open(path: &Path, compression: Option<Compression>) -> io::Result<Source> {
        let read = Arc::new(AtomicU64::new(0));
        let mut file = File::open(path)?;
        let Some(c) = compression else {
            return Ok(Source {
                reader: Box::new(Counting {
                    inner: file,
                    count: read.clone(),
                }),
                child: None,
                feeder: None,
                read,
            });
        };
        let mut child = std::process::Command::new(c.tool())
            .args(["-dc"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| io::Error::new(e.kind(), format!("{} is needed to unpack this image: {e}", c.tool())))?;
        let mut stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let count = read.clone();
        let feeder = std::thread::spawn(move || {
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    return Ok(());
                }
                // The unpacker gone (error, or we stopped reading): stop too.
                if stdin.write_all(&buf[..n]).is_err() {
                    return Ok(());
                }
                count.fetch_add(n as u64, Ordering::Relaxed);
            }
        });
        Ok(Source {
            reader: Box::new(stdout),
            child: Some(child),
            feeder: Some(feeder),
            read,
        })
    }

    /// After the copy: whether the unpacker finished cleanly.
    pub fn finish(mut self) -> io::Result<()> {
        self.reader = Box::new(std::io::empty());
        if let Some(f) = self.feeder.take() {
            f.join().map_err(|_| io::Error::other("reader thread panicked"))??;
        }
        if let Some(mut child) = self.child.take() {
            let mut err = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_string(&mut err);
            }
            let status = child.wait()?;
            if !status.success() {
                return Err(io::Error::other(format!("unpacking failed: {}", err.trim())));
            }
        }
        Ok(())
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        if let Some(c) = &mut self.child {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Read for Source {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

struct Counting {
    inner: File,
    count: Arc<AtomicU64>,
}

impl Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom};

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 % 251) as u8).collect()
    }

    #[test]
    fn copy_then_verify_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let data = pattern(9 << 20 | 123);
        let img = tmp.path().join("a.img");
        std::fs::write(&img, &data).unwrap();
        // The "stick": bigger than the image, with old content behind it.
        let dev_path = tmp.path().join("dev");
        std::fs::write(&dev_path, vec![0xAAu8; 16 << 20]).unwrap();
        let mut dev = std::fs::OpenOptions::new().read(true).write(true).open(&dev_path).unwrap();
        let cancel = AtomicBool::new(false);
        let mut src = Source::open(&img, None).unwrap();
        let mut seen = Vec::new();
        let (n, hash) = copy(&mut src, &mut dev, &cancel, |d| seen.push(d)).unwrap();
        assert_eq!(src.read.load(Ordering::Relaxed), data.len() as u64);
        src.finish().unwrap();
        assert_eq!(n, data.len() as u64);
        assert!(seen.windows(2).all(|w| w[0] <= w[1]) && *seen.last().unwrap() == n);
        let written = std::fs::read(&dev_path).unwrap();
        assert_eq!(&written[..data.len()], &data[..]);
        assert_eq!(written[data.len()], 0xAA);

        let mut reader = File::open(&dev_path).unwrap();
        assert!(verify(&mut reader, n, &hash, &cancel, |_| {}).unwrap());
        // A flipped byte on the "stick" is caught.
        let mut w = std::fs::OpenOptions::new().write(true).open(&dev_path).unwrap();
        w.seek(SeekFrom::Start(5 << 20)).unwrap();
        w.write_all(&[!data[5 << 20]]).unwrap();
        let mut reader = File::open(&dev_path).unwrap();
        assert!(!verify(&mut reader, n, &hash, &cancel, |_| {}).unwrap());
    }

    #[test]
    fn cancel_stops_the_copy() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.img"), pattern(1 << 20)).unwrap();
        let mut dev = File::create(tmp.path().join("dev")).unwrap();
        let cancel = AtomicBool::new(true);
        let mut src = Source::open(&tmp.path().join("a.img"), None).unwrap();
        let err = copy(&mut src, &mut dev, &cancel, |_| {}).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn packed_images_are_unpacked_on_the_way() {
        let tmp = tempfile::tempdir().unwrap();
        let data = pattern(3 << 20);
        let packed = tmp.path().join("a.img.gz");
        let mut gz = std::process::Command::new("gzip")
            .arg("-c")
            .stdin(std::process::Stdio::piped())
            .stdout(File::create(&packed).unwrap())
            .spawn()
            .unwrap();
        gz.stdin.take().unwrap().write_all(&data).unwrap();
        assert!(gz.wait().unwrap().success());
        let mut dev = File::create(tmp.path().join("dev")).unwrap();
        let mut src = Source::open(&packed, Some(Compression::Gzip)).unwrap();
        let (n, _) = copy(&mut src, &mut dev, &AtomicBool::new(false), |_| {}).unwrap();
        let fed = src.read.load(Ordering::Relaxed);
        src.finish().unwrap();
        assert_eq!(n, data.len() as u64);
        assert_eq!(fed, std::fs::metadata(&packed).unwrap().len());
        assert_eq!(std::fs::read(tmp.path().join("dev")).unwrap(), data);

        // A broken archive is an error, not a short image.
        let broken = tmp.path().join("b.img.gz");
        let mut bytes = std::fs::read(&packed).unwrap();
        bytes.truncate(bytes.len() / 2);
        std::fs::write(&broken, bytes).unwrap();
        let mut dev = File::create(tmp.path().join("dev2")).unwrap();
        let mut src = Source::open(&broken, Some(Compression::Gzip)).unwrap();
        let copied = copy(&mut src, &mut dev, &AtomicBool::new(false), |_| {});
        assert!(copied.is_err() || src.finish().is_err());
    }
}
