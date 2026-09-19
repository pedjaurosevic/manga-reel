//! Managed local archive imports, cover thumbnails, and reading progress.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryState {
    #[serde(default)]
    pub files: Vec<PathBuf>,
    /// Legacy paths retained for compatibility; never scanned.
    #[serde(default)]
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
    let pending = path.with_extension("json.pending");
    fs::write(&pending, text)?;
    fs::rename(pending, path)?;
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

/// Register one explicitly chosen archive without probing its directory or contents.
pub fn add_file(state: &mut LibraryState, path: PathBuf) -> Result<()> {
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    anyhow::ensure!(
        extension.eq_ignore_ascii_case("cbz") || extension.eq_ignore_ascii_case("cbr"),
        "Select a CBZ or CBR file."
    );
    if !state.files.contains(&path) {
        state.files.push(path);
    }
    Ok(())
}

/// Build the visible library from saved names only, including offline entries.
pub fn entries(state: &LibraryState) -> Vec<ComicEntry> {
    state
        .files
        .iter()
        .map(|path| ComicEntry {
            path: path.clone(),
            title: path
                .file_stem()
                .map(|s| clean_comic_title(&s.to_string_lossy()))
                .unwrap_or_default(),
            progress: state.progress.get(&key_for(path)).cloned(),
        })
        .collect()
}

pub fn books_dir() -> PathBuf {
    data_dir().join("books")
}

pub fn is_managed(path: &Path) -> bool {
    path.starts_with(books_dir())
}

pub fn cover_path(path: &Path) -> PathBuf {
    let hash = hex::encode(Sha256::digest(path.to_string_lossy().as_bytes()));
    data_dir().join("covers").join(format!("{hash}.png"))
}

/// Copy into a private staging directory, verify, then publish the complete archive.
/// Hash directories distinguish same-named books and make repeated imports stable.
pub fn import_into(
    source: &Path,
    root: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf> {
    let mut check = LibraryState::default();
    add_file(&mut check, source.to_path_buf())?;
    fs::create_dir_all(root)?;
    let name = source.file_name().context("missing file name")?;
    let mut input = fs::File::open(source).with_context(|| format!("open {}", source.display()))?;
    let total = input.metadata()?.len();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let staging = root.join(format!(".import-{}-{stamp}", std::process::id()));
    fs::create_dir(&staging)?;
    let pending = staging.join(name);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    let mut buffer = vec![0; 1024 * 1024];
    let mut hash = Sha256::new();
    let mut copied = 0;
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        copied += count as u64;
        progress(copied, total);
    }
    anyhow::ensure!(
        copied == total,
        "Source size changed during import; please retry"
    );
    output.sync_all()?;
    drop(output);
    // Invalid archives never enter the visible library. Partial files are retained
    // in hidden staging directories on failure, rather than risking source data.
    crate::archive::ComicArchive::open(&pending).context("validate copied comic")?;
    let destination_dir = root.join(hex::encode(hash.finalize()));
    fs::create_dir_all(&destination_dir)?;
    let destination = destination_dir.join(name);
    fs::rename(&pending, &destination)?;
    Ok(destination)
}

/// Only call on managed local archives, from a worker thread.
pub fn ensure_cover(path: &Path) -> Result<PathBuf> {
    let target = cover_path(path);
    if target.is_file() {
        return Ok(target);
    }
    let archive = crate::archive::ComicArchive::open(path)?;
    let bytes = archive.cover_bytes()?;
    let img = image::load_from_memory(&bytes)?;
    let thumbnail = img.thumbnail(400, 560);
    fs::create_dir_all(target.parent().unwrap())?;
    let pending = target.with_extension("pending.png");
    thumbnail.save(&pending)?;
    fs::rename(pending, &target)?;
    Ok(target)
}

pub fn register_import(
    state: &mut LibraryState,
    source: &Path,
    destination: PathBuf,
) -> Result<()> {
    if let Some(progress) = state.progress.get(&key_for(source)).cloned() {
        state
            .progress
            .entry(key_for(&destination))
            .or_insert(progress);
    }
    if state.last_opened.as_deref() == Some(source) {
        state.last_opened = Some(destination.clone());
    }
    state.files.retain(|p| p != source || *p == destination);
    add_file(state, destination)
}

/// Remove comics from the library state, deleting managed files from disk and removing cached covers.
pub fn remove_files(
    state: &mut LibraryState,
    paths: &[PathBuf],
    delete_managed_disk: bool,
) -> Result<usize> {
    let mut removed = 0;
    for path in paths {
        let key = key_for(path);
        state.progress.remove(&key);
        if state.last_opened.as_deref() == Some(path) {
            state.last_opened = None;
        }
        let cover = cover_path(path);
        if cover.is_file() {
            let _ = fs::remove_file(&cover);
        }
        if delete_managed_disk && is_managed(path) {
            if path.is_file() {
                let _ = fs::remove_file(path);
            }
            if let Some(parent) = path.parent() {
                if is_managed(parent) && parent != books_dir() {
                    let _ = fs::remove_dir(parent);
                }
            }
        }
        removed += 1;
    }
    state.files.retain(|p| !paths.contains(p));
    save_state(state)?;
    Ok(removed)
}

