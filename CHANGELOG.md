# Changelog

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
