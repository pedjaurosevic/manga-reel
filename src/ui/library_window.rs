//! Library window: folder pick, cover grid, open reader.

use crate::archive::ComicArchive;
use crate::library::{self, ComicEntry, LibraryState};
use crate::ui::reader_window;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, FileDialog, FileFilter, FlowBox, FlowBoxChild, Frame, Label,
    ListBox, ListBoxRow, Orientation, PolicyType, Picture, ScrolledWindow, SelectionMode,
};
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow, HeaderBar, ToolbarView, WindowTitle};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

/// Primary comics disk on stari (same place Nautilus shows as My Passport).
const DEFAULT_LIBRARY_URI: &str = "sftp://po@stari/media/po/My%20Passport";


fn file_to_local_path(file: &gio::File) -> Option<PathBuf> {
    if let Some(path) = file.path() {
        return Some(path);
    }
    let _ = file.query_info(
        "standard::name",
        gio::FileQueryInfoFlags::NONE,
        None::<&gio::Cancellable>,
    );
    file.path()
}

/// Returns Ok(root) when My Passport is reachable, Err(message) when unmounted/offline.
fn library_browse_root() -> Result<gio::File, String> {
    let remote = gio::File::for_uri(DEFAULT_LIBRARY_URI);
    let q = remote.query_info(
        "standard::name,standard::type",
        gio::FileQueryInfoFlags::NONE,
        None::<&gio::Cancellable>,
    );
    if let Err(e) = q {
        return Err(format!(
            "My Passport unreachable ({e}). Mount the disk on stari or open a local CBZ/CBR."
        ));
    }
    if let Some(path) = remote.path() {
        if path.is_dir() {
            return Ok(gio::File::for_path(path));
        }
    }
    if let Ok(uid) = std::env::var("UID") {
        let candidates = [
            format!("/run/user/{uid}/gvfs/sftp:host=stari,user=po/media/po/My Passport"),
            format!("/run/user/{uid}/gvfs/sftp:host=stari,user=po/mnt/passport"),
        ];
        for c in candidates {
            let pb = PathBuf::from(&c);
            if pb.is_dir() {
                return Ok(gio::File::for_path(pb));
            }
        }
    }
    if let Ok(uid) = std::process::Command::new("id").arg("-u").output() {
        let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
        for suffix in [
            "gvfs/sftp:host=stari,user=po/media/po/My Passport",
            "gvfs/sftp:host=stari,user=po/mnt/passport",
        ] {
            let pb = PathBuf::from(format!("/run/user/{uid}/{suffix}"));
            if pb.is_dir() {
                return Ok(gio::File::for_path(pb));
            }
        }
    }
    // URI object exists but may still fail later — surface a clear status.
    if remote.path().is_none() {
        return Err(
            "My Passport not mounted (no local GVFS path). Open Nautilus → stari, or add a local folder."
                .into(),
        );
    }
    Ok(remote)
}

