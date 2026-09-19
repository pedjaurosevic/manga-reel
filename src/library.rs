//! Library folder scan and reading progress.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
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
    #[allow(dead_code)]
    pub progress: Option<ComicProgress>,
}

fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("app", "MangaReel", "manga-reel")
        .map(|d| d.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join(".local/share/manga-reel"))
}

pub fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("app", "MangaReel", "manga-reel")
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join(".cache/manga-reel"))
}

pub fn covers_cache_dir() -> PathBuf {
    cache_dir().join("covers")
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

/// Clean display title from filename/folder stem (strip path junk, leading issue numbers).
pub fn clean_comic_title(stem: &str) -> String {
    let mut s = stem.trim().to_string();
    s = s.replace('_', " ");
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && i < bytes.len() {
        let rest = s[i..].trim_start_matches(['-', '.', ' ', '_']);
        if !rest.is_empty() && rest.len() + 2 < s.len() {
            s = rest.to_string();
        }
    }
    let parts: Vec<_> = s.split_whitespace().collect();
    parts.join(" ")
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

/// If `folders` is empty but progress has paths, re-add unique parent dirs
/// (does **not** require `is_dir` — works while My Passport is unmounted).
pub fn recover_folders_from_progress(state: &mut LibraryState) -> usize {
    if !state.folders.is_empty() || state.progress.is_empty() {
        return 0;
    }
    let mut parents: HashSet<PathBuf> = HashSet::new();
    for key in state.progress.keys() {
        let p = Path::new(key);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                parents.insert(parent.to_path_buf());
            }
        }
    }
    let n = parents.len();
    for p in parents {
        add_folder(state, p);
    }
    n
}

/// Short name for status (e.g. "My Passport").
pub fn folder_label(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.contains("My Passport") || s.contains("My%20Passport") {
        return "My Passport".into();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Probe whether a folder is currently reachable without hanging forever on GVFS.
pub fn folder_reachable(path: &Path) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    // Fast local path.
    if !crate::archive::is_remote_or_gvfs(path) {
        return path.is_dir();
    }
    let path = path.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(path.is_dir());
    });
    rx.recv_timeout(Duration::from_secs(3)).unwrap_or(false)
}

/// Split library folders into (online, offline).
pub fn partition_folders(state: &LibraryState) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut online = Vec::new();
    let mut offline = Vec::new();
    for f in &state.folders {
        if folder_reachable(f) {
            online.push(f.clone());
        } else {
            offline.push(f.clone());
        }
    }
    (online, offline)
}

/// Detect whether gvfsd-fuse is providing `/run/user/$UID/gvfs`.
/// When missing, Gio SFTP may still work but `Path::is_dir` is always false —
/// that used to wipe the library permanently.
pub fn gvfs_fuse_status() -> GvfsFuseStatus {
    let uid = current_uid();
    let gvfs = PathBuf::from(format!("/run/user/{uid}/gvfs"));
    if !gvfs.exists() {
        // Directory often exists only after first gvfs use; check process.
        if gvfsd_fuse_running() {
            return GvfsFuseStatus::Ok;
        }
        return GvfsFuseStatus::Missing;
    }
    // Mounted fuse?
    if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
        let needle = gvfs.to_string_lossy();
        if mounts.lines().any(|l| l.contains(needle.as_ref()) && l.contains("fuse")) {
            return GvfsFuseStatus::Ok;
        }
    }
    if gvfsd_fuse_running() {
        return GvfsFuseStatus::Ok;
    }
    // Path exists but looks empty / not a fuse mount → likely missing fuse.
    match fs::read_dir(&gvfs) {
        Ok(mut rd) => {
            if rd.next().is_some() {
                GvfsFuseStatus::Ok
            } else if gvfsd_fuse_running() {
                GvfsFuseStatus::Ok
            } else {
                GvfsFuseStatus::Missing
            }
        }
        Err(_) => GvfsFuseStatus::Missing,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GvfsFuseStatus {
    Ok,
    Missing,
}

fn gvfsd_fuse_running() -> bool {
    pgrep_running("gvfsd-fuse")
}

fn pgrep_running(name: &str) -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn current_uid() -> String {
    if let Ok(uid) = std::env::var("UID") {
        if !uid.is_empty() {
            return uid;
        }
    }
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "1000".into())
}

/// Soft cap for first paint / UI responsiveness over huge SFTP trees.
pub const DEFAULT_SCAN_CAP: usize = 400;