#[allow(dead_code)]
pub fn remove_file(state: &mut LibraryState, path: &Path, delete_managed_disk: bool) -> Result<()> {
    remove_files(state, &[path.to_path_buf()], delete_managed_disk)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_folders_and_progress_do_not_populate_library() {
        let state: LibraryState = serde_json::from_str(r#"{"folders":["/offline/huge-tree"],"progress":{"/offline/huge-tree/book.cbz":{"page_index":7,"panel_index":2,"updated_unix":1}},"last_opened":null}"#).unwrap();
        assert!(entries(&state).is_empty());
        assert_eq!(state.progress.values().next().unwrap().page_index, 7);
    }
    #[test]
    fn explicit_files_are_deduplicated_and_survive_reload_without_io() {
        let mut state = LibraryState::default();
        let path = PathBuf::from("/offline/no-such-mount/Book.CBZ");
        add_file(&mut state, path.clone()).unwrap();
        add_file(&mut state, path.clone()).unwrap();
        assert!(add_file(&mut state, PathBuf::from("/folder")).is_err());
        let restored: LibraryState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(entries(&restored).len(), 1);
        assert_eq!(entries(&restored)[0].path, path);
        assert!(restored.folders.is_empty());
    }
    #[test]
    fn remove_files_cleans_state_and_progress_without_touching_unmanaged_source() {
        let dir = std::env::temp_dir().join(format!("manga-test-remove-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let unmanaged = dir.join("Outside.CBZ");
        fs::write(&unmanaged, b"dummy").unwrap();

        let mut state = LibraryState::default();
        add_file(&mut state, unmanaged.clone()).unwrap();
        set_progress(&mut state, &unmanaged, 5, 1);
        assert_eq!(state.files.len(), 1);
        assert!(state.progress.contains_key(&key_for(&unmanaged)));

        let count = remove_files(&mut state, &[unmanaged.clone()], true).unwrap();
        assert_eq!(count, 1);
        assert!(state.files.is_empty());
        assert!(!state.progress.contains_key(&key_for(&unmanaged)));
        assert_eq!(state.last_opened, None);
        assert!(unmanaged.is_file(), "unmanaged source file must not be deleted");

        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn remove_files_deletes_managed_disk_file() {
        let bdir = books_dir();
        let test_subdir = bdir.join(format!("test-hash-{}", std::process::id()));
        fs::create_dir_all(&test_subdir).unwrap();
        let file = test_subdir.join("Sample.CBZ");
        fs::write(&file, b"sample content").unwrap();

        let cov = cover_path(&file);
        if let Some(p) = cov.parent() {
            let _ = fs::create_dir_all(p);
        }
        let _ = fs::write(&cov, b"fake cover");

        let mut state = LibraryState::default();
        add_file(&mut state, file.clone()).unwrap();
        set_progress(&mut state, &file, 2, 0);

        assert!(is_managed(&file));
        assert!(file.is_file());
        assert!(cov.is_file());

        let count = remove_files(&mut state, &[file.clone()], true).unwrap();
        assert_eq!(count, 1);
        assert!(!file.exists());
        assert!(!cov.exists());
        assert!(!test_subdir.exists(), "empty hash subdir should be removed");
        assert!(state.files.is_empty());
        assert!(!state.progress.contains_key(&key_for(&file)));
    }
}

#[cfg(test)]
mod import_tests {
    use super::*;
    fn sandbox() -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("manga-import-test-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn full_copy_duplicates_collisions_and_offline_reading() {
        let dir = sandbox();
        let root = dir.join("books");
        let a = dir.join("a");
        let b = dir.join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let source = a.join("Same.CBZ");
        let other = b.join("Same.CBZ");
        let bytes = include_bytes!("../testdata/sample-panels.cbz");
        fs::write(&source, bytes).unwrap();
        let mut different = bytes.to_vec();
        different.extend(b"different edition");
        fs::write(&other, &different).unwrap();
        let mut copied = 0;
        let first = import_into(&source, &root, |n, _| copied = n).unwrap();
        assert_eq!(copied, bytes.len() as u64);
        assert_eq!(fs::read(&first).unwrap(), bytes);
        assert_eq!(import_into(&source, &root, |_, _| {}).unwrap(), first);
        let second = import_into(&other, &root, |_, _| {}).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(&second).unwrap(), different);
        fs::rename(&source, a.join("source-unavailable.cbz")).unwrap();
        assert!(
            crate::archive::ComicArchive::open(&first)
                .unwrap()
                .page_count()
                > 0
        );
    }
    #[test]
    fn invalid_import_is_not_published() {
        let dir = sandbox();
        let source = dir.join("broken.cbz");
        let root = dir.join("books");
        fs::write(&source, b"not a comic").unwrap();
        assert!(import_into(&source, &root, |_, _| {}).is_err());
        assert!(fs::read_dir(root).unwrap().all(|p| p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')));
        assert!(source.exists());
    }
    #[test]
    fn importing_linked_book_preserves_progress_and_deduplicates() {
        let source = PathBuf::from("/offline/old.cbz");
        let dest = PathBuf::from("/local/books/hash/old.cbz");
        let mut state = LibraryState::default();
        add_file(&mut state, source.clone()).unwrap();
        set_progress(&mut state, &source, 17, 2);
        register_import(&mut state, &source, dest.clone()).unwrap();
        register_import(&mut state, &source, dest.clone()).unwrap();
        assert_eq!(state.files, vec![dest.clone()]);
        assert_eq!(state.last_opened, Some(dest.clone()));
        assert_eq!(state.progress[&key_for(&dest)].page_index, 17);
        assert_eq!(state.progress[&key_for(&source)].panel_index, 2);
    }
}
