//! CBZ (zip) and CBR (rar via `unrar` CLI) archive reading.
//!
//! Remote / GVFS paths are copied into the local cache before unrar/zip so a
//! stuck SFTP mount cannot hang the UI forever. `unrar` is wrapped with
//! `timeout(1)`.

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use zip::ZipArchive;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// Max wall time for `unrar lb` (list).
pub const UNRAR_LIST_TIMEOUT_SECS: u64 = 60;
/// Max wall time for `unrar p` (extract one page).
pub const UNRAR_PAGE_TIMEOUT_SECS: u64 = 120;
/// Max wall time when copying a remote archive into cache.
pub const REMOTE_COPY_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone)]
pub struct ComicArchive {
    /// Original library / user path (progress keys, display).
    pub path: PathBuf,
    /// Local readable file (may equal `path`, or a cache copy for GVFS).
    local_path: PathBuf,
    pub kind: ArchiveKind,
    /// Sorted page entry names (natural-ish sort).
    pub pages: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Cbz,
    Cbr,
}

impl ComicArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_status(path, |_| {})
    }

    /// Open archive; `status` is called with short UI strings (e.g. copy progress).
    pub fn open_with_status<F>(path: impl AsRef<Path>, mut status: F) -> Result<Self>
    where
        F: FnMut(&str),
    {
        let path = path.as_ref().to_path_buf();
        if !path_looks_like_file(&path) {
            bail!("not a file: {}", path.display());
        }
        let local = ensure_local_archive(&path, &mut status)?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "cbz" | "zip" => Self::open_cbz(path, local),
            "cbr" | "rar" => Self::open_cbr(path, local),
            _ => bail!("unsupported archive type: .{ext}"),
        }
    }

    fn open_cbz(path: PathBuf, local_path: PathBuf) -> Result<Self> {
        let file = File::open(&local_path).with_context(|| format!("open {}", local_path.display()))?;
        let mut zip = ZipArchive::new(file).context("read zip")?;
        let mut pages = Vec::new();
        for i in 0..zip.len() {
            let entry = zip.by_index(i)?;
            let name = entry.name().to_string();
            if entry.is_dir()
                || name.contains("__MACOSX")
                || name.rsplit('/').next() == Some(".DS_Store")
            {
                continue;
            }
            if is_image_name(&name) {
                pages.push(name);
            }
        }
        sort_pages(&mut pages);
        if pages.is_empty() {
            bail!("no images in CBZ");
        }
        Ok(Self {
            path,
            local_path,
            kind: ArchiveKind::Cbz,
            pages,
        })
    }

    fn open_cbr(path: PathBuf, local_path: PathBuf) -> Result<Self> {
        let out = run_unrar(
            &["lb", "-p-"],
            &local_path,
            None,
            UNRAR_LIST_TIMEOUT_SECS,
        )?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("unrar list failed: {err}");
        }
        let listing = String::from_utf8_lossy(&out.stdout);
        let mut pages: Vec<String> = listing
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|n| !n.is_empty() && is_image_name(n) && !n.contains("__MACOSX"))
            .collect();
        sort_pages(&mut pages);
        if pages.is_empty() {
            bail!("no images in CBR");
        }
        Ok(Self {
            path,
            local_path,
            kind: ArchiveKind::Cbr,
            pages,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn read_page(&self, index: usize) -> Result<Vec<u8>> {
        let name = self
            .pages
            .get(index)
            .ok_or_else(|| anyhow!("page index out of range: {index}"))?;
        match self.kind {
            ArchiveKind::Cbz => self.read_cbz_entry(name),
            ArchiveKind::Cbr => self.read_cbr_entry(name),
        }
    }

    fn read_cbz_entry(&self, name: &str) -> Result<Vec<u8>> {
        let file = File::open(&self.local_path)?;
        let mut zip = ZipArchive::new(file)?;
        let mut entry = zip
            .by_name(name)
            .with_context(|| format!("zip entry {name}"))?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn read_cbr_entry(&self, name: &str) -> Result<Vec<u8>> {
        let out = run_unrar(
            &["p", "-inul", "-p-"],
            &self.local_path,
            Some(name),
            UNRAR_PAGE_TIMEOUT_SECS,
        )?;
        if !out.status.success() || out.stdout.is_empty() {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("unrar extract failed for {name}: {err}");
        }
        Ok(out.stdout)
    }

    /// Decode first page bytes for cover thumbnail.
    pub fn cover_bytes(&self) -> Result<Vec<u8>> {
        self.read_page(0)
    }
}

/// True when the path is under GVFS / looks non-local (SFTP, smb, etc.).
pub fn is_remote_or_gvfs(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/gvfs/") || s.contains("/run/user/") && s.contains("gvfs") {
        return true;
    }
    // Gio sometimes exposes fuse mounts with host= in the component name.
    path.components().any(|c| {
        let c = c.as_os_str().to_string_lossy();
        c.starts_with("sftp:") || c.starts_with("smb:") || c.starts_with("ftp:")
    })
}

fn path_looks_like_file(path: &Path) -> bool {
    // Avoid hanging forever on a dead GVFS stat: try metadata with a short
    // timeout via a helper thread when remote.
    if !is_remote_or_gvfs(path) {
        return path.is_file();
    }
    let path = path.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(path.is_file());
    });
    rx.recv_timeout(Duration::from_secs(8)).unwrap_or(false)
}

fn cache_root() -> PathBuf {
    directories::ProjectDirs::from("app", "MangaReel", "manga-reel")
        .map(|d| d.cache_dir().join("archives"))
        .unwrap_or_else(|| PathBuf::from(".").join(".cache/manga-reel/archives"))
}