#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    /// Stop after this many comics (0 = unlimited).
    pub max_entries: usize,
    /// WalkDir max_depth (0 = unlimited). Prefer shallow first for remote trees.
    pub max_depth: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_SCAN_CAP,
            max_depth: 8,
        }
    }
}

pub fn scan_folder(folder: &Path) -> Result<Vec<PathBuf>> {
    scan_folder_opts(folder, ScanOptions {
        max_entries: 0,
        max_depth: 0,
    })
}

pub fn scan_folder_opts(folder: &Path, opts: ScanOptions) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut walk = WalkDir::new(folder).follow_links(true);
    if opts.max_depth > 0 {
        walk = walk.max_depth(opts.max_depth);
    }
    for entry in walk.into_iter().filter_map(|e| e.ok()) {
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
            if opts.max_entries > 0 && out.len() >= opts.max_entries {
                break;
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn scan_all(state: &LibraryState) -> Vec<ComicEntry> {
    scan_all_opts(state, ScanOptions::default()).0
}

/// Returns (entries, truncated) — truncated when we hit max_entries.
pub fn scan_all_opts(state: &LibraryState, opts: ScanOptions) -> (Vec<ComicEntry>, bool) {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    let mut truncated = false;
    for folder in &state.folders {
        if !folder_reachable(folder) {
            continue;
        }
        let remaining = if opts.max_entries > 0 {
            opts.max_entries.saturating_sub(entries.len())
        } else {
            0
        };
        if opts.max_entries > 0 && remaining == 0 {
            truncated = true;
            break;
        }
        let folder_opts = ScanOptions {
            max_entries: remaining,
            max_depth: opts.max_depth,
        };
        if let Ok(paths) = scan_folder_opts(folder, folder_opts) {
            if opts.max_entries > 0 && paths.len() >= remaining && remaining > 0 {
                // Might have more; mark truncated if we filled the budget.
                truncated = true;
            }
            for path in paths {
                let key = key_for(&path);
                if !seen.insert(key.clone()) {
                    continue;
                }
                let title = path
                    .file_stem()
                    .map(|s| clean_comic_title(&s.to_string_lossy()))
                    .unwrap_or_else(|| path.display().to_string());
                let progress = state.progress.get(&key).cloned();
                entries.push(ComicEntry {
                    path,
                    title,
                    progress,
                });
                if opts.max_entries > 0 && entries.len() >= opts.max_entries {
                    truncated = true;
                    break;
                }
            }
        }
        if truncated && opts.max_entries > 0 && entries.len() >= opts.max_entries {
            break;
        }
    }
    entries.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    (entries, truncated)
}

/// Snapshot used by background refresh (Send).
#[derive(Debug, Clone)]
pub struct ScanSnapshot {
    pub entries: Vec<ComicEntry>,
    pub truncated: bool,
    pub online: Vec<PathBuf>,
    pub offline: Vec<PathBuf>,
    pub fuse: GvfsFuseStatus,
    pub recovered: usize,
    pub folder_count: usize,
}

/// Full library probe — run ONLY off the GTK main thread.
pub fn scan_snapshot(mut state: LibraryState, opts: ScanOptions) -> (LibraryState, ScanSnapshot) {
    let recovered = recover_folders_from_progress(&mut state);
    let fuse = gvfs_fuse_status();
    let (online, offline) = partition_folders(&state);
    let (entries, truncated) = scan_all_opts(&state, opts);
    let folder_count = state.folders.len();
    let snap = ScanSnapshot {
        entries,
        truncated,
        online,
        offline,
        fuse,
        recovered,
        folder_count,
    };
    (state, snap)
}

/// Cover thumb cache path keyed by path + mtime/size.
pub fn cover_cache_path(comic: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(comic.to_string_lossy().as_bytes());
    if let Ok(meta) = fs::metadata(comic) {
        hasher.update(meta.len().to_le_bytes());
        if let Ok(m) = meta.modified() {
            if let Ok(d) = m.duration_since(std::time::UNIX_EPOCH) {
                hasher.update(d.as_secs().to_le_bytes());
            }
        }
    }
    covers_cache_dir().join(format!("{}.png", hex::encode(hasher.finalize())))
}

pub fn ensure_data_dirs() -> Result<()> {
    fs::create_dir_all(data_dir()).context("create data dir")?;
    let _ = fs::create_dir_all(covers_cache_dir());
    if let Some(dirs) = directories::ProjectDirs::from("app", "MangaReel", "manga-reel") {
        let _ = fs::create_dir_all(dirs.cache_dir());
        let _ = fs::create_dir_all(dirs.config_dir());
    }
    Ok(())
}
