//! Library folder scan and reading progress.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryState {
    pub folders: Vec<PathBuf>,
    pub progress: HashMap<String, ComicProgress>,
    pub last_opened: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComicProgress {
    pub page_index: usize,
    pub panel_index: usize,
    pub updated_unix: u64,
}

#[derive(Debug, Clone)]
pub struct ComicEntry {
    pub path: PathBuf,
    pub title: String,
    pub progress: Option<ComicProgress>,
}

fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("app", "MangaReel", "manga-reel")
        .map(|d| d.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join(".local/share/manga-reel"))
}

pub fn state_path() -> PathBuf {
    data_dir().join("library.json")
}

pub fn load_state() -> LibraryState {
    let path = state_path();
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => LibraryState::default(),
    }
}

pub fn save_state(state: &LibraryState) -> Result<()> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(state)?;
    fs::write(path, text)?;
    Ok(())
}

pub fn key_for(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub fn set_progress(state: &mut LibraryState, path: &Path, page: usize, panel: usize) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    state.progress.insert(
        key_for(path),
        ComicProgress {
            page_index: page,
            panel_index: panel,
            updated_unix: now,
        },
    );
    state.last_opened = Some(path.to_path_buf());
}

pub fn add_folder(state: &mut LibraryState, folder: PathBuf) {
    if !state.folders.iter().any(|f| f == &folder) {
        state.folders.push(folder);
    }
}

pub fn scan_folder(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(folder).follow_links(true).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(ext.as_str(), "cbz" | "cbr") {
            out.push(path.to_path_buf());
        }
    }
    out.sort();
    Ok(out)
}

pub fn scan_all(state: &LibraryState) -> Vec<ComicEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for folder in &state.folders {
        if let Ok(paths) = scan_folder(folder) {
            for path in paths {
                let key = key_for(&path);
                if !seen.insert(key.clone()) {
                    continue;
                }
                let title = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                let progress = state.progress.get(&key).cloned();
                entries.push(ComicEntry {
                    path,
                    title,
                    progress,
                });
            }
        }
    }
    entries.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    entries
}

pub fn ensure_data_dirs() -> Result<()> {
    fs::create_dir_all(data_dir()).context("create data dir")?;
    if let Some(dirs) = directories::ProjectDirs::from("app", "MangaReel", "manga-reel") {
        let _ = fs::create_dir_all(dirs.cache_dir());
        let _ = fs::create_dir_all(dirs.config_dir());
    }
    Ok(())
}
