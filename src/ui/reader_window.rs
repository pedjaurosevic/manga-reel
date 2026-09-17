//! Reader window: panel-by-panel guided reading + film-strip + vertical.

use crate::archive::ComicArchive;
use crate::detect::{self, PanelRect};
use crate::library::{self, ComicProgress, LibraryState};
use crate::page::{self, PanelCacheFile};
use crate::settings::{self, Letterbox, ReaderMode, ReadingOrder, Settings};
use glib::clone;
use gtk4::gdk::{Key, ModifierType};
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, DrawingArea, DropDown, EventControllerKey, Label, Orientation,
    Overlay, SpinButton, ToggleButton,
};
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow, HeaderBar, ToolbarView, WindowTitle};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;

struct ReaderState {
    archive: ComicArchive,
    cache: PanelCacheFile,
    settings: Settings,
    page_index: usize,
    panel_index: usize,
    /// Current page RGBA
    page_w: u32,
    page_h: u32,
    page_rgba: Vec<u8>,
    edit_mode: bool,
    /// Working panels for current page (editable).
    edit_panels: Vec<PanelRect>,
    drag: Option<DragOp>,
}

#[derive(Clone, Copy)]
enum DragOp {
    New { x0: f64, y0: f64, x1: f64, y1: f64 },
    Move { idx: usize, last_x: f64, last_y: f64 },
    Resize { idx: usize, last_x: f64, last_y: f64 },
}

