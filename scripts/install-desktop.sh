#!/usr/bin/env bash
# Install Manga Reel launcher, icons, MIME defaults, and PATH symlink.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_SRC="${1:-$ROOT/target/release/manga-reel}"
APP_ID_DESKTOP="manga-reel.desktop"

if [[ ! -x "$BIN_SRC" ]]; then
  echo "error: release binary not found at $BIN_SRC" >&2
  echo "Build first: cargo build --release" >&2
  exit 1
fi

mkdir -p "$HOME/.local/bin"
mkdir -p "$HOME/.local/share/applications"
mkdir -p "$HOME/.local/share/icons/hicolor"
mkdir -p "$HOME/.local/share/mime/packages"

# Binary on PATH
ln -sfn "$BIN_SRC" "$HOME/.local/bin/manga-reel"
echo "Linked $HOME/.local/bin/manga-reel -> $BIN_SRC"

# Icons (prefer repo hicolor tree; resize from assets PNG if magick available)
# Restore committed binary assets if missing (icons / sample CBZ)
if [[ ! -f "$ROOT/assets/manga-reel-icon.png" || ! -f "$ROOT/icons/hicolor/256x256/apps/manga-reel.png" ]]; then
  python3 "$ROOT/scripts/bootstrap-assets.py"
fi
ICON_SRC="$ROOT/assets/manga-reel-icon.png"
if [[ -d "$ROOT/icons/hicolor" ]]; then
  cp -a "$ROOT/icons/hicolor/." "$HOME/.local/share/icons/hicolor/"
fi

if [[ -f "$ICON_SRC" ]] && command -v magick >/dev/null 2>&1; then
  for s in 16 32 48 128 256; do
    dest="$HOME/.local/share/icons/hicolor/${s}x${s}/apps"
    mkdir -p "$dest"
    magick "$ICON_SRC" -resize "${s}x${s}" "$dest/manga-reel.png"
  done
elif [[ -f "$ICON_SRC" ]] && command -v convert >/dev/null 2>&1; then
  for s in 16 32 48 128 256; do
    dest="$HOME/.local/share/icons/hicolor/${s}x${s}/apps"
    mkdir -p "$dest"
    convert "$ICON_SRC" -resize "${s}x${s}" "$dest/manga-reel.png"
  done
elif [[ -f "$ICON_SRC" ]]; then
  for s in 16 32 48 128 256; do
    dest="$HOME/.local/share/icons/hicolor/${s}x${s}/apps"
    mkdir -p "$dest"
    cp -f "$ICON_SRC" "$dest/manga-reel.png"
  done
fi

# Desktop entry
sed "s|^Exec=.*|Exec=$HOME/.local/bin/manga-reel %F|" \
  "$ROOT/manga-reel.desktop" > "$HOME/.local/share/applications/$APP_ID_DESKTOP"
# Keep Icon=manga-reel (theme lookup)
chmod 644 "$HOME/.local/share/applications/$APP_ID_DESKTOP"
echo "Installed $HOME/.local/share/applications/$APP_ID_DESKTOP"

# MIME XML (helps some desktops)
cat > "$HOME/.local/share/mime/packages/manga-reel.xml" << 'MIME'
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/vnd.comicbook+zip">
    <comment>Comic book archive (CBZ)</comment>
    <glob pattern="*.cbz"/>
  </mime-type>
  <mime-type type="application/vnd.comicbook-rar">
    <comment>Comic book archive (CBR)</comment>
    <glob pattern="*.cbr"/>
  </mime-type>
  <mime-type type="application/x-cbz">
    <comment>Comic book ZIP</comment>
    <glob pattern="*.cbz"/>
  </mime-type>
  <mime-type type="application/x-cbr">
    <comment>Comic book RAR</comment>
    <glob pattern="*.cbr"/>
  </mime-type>
</mime-info>
MIME

update-mime-database "$HOME/.local/share/mime" 2>/dev/null || true
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

# Default handler
MIMEAPPS="$HOME/.config/mimeapps.list"
mkdir -p "$HOME/.config"
touch "$MIMEAPPS"
for mt in \
  application/vnd.comicbook+zip \
  application/vnd.comicbook-rar \
  application/x-cbz \
  application/x-cbr \
  application/x-cdisplay
do
  xdg-mime default "$APP_ID_DESKTOP" "$mt" 2>/dev/null || true
done

# Ensure mimeapps.list has Default Applications section entries
python3 - << PY
from pathlib import Path
p = Path("$MIMEAPPS")
text = p.read_text() if p.exists() else ""
desktop = "$APP_ID_DESKTOP"
mimes = [
  "application/vnd.comicbook+zip",
  "application/vnd.comicbook-rar",
  "application/x-cbz",
  "application/x-cbr",
  "application/x-cdisplay",
]
if "[Default Applications]" not in text:
    text = text.rstrip() + "\n\n[Default Applications]\n"
lines = text.splitlines()
out = []
in_default = False
seen = set()
for line in lines:
    if line.strip() == "[Default Applications]":
        in_default = True
        out.append(line)
        continue
    if in_default and line.startswith("["):
        for mt in mimes:
            if mt not in seen:
                out.append(f"{mt}={desktop}")
                seen.add(mt)
        in_default = False
        out.append(line)
        continue
    if in_default and "=" in line:
        key = line.split("=",1)[0].strip()
        if key in mimes:
            out.append(f"{key}={desktop}")
            seen.add(key)
            continue
    out.append(line)
if in_default:
    for mt in mimes:
        if mt not in seen:
            out.append(f"{mt}={desktop}")
p.write_text("\n".join(out).rstrip() + "\n")
print(f"Updated {p}")
PY

echo
echo "Done. Launch with: manga-reel"
echo "Or Super+Space → search 'Manga Reel'"
echo "CBZ/CBR double-click should open Manga Reel."
echo "Smoke test: manga-reel $ROOT/testdata/sample-panels.cbz"
