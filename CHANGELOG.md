# Changelog

## Unreleased

- Add selection mode and comic deletion in the library: mark individual or multiple comics with checkmark badges, toggle Select All, and delete selected comics with a confirmation dialog.
- Automatically clean up library state, reading progress, cached cover thumbnails, and local managed archive files from disk upon deletion.
- Add right-click context menu on comic tiles for instant "Select" or "Delete…".
- Add keyboard shortcuts: Delete/BackSpace to delete selected comics, Escape to exit selection mode, and Ctrl+A to select/deselect all.

## 0.1.3 — 2026-09-19

- Import multiple selected CBZ/CBR files into managed local storage with per-file progress, content-hash destinations, archive validation, and partial-failure reporting. Preserve reading progress when replacing legacy links.
- Show cover shelves with cached first-page thumbnails, titles, and continue-reading indicators; process cover jobs sequentially outside GTK.

- Use English for all library controls, status messages, and archive copy notifications.

- Replace folder scanning and automatic cover loading with explicit, persistent file-by-file library selection. Legacy folders are ignored and reading progress retained. Add with “Add Comic” / Ctrl+O; load only on “Read”. Local and remote archives both open on a worker.

- Replace v7 mixed black/white gutter detection with paper-color segmentation; retain printed frame edges and invalidate old detection caches (v8).
- Center and contain complete panels on pure black with an 8 px inset; wide panels are no longer cropped at the viewport edges. Clip Cairo sampling to the panel bounds.
- Apply LTR/RTL changes immediately to cached panels.
- Keep reader Rust source directly editable; remove the encoded reader build pipeline and superseded detector parts.
- Add detection regressions, an actual Cairo rendering test, and the `panel_probe` visual review tool.

## 0.1.2 — 2026-09-19

### Fixes
- **Library wipe on unmount**: refresh no longer `retain(|f| f.is_dir())` + save. Offline / unmounted folders (My Passport, GVFS) stay in `library.json`; status shows e.g. `N folder(a) offline (My Passport)`.
- If `folders` is empty but progress has paths, auto-re-add unique parent dirs (no `is_dir` required).
- **Async library scan**: window paints immediately with “Skeniram biblioteku…”; WalkDir runs off the GTK thread (cap 400 comics, depth 8). Soft-cap note in status when truncated.
- **Async covers**: grid builds with title + placeholder; cover thumbs load on a worker thread and are cached under `~/.cache/manga-reel/covers/` (path+mtime/size key). Never run `unrar` for dozens of remote CBRs on the GTK thread.
- **GVFS / SFTP open**: archives under `/run/user/*/gvfs/` are copied to `~/.cache/manga-reel/archives/` before zip/unrar; subsequent opens reuse the cache. Status: `Kopiram sa starog (SFTP)…`.
- **`unrar` timeouts**: `timeout(1)` wraps list (60s) and page extract (120s) so a stuck SFTP cannot hang forever.
- **gvfsd-fuse**: if FUSE mount is missing, status warns clearly (Gio SFTP can work while `Path::is_dir` is always false — that was the wipe root cause).

## 0.1.1 — 2026-09-19

### Fixes
- **Panel detection v7**: run Comic Trim BSP on **pre-bake** raw pixels (Real Paper cream/grain no longer collapses Italian 2-up grids to `panel 1/1`). Cream-tolerant paper sampling; `DETECT_VERSION` bumped (cache invalidated).
- **Letterbox Black = `#000000`**: surround / panel side bars are true black, not `paper_stock` gray/cream. White letterbox is pure white. Opaque CSS clear so desktop Real Paper cannot show through.
- **Parallax**: paper stays baked into the page pixbuf only; letterbox is flat black (no textured surround to scroll against). Integer pan snap retained; Hyprland shader still suspended while reading.

### Library
- Cover thumbnails (first page of each CBZ/CBR) in a FlowBox card grid.
- Title-only cards with cleaned names (strip leading catalog numbers like `284-`).
- Clear status when **My Passport** is unmounted / unreachable (no silent empty).

### Internals
- Wire `page` panel cache (`DETECT_VERSION` gated) into the reader; drop dual rgba+pixbuf buffers after bake.
- Easy unused-warning cleanups.

## 0.1.0 — 2026-09-19
First tagged release of **Manga Reel**, a Linux CBZ/CBR comic reader (Rust + GTK4 + libadwaita).

### Features
- Open **CBZ** (zip) and **CBR** (rar via `unrar`)
- Fullscreen page reading with fit width / height / page
- Smooth pan (arrows, WASD, drag, scroll wheel) with page-turn at pan edges
- **LTR / RTL** reading order
- **Bottom toolbar** (double-click or `T`); immersive chrome hide
- **Auto** mode: Space run/pause while chrome can stay hidden; ± speed
- **Manual** mode: Space jumps ⅓ viewport
- **Panel** mode: full-height detected frames (detect v6 gutter BSP)
- **Real Paper 2** baked once per page load; Hyprland screen shader suspended while reading
- Seamless vertical strip autoscroll across page boundaries
- Library folder scan + progress under `~/.local/share/manga-reel/`
- **My Passport** / GVFS SFTP browse (`sftp://po@stari/...`)
- Desktop launcher + MIME defaults (`scripts/install-desktop.sh`)

### Fixes in this release
- Restore Hyprland `decoration:screen_shader` if the first page fails to load (before close handlers are attached)
