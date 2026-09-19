# Manga Reel 0.1.2

**Version 0.1.2** — non-blocking library (async scan/covers), GVFS-safe folders, SFTP cache + unrar timeouts.

**Manga Reel** is an open-source CBZ/CBR comic reader for Linux (Rust + GTK4 + libadwaita).

**0.1.2 notes:** Library window paints immediately; folder scan and covers run off the GTK thread (cap 400). Offline folders are never wiped. Remote CBR/CBZ copy to local cache before unrar. Detects missing gvfsd-fuse.

**0.1.1 notes:** Panel mode uses pre-bake detection (Italian grids → multiple panels). Letterbox **Black** is pure `#000000`. Library shows cover cards with clean titles. Real Paper is baked into page pixels only (no gray surround parallax).

Fullscreen page reading with smooth pan, bottom chrome, and seamless autoscroll. Real Paper 2 is baked into each page so paper and ink scroll as one sheet.

## Features

- Open **CBZ** (zip) and **CBR** (rar via `unrar` CLI)
- Fullscreen **full-page** view (fit width / height / page)
- Smooth pan: arrows, WASD, drag, scroll wheel; page-turn at pan edges
- **LTR / RTL** page-turn direction
- **Bottom toolbar** (double-click or `T` toggles chrome while reading)
- **Auto** / **Manual** scroll modes (mutually exclusive) + **Panel** mode
- Cover-card library (first-page thumbnails + cleaned titles)
- `.desktop` launcher + MIME defaults for CBZ/CBR
- My Passport / GVFS browse in the library (clear error if unmounted)
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

Toggle **Panel** on the toolbar (or `P`). Shows the next detected frame at full height with **black** side bars. Detection runs on **pre-bake** pixels (DETECT_VERSION 7). Arrows / Space / scroll move panel-to-panel, including across pages. Toggle **Panel** again to return to page pan.

### Library folders

**Add folder** / **My Passport** browse `sftp://po@stari/media/po/My Passport` so you can pick individual folders (e.g. under `MUJA BACKUP/!STRIPOVI`). Requires GVFS/SFTP access to `stari`. If the disk is unmounted, the status line shows a clear error instead of an empty list.

## License

MIT — see [LICENSE](LICENSE).
