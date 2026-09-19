# Manga Reel work log

## 2026-09-19 — Panel replacement

- Replaced v7 with independent RGB paper-color gutter segmentation (v8). The old black-or-white mask reduced a four-frame monochrome regression to one page; the replacement detects all four and retains their borders.
- Reference inspected locally: Comic Trim 4.20 APK, decompiled `ap` margin sampling/gutter analysis and `ImmActivity.b0` centered contain transform. This is a native Rust implementation, not an Android runtime or a claim of exact behavioral equivalence.
- Reader now contains the entire panel with a black inset. A real Cairo test caught a one-level interpolation bleed into the surround; explicit clipping fixed it.
- Tests: 6/6 passed, including malformed dimensions, monochrome/cream/black gutters, RTL rows, wide-panel layout, and Cairo pixel checks. Visual review: Martin Mystere 279, image indices 2, 5, 10 (five, four, four detected regions). On index 5, lettering bridges the two bottom frames: they deliberately remain grouped to avoid slicing the lettering. Irregular/diagonal and overlapping panels are not fully solved.
- Review artifacts: `/tmp/manga-review-{2,5,10}/`; reusable `examples/panel_probe.rs` generates bounds and isolated previews. Copyrighted pages remain outside the repository.
- Reader source is now plain Rust; removed superseded detector fragments and encoded reader build pipeline. Existing deprecated GTK API warnings remain unrelated.

## 2026-09-19 — Manual library selection

- Removed startup folder traversal, folder recovery, remote-root probing, and automatic cover extraction. The library now renders only explicitly saved `files`; legacy folder paths and all reading progress remain preserved.
- “Dodaj strip” / Ctrl+O selects one CBZ/CBR and persists its path without opening the archive. “Čitaj” loads only that archive on a worker; duplicate opening requests are blocked while loading. File associations also register the selected file.
- Verified 8/8 tests, release build, and `git diff --check`. Restarted the previous Manga Reel process and visually confirmed the empty manual library and add button on the desktop. Startup log: `/tmp/manga-manual-library.log`; screenshot: `/tmp/manga-manual-library.png`. Existing 23 progress entries were retained; no library data migration or deletion was required.

## 2026-09-19 — English interface

- Translated library labels, dialogs, errors, and archive copy notifications into English; synchronized README control names. Name remains Manga Reel.
- Release build passed; launched the app and visually confirmed Library / Add Comic / Read with the user's two saved entries. No remaining Serbian UI strings found in Rust sources. `git diff --check` passed.
- GitHub checked live: public repository, latest published release/tag v0.1.2, published 2026-09-19 11:36:29 UTC.

## 2026-09-19 — Release 0.1.3

- Sealed 0.1.3 release: bumped manifest and README to 0.1.3, transitioned CHANGELOG to 0.1.3.
- Removed unused `ui::open_reader` export; silenced module-reimport warnings in standalone examples with `#![allow(dead_code)]`.
- Verified 11/11 unit and integration tests passing (`cargo test`). Release build compiled cleanly (`cargo build --release`).
- Ready for publishing to GitHub with tag `v0.1.3`.

## 2026-09-19 — Library selection and comic deletion

- Added `library::remove_files` with cleanup of `state.files`, `state.progress`, `state.last_opened`, cached covers, and managed archive files in `books/` (including removal of empty hash parent folders).
- Implemented Selection Mode in `LibraryWindow`: "Select" header button, card selection badge overlays (`checkbox-symbolic` / `checkbox-checked-symbolic`), "Select All" / "Deselect All", and destructive "Delete" action with Libadwaita confirmation `AlertDialog`.
- Added right-click context menu per card (`gtk4::Popover`) with "Select" and "Delete…".
- Added key bindings: `Escape` exits selection mode, `Delete`/`BackSpace` triggers deletion, `Ctrl+A` toggles select all.
- Tests: 13/13 passed (added unmanaged source preservation and managed file deletion unit tests). Release build compiled cleanly.

