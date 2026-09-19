# Changelog

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
