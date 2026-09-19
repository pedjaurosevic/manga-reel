//! Manga Reel — CBZ/CBR comic reader with smooth page pan (GTK4 + libadwaita).

mod archive;
mod detect;
mod library;
mod page;
mod paper;
mod settings;
mod ui;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::Application;
use std::cell::RefCell;
use std::rc::Rc;
use ui::LibraryWindow;

const APP_ID: &str = "app.mangareel.MangaReel";

fn main() {
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    let library: Rc<RefCell<Option<LibraryWindow>>> = Rc::new(RefCell::new(None));
    app.connect_activate({
        let library = library.clone();
        move |app| {
            let mut current = library.borrow_mut();
            if current
                .as_ref()
                .is_none_or(|lib| !app.windows().contains(&lib.window.clone().upcast()))
            {
                *current = Some(LibraryWindow::new(app, Vec::new()));
            }
            current.as_ref().unwrap().present();
        }
    });
    app.connect_open(move |app, files, _hint| {
        let paths = files.iter().filter_map(|f| f.path()).collect();
        let mut current = library.borrow_mut();
        if let Some(lib) = current
            .as_ref()
            .filter(|lib| app.windows().contains(&lib.window.clone().upcast()))
        {
            lib.import_files(paths);
            lib.present();
        } else {
            let lib = LibraryWindow::new(app, paths);
            lib.present();
            *current = Some(lib);
        }
    });

    app.run();
}
