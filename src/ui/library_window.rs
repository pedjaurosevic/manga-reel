//! Library window: folder pick, comic list, open reader.

use crate::archive::ComicArchive;
use crate::library::{self, ComicEntry, LibraryState};
use crate::ui::reader_window;
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, FileDialog, FileFilter, Label, ListBox, ListBoxRow,
    Orientation, PolicyType, ScrolledWindow, SelectionMode,
};
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow, HeaderBar, ToolbarView, WindowTitle};
use std::cell::RefCell;
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

/// Resolve My Passport to a local GVFS path the portal file dialog can open.
/// Portal choosers often ignore sftp:// initial folders and hide network bookmarks.
fn library_browse_root() -> gio::File {
    let remote = gio::File::for_uri(DEFAULT_LIBRARY_URI);
    // Touch the URI so gvfs mounts it (like opening in Nautilus).
    let _ = remote.query_info(
        "standard::name,standard::type",
        gio::FileQueryInfoFlags::NONE,
        None::<&gio::Cancellable>,
    );
    if let Some(path) = remote.path() {
        if path.is_dir() {
            return gio::File::for_path(path);
        }
    }
    // Common fuse layouts if path() is empty briefly after mount.
    if let Ok(uid) = std::env::var("UID") {
        let candidates = [
            format!("/run/user/{uid}/gvfs/sftp:host=stari,user=po/media/po/My Passport"),
            format!("/run/user/{uid}/gvfs/sftp:host=stari,user=po/mnt/passport"),
        ];
        for c in candidates {
            let pb = PathBuf::from(&c);
            if pb.is_dir() {
                return gio::File::for_path(pb);
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
                return gio::File::for_path(pb);
            }
        }
    }
    remote
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
) {
    let root = library_browse_root();
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

    let hint = Label::new(Some("Double-click a folder to open it. Add this folder to library when ready."));
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

    let reload = {
        let list = list.clone();
        let path_label = path_label.clone();
        let current = current.clone();
        Rc::new(move || {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let dir = current.borrow().clone();
            path_label.set_text(&dir.parse_name());
            for (name, child) in list_subdirs(&dir) {
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
        let root_uri = library_browse_root().uri().to_string();
        up_btn.connect_clicked(move |_| {
            let cur = current.borrow().clone();
            if cur.uri().as_str() == root_uri {
                return;
            }
            if let Some(parent) = cur.parent() {
                // Do not leave My Passport root.
                let root = gio::File::for_uri(&root_uri);
                if parent.equal(&root) || parent.uri().starts_with(root.uri().as_str()) || parent.path().and_then(|p| {
                    root.path().map(|r| p.starts_with(r))
                }).unwrap_or(false) {
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
        add_here_btn.connect_clicked(move |_| {
            let file = current.borrow().clone();
            if let Some(path) = file_to_local_path(&file) {
                library::add_folder(&mut state.borrow_mut(), path);
                let _ = library::save_state(&state.borrow());
                refresh();
                browser.close();
            } else {
                eprintln!("manga-reel: cannot resolve {}", file.uri());
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
            .default_width(920)
            .default_height(640)
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

        let list = ListBox::new();
        list.set_selection_mode(SelectionMode::Single);
        list.add_css_class("boxed-list");
        list.set_margin_top(12);
        list.set_margin_bottom(12);
        list.set_margin_start(12);
        list.set_margin_end(12);

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .vexpand(true)
            .child(&list)
            .build();

        let status = Label::new(Some("Add folders from My Passport (stari), or open a CBZ/CBR."));
        status.add_css_class("dim-label");
        status.set_margin_bottom(8);
        status.set_halign(Align::Center);

        let content = GtkBox::new(Orientation::Vertical, 0);
        content.append(&scrolled);
        content.append(&status);

        let toolbar = ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));
        window.set_content(Some(&toolbar));

        let refresh = {
            let state = state.clone();
            let list = list.clone();
            let status = status.clone();
            Rc::new(move || {
                while let Some(child) = list.first_child() {
                    list.remove(&child);
                }
                let entries = library::scan_all(&state.borrow());
                if entries.is_empty() {
                    status.set_text("No comics yet — use My Passport to browse stari like Nautilus.");
                } else {
                    status.set_text(&format!("{} comic(s) in library", entries.len()));
                    for entry in &entries {
                        list.append(&make_row(entry));
                    }
                }
            })
        };
        refresh();

        {
            let refresh = refresh.clone();
            let state = state.clone();
            refresh_btn.connect_clicked(move |_| {
                let _ = library::save_state(&state.borrow());
                refresh();
            });
        }

        let open_library_folder_dialog = {
            let window = window.clone();
            Rc::new(move |title: &str| {
                let root = library_browse_root();
                eprintln!(
                    "manga-reel: library browse root uri={} path={:?}",
                    root.uri(),
                    root.path()
                );
                let dialog = FileDialog::new();
                dialog.set_title(title);
                dialog.set_initial_folder(Some(&root));
                dialog
            })
        };

        {
            let window = window.clone();
            let state = state.clone();
            let refresh = refresh.clone();
            let open_library_folder_dialog = open_library_folder_dialog.clone();
            add_folder_btn.connect_clicked(move |_| {
                let dialog = open_library_folder_dialog("Select library folder");
                let state = state.clone();
                let refresh = refresh.clone();
                dialog.select_folder(
                    Some(&window),
                    None::<&gio::Cancellable>,
                    move |result| {
                        if let Ok(file) = result {
                            if let Some(path) = file_to_local_path(&file) {
                                library::add_folder(&mut state.borrow_mut(), path);
                                let _ = library::save_state(&state.borrow());
                                refresh();
                            } else {
                                eprintln!(
                                    "manga-reel: could not resolve folder path for {}",
                                    file.uri()
                                );
                            }
                        }
                    },
                );
            });
        }

        {
            let window = window.clone();
            let state = state.clone();
            let refresh = refresh.clone();
            passport_btn.connect_clicked(move |_| {
                let refresh_dyn: Rc<dyn Fn()> = Rc::new({
                    let refresh = refresh.clone();
                    move || refresh()
                });
                open_passport_browser(&window, state.clone(), refresh_dyn);
            });
        }

        {
            let window = window.clone();
            let app = app.clone();
            let state = state.clone();
            open_btn.connect_clicked(move |_| {
                let dialog = FileDialog::new();
                dialog.set_title("Open comic");
                dialog.set_initial_folder(Some(&library_browse_root()));
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
                dialog.open(Some(&window), None::<&gio::Cancellable>, move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file_to_local_path(&file) {
                            open_comic(&app, &state, path);
                        }
                    }
                });
            });
        }

        {
            let app = app.clone();
            let state = state.clone();
            list.connect_row_activated(move |_, row| {
                let name = row.widget_name();
                if name.is_empty() {
                    return;
                }
                open_comic(&app, &state, PathBuf::from(name.as_str()));
            });
        }

        // Keyboard: Ctrl+O open
        {
            let window = window.clone();
            let app = app.clone();
            let state = state.clone();
            let controller = gtk4::EventControllerKey::new();
            let window_for_dialog = window.clone();
            controller.connect_key_pressed(move |_, key, _, mods| {
                if mods.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
                    && (key == gtk4::gdk::Key::o || key == gtk4::gdk::Key::O)
                {
                    let dialog = FileDialog::new();
                    dialog.set_title("Open comic");
                    dialog.set_initial_folder(Some(&library_browse_root()));
                    let filter = FileFilter::new();
                    filter.set_name(Some("Comics (CBZ/CBR)"));
                    filter.add_pattern("*.cbz");
                    filter.add_pattern("*.cbr");
                    let filters = gio::ListStore::new::<FileFilter>();
                    filters.append(&filter);
                    dialog.set_filters(Some(&filters));
                    let app = app.clone();
                    let state = state.clone();
                    dialog.open(Some(&window_for_dialog), None::<&gio::Cancellable>, move |result| {
                        if let Ok(file) = result {
                            if let Some(path) = file_to_local_path(&file) {
                                open_comic(&app, &state, path);
                            }
                        }
                    });
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            window.add_controller(controller);
        }

        if let Some(path) = open_path {
            open_comic(app, &state, path);
        }

        Self { window }
    }

    pub fn present(&self) {
        self.window.present();
    }
}

fn make_row(entry: &ComicEntry) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_widget_name(entry.path.to_string_lossy().as_ref());

    let title = Label::new(Some(&entry.title));
    title.set_halign(Align::Start);
    title.set_hexpand(true);
    title.add_css_class("title-3");

    let subtitle = if let Some(p) = &entry.progress {
        format!("Page {} · Panel {}", p.page_index + 1, p.panel_index + 1)
    } else {
        entry.path.display().to_string()
    };
    let sub = Label::new(Some(&subtitle));
    sub.set_halign(Align::Start);
    sub.add_css_class("dim-label");
    sub.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);

    let v = GtkBox::new(Orientation::Vertical, 2);
    v.set_margin_top(8);
    v.set_margin_bottom(8);
    v.set_margin_start(8);
    v.set_margin_end(8);
    v.append(&title);
    v.append(&sub);
    row.set_child(Some(&v));
    row
}

fn open_comic(app: &Application, state: &Rc<RefCell<LibraryState>>, path: PathBuf) {
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
            eprintln!("manga-reel: open failed: {err:#}");
            let toast_win = ApplicationWindow::builder()
                .application(app)
                .title("Manga Reel")
                .default_width(420)
                .default_height(160)
                .build();
            let label = Label::new(Some(&format!("Could not open comic:\n{err:#}")));
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
