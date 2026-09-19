//! Managed local comic library with a cover grid and sequential background import.
use crate::archive::ComicArchive;
use crate::library::{self, LibraryState};
use crate::ui::reader_window;
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, FileDialog, FileFilter, FlowBox, Label, Orientation, Picture,
    PolicyType, ProgressBar, ScrolledWindow, SelectionMode,
};
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow, HeaderBar, ToolbarView, WindowTitle};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

pub struct LibraryWindow {
    pub window: ApplicationWindow,
    import: Rc<dyn Fn(Vec<PathBuf>)>,
}
impl LibraryWindow {
    pub fn new(app: &Application, open_paths: Vec<PathBuf>) -> Self {
        let state = Rc::new(RefCell::new(library::load_state()));
        let busy = Rc::new(Cell::new(false));
        let importing = Rc::new(Cell::new(false));
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Manga Reel")
            .default_width(1060)
            .default_height(760)
            .build();
        let css = gtk4::CssProvider::new();
        css.load_from_data(".comic-shelf { padding: 14px; } .comic-tile { background: transparent; border: none; padding: 10px; box-shadow: none; } .comic-tile:hover { background: alpha(@accent_bg_color, 0.12); } .comic-cover { background: #17191d; border-radius: 8px; box-shadow: 0 5px 12px alpha(black, 0.3); } .comic-title { font-weight: 600; font-size: 13px; } .shelf-heading { font-size: 27px; font-weight: 800; }");
        gtk4::style_context_add_provider_for_display(
            &gtk4::prelude::WidgetExt::display(&window),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let header = HeaderBar::new();
        header.set_title_widget(Some(&WindowTitle::new("Manga Reel", "Library")));
        let add = Button::with_label("Add Comics");
        add.add_css_class("suggested-action");
        header.pack_end(&add);
        let migrate = Button::with_label("Import Linked Books");
        migrate.set_tooltip_text(Some(
            "Copy previously linked comics into your local library",
        ));
        header.pack_start(&migrate);
        let heading = Label::new(Some("Your collection"));
        heading.add_css_class("shelf-heading");
        heading.set_halign(Align::Start);
        let count = Label::new(None);
        count.add_css_class("dim-label");
        count.set_halign(Align::Start);
        let intro = GtkBox::new(Orientation::Vertical, 5);
        intro.set_margin_start(26);
        intro.set_margin_top(20);
        intro.set_margin_bottom(8);
        intro.append(&heading);
        intro.append(&count);
        let flow = FlowBox::new();
        flow.set_selection_mode(SelectionMode::None);
        flow.set_min_children_per_line(1);
        flow.set_max_children_per_line(8);
        flow.set_homogeneous(true);
        flow.set_column_spacing(8);
        flow.set_row_spacing(14);
        flow.set_valign(Align::Start);
        flow.add_css_class("comic-shelf");
        let empty = Label::new(Some("A home for your comics\n\nChoose Add Comics to import CBZ or CBR files.\nSelect several files with Ctrl or Shift."));
        empty.set_wrap(true);
        empty.set_margin_top(90);
        empty.set_margin_bottom(90);
        let shelf = GtkBox::new(Orientation::Vertical, 0);
        shelf.append(&empty);
        shelf.append(&flow);
        let scroll = ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(PolicyType::Never)
            .child(&shelf)
            .build();
        let status = Label::new(Some(
            "Select a cover to read. Imported books are available offline.",
        ));
        status.set_wrap(true);
        status.set_margin_start(20);
        status.set_margin_end(20);
        status.set_margin_bottom(12);
        let progress = ProgressBar::new();
        progress.set_margin_start(26);
        progress.set_margin_end(26);
        progress.set_visible(false);
        let body = GtkBox::new(Orientation::Vertical, 10);
        body.append(&intro);
        body.append(&scroll);
        body.append(&progress);
        body.append(&status);
        let toolbar = ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&body));
        window.set_content(Some(&toolbar));
        let generation = Rc::new(Cell::new(0u64));
        let cover_busy = Rc::new(Cell::new(false));
        let refresh: Rc<dyn Fn()> = Rc::new({
            let state = state.clone();
            let flow = flow.clone();
            let app = app.clone();
            let status = status.clone();
            let busy = busy.clone();
            let count = count.clone();
            let empty = empty.clone();
            let migrate = migrate.clone();
            let generation = generation.clone();
            let cover_busy = cover_busy.clone();
            move || {
                let current = generation.get().wrapping_add(1);
                generation.set(current);
                while let Some(child) = flow.first_child() {
                    flow.remove(&child);
                }
                let entries = library::entries(&state.borrow());
                empty.set_visible(entries.is_empty());
                let linked = entries
                    .iter()
                    .filter(|e| !library::is_managed(&e.path))
                    .count();
                migrate.set_visible(linked > 0);
                count.set_text(&format!(
                    "{} books · {} stored locally",
                    entries.len(),
                    entries.len() - linked
                ));
                let mut covers = Vec::new();
                for entry in entries {
                    let card = Button::new();
                    card.add_css_class("comic-tile");
                    card.set_width_request(180);
                    card.set_tooltip_text(Some(&entry.title));
                    let content = GtkBox::new(Orientation::Vertical, 10);
                    content.set_halign(Align::Center);
                    let cover = gtk4::Overlay::new();
                    cover.set_size_request(160, 226);
                    cover.add_css_class("comic-cover");
                    cover.set_overflow(gtk4::Overflow::Hidden);
                    let picture = Picture::new();
                    picture.set_content_fit(gtk4::ContentFit::Contain);
                    picture.set_can_shrink(true);
                    picture.set_size_request(160, 226);
                    cover.set_child(Some(&picture));
                    let placeholder = gtk4::Image::from_icon_name("text-x-generic-symbolic");
                    placeholder.set_pixel_size(48);
                    placeholder.add_css_class("dim-label");
                    placeholder.set_halign(Align::Center);
                    placeholder.set_valign(Align::Center);
                    cover.add_overlay(&placeholder);
                    content.append(&cover);
                    let title = Label::new(Some(&entry.title));
                    title.add_css_class("comic-title");
                    title.set_wrap(true);
                    title.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
                    title.set_max_width_chars(21);
                    title.set_width_chars(21);
                    title.set_lines(2);
                    title.set_height_request(38);
                    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                    title.set_justify(gtk4::Justification::Center);
                    content.append(&title);
                    let detail = if let Some(p) = &entry.progress {
                        format!("Continue · page {}", p.page_index + 1)
                    } else if library::is_managed(&entry.path) {
                        "Ready to read".into()
                    } else {
                        "Linked file · import for offline reading".into()
                    };
                    let detail = Label::new(Some(&detail));
                    detail.add_css_class("dim-label");
                    detail.set_max_width_chars(23);
                    detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                    content.append(&detail);
                    card.set_child(Some(&content));
                    let path = entry.path.clone();
                    let app = app.clone();
                    let state = state.clone();
                    let status = status.clone();
                    let busy = busy.clone();
                    card.connect_clicked(move |_| {
                        open_comic(&app, &state, path.clone(), &status, &busy)
                    });
                    flow.insert(&card, -1);
                    if library::is_managed(&entry.path) {
                        covers.push((entry.path, picture, placeholder));
                    }
                }
                // One cover job at a time. Never probe legacy network paths on startup.
                let generation = generation.clone();
                let cover_busy = cover_busy.clone();
                glib::spawn_future_local(async move {
                    for (path, picture, placeholder) in covers {
                        if generation.get() != current {
                            break;
                        }
                        while cover_busy.get() && generation.get() == current {
                            glib::timeout_future(Duration::from_millis(30)).await;
                        }
                        if generation.get() != current {
                            break;
                        }
                        cover_busy.set(true);
                        let result =
                            gio::spawn_blocking(move || library::ensure_cover(&path)).await;
                        cover_busy.set(false);
                        if generation.get() != current {
                            break;
                        }
                        if let Ok(Ok(path)) = result {
                            if let Ok(pixbuf) =
                                gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, 160, 226, true)
                            {
                                let texture = gtk4::gdk::Texture::for_pixbuf(&pixbuf);
                                picture.set_paintable(Some(&texture));
                                placeholder.set_visible(false);
                            }
                        }
                    }
                });
            }
        });
        refresh();
        let import: Rc<dyn Fn(Vec<PathBuf>)> = Rc::new({
            let state = state.clone();
            let status = status.clone();
            let progress = progress.clone();
            let refresh = refresh.clone();
            let importing = importing.clone();
            let add = add.clone();
            let migrate = migrate.clone();
            move |paths| {
                if paths.is_empty() {
                    return;
                }
                if importing.replace(true) {
                    status.set_text(
                        "An import is already running. Please add these files when it finishes.",
                    );
                    return;
                }
                add.set_sensitive(false);
                migrate.set_sensitive(false);
                progress.set_visible(true);
                progress.set_fraction(0.0);
                let state = state.clone();
                let status = status.clone();
                let progress = progress.clone();
                let refresh = refresh.clone();
                let importing = importing.clone();
                let add = add.clone();
                let migrate = migrate.clone();
                glib::spawn_future_local(async move {
                    let total = paths.len();
                    let mut imported = 0;
                    let mut errors = Vec::new();
                    for (index, source) in paths.into_iter().enumerate() {
                        let title = source
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        status.set_text(&format!("Importing {}/{} · {}", index + 1, total, title));
                        let (tx, rx) = std::sync::mpsc::channel();
                        let bar = progress.clone();
                        let tick = glib::timeout_add_local(Duration::from_millis(100), move || {
                            if let Some((done, size)) = rx.try_iter().last() {
                                let part = if size > 0 {
                                    done as f64 / size as f64
                                } else {
                                    0.0
                                };
                                bar.set_fraction((index as f64 + part) / total as f64);
                            }
                            glib::ControlFlow::Continue
                        });
                        let input = source.clone();
                        let result = gio::spawn_blocking(move || {
                            if library::is_managed(&input) {
                                return Ok(input);
                            }
                            library::import_into(&input, &library::books_dir(), |done, size| {
                                let _ = tx.send((done, size));
                            })
                        })
                        .await;
                        tick.remove();
                        match result {
                            Ok(Ok(destination)) => {
                                let mut next = state.borrow().clone();
                                let saved =
                                    library::register_import(&mut next, &source, destination)
                                        .and_then(|_| library::save_state(&next));
                                match saved {
                                    Ok(()) => {
                                        *state.borrow_mut() = next;
                                        imported += 1;
                                        refresh();
                                    }
                                    Err(e) => errors.push(format!("{title}: {e:#}")),
                                }
                            }
                            Ok(Err(e)) => errors.push(format!("{title}: {e:#}")),
                            Err(_) => errors.push(format!("{title}: import worker failed")),
                        }
                        progress.set_fraction((index + 1) as f64 / total as f64);
                    }
                    importing.set(false);
                    add.set_sensitive(true);
                    migrate.set_sensitive(true);
                    progress.set_visible(false);
                    if errors.is_empty() {
                        status.set_text(&format!(
                            "Imported {imported} books · saved locally and ready to read."
                        ));
                    } else {
                        status.set_text(&format!(
                            "Imported {imported} of {total}. {} failed. {}",
                            errors.len(),
                            errors[0]
                        ));
                        status.set_tooltip_text(Some(&errors.join("\n")));
                    }
                });
            }
        });
        migrate.connect_clicked({
            let state = state.clone();
            let import = import.clone();
            move |_| {
                let paths = state
                    .borrow()
                    .files
                    .iter()
                    .filter(|p| !library::is_managed(p))
                    .cloned()
                    .collect();
                import(paths);
            }
        });
        let choose: Rc<dyn Fn()> = Rc::new({
            let window = window.clone();
            let import = import.clone();
            let importing = importing.clone();
            let status = status.clone();
            move || {
                if importing.get() {
                    return;
                }
                let dialog = FileDialog::new();
                dialog.set_title("Import Comics");
                dialog.set_accept_label(Some("Import"));
                let filter = FileFilter::new();
                filter.set_name(Some("Comics (CBZ/CBR)"));
                for suffix in ["cbz", "cbr", "CBZ", "CBR"] {
                    filter.add_pattern(&format!("*.{suffix}"));
                }
                let filters = gio::ListStore::new::<FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));
                let import = import.clone();
                let status = status.clone();
                dialog.open_multiple(Some(&window), None::<&gio::Cancellable>, move |result| {
                if let Ok(files) = result {
                    let mut paths = Vec::new(); let mut missing = 0;
                    for i in 0..files.n_items() {
                        if let Some(file) = files.item(i).and_downcast::<gio::File>() {
                            if let Some(path) = file.path() { paths.push(path); } else { missing += 1; }
                        }
                    }
                    if missing > 0 { status.set_text("Some files have no local path. Mount the network drive in your file manager first."); }
                    import(paths);
                }
            });
            }
        });
        add.connect_clicked({
            let choose = choose.clone();
            move |_| choose()
        });
        let keys = gtk4::EventControllerKey::new();
        keys.connect_key_pressed(move |_, key, _, mods| {
            if mods.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
                && matches!(key, gtk4::gdk::Key::o | gtk4::gdk::Key::O)
            {
                choose();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(keys);
        if !open_paths.is_empty() {
            import(open_paths);
        }
        Self { window, import }
    }
    pub fn import_files(&self, paths: Vec<PathBuf>) {
        (self.import)(paths);
    }
    pub fn present(&self) {
        self.window.present();
    }
}
fn open_comic(
    app: &Application,
    state: &Rc<RefCell<LibraryState>>,
    path: PathBuf,
    status: &Label,
    busy: &Rc<Cell<bool>>,
) {
    if busy.replace(true) {
        status.set_text("Please wait for the selected comic to finish opening.");
        return;
    }
    status.set_text("Opening selected comic…");
    let app = app.clone();
    let state = state.clone();
    let status = status.clone();
    let busy = busy.clone();
    glib::spawn_future_local(async move {
        let key = library::key_for(&path);
        let result = gio::spawn_blocking(move || ComicArchive::open(&path)).await;
        busy.set(false);
        match result {
            Ok(Ok(archive)) => {
                let progress = state.borrow().progress.get(&key).cloned();
                status.set_text("Opened.");
                reader_window::open_reader(&app, archive, state.clone(), progress);
            }
            Ok(Err(e)) => status.set_text(&format!("Could not open comic: {e:#}")),
            Err(_) => status.set_text("Could not open the comic."),
        }
    });
}
