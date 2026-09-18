//! Reader: fullscreen page pan + chrome toggle + seamless strip autoscroll.

use crate::archive::ComicArchive;
use crate::library::{self, ComicProgress, LibraryState};
use crate::settings::{self, FitMode, Letterbox, ReadingOrder, Settings};
use glib::clone;
use gtk4::gdk::{Key, ModifierType};
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, DrawingArea, DropDown, EventControllerKey, GestureClick,
    GestureDrag, Label, Orientation, Overlay, ToggleButton,
};
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow, HeaderBar, ToolbarView, WindowTitle};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

// WIP placeholder — replaced in following commit with full implementation.
pub fn open_reader(
    _app: &Application,
    _archive: ComicArchive,
    _lib_state: Rc<RefCell<LibraryState>>,
    _progress: Option<ComicProgress>,
) {
    eprintln!("manga-reel: reader rewrite in progress");
}