fn list_subdirs(dir: &gio::File) -> Vec<(String, gio::File)> {
    let mut out = Vec::new();
    let Ok(enumerator) = dir.enumerate_children(
        "standard::name,standard::type,standard::display-name",
        gio::FileQueryInfoFlags::NONE,
        None::<&gio::Cancellable>,
    ) else {
        return out;
    };
    while let Some(info) = enumerator.next_file(None::<&gio::Cancellable>).ok().flatten() {
        if info.file_type() != gio::FileType::Directory {
            continue;
        }
        let name = info.display_name().to_string();
        if name.starts_with('.') {
            continue;
        }
        let child = dir.child(info.name());
        out.push((name, child));
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

/// In-app folder browser for My Passport — portal dialogs often omit network mounts.
fn open_passport_browser(
    parent: &ApplicationWindow,
    state: Rc<RefCell<LibraryState>>,
    refresh: Rc<dyn Fn()>,
    status: Label,
) {
    let root = match library_browse_root() {
        Ok(f) => f,
        Err(msg) => {
            status.set_text(&msg);
            return;
        }
    };
    let browser = ApplicationWindow::builder()
        .transient_for(parent)
        .modal(true)
        .title("My Passport")
        .default_width(560)
        .default_height(640)
        .build();

    let header = HeaderBar::new();
    header.set_title_widget(Some(&WindowTitle::new("My Passport", "stari")));
    let up_btn = Button::from_icon_name("go-up-symbolic");
    up_btn.set_tooltip_text(Some("Parent folder"));
    let add_here_btn = Button::with_label("Add this folder");
    add_here_btn.add_css_class("suggested-action");
    header.pack_start(&up_btn);
    header.pack_end(&add_here_btn);

    let path_label = Label::new(Some(&root.parse_name()));
    path_label.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
    path_label.set_halign(Align::Start);
    path_label.set_margin_start(12);
    path_label.set_margin_end(12);
    path_label.set_margin_top(8);
    path_label.add_css_class("dim-label");

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.set_margin_top(8);
    list.set_margin_bottom(12);
    list.set_margin_start(12);
    list.set_margin_end(12);

    let scrolled = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .vexpand(true)
        .child(&list)
        .build();

    let hint = Label::new(Some(
        "Double-click a folder to open it. Add this folder to library when ready.",
    ));
    hint.add_css_class("dim-label");
    hint.set_margin_bottom(10);
    hint.set_halign(Align::Center);

    let body = GtkBox::new(Orientation::Vertical, 0);
    body.append(&path_label);
    body.append(&scrolled);
    body.append(&hint);

    let toolbar = ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&body));
    browser.set_content(Some(&toolbar));

    let current = Rc::new(RefCell::new(root));
    let root_uri = current.borrow().uri().to_string();

    let reload = {
        let list = list.clone();
        let path_label = path_label.clone();
        let current = current.clone();
        let status = status.clone();
        Rc::new(move || {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let dir = current.borrow().clone();
            path_label.set_text(&dir.parse_name());
            let kids = list_subdirs(&dir);
            if kids.is_empty() {
                status.set_text("Folder empty or unreadable — is My Passport still mounted?");
            }
            for (name, child) in kids {
                let row = ListBoxRow::new();
                row.set_widget_name(&child.uri());
                let label = Label::new(Some(&name));
                label.set_halign(Align::Start);
                label.set_margin_top(10);
                label.set_margin_bottom(10);
                label.set_margin_start(10);
                label.set_margin_end(10);
                row.set_child(Some(&label));
                list.append(&row);
            }
        })
    };
    reload();

    {
        let current = current.clone();
        let reload = reload.clone();
        let root_uri = root_uri.clone();
        up_btn.connect_clicked(move |_| {
            let cur = current.borrow().clone();
            if cur.uri().as_str() == root_uri {
                return;
            }
            if let Some(parent) = cur.parent() {
                let root = gio::File::for_uri(&root_uri);
                if parent.equal(&root)
                    || parent.uri().starts_with(root.uri().as_str())
                    || parent
                        .path()
                        .and_then(|p| root.path().map(|r| p.starts_with(r)))
                        .unwrap_or(false)
                {
                    *current.borrow_mut() = parent;
                    reload();
                } else {
                    *current.borrow_mut() = root;
                    reload();
                }
            }
        });
    }

    {
        let current = current.clone();
        let reload = reload.clone();
        list.connect_row_activated(move |_, row| {
            let uri = row.widget_name();
            if uri.is_empty() {
                return;
            }
            *current.borrow_mut() = gio::File::for_uri(uri.as_str());
            reload();
        });
    }

    {
        let current = current.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let browser = browser.clone();
        let status = status.clone();
        add_here_btn.connect_clicked(move |_| {
            let file = current.borrow().clone();
            if let Some(path) = file_to_local_path(&file) {
                library::add_folder(&mut state.borrow_mut(), path);
                let _ = library::save_state(&state.borrow());
                refresh();
                browser.close();
            } else {
                status.set_text(&format!("Cannot resolve {}", file.uri()));
            }
        });
    }

    browser.present();
}

pub struct LibraryWindow {
    pub window: ApplicationWindow,
}

