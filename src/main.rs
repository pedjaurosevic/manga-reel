//! Manga Reel — panel-by-panel CBZ/CBR comic reader (GTK4 + libadwaita).

mod archive;
mod detect;
mod library;
mod page;
mod settings;
mod ui;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use libadwaita::Application;
use std::env;
use std::path::PathBuf;
use ui::LibraryWindow;

const APP_ID: &str = "app.mangareel.MangaReel";

fn main() {
    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_activate(|app| {
        let open_path = env::args()
            .skip(1)
            .find(|a| !a.starts_with('-'))
            .map(PathBuf::from)
            .filter(|p| p.is_file());

        // Single primary library window
        for win in app.windows() {
            if win.title().as_deref() == Some("Manga Reel") {
                if open_path.is_none() {
                    win.present();
                    return;
                }
            }
        }

        let lib = LibraryWindow::new(app, open_path);
        lib.present();
    });

    app.connect_open(|app, files, _hint| {
        let path = files.first().and_then(|f| f.path());
        let lib = LibraryWindow::new(app, path);
        lib.present();
    });

    app.run();
}