fn archive_cache_key(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    if let Ok(meta) = fs::metadata(path) {
        hasher.update(meta.len().to_le_bytes());
        if let Ok(m) = meta.modified() {
            if let Ok(d) = m.duration_since(std::time::UNIX_EPOCH) {
                hasher.update(d.as_secs().to_le_bytes());
                hasher.update(d.subsec_nanos().to_le_bytes());
            }
        }
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_ascii_lowercase();
    format!("{}.{}", hex::encode(hasher.finalize()), ext)
}

/// Ensure we have a local file for zip/unrar. Copies remote/GVFS archives into cache.
fn ensure_local_archive<F>(path: &Path, status: &mut F) -> Result<PathBuf>
where
    F: FnMut(&str),
{
    if !is_remote_or_gvfs(path) {
        return Ok(path.to_path_buf());
    }
    let _ = fs::create_dir_all(cache_root());
    let dest = cache_root().join(archive_cache_key(path));
    if dest.is_file() {
        // Reuse cache if size matches (best-effort; key already includes mtime/size).
        if let (Ok(src_meta), Ok(dst_meta)) = (fs::metadata(path), fs::metadata(&dest)) {
            if src_meta.len() == dst_meta.len() && dst_meta.len() > 0 {
                status("Using local cache…");
                return Ok(dest);
            }
        } else if dest.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            // Source stat failed; still prefer existing cache over hanging.
            status("Using local cache (source offline)…");
            return Ok(dest);
        }
    }

    status("Copying from network storage (SFTP)…");
    let path = path.to_path_buf();
    let dest_tmp = dest.with_extension("partial");
    let dest_clone = dest.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<PathBuf> {
            let mut src = File::open(&path)
                .with_context(|| format!("open remote {}", path.display()))?;
            let mut dst = File::create(&dest_tmp)
                .with_context(|| format!("create cache {}", dest_tmp.display()))?;
            let mut buf = vec![0u8; 1024 * 256];
            loop {
                let n = src.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                dst.write_all(&buf[..n])?;
            }
            dst.flush()?;
            drop(dst);
            fs::rename(&dest_tmp, &dest_clone)?;
            Ok(dest_clone)
        })();
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_secs(REMOTE_COPY_TIMEOUT_SECS)) {
        Ok(Ok(p)) => {
            status("Copy complete.");
            Ok(p)
        }
        Ok(Err(e)) => {
            let _ = fs::remove_file(&dest.with_extension("partial"));
            Err(e)
        }
        Err(_) => {
            let _ = fs::remove_file(&dest.with_extension("partial"));
            bail!(
                "kopiranje sa GVFS/SFTP je isteklo ({}s) — montiraj My Passport ili otvori lokalnu kopiju",
                REMOTE_COPY_TIMEOUT_SECS
            )
        }
    }
}

fn run_unrar(
    args: &[&str],
    archive: &Path,
    entry: Option<&str>,
    timeout_secs: u64,
) -> Result<std::process::Output> {
    let mut cmd = Command::new("timeout");
    cmd.arg(format!("{timeout_secs}s"));
    cmd.arg("unrar");
    for a in args {
        cmd.arg(a);
    }
    cmd.arg(archive);
    if let Some(name) = entry {
        cmd.arg(name);
    }
    let out = cmd
        .output()
        .context("run unrar (is unrar + timeout installed?)")?;
    // GNU timeout exits 124 on timeout.
    if out.status.code() == Some(124) {
        bail!("unrar timeout after {timeout_secs}s (stuck SFTP/GVFS?)");
    }
    Ok(out)
}

fn is_image_name(name: &str) -> bool {
    let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) else {
        return false;
    };
    IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

fn sort_pages(pages: &mut [String]) {
    pages.sort_by(|a, b| nat_cmp(a, b));
}

/// Simple natural compare for paths like page_2.jpg vs page_10.jpg.
fn nat_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a = a.to_ascii_lowercase();
    let b = b.to_ascii_lowercase();
    let mut aa = a.chars().peekable();
    let mut bb = b.chars().peekable();
    loop {
        match (aa.peek().copied(), bb.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na = 0u64;
                while matches!(aa.peek(), Some(c) if c.is_ascii_digit()) {
                    na = na * 10 + (aa.next().unwrap().to_digit(10).unwrap() as u64);
                }
                let mut nb = 0u64;
                while matches!(bb.peek(), Some(c) if c.is_ascii_digit()) {
                    nb = nb * 10 + (bb.next().unwrap().to_digit(10).unwrap() as u64);
                }
                match na.cmp(&nb) {
                    std::cmp::Ordering::Equal => {}
                    o => return o,
                }
            }
            (Some(ca), Some(cb)) => {
                aa.next();
                bb.next();
                match ca.cmp(&cb) {
                    std::cmp::Ordering::Equal => {}
                    o => return o,
                }
            }
        }
    }
}

/// Helper used by testdata generation.
#[allow(dead_code)]
pub fn write_minimal_cbz(path: &Path, pages: &[(&str, &[u8])]) -> Result<()> {
    use std::io::Write;
    let file = File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    for (name, data) in pages {
        zip.start_file(*name, opts)?;
        zip.write_all(data)?;
    }
    zip.finish()?;
    Ok(())
}

#[allow(dead_code)]
pub fn decode_image(bytes: &[u8]) -> Result<image::DynamicImage> {
    image::load_from_memory(bytes).context("decode image")
}
