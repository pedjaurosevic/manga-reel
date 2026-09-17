# Manga Reel — agent delivery status

## Blocker
This Grok Bot executor **cannot** run Shell/Read with `machineId` `3a389d87-8875-4b85-9cdb-bbbbb6152787` (po-master). All Shell calls execute on the box (`hostname=cursor`, user `box`). `ListMachines` / cursor MCP is unavailable here.

## What was built (complete v1 source on the box)
Absolute path on **box** (not po-master): `/workspace/manga-reel`

Includes:
- Rust GTK4 + libadwaita app (archive, detect, library, page, settings, library UI, reader UI)
- Panel detection (gutter heuristics), CBZ/CBR, film-strip / vertical / guided modes
- LTR/RTL, letterbox, manual panel edit, progress in `~/.local/share/manga-reel/`
- `manga-reel.desktop` with `Exec=manga-reel %F`, `Icon=manga-reel`
- `scripts/install-desktop.sh` (PATH symlink, icons, MIME defaults, update-desktop-database)
- `assets/manga-reel.svg` + generated PNG icons + `scripts/bootstrap-assets.py`
- `testdata/sample-panels.cbz`

## GitHub (partial)
Pushed to `main` via GitHub MCP (`user-GitHub-xai`):
- .gitignore, Cargo.toml, LICENSE, manga-reel.desktop
- assets/manga-reel.svg
- src/main.rs, src/ui/mod.rs, src/settings.rs, src/library.rs, src/page.rs

**Still need upload** (exist only on box `/workspace/manga-reel` until parent syncs):
- src/archive.rs, src/detect.rs
- src/ui/library_window.rs, src/ui/reader_window.rs
- scripts/install-desktop.sh, scripts/bootstrap-assets.py
- icons/**, assets/manga-reel-icon.png, testdata/sample-panels.cbz
- full README.md

## Parent: finish on po-master
```bash
# On po-master (machineId 3a389d87-…):
mkdir -p ~/src
# Option A: rsync/copy full tree from agent box workspace if CopyFromBox available
# Option B: git clone then copy missing files from /workspace/manga-reel

git clone git@github.com:pedjaurosevic/manga-reel.git ~/src/manga-reel
# Copy remaining files from box /workspace/manga-reel into ~/src/manga-reel

cd ~/src/manga-reel
# If icon already present at assets/manga-reel-icon.png, keep it; else:
python3 scripts/bootstrap-assets.py
cargo build --release
./scripts/install-desktop.sh
manga-reel testdata/sample-panels.cbz
```

Smoke test path: `~/src/manga-reel/testdata/sample-panels.cbz`
Desktop: `~/.local/share/applications/manga-reel.desktop` after install
MIME: xdg-mime default for cbz/cbr
Launcher: Super+Space → "Manga Reel"
