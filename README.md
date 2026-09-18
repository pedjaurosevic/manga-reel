# Manga Reel

**Manga Reel** is an open-source CBZ/CBR comic reader for Linux (Rust + GTK4 + libadwaita).

Fullscreen page reading with smooth pan, toggleable chrome, and seamless autoscroll.

## Features

- Open **CBZ** (zip) and **CBR** (rar via `unrar` CLI)
- Fullscreen **full-page** view (fit width / height / page)
- Smooth pan: arrows, WASD, drag, scroll wheel; page-turn at pan edges
- **LTR / RTL** page-turn direction
- **Double-click** (or `T`) toggles toolbar + status while reading
- **Autoscroll** (`Space` or **Auto** button) on a continuous vertical page strip — no visible hard cut at page boundaries
- Library folder scan; progress in `~/.local/share/manga-reel/`
- `.desktop` launcher + MIME defaults for CBZ/CBR

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
| **Space** | Toggle **autoscroll** |
| **Double-click** / `T` | Toggle toolbar + status (chrome) |
| Drag | Pan (pauses autoscroll) |
| Scroll wheel | Pan vertically (pauses autoscroll) |
| `F11` | Toggle fullscreen |
| `Esc` | Exit fullscreen |

Toolbar: previous/next, Fit width/height/page, LTR/RTL, letterbox, **Auto**.

Autoscroll uses a **continuous vertical strip** (current page + preloaded next/prev). Crossing a page boundary rewires the strip offset so you never see a hard page swap.

## License

MIT — see [LICENSE](LICENSE).


While **Auto** is on, the toolbar shows ↓ / ↑ and a `px/s` label to change autoscroll speed (also `-` / `+` keys). Speed is saved.


### Panel mode
Toggle **Panel** on the toolbar (or `P`). Shows the next detected frame at full height with black side bars. Arrows / Space / scroll move panel-to-panel, including across pages. Toggle **Panel** again to return to page pan.
