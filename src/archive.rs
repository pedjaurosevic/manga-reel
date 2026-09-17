//! CBZ (zip) and CBR (rar via `unrar` CLI) archive reading.

use anyhow::{anyhow, bail, Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::ZipArchive;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

#[derive(Debug, Clone)]
pub struct ComicArchive {
    pub path: PathBuf,
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
        let path = path.as_ref().to_path_buf();
        if !path.is_file() {
            bail!("not a file: {}", path.display());
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "cbz" | "zip" => Self::open_cbz(path),
            "cbr" | "rar" => Self::open_cbr(path),
            _ => bail!("unsupported archive type: .{ext}"),
        }
    }

    fn open_cbz(path: PathBuf) -> Result<Self> {
        let file = File::open(&path).with_context(|| format!("open {}", path.display()))?;
        let mut zip = ZipArchive::new(file).context("read zip")?;
        let mut pages = Vec::new();
        for i in 0..zip.len() {
            let entry = zip.by_index(i)?;
            let name = entry.name().to_string();
            if entry.is_dir() || name.contains("__MACOSX") || name.rsplit('/').next() == Some(".DS_Store")
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
            kind: ArchiveKind::Cbz,
            pages,
        })
    }

    fn open_cbr(path: PathBuf) -> Result<Self> {
        // Prefer listing with unrar
        let out = Command::new("unrar")
            .args(["lb", "-p-"])
            .arg(&path)
            .output()
            .context("run unrar (is unrar installed?)")?;
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
        let file = File::open(&self.path)?;
        let mut zip = ZipArchive::new(file)?;
        let mut entry = zip
            .by_name(name)
            .with_context(|| format!("zip entry {name}"))?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn read_cbr_entry(&self, name: &str) -> Result<Vec<u8>> {
        let out = Command::new("unrar")
            .args(["p", "-inul", "-p-"])
            .arg(&self.path)
            .arg(name)
            .output()
            .context("unrar extract")?;
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

