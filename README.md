# Manga Reel 0.1.0

**Version 0.1.0** — first tagged release.

**Manga Reel** is an open-source CBZ/CBR comic reader for Linux (Rust + GTK4 + libadwaita).

Fullscreen page reading with smooth pan, bottom chrome, and seamless autoscroll. Real Paper 2 is baked into each page so paper and ink scroll as one sheet.

## Features

- Open **CBZ** (zip) and **CBR** (rar via `unrar` CLI)
- Fullscreen **full-page** view (fit width / height / page)
- Smooth pan: arrows, WASD, drag, scroll wheel; page-turn at pan edges
- **LTR / RTL** page-turn direction
- **Bottom toolbar** (double-click or `T` toggles chrome while reading)
- **Auto** / **Manual** scroll modes (mutually exclusive) + **Panel** mode
- Library folder scan; progress in `~/.local/share/manga-reel/`
- `.desktop` launcher + MIME defaults for CBZ/CBR
- My Passport / GVFS browse in the library
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

Toolbar (bottom): previous/next, page info, Fit, **Auto**, **Manual**, **Panel**, speed ↓/↑, LTR/RTL, letterbox.

**Auto** selects Auto mode and starts scrolling. Hide the toolbar (double-click / `T`) for immersive reading; **Space** pauses and resumes without leaving Auto. Show the toolbar again to change speed or switch to Manual/Panel.

Autoscroll uses a **continuous vertical strip** (current page + preloaded next/prev). Crossing a page boundary rewires the strip offset so you never see a hard page swap. Paper is baked once per page load (not every frame).

### Panel mode

Toggle **Panel** on the toolbar (or `P`). Shows the next detected frame at full height with paper-stock side bars. Arrows / Space / scroll move panel-to-panel, including across pages. Toggle **Panel** again to return to page pan.

### Library folders

**Add folder** / **My Passport** browse `sftp://po@stari/media/po/My Passport` so you can pick individual folders (e.g. under `MUJA BACKUP/!STRIPOVI`). Requires GVFS/SFTP access to `stari`.

## License

MIT — see [LICENSE](LICENSE).
