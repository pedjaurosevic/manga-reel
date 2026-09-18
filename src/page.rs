//! Page image loading and panel cache structures.

use crate::archive::ComicArchive;
use crate::detect::{self, PanelRect};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelCacheFile {
    pub version: u32,
    pub archive_hash: String,
    pub pages: Vec<PagePanels>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagePanels {
    pub page_index: usize,
    pub width: u32,
    pub height: u32,
    pub panels: Vec<PanelRect>,
}

#[derive(Debug, Clone)]
pub struct LoadedPage {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub panels: Vec<PanelRect>,
}

impl ComicArchive {
    pub fn load_page_rgba(&self, index: usize) -> Result<(u32, u32, Vec<u8>)> {
        let bytes = self.read_page(index)?;
        let img = image::load_from_memory(&bytes).context("decode page")?;
        let rgba = img.to_rgba8();
        Ok((rgba.width(), rgba.height(), rgba.into_raw()))
    }
}

pub fn archive_content_hash(path: &Path) -> Result<String> {
    let meta = fs::metadata(path)?;
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(meta.len().to_le_bytes());
    if let Ok(modified) = meta.modified() {
        if let Ok(d) = modified.duration_since(std::time::UNIX_EPOCH) {
            hasher.update(d.as_secs().to_le_bytes());
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sidecar_path(comic: &Path) -> PathBuf {
    let mut s = comic.as_os_str().to_owned();
    s.push(".manga-reel.json");
    PathBuf::from(s)
}

pub fn cache_dir_path(comic: &Path, hash: &str) -> PathBuf {
    let base = directories::ProjectDirs::from("app", "MangaReel", "manga-reel")
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join(".cache").join("manga-reel"));
    let name = comic
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "comic".into());
    base.join(format!("{name}-{hash}.json"))
}

pub fn load_panel_cache(comic: &Path) -> Option<PanelCacheFile> {
    let hash = archive_content_hash(comic).ok()?;
    for candidate in [sidecar_path(comic), cache_dir_path(comic, &hash)] {
        if let Ok(text) = fs::read_to_string(&candidate) {
            if let Ok(cache) = serde_json::from_str::<PanelCacheFile>(&text) {
                if cache.archive_hash == hash && cache.version == 2 {
                    return Some(cache);
                }
            }
        }
    }
    None
}

pub fn save_panel_cache(comic: &Path, cache: &PanelCacheFile, prefer_sidecar: bool) -> Result<()> {
    let text = serde_json::to_string_pretty(cache)?;
    if prefer_sidecar {
        let side = sidecar_path(comic);
        match fs::write(&side, &text) {
            Ok(()) => return Ok(()),
            Err(_) => { /* fall through to user cache */ }
        }
    }
    let path = cache_dir_path(comic, &cache.archive_hash);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

/// Detect panels for all pages (or reuse cache). Runs synchronously — call from a worker thread.
pub fn ensure_panels(archive: &ComicArchive) -> Result<PanelCacheFile> {
    if let Some(cache) = load_panel_cache(&archive.path) {
        if cache.pages.len() == archive.page_count() {
            return Ok(cache);
        }
    }
    let hash = archive_content_hash(&archive.path)?;
    let mut pages = Vec::with_capacity(archive.page_count());
    for i in 0..archive.page_count() {
        let (w, h, rgba) = archive.load_page_rgba(i)?;
        let panels = detect::detect_panels(&rgba, w, h);
        pages.push(PagePanels {
            page_index: i,
            width: w,
            height: h,
            panels,
        });
    }
    let cache = PanelCacheFile {
        version: 2,
        archive_hash: hash,
        pages,
    };
    let _ = save_panel_cache(&archive.path, &cache, true);
    Ok(cache)
}

pub fn panels_for_page(cache: &PanelCacheFile, page: usize) -> Vec<PanelRect> {
    cache
        .pages
        .iter()
        .find(|p| p.page_index == page)
        .map(|p| p.panels.clone())
        .unwrap_or_default()
}
