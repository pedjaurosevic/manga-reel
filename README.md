# Manga Reel

**Manga Reel** is an open-source **panel-by-panel** CBZ/CBR comic reader for Linux (Rust + GTK4 + libadwaita).

Instead of page-first reading, Manga Reel frames each panel so you can step through manga and comics like a guided reel — with film-strip auto-advance and vertical modes.

## Features (v1)

- Open **CBZ** (zip) and **CBR** (rar via `unrar` CLI)
- **Panel detection** (classical gutter / projection heuristics; no ML)
- Panel cache in sidecar `.manga-reel.json` or `~/.cache/manga-reel/`
- Reader: step panels, **black/white letterbox**, **LTR/RTL** order
- Modes: **guided**, **horizontal film-strip** (adjustable speed), **vertical** scroll-step
- Library: pick folder, recursive CBZ/CBR scan, remember progress in `~/.local/share/manga-reel/`
- Manual panel **edit** (add / move / resize / delete / save)
- Keyboard shortcuts, `.desktop` launcher, MIME defaults for CBZ/CBR

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
cargo run --release
# or open a file directly:
cargo run --release -- testdata/sample-panels.cbz
```

## Install (launcher + MIME + PATH)

After a release build:

```bash
./scripts/install-desktop.sh
```

This will:

1. Symlink the binary to `~/.local/bin/manga-reel`
2. Install icons under `~/.local/share/icons/hicolor/*/apps/manga-reel.png`
3. Install `~/.local/share/applications/manga-reel.desktop` (`Icon=manga-reel`, `Exec=… %F`)
4. Register as default handler for CBZ/CBR MIME types
5. Run `update-desktop-database` / icon cache updates

Then:

- **Super+Space** (Omarchy) → type “Manga Reel”
- Double-click a `.cbz` / `.cbr` file
- CLI: `manga-reel /path/to/file.cbz`

## Shortcuts (reader)

| Key | Action |
|-----|--------|
| `→` / `Space` / `D` / `J` | Next panel |
| `←` / `A` / `K` | Previous panel |
| Scroll wheel | Next / previous |
| `Delete` | Delete panel (edit mode) |
| `Esc` | Exit edit mode |

Toolbar toggles: RTL/LTR, letterbox, Guided / Film strip / Vertical, Edit.

## License

MIT — see [LICENSE](LICENSE).
