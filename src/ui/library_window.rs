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
        add_folder_btn.set_tooltip_text(Some("Add library folder"));
        let refresh_btn = Button::from_icon_name("view-refresh-symbolic");
        refresh_btn.set_tooltip_text(Some("Refresh library"));
        header.pack_start(&open_btn);
        header.pack_start(&add_folder_btn);
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

        let status = Label::new(Some("Add a folder or open a comic to begin."));
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
                    status.set_text("No comics yet — add a folder or open a CBZ/CBR file.");
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

        {
            let window = window.clone();
            let state = state.clone();
            let refresh = refresh.clone();
            add_folder_btn.connect_clicked(move |_| {
                let dialog = FileDialog::new();
                dialog.set_title("Select library folder");
                let state = state.clone();
                let refresh = refresh.clone();
                dialog.select_folder(
                    Some(&window),
                    None::<&gio::Cancellable>,
                    move |result| {
                        if let Ok(file) = result {
                            if let Some(path) = file.path() {
                                library::add_folder(&mut state.borrow_mut(), path);
                                let _ = library::save_state(&state.borrow());
                                refresh();
                            }
                        }
                    },
                );
            });
        }

        {
            let window = window.clone();
            let app = app.clone();
            let state = state.clone();
            open_btn.connect_clicked(move |_| {
                let dialog = FileDialog::new();
                dialog.set_title("Open comic");
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
                        if let Some(path) = file.path() {
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
                            if let Some(path) = file.path() {
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