impl LibraryWindow {
    pub fn new(app: &Application, open_path: Option<PathBuf>) -> Self {
        let _ = library::ensure_data_dirs();
        let state = Rc::new(RefCell::new(library::load_state()));

        let window = ApplicationWindow::builder()
            .application(app)
            .title("Manga Reel")
            .default_width(1040)
            .default_height(720)
            .build();

        let header = HeaderBar::new();
        header.set_title_widget(Some(&WindowTitle::new("Manga Reel", "Library")));

        let open_btn = Button::from_icon_name("document-open-symbolic");
        open_btn.set_tooltip_text(Some("Open CBZ/CBR"));
        let add_folder_btn = Button::from_icon_name("folder-new-symbolic");
        add_folder_btn.set_tooltip_text(Some("Add library folder (starts inside My Passport)"));
        let passport_btn = Button::with_label("My Passport");
        passport_btn.set_tooltip_text(Some("Browse My Passport on stari (same as Nautilus)"));
        let refresh_btn = Button::from_icon_name("view-refresh-symbolic");
        refresh_btn.set_tooltip_text(Some("Refresh library"));
        header.pack_start(&open_btn);
        header.pack_start(&add_folder_btn);
        header.pack_start(&passport_btn);
        header.pack_end(&refresh_btn);

        let flow = FlowBox::new();
        flow.set_valign(Align::Start);
        flow.set_max_children_per_line(8);
        flow.set_min_children_per_line(2);
        flow.set_selection_mode(SelectionMode::None);
        flow.set_homogeneous(true);
        flow.set_column_spacing(16);
        flow.set_row_spacing(16);
        flow.set_margin_top(16);
        flow.set_margin_bottom(16);
        flow.set_margin_start(16);
        flow.set_margin_end(16);
        flow.add_css_class("manga-reel-library");

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .child(&flow)
            .build();

        let status = Label::new(Some(
            "Add folders from My Passport (stari), or open a CBZ/CBR.",
        ));
        status.add_css_class("dim-label");
        status.set_margin_bottom(10);
        status.set_margin_top(4);
        status.set_halign(Align::Center);
        status.set_wrap(true);

        let content = GtkBox::new(Orientation::Vertical, 0);
        content.append(&scrolled);
        content.append(&status);

        let toolbar = ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));
        window.set_content(Some(&toolbar));


        // Generation so stale background scans are ignored after a new Refresh.
        let scan_gen = Rc::new(Cell::new(0u64));

        let refresh = {
            let state = state.clone();
            let flow = flow.clone();
            let status = status.clone();
            let app = app.clone();
            let scan_gen = scan_gen.clone();
            Rc::new(move || {
                // Paint immediately — clear grid, show scanning, never WalkDir/unrar here.
                while let Some(child) = flow.first_child() {
                    flow.remove(&child);
                }
                status.set_text("Skeniram biblioteku…");
                let gen = scan_gen.get().wrapping_add(1);
                scan_gen.set(gen);

                let state_snap = state.borrow().clone();
                let state_ui = state.clone();
                let flow_ui = flow.clone();
                let status_ui = status.clone();
                let app_ui = app.clone();
                let scan_gen_ui = scan_gen.clone();

                glib::spawn_future_local(async move {
                    let opts = library::ScanOptions::default();
                    let result = gio::spawn_blocking(move || {
                        library::scan_snapshot(state_snap, opts)
                    })
                    .await;
                    let Ok((new_state, snap)) = result else {
                        status_ui.set_text("Skeniranje nije uspelo.");
                        return;
                    };
                    if scan_gen_ui.get() != gen {
                        return; // superseded
                    }
                    if snap.recovered > 0 {
                        *state_ui.borrow_mut() = new_state;
                        let _ = library::save_state(&state_ui.borrow());
                    } else {
                        state_ui.borrow_mut().folders = new_state.folders;
                    }

                    while let Some(child) = flow_ui.first_child() {
                        flow_ui.remove(&child);
                    }

                    let mut bits: Vec<String> = Vec::new();
                    if snap.fuse == library::GvfsFuseStatus::Missing {
                        bits.push(
                            "gvfsd-fuse nije aktivan — pokreni: /usr/lib/gvfsd-fuse /run/user/$(id -u)/gvfs -f"
                                .into(),
                        );
                    }
                    if !snap.offline.is_empty() {
                        let mut labels: Vec<String> = snap
                            .offline
                            .iter()
                            .map(|p| library::folder_label(p))
                            .collect::<HashSet<_>>()
                            .into_iter()
                            .collect();
                        labels.sort();
                        bits.push(format!(
                            "{} folder(a) offline ({})",
                            snap.offline.len(),
                            labels.join(", ")
                        ));
                    }
                    if snap.entries.is_empty() {
                        if snap.online.is_empty() && snap.folder_count > 0 {
                            bits.push(
                                "Nema dostupnih foldera — montiraj My Passport pa Refresh."
                                    .into(),
                            );
                        } else {
                            bits.push(
                                "Nema stripova — My Passport → dodaj folder (npr. !STRIPOVI)."
                                    .into(),
                            );
                        }
                        status_ui.set_text(&bits.join(" · "));
                    } else {
                        let mut head = format!("{} strip(ova)", snap.entries.len());
                        if snap.truncated {
                            head.push_str(&format!(
                                " (prikazano prvih {}; ostalo na Refresh / uži folder)",
                                library::DEFAULT_SCAN_CAP
                            ));
                        }
                        bits.insert(0, head);
                        status_ui.set_text(&bits.join(" · "));
                        for entry in &snap.entries {
                            flow_ui.append(&make_cover_card(entry, &app_ui, &state_ui));
                        }
                    }
                });
            })
        };
        // Kick off async scan AFTER widgets exist; window presents immediately.
        refresh();

        {
            let refresh = refresh.clone();
            let state = state.clone();
            refresh_btn.connect_clicked(move |_| {
                let _ = library::save_state(&state.borrow());
                refresh();
            });
        }

        {
            let window = window.clone();
            let state = state.clone();
            let refresh = refresh.clone();
            let status = status.clone();
            add_folder_btn.connect_clicked(move |_| {
                let root = match library_browse_root() {
                    Ok(f) => f,
                    Err(msg) => {
                        status.set_text(&msg);
                        return;
                    }
                };
                let dialog = FileDialog::new();
                dialog.set_title("Select library folder");
                dialog.set_initial_folder(Some(&root));
                let state = state.clone();
                let refresh = refresh.clone();
                let status = status.clone();
                dialog.select_folder(Some(&window), None::<&gio::Cancellable>, move |result| {
                    match result {
                        Ok(file) => {
                            if let Some(path) = file_to_local_path(&file) {
                                library::add_folder(&mut state.borrow_mut(), path);
                                let _ = library::save_state(&state.borrow());
                                refresh();
                            } else {
                                status.set_text(&format!(
                                    "Could not resolve folder path for {}",
                                    file.uri()
                                ));
                            }
                        }
                        Err(_) => {}
                    }
                });
            });
        }

        {
            let window = window.clone();
            let state = state.clone();
            let refresh = refresh.clone();
            let status = status.clone();
            passport_btn.connect_clicked(move |_| {
                let refresh_dyn: Rc<dyn Fn()> = Rc::new({
                    let refresh = refresh.clone();
                    move || refresh()
                });
                open_passport_browser(&window, state.clone(), refresh_dyn, status.clone());
            });
        }

        {
            let window = window.clone();
            let app = app.clone();
            let state = state.clone();
            let status = status.clone();
            open_btn.connect_clicked(move |_| {
                let dialog = FileDialog::new();
                dialog.set_title("Open comic");
                if let Ok(root) = library_browse_root() {
                    dialog.set_initial_folder(Some(&root));
                }
                let filter = FileFilter::new();
                filter.set_name(Some("Comics (CBZ/CBR)"));
                filter.add_pattern("*.cbz");
                filter.add_pattern("*.cbr");
                filter.add_pattern("*.CBZ");
                filter.add_pattern("*.CBR");
                let filters = gio::ListStore::new::<FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));
                let app = app.clone();
                let state = state.clone();
                let status = status.clone();
                dialog.open(Some(&window), None::<&gio::Cancellable>, move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file_to_local_path(&file) {
                            open_comic(&app, &state, path, Some(&status));
                        }
                    }
                });
            });
        }

        {
            let window = window.clone();
            let app = app.clone();
            let state = state.clone();
            let status = status.clone();
            let controller = gtk4::EventControllerKey::new();
            let window_for_dialog = window.clone();
            controller.connect_key_pressed(move |_, key, _, mods| {
                if mods.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
                    && (key == gtk4::gdk::Key::o || key == gtk4::gdk::Key::O)
                {
                    let dialog = FileDialog::new();
                    dialog.set_title("Open comic");
                    if let Ok(root) = library_browse_root() {
                        dialog.set_initial_folder(Some(&root));
                    }
                    let filter = FileFilter::new();
                    filter.set_name(Some("Comics (CBZ/CBR)"));
                    filter.add_pattern("*.cbz");
                    filter.add_pattern("*.cbr");
                    let filters = gio::ListStore::new::<FileFilter>();
                    filters.append(&filter);
                    dialog.set_filters(Some(&filters));
                    let app = app.clone();
                    let state = state.clone();
                    let status = status.clone();
                    dialog.open(
                        Some(&window_for_dialog),
                        None::<&gio::Cancellable>,
                        move |result| {
                            if let Ok(file) = result {
                                if let Some(path) = file_to_local_path(&file) {
                                    open_comic(&app, &state, path, Some(&status));
                                }
                            }
                        },
                    );
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            window.add_controller(controller);
        }

        if let Some(path) = open_path {
            open_comic(app, &state, path, Some(&status));
        }

        Self { window }
    }

    pub fn present(&self) {
        self.window.present();
    }
}

/// Load cover on a worker thread; never call unrar on the GTK main thread.
/// Ensure a PNG cover thumb exists under cache. Runs off the UI thread.
/// Returns the cache path when ready (main thread loads Pixbuf from it).
fn ensure_cover_cache(path: &std::path::Path) -> Option<PathBuf> {
    let cache = library::cover_cache_path(path);
    if cache.is_file() && cache.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Some(cache);
    }
    let archive = ComicArchive::open(path).ok()?;
    let bytes = archive.cover_bytes().ok()?;
    let img = image::load_from_memory(&bytes).ok()?.into_rgba8();
    let (w, h) = (img.width(), img.height());
    let target_h = 220u32;
    let scale = target_h as f32 / h.max(1) as f32;
    let tw = ((w as f32) * scale).round().max(40.0) as u32;
    let th = target_h;
    let resized = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
    if let Some(parent) = cache.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    image::DynamicImage::ImageRgba8(resized)
        .save(&cache)
        .ok()?;
    Some(cache)
}