pub fn open_reader(
    app: &Application,
    archive: ComicArchive,
    lib_state: Rc<RefCell<LibraryState>>,
    progress: Option<ComicProgress>,
) {
    let title = archive
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Reader".into());

    let window = ApplicationWindow::builder()
        .application(app)
        .title(format!("Manga Reel — {title}"))
        .default_width(1100)
        .default_height(800)
        .build();

    let header = HeaderBar::new();
    header.set_title_widget(Some(&WindowTitle::new("Manga Reel", &title)));

    let prev_btn = Button::from_icon_name("go-previous-symbolic");
    let next_btn = Button::from_icon_name("go-next-symbolic");
    prev_btn.set_tooltip_text(Some("Previous panel (Left / A)"));
    next_btn.set_tooltip_text(Some("Next panel (Right / D / Space)"));

    let order_btn = ToggleButton::with_label("RTL");
    order_btn.set_tooltip_text(Some("Toggle LTR / RTL reading order"));
    let letter_btn = ToggleButton::with_label("Black");
    letter_btn.set_tooltip_text(Some("Toggle letterbox black / white"));
    let edit_btn = ToggleButton::with_label("Edit");
    edit_btn.set_tooltip_text(Some("Manual panel edit"));

    let mode_drop = DropDown::from_strings(&["Guided", "Film strip", "Vertical"]);
    mode_drop.set_tooltip_text(Some("Reading mode"));

    let speed = SpinButton::with_range(500.0, 10000.0, 250.0);
    speed.set_tooltip_text(Some("Film-strip interval (ms)"));
    speed.set_value(2500.0);

    let info = Label::new(Some("Loading panels…"));
    info.add_css_class("dim-label");

    header.pack_start(&prev_btn);
    header.pack_start(&next_btn);
    header.pack_end(&edit_btn);
    header.pack_end(&letter_btn);
    header.pack_end(&order_btn);
    header.pack_end(&speed);
    header.pack_end(&mode_drop);

    let area = DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.set_content_width(800);
    area.set_content_height(600);
    area.set_draw_func(|_, _, _, _| {});

    let overlay = Overlay::new();
    overlay.set_child(Some(&area));
    let status_bar = GtkBox::new(Orientation::Horizontal, 8);
    status_bar.set_halign(Align::Center);
    status_bar.set_valign(Align::End);
    status_bar.set_margin_bottom(10);
    status_bar.append(&info);
    overlay.add_overlay(&status_bar);

    let toolbar = ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&overlay));
    window.set_content(Some(&toolbar));

    // Load panels off UI thread
    let (tx, rx) = mpsc::channel::<Result<(PanelCacheFile, Settings), String>>();
    let path = archive.path.clone();
    let page_count = archive.page_count();
    thread::spawn(move || {
        let settings = settings::load();
        match ComicArchive::open(&path).and_then(|a| page::ensure_panels(&a)) {
            Ok(cache) => {
                let _ = tx.send(Ok((cache, settings)));
            }
            Err(e) => {
                let _ = tx.send(Err(format!("{e:#}")));
            }
        }
    });

    let start_page = progress.as_ref().map(|p| p.page_index.min(page_count.saturating_sub(1))).unwrap_or(0);
    let start_panel = progress.as_ref().map(|p| p.panel_index).unwrap_or(0);

    // Placeholder state until cache arrives
    let rs = Rc::new(RefCell::new(None::<ReaderState>));
    let film_source: Rc<Cell<Option<glib::SourceId>>> = Rc::new(Cell::new(None));

    // Poll channel
    let area_c = area.clone();
    let info_c = info.clone();
    let rs_c = rs.clone();
    let order_btn_c = order_btn.clone();
    let letter_btn_c = letter_btn.clone();
    let mode_drop_c = mode_drop.clone();
    let speed_c = speed.clone();
    let archive_for_init = archive;
    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        match rx.try_recv() {
            Ok(Ok((cache, settings))) => {
                order_btn_c.set_active(matches!(settings.reading_order, ReadingOrder::Rtl));
                order_btn_c.set_label(if matches!(settings.reading_order, ReadingOrder::Rtl) {
                    "RTL"
                } else {
                    "LTR"
                });
                letter_btn_c.set_active(matches!(settings.letterbox, Letterbox::White));
                letter_btn_c.set_label(if matches!(settings.letterbox, Letterbox::White) {
                    "White"
                } else {
                    "Black"
                });
                mode_drop_c.set_selected(match settings.mode {
                    ReaderMode::Guided => 0,
                    ReaderMode::FilmStrip => 1,
                    ReaderMode::Vertical => 2,
                });
                speed_c.set_value(settings.film_strip_ms as f64);

                let page_index = start_page.min(cache.pages.len().saturating_sub(1));
                let (page_w, page_h, page_rgba) = match archive_for_init.load_page_rgba(page_index) {
                    Ok(v) => v,
                    Err(e) => {
                        info_c.set_text(&format!("Failed to load page: {e:#}"));
                        return glib::ControlFlow::Break;
                    }
                };
                let mut panels = page::panels_for_page(&cache, page_index);
                let rtl = matches!(settings.reading_order, ReadingOrder::Rtl);
                panels = detect::order_panels(&panels, rtl);
                let panel_index = start_panel.min(panels.len().saturating_sub(1));

                *rs_c.borrow_mut() = Some(ReaderState {
                    archive: archive_for_init.clone(),
                    cache,
                    settings,
                    page_index,
                    panel_index,
                    page_w,
                    page_h,
                    page_rgba,
                    edit_mode: false,
                    edit_panels: panels,
                    drag: None,
                });
                info_c.set_text("Ready");
                area_c.queue_draw();
                update_info(&info_c, &rs_c);
                glib::ControlFlow::Break
            }
            Ok(Err(e)) => {
                info_c.set_text(&format!("Panel detect failed: {e}"));
                // Still try single full-page fallback
                if let Ok((page_w, page_h, page_rgba)) = archive_for_init.load_page_rgba(0) {
                    let settings = settings::load();
                    let panels = vec![PanelRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }];
                    *rs_c.borrow_mut() = Some(ReaderState {
                        archive: archive_for_init.clone(),
                        cache: PanelCacheFile {
                            version: 1,
                            archive_hash: String::new(),
                            pages: vec![],
                        },
                        settings,
                        page_index: 0,
                        panel_index: 0,
                        page_w,
                        page_h,
                        page_rgba,
                        edit_mode: false,
                        edit_panels: panels,
                        drag: None,
                    });
                    area_c.queue_draw();
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });

    // Draw
    {
        let rs = rs.clone();
        area.set_draw_func(move |_, cr, width, height| {
            let borrow = rs.borrow();
            let Some(st) = borrow.as_ref() else {
                cr.set_source_rgb(0.1, 0.1, 0.1);
                cr.paint().ok();
                return;
            };
            let bg = match st.settings.letterbox {
                Letterbox::Black => (0.0, 0.0, 0.0),
                Letterbox::White => (1.0, 1.0, 1.0),
            };
            cr.set_source_rgb(bg.0, bg.1, bg.2);
            cr.paint().ok();

            let panels = &st.edit_panels;
            if panels.is_empty() {
                return;
            }
            let panel = panels[st.panel_index.min(panels.len() - 1)];

            // Vertical mode: show panel with next peek — still crop current
            let src_x = (panel.x * st.page_w as f64).round().max(0.0) as i32;
            let src_y = (panel.y * st.page_h as f64).round().max(0.0) as i32;
            let src_w = (panel.w * st.page_w as f64).round().max(1.0) as i32;
            let src_h = (panel.h * st.page_h as f64).round().max(1.0) as i32;

            if let Some(pixbuf) = rgba_to_pixbuf(&st.page_rgba, st.page_w, st.page_h) {
                let cropped = pixbuf.new_subpixbuf(
                    src_x.clamp(0, st.page_w as i32 - 1),
                    src_y.clamp(0, st.page_h as i32 - 1),
                    src_w.min(st.page_w as i32 - src_x.max(0)).max(1),
                    src_h.min(st.page_h as i32 - src_y.max(0)).max(1),
                );

                let cw = cropped.width() as f64;
                let ch = cropped.height() as f64;
                let scale = (width as f64 / cw).min(height as f64 / ch);
                let dw = cw * scale;
                let dh = ch * scale;
                let dx = (width as f64 - dw) / 2.0;
                let dy = (height as f64 - dh) / 2.0;

                cr.save().ok();
                cr.translate(dx, dy);
                cr.scale(scale, scale);
                cr.set_source_pixbuf(&cropped, 0.0, 0.0);
                cr.paint().ok();
                cr.restore().ok();

                if st.edit_mode {
                    // Draw all panel outlines in page space mapped to widget
                    // Map full page into letterboxed fit
                    let ps = (width as f64 / st.page_w as f64).min(height as f64 / st.page_h as f64);
                    let pw = st.page_w as f64 * ps;
                    let ph = st.page_h as f64 * ps;
                    let px = (width as f64 - pw) / 2.0;
                    let py = (height as f64 - ph) / 2.0;
                    // Show full page underneath outlines in edit mode
                    cr.set_source_rgb(bg.0, bg.1, bg.2);
                    cr.paint().ok();
                    cr.save().ok();
                    cr.translate(px, py);
                    cr.scale(ps, ps);
                    if let Some(full) = rgba_to_pixbuf(&st.page_rgba, st.page_w, st.page_h) {
                        cr.set_source_pixbuf(&full, 0.0, 0.0);
                        cr.paint().ok();
                    }
                    for (i, p) in st.edit_panels.iter().enumerate() {
                        let x = p.x * st.page_w as f64;
                        let y = p.y * st.page_h as f64;
                        let w = p.w * st.page_w as f64;
                        let h = p.h * st.page_h as f64;
                        if i == st.panel_index {
                            cr.set_source_rgba(0.2, 0.7, 1.0, 0.9);
                            cr.set_line_width(3.0 / ps);
                        } else {
                            cr.set_source_rgba(1.0, 0.85, 0.2, 0.8);
                            cr.set_line_width(2.0 / ps);
                        }
                        cr.rectangle(x, y, w, h);
                        cr.stroke().ok();
                    }
                    if let Some(DragOp::New { x0, y0, x1, y1 }) = st.drag {
                        cr.set_source_rgba(0.3, 1.0, 0.4, 0.9);
                        cr.set_line_width(2.0 / ps);
                        cr.rectangle(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
                        cr.stroke().ok();
                    }
                    cr.restore().ok();
                }
            }
        });
    }

    let redraw = {
        let area = area.clone();
        let info = info.clone();
        let rs = rs.clone();
        Rc::new(move || {
            update_info(&info, &rs);
            area.queue_draw();
        })
    };

    let save_progress = {
        let rs = rs.clone();
        let lib_state = lib_state.clone();
        Rc::new(move || {
            if let Some(st) = rs.borrow().as_ref() {
                library::set_progress(
                    &mut lib_state.borrow_mut(),
                    &st.archive.path,
                    st.page_index,
                    st.panel_index,
                );
                let _ = library::save_state(&lib_state.borrow());
                let _ = settings::save(&st.settings);
            }
        })
    };

    let step = {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let save_progress = save_progress.clone();
        Rc::new(move |delta: i32| {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return };
            if st.edit_mode {
                return;
            }
            let rtl = matches!(st.settings.reading_order, ReadingOrder::Rtl);
            let mut panels = page::panels_for_page(&st.cache, st.page_index);
            panels = detect::order_panels(&panels, rtl);
            st.edit_panels = panels.clone();

            let len = st.edit_panels.len().max(1);
            let next = st.panel_index as i32 + delta;
            if next < 0 {
                if st.page_index == 0 {
                    return;
                }
                // prev page
                let new_page = st.page_index - 1;
                if let Ok((w, h, rgba)) = st.archive.load_page_rgba(new_page) {
                    st.page_index = new_page;
                    st.page_w = w;
                    st.page_h = h;
                    st.page_rgba = rgba;
                    let mut panels = page::panels_for_page(&st.cache, new_page);
                    panels = detect::order_panels(&panels, rtl);
                    st.edit_panels = panels;
                    st.panel_index = st.edit_panels.len().saturating_sub(1);
                }
            } else if next as usize >= len {
                if st.page_index + 1 >= st.archive.page_count() {
                    return;
                }
                let new_page = st.page_index + 1;
                if let Ok((w, h, rgba)) = st.archive.load_page_rgba(new_page) {
                    st.page_index = new_page;
                    st.page_w = w;
                    st.page_h = h;
                    st.page_rgba = rgba;
                    let mut panels = page::panels_for_page(&st.cache, new_page);
                    panels = detect::order_panels(&panels, rtl);
                    st.edit_panels = panels;
                    st.panel_index = 0;
                }
            } else {
                st.panel_index = next as usize;
            }
            drop(borrow);
            save_progress();
            redraw();
        })
    };

    prev_btn.connect_clicked(clone!(@strong step => move |_| step(-1)));
    next_btn.connect_clicked(clone!(@strong step => move |_| step(1)));

    {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let save_progress = save_progress.clone();
        order_btn.connect_toggled(move |btn| {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return };
            st.settings.reading_order = if btn.is_active() {
                btn.set_label("RTL");
                ReadingOrder::Rtl
            } else {
                btn.set_label("LTR");
                ReadingOrder::Ltr
            };
            let rtl = matches!(st.settings.reading_order, ReadingOrder::Rtl);
            st.edit_panels = detect::order_panels(
                &page::panels_for_page(&st.cache, st.page_index),
                rtl,
            );
            st.panel_index = st.panel_index.min(st.edit_panels.len().saturating_sub(1));
            drop(borrow);
            save_progress();
            redraw();
        });
    }

    {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let save_progress = save_progress.clone();
        letter_btn.connect_toggled(move |btn| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                if btn.is_active() {
                    btn.set_label("White");
                    st.settings.letterbox = Letterbox::White;
                } else {
                    btn.set_label("Black");
                    st.settings.letterbox = Letterbox::Black;
                }
            }
            save_progress();
            redraw();
        });
    }

    {
        let rs = rs.clone();
        let redraw = redraw.clone();
        edit_btn.connect_toggled(move |btn| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.edit_mode = btn.is_active();
                if !st.edit_mode {
                    // Persist edits into cache
                    if let Some(page) = st.cache.pages.iter_mut().find(|p| p.page_index == st.page_index) {
                        page.panels = st.edit_panels.clone();
                    } else {
                        st.cache.pages.push(page::PagePanels {
                            page_index: st.page_index,
                            width: st.page_w,
                            height: st.page_h,
                            panels: st.edit_panels.clone(),
                        });
                    }
                    let _ = page::save_panel_cache(&st.archive.path, &st.cache, true);
                }
            }
            redraw();
        });
    }

    // Mode + film strip timer
    {
        let rs = rs.clone();
        let film_source = film_source.clone();
        let step = step.clone();
        let speed = speed.clone();
        let save_progress = save_progress.clone();
        mode_drop.connect_selected_notify(move |drop| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.settings.mode = match drop.selected() {
                    1 => ReaderMode::FilmStrip,
                    2 => ReaderMode::Vertical,
                    _ => ReaderMode::Guided,
                };
            }
            if let Some(id) = film_source.take() {
                id.remove();
            }
            let mode = rs.borrow().as_ref().map(|s| s.settings.mode).unwrap_or_default();
            if mode == ReaderMode::FilmStrip {
                let ms = speed.value().max(500.0) as u64;
                let step = step.clone();
                let id = glib::timeout_add_local(std::time::Duration::from_millis(ms), move || {
                    step(1);
                    glib::ControlFlow::Continue
                });
                film_source.set(Some(id));
            }
            save_progress();
        });
    }

    {
        let rs = rs.clone();
        let save_progress = save_progress.clone();
        speed.connect_value_changed(move |sp| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.settings.film_strip_ms = sp.value() as u32;
            }
            save_progress();
        });
    }

    // Mouse for edit mode
    {
        let rs = rs.clone();
        let area2 = area.clone();
        let click = gtk4::GestureClick::new();
        click.set_button(1);
        click.connect_pressed(clone!(@strong rs, @strong area2 => move |_, _, x, y| {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return };
            if !st.edit_mode { return; }
            let Some((px, py, ps)) = page_map(area2.width(), area2.height(), st.page_w, st.page_h) else { return };
            let ix = (x - px) / ps;
            let iy = (y - py) / ps;
            // Hit-test existing panel (resize corner vs move)
            let mut hit = None;
            for (i, p) in st.edit_panels.iter().enumerate() {
                let x0 = p.x * st.page_w as f64;
                let y0 = p.y * st.page_h as f64;
                let x1 = x0 + p.w * st.page_w as f64;
                let y1 = y0 + p.h * st.page_h as f64;
                if ix >= x0 && ix <= x1 && iy >= y0 && iy <= y1 {
                    let near_br = (x1 - ix).abs() < 12.0 && (y1 - iy).abs() < 12.0;
                    hit = Some((i, near_br));
                    break;
                }
            }
            st.drag = match hit {
                Some((idx, true)) => Some(DragOp::Resize { idx, last_x: ix, last_y: iy }),
                Some((idx, false)) => {
                    st.panel_index = idx;
                    Some(DragOp::Move { idx, last_x: ix, last_y: iy })
                }
                None => Some(DragOp::New { x0: ix, y0: iy, x1: ix, y1: iy }),
            };
        }));
        click.connect_released(clone!(@strong rs, @strong area2 => move |_, _, _, _| {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return };
            if let Some(DragOp::New { x0, y0, x1, y1 }) = st.drag.take() {
                let nx0 = x0.min(x1).max(0.0);
                let ny0 = y0.min(y1).max(0.0);
                let nx1 = x0.max(x1).min(st.page_w as f64);
                let ny1 = y0.max(y1).min(st.page_h as f64);
                let w = nx1 - nx0;
                let h = ny1 - ny0;
                if w > 8.0 && h > 8.0 {
                    st.edit_panels.push(PanelRect {
                        x: nx0 / st.page_w as f64,
                        y: ny0 / st.page_h as f64,
                        w: w / st.page_w as f64,
                        h: h / st.page_h as f64,
                    });
                    st.panel_index = st.edit_panels.len() - 1;
                }
            }
            area2.queue_draw();
        }));
        area.add_controller(click);

        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion(clone!(@strong rs, @strong area2 => move |_, x, y| {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return };
            if !st.edit_mode { return; }
            let Some((px, py, ps)) = page_map(area2.width(), area2.height(), st.page_w, st.page_h) else { return };
            let ix = (x - px) / ps;
            let iy = (y - py) / ps;
            match st.drag {
                Some(DragOp::New { x0, y0, .. }) => {
                    st.drag = Some(DragOp::New { x0, y0, x1: ix, y1: iy });
                    area2.queue_draw();
                }
                Some(DragOp::Move { idx, last_x, last_y }) => {
                    if let Some(p) = st.edit_panels.get_mut(idx) {
                        let dx = (ix - last_x) / st.page_w as f64;
                        let dy = (iy - last_y) / st.page_h as f64;
                        p.x = (p.x + dx).clamp(0.0, 1.0 - p.w);
                        p.y = (p.y + dy).clamp(0.0, 1.0 - p.h);
                    }
                    st.drag = Some(DragOp::Move { idx, last_x: ix, last_y: iy });
                    area2.queue_draw();
                }
                Some(DragOp::Resize { idx, last_x, last_y }) => {
                    if let Some(p) = st.edit_panels.get_mut(idx) {
                        let dx = (ix - last_x) / st.page_w as f64;
                        let dy = (iy - last_y) / st.page_h as f64;
                        p.w = (p.w + dx).clamp(0.02, 1.0 - p.x);
                        p.h = (p.h + dy).clamp(0.02, 1.0 - p.y);
                    }
                    st.drag = Some(DragOp::Resize { idx, last_x: ix, last_y: iy });
                    area2.queue_draw();
                }
                None => {}
            }
        }));
        area.add_controller(motion);
    }

    // Keys
    {
        let step = step.clone();
        let rs = rs.clone();
        let redraw = redraw.clone();
        let controller = EventControllerKey::new();
        controller.connect_key_pressed(move |_, key, _, mods| {
            // Delete panel in edit mode
            if key == Key::Delete || key == Key::BackSpace {
                let mut borrow = rs.borrow_mut();
                if let Some(st) = borrow.as_mut() {
                    if st.edit_mode && !st.edit_panels.is_empty() {
                        st.edit_panels.remove(st.panel_index.min(st.edit_panels.len() - 1));
                        if st.edit_panels.is_empty() {
                            st.edit_panels.push(PanelRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 });
                        }
                        st.panel_index = st.panel_index.min(st.edit_panels.len() - 1);
                        drop(borrow);
                        redraw();
                        return glib::Propagation::Stop;
                    }
                }
            }
            if mods.contains(ModifierType::CONTROL_MASK) {
                return glib::Propagation::Proceed;
            }
            match key {
                Key::Right | Key::space | Key::d | Key::D | Key::Page_Down | Key::Down | Key::j | Key::J => {
                    step(1);
                    glib::Propagation::Stop
                }
                Key::Left | Key::a | Key::A | Key::Page_Up | Key::Up | Key::k | Key::K | Key::BackSpace => {
                    step(-1);
                    glib::Propagation::Stop
                }
                Key::Escape => {
                    if let Some(st) = rs.borrow_mut().as_mut() {
                        st.edit_mode = false;
                    }
                    redraw();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        window.add_controller(controller);
    }

    // Scroll wheel → next/prev (vertical mode especially)
    {
        let step = step.clone();
        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(move |_, _dx, dy| {
            if dy > 0.0 {
                step(1);
            } else if dy < 0.0 {
                step(-1);
            }
            glib::Propagation::Stop
        });
        area.add_controller(scroll);
    }

    window.connect_close_request(clone!(@strong save_progress, @strong film_source => move |_| {
        save_progress();
        if let Some(id) = film_source.take() {
            id.remove();
        }
        glib::Propagation::Proceed
    }));

    window.present();
}


fn update_info(info: &Label, rs: &Rc<RefCell<Option<ReaderState>>>) {
    let borrow = rs.borrow();
    let Some(st) = borrow.as_ref() else {
        info.set_text("Loading…");
        return;
    };
    let mode = match st.settings.mode {
        ReaderMode::Guided => "Guided",
        ReaderMode::FilmStrip => "Film strip",
        ReaderMode::Vertical => "Vertical",
    };
    info.set_text(&format!(
        "Page {}/{} · Panel {}/{} · {} · {}",
        st.page_index + 1,
        st.archive.page_count(),
        st.panel_index + 1,
        st.edit_panels.len().max(1),
        mode,
        if matches!(st.settings.reading_order, ReadingOrder::Rtl) { "RTL" } else { "LTR" }
    ));
}

fn rgba_to_pixbuf(rgba: &[u8], w: u32, h: u32) -> Option<Pixbuf> {
    Some(Pixbuf::from_bytes(
        &glib::Bytes::from(rgba),
        gtk4::gdk_pixbuf::Colorspace::Rgb,
        true,
        8,
        w as i32,
        h as i32,
        (w * 4) as i32,
    ))
}

fn page_map(widget_w: i32, widget_h: i32, page_w: u32, page_h: u32) -> Option<(f64, f64, f64)> {
    if widget_w <= 0 || widget_h <= 0 || page_w == 0 || page_h == 0 {
        return None;
    }
    let ps = (widget_w as f64 / page_w as f64).min(widget_h as f64 / page_h as f64);
    let pw = page_w as f64 * ps;
    let ph = page_h as f64 * ps;
    let px = (widget_w as f64 - pw) / 2.0;
    let py = (widget_h as f64 - ph) / 2.0;
    Some((px, py, ps))
}
