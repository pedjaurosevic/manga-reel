# Manga Reel 0.1.3

**Working version** — local comic collection, batch imports, cover shelves, and centered panel reading.

**Manga Reel** is an open-source CBZ/CBR comic reader for Linux (Rust + GTK4 + libadwaita).

The library lists only your selected books. Imported archives are stored locally; cover thumbnails are cached and loaded in the background. No source folders are scanned.

Fullscreen page reading with smooth pan, bottom chrome, and seamless autoscroll. Real Paper 2 is baked into each page so paper and ink scroll as one sheet.

## Features

- Open **CBZ** (zip) and **CBR** (rar via `unrar` CLI)
- Fullscreen **full-page** view (fit width / height / page)
- Smooth pan: arrows, WASD, drag, scroll wheel; page-turn at pan edges
- **LTR / RTL** page-turn direction
- **Bottom toolbar** (double-click or `T` toggles chrome while reading)
- **Auto** / **Manual** scroll modes (mutually exclusive) + **Panel** mode
- Cover grid with titles and reading progress
- Multi-file CBZ/CBR imports copied in full into local storage
- `.desktop` launcher + MIME defaults for CBZ/CBR
- Local or mounted GVFS CBZ/CBR files selected through the file picker
- **Real Paper 2** baked once per page load (paper + ink scroll as one sheet)

## Build

Dependencies (Arch example):

```bash
sudo pacman -S --needed rust gtk4 libadwaita unrar base-devel
```

```bash
git clone git@github.com:pedjaurosevic/manga-reel.git ~/src/manga-reel
cd ~/src/manga-reel
python3 scripts/bootstrap-assets.py   # icons + sample CBZ
cargo build --release
cargo run --release -- testdata/sample-panels.cbz
```

## Install

```bash
./scripts/install-desktop.sh
```

Symlinks `~/.local/bin/manga-reel`, installs icons + desktop entry, registers CBZ/CBR MIME.

## Shortcuts (reader)

| Key / action | What it does |
|--------------|--------------|
| `→` `D` `L` | Pan right (turn page at edge; RTL-aware) |
| `←` `A` `H` | Pan left |
| `↓` `J` `PageDown` | Pan down |
| `↑` `K` `PageUp` | Pan up |
| **Space** (Auto mode) | **Run / pause** autoscroll (mode stays on; chrome can stay hidden) |
| **Space** (Manual mode) | Jump **⅓** viewport down (`Shift+Space` up) |
| **Space** (neither) | Enter Manual and jump ⅓ |
| **Space** (Panel mode) | Next panel |
| **Double-click** / `T` | Toggle bottom toolbar (immersive ↔ settings) |
| Drag | Pan (pauses Auto run; mode stays) |
| Scroll wheel | Pan vertically (pauses Auto run) |
| `P` | Enter Panel mode (`P` again / toolbar to leave) |
| `-` / `+` | Autoscroll speed (while Auto mode selected) |
| `F11` | Toggle fullscreen |
| `Esc` | Exit fullscreen |

Toolbar (bottom): previous/next, page info, Fit, **Auto**, **Manual**, **Panel**, speed ↓/↑, LTR/RTL, letterbox (**Black** = `#000000`).

**Auto** selects Auto mode and starts scrolling. Hide the toolbar (double-click / `T`) for immersive reading; **Space** pauses and resumes without leaving Auto. Show the toolbar again to change speed or switch to Manual/Panel.

Autoscroll uses a **continuous vertical strip** (current page + preloaded next/prev). Crossing a page boundary rewires the strip offset so you never see a hard page swap. Paper is baked once per page load (not every frame).

### Panel mode

Toggle **Panel** on the toolbar (or `P`). Fits the entire detected frame inside the window, centered with a pure **black** surround and an 8 px inset. Wide frames remain fully visible. Detection samples the paper color on **pre-bake** pixels (DETECT_VERSION 8), preserves black frame borders, and supports light, cream, and dark gutters. Ambiguous frames connected by artwork or lettering remain grouped to avoid cutting content. Arrows / Space / scroll move panel-to-panel, including across pages. Toggle **Panel** again to return to page pan.

### Library files

Click **Add Comics** (or **Ctrl+O**) and select one or more CBZ/CBR files with Ctrl or Shift. **Import** copies each complete archive into `~/.local/share/manga-reel/books/` (or the corresponding XDG data directory). Original files are left untouched. A progress bar tracks the batch; failed imports are reported without stopping the remaining files.

Books are stored in content-hash directories, so different files with the same name cannot overwrite each other. Reimporting the same file reuses its library entry. Click a cover to read; the title and saved reading position appear below it. First-page thumbnails are cached in the local `covers/` directory and generated one at a time off the UI thread.

For previously linked books, **Import Linked Books** copies just those saved files and carries their reading progress over. No source directory scanning occurs. Network shares must be mounted in the file manager before selection. Failed copies remain in hidden `.import-*` staging directories and never appear as complete books.

## License

MIT — see [LICENSE](LICENSE).

### Panel detection review

`cargo run --example panel_probe -- page.png /tmp/panel-review` writes detected bounds and individual panel previews. `cargo test --offline` checks monochrome borders, cream/dark gutters, malformed images, RTL order, and the actual Cairo rendering.