fn make_cover_card(
    entry: &ComicEntry,
    app: &Application,
    state: &Rc<RefCell<LibraryState>>,
) -> FlowBoxChild {
    let child = FlowBoxChild::new();
    child.set_widget_name(entry.path.to_string_lossy().as_ref());

    let frame = Frame::new(None);
    frame.add_css_class("card");
    frame.set_hexpand(false);
    frame.set_width_request(160);

    let v = GtkBox::new(Orientation::Vertical, 8);
    v.set_margin_top(10);
    v.set_margin_bottom(12);
    v.set_margin_start(10);
    v.set_margin_end(10);
    v.set_halign(Align::Center);

    // Placeholder immediately — covers load async so remote CBR cannot freeze UI.
    let picture = Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk4::ContentFit::Contain);
    picture.set_height_request(220);
    picture.set_width_request(140);
    let placeholder = Label::new(Some("📕"));
    placeholder.set_margin_top(80);
    placeholder.set_margin_bottom(80);
    placeholder.set_height_request(220);
    placeholder.add_css_class("title-1");
    let cover_box = GtkBox::new(Orientation::Vertical, 0);
    cover_box.set_height_request(220);
    cover_box.append(&placeholder);
    cover_box.append(&picture);
    picture.set_visible(false);
    v.append(&cover_box);

    let path_for_cover = entry.path.clone();
    let picture_w = picture.clone();
    let placeholder_w = placeholder.clone();
    glib::spawn_future_local(async move {
        let cached = gio::spawn_blocking(move || ensure_cover_cache(&path_for_cover))
            .await
            .ok()
            .flatten();
        if let Some(cache_path) = cached {
            if let Ok(pb) = Pixbuf::from_file(&cache_path) {
                picture_w.set_pixbuf(Some(&pb));
                picture_w.set_visible(true);
                placeholder_w.set_visible(false);
            }
        }
    });

    let title = Label::new(Some(&entry.title));
    title.set_halign(Align::Center);
    title.set_justify(gtk4::Justification::Center);
    title.set_wrap(true);
    title.set_max_width_chars(18);
    title.set_lines(2);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    v.append(&title);

    frame.set_child(Some(&v));
    child.set_child(Some(&frame));

    let app = app.clone();
    let state = state.clone();
    let path = entry.path.clone();
    let click = gtk4::GestureClick::new();
    click.connect_released(move |_, n, _, _| {
        if n == 1 {
            open_comic(&app, &state, path.clone(), None);
        }
    });
    child.add_controller(click);

    child
}

fn open_comic(
    app: &Application,
    state: &Rc<RefCell<LibraryState>>,
    path: PathBuf,
    status: Option<&Label>,
) {
    if crate::archive::is_remote_or_gvfs(&path) {
        if let Some(s) = status {
            s.set_text("Kopiram sa starog (SFTP)…");
        }
        let app = app.clone();
        let state = state.clone();
        let status_opt = status.cloned();
        let path_bg = path.clone();
        glib::spawn_future_local(async move {
            let key = library::key_for(&path_bg);
            let result = gio::spawn_blocking(move || ComicArchive::open_with_status(&path_bg, |_| {}))
                .await;
            match result {
                Ok(Ok(archive)) => {
                    let progress = state.borrow().progress.get(&key).cloned();
                    if let Some(s) = &status_opt {
                        s.set_text("Otvoreno.");
                    }
                    reader_window::open_reader(&app, archive, state.clone(), progress);
                }
                Ok(Err(err)) => {
                    let msg = format!("Ne mogu da otvorim: {err:#}");
                    if let Some(s) = &status_opt {
                        s.set_text(&msg);
                    }
                    eprintln!("manga-reel: open failed: {err:#}");
                    let toast_win = ApplicationWindow::builder()
                        .application(&app)
                        .title("Manga Reel")
                        .default_width(420)
                        .default_height(160)
                        .build();
                    let label = Label::new(Some(&msg));
                    label.set_wrap(true);
                    label.set_margin_top(24);
                    label.set_margin_bottom(24);
                    label.set_margin_start(24);
                    label.set_margin_end(24);
                    toast_win.set_content(Some(&label));
                    toast_win.present();
                }
                Err(_) => {
                    if let Some(s) = &status_opt {
                        s.set_text("Otvaranje prekinuto.");
                    }
                }
            }
        });
        return;
    }

    if let Some(s) = status {
        s.set_text("Otvaram…");
    }
    match ComicArchive::open(&path) {
        Ok(archive) => {
            let progress = state
                .borrow()
                .progress
                .get(&library::key_for(&path))
                .cloned();
            reader_window::open_reader(app, archive, state.clone(), progress);
        }
        Err(err) => {
            let msg = format!("Ne mogu da otvorim: {err:#}");
            if let Some(s) = status {
                s.set_text(&msg);
            }
            eprintln!("manga-reel: open failed: {err:#}");
            let toast_win = ApplicationWindow::builder()
                .application(app)
                .title("Manga Reel")
                .default_width(420)
                .default_height(160)
                .build();
            let label = Label::new(Some(&msg));
            label.set_wrap(true);
            label.set_margin_top(24);
            label.set_margin_bottom(24);
            label.set_margin_start(24);
            label.set_margin_end(24);
            toast_win.set_content(Some(&label));
            toast_win.present();
        }
    }
}
