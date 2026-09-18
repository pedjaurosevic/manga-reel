//! Reader: fullscreen page view with smooth pan (no panel mode).

use crate::archive::ComicArchive;
use crate::library::{self, ComicProgress, LibraryState};
use crate::settings::{self, FitMode, Letterbox, ReadingOrder, Settings};
use glib::clone;
use gtk4::gdk::{Key, ModifierType};
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, DrawingArea, DropDown, EventControllerKey, GestureDrag, Label,
    Orientation, Overlay, ToggleButton,
};
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow, HeaderBar, ToolbarView, WindowTitle};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct ReaderState {
    archive: ComicArchive,
    settings: Settings,
    page_index: usize,
    page_w: u32,
    page_h: u32,
    page_rgba: Vec<u8>,
    /// Pan offsets in page-pixels after scale (viewport space).
    pan_x: f64,
    pan_y: f64,
    /// Animated pan target.
    target_x: f64,
    target_y: f64,
    drag_last: Option<(f64, f64)>,
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
    prev_btn.set_tooltip_text(Some("Previous page"));
    next_btn.set_tooltip_text(Some("Next page"));

    let order_btn = ToggleButton::with_label("LTR");
    order_btn.set_tooltip_text(Some("Page-turn direction LTR / RTL"));
    let letter_btn = ToggleButton::with_label("Black");
    letter_btn.set_tooltip_text(Some("Letterbox black / white"));
    let fit_drop = DropDown::from_strings(&["Fit width", "Fit height", "Fit page"]);
    fit_drop.set_tooltip_text(Some("How the page fills the screen"));

    let info = Label::new(Some("Loading…"));
    info.add_css_class("dim-label");

    header.pack_start(&prev_btn);
    header.pack_start(&next_btn);
    header.pack_end(&letter_btn);
    header.pack_end(&order_btn);
    header.pack_end(&fit_drop);

    let area = DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.set_content_width(800);
    area.set_content_height(600);
    area.set_draw_func(|_, _, _, _| {});
    area.set_can_focus(true);

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

    let sync_chrome = Rc::new({
        let toolbar = toolbar.clone();
        let header = header.clone();
        let status_bar = status_bar.clone();
        let window = window.clone();
        move || {
            let fs = window.is_fullscreen();
            toolbar.set_reveal_top_bars(!fs);
            header.set_visible(!fs);
            status_bar.set_visible(!fs);
        }
    });
    sync_chrome();
    {
        let sync_chrome = sync_chrome.clone();
        window.connect_fullscreened_notify(move |_| sync_chrome());
    }

    let settings = settings::load();
    order_btn.set_active(matches!(settings.reading_order, ReadingOrder::Rtl));
    order_btn.set_label(if matches!(settings.reading_order, ReadingOrder::Rtl) {
        "RTL"
    } else {
        "LTR"
    });
    letter_btn.set_active(matches!(settings.letterbox, Letterbox::White));
    letter_btn.set_label(if matches!(settings.letterbox, Letterbox::White) {
        "White"
    } else {
        "Black"
    });
    fit_drop.set_selected(match settings.fit {
        FitMode::Width => 0,
        FitMode::Height => 1,
        FitMode::Contain => 2,
    });

    let page_count = archive.page_count();
    let start_page = progress
        .as_ref()
        .map(|p| p.page_index.min(page_count.saturating_sub(1)))
        .unwrap_or(0);

    let (page_w, page_h, page_rgba) = match archive.load_page_rgba(start_page) {
        Ok(v) => v,
        Err(e) => {
            info.set_text(&format!("Failed to load: {e:#}"));
            window.present();
            return;
        }
    };

    let rs = Rc::new(RefCell::new(Some(ReaderState {
        archive,
        settings,
        page_index: start_page,
        page_w,
        page_h,
        page_rgba,
        pan_x: 0.0,
        pan_y: 0.0,
        target_x: 0.0,
        target_y: 0.0,
        drag_last: None,
    })));

    let animating = Rc::new(Cell::new(false));

    let layout = |st: &ReaderState, vw: f64, vh: f64| -> (f64, f64, f64, f64, f64) {
        // returns scale, draw_w, draw_h, max_pan_x, max_pan_y
        let pw = st.page_w as f64;
        let ph = st.page_h as f64;
        let scale = match st.settings.fit {
            FitMode::Width => vw / pw,
            FitMode::Height => vh / ph,
            FitMode::Contain => (vw / pw).min(vh / ph),
        };
        let dw = pw * scale;
        let dh = ph * scale;
        let max_x = (dw - vw).max(0.0);
        let max_y = (dh - vh).max(0.0);
        (scale, dw, dh, max_x, max_y)
    };

    // Draw full page with pan
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

            let vw = width as f64;
            let vh = height as f64;
            let (scale, dw, dh, max_x, max_y) = layout(st, vw, vh);
            let pan_x = st.pan_x.clamp(0.0, max_x);
            let pan_y = st.pan_y.clamp(0.0, max_y);
            // Center when page smaller than viewport
            let ox = if dw <= vw {
                (vw - dw) / 2.0
            } else {
                -pan_x
            };
            let oy = if dh <= vh {
                (vh - dh) / 2.0
            } else {
                -pan_y
            };

            if let Some(pixbuf) = rgba_to_pixbuf(&st.page_rgba, st.page_w, st.page_h) {
                cr.save().ok();
                cr.translate(ox, oy);
                cr.scale(scale, scale);
                cr.set_source_pixbuf(&pixbuf, 0.0, 0.0);
                cr.paint().ok();
                cr.restore().ok();
            }
        });
    }

    let update_info = {
        let info = info.clone();
        let rs = rs.clone();
        Rc::new(move || {
            let borrow = rs.borrow();
            let Some(st) = borrow.as_ref() else {
                info.set_text("…");
                return;
            };
            let fit = match st.settings.fit {
                FitMode::Width => "width",
                FitMode::Height => "height",
                FitMode::Contain => "page",
            };
            info.set_text(&format!(
                "Page {}/{} · fit {} · arrows pan · edges turn page",
                st.page_index + 1,
                st.archive.page_count(),
                fit
            ));
        })
    };

    let redraw = {
        let area = area.clone();
        let update_info = update_info.clone();
        Rc::new(move || {
            update_info();
            area.queue_draw();
        })
    };

    let save_progress = {
        let rs = rs.clone();
        let lib_state = lib_state.clone();
        Rc::new(move || {
            if let Some(st) = rs.borrow().as_ref() {
                library::set_progress(&mut lib_state.borrow_mut(), &st.archive.path, st.page_index, 0);
                let _ = library::save_state(&lib_state.borrow());
                let _ = settings::save(&st.settings);
            }
        })
    };

    let clamp_pan = {
        let rs = rs.clone();
        let area = area.clone();
        Rc::new(move || {
            let alloc = area.allocation();
            let vw = alloc.width() as f64;
            let vh = alloc.height() as f64;
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return };
            let (_, _, _, max_x, max_y) = layout(st, vw, vh);
            st.pan_x = st.pan_x.clamp(0.0, max_x);
            st.pan_y = st.pan_y.clamp(0.0, max_y);
            st.target_x = st.target_x.clamp(0.0, max_x);
            st.target_y = st.target_y.clamp(0.0, max_y);
        })
    };

    let goto_page = {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let save_progress = save_progress.clone();
        let clamp_pan = clamp_pan.clone();
        Rc::new(move |index: usize| {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return };
            if index >= st.archive.page_count() {
                return;
            }
            if let Ok((w, h, rgba)) = st.archive.load_page_rgba(index) {
                st.page_index = index;
                st.page_w = w;
                st.page_h = h;
                st.page_rgba = rgba;
                st.pan_x = 0.0;
                st.pan_y = 0.0;
                st.target_x = 0.0;
                st.target_y = 0.0;
                drop(borrow);
                clamp_pan();
                save_progress();
                redraw();
            }
        })
    };

    // Smooth animation toward target pan
    let tick_anim = {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let animating = animating.clone();
        Rc::new(move || {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else {
                animating.set(false);
                return;
            };
            let dx = st.target_x - st.pan_x;
            let dy = st.target_y - st.pan_y;
            if dx.abs() < 0.5 && dy.abs() < 0.5 {
                st.pan_x = st.target_x;
                st.pan_y = st.target_y;
                animating.set(false);
                drop(borrow);
                redraw();
                return;
            }
            // Ease: move ~22% of remaining each frame (~smooth)
            st.pan_x += dx * 0.22;
            st.pan_y += dy * 0.22;
            drop(borrow);
            redraw();
        })
    };

    let ensure_anim = {
        let animating = animating.clone();
        let tick_anim = tick_anim.clone();
        Rc::new(move || {
            if animating.get() {
                return;
            }
            animating.set(true);
            let tick_anim = tick_anim.clone();
            let animating = animating.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
                tick_anim();
                if animating.get() {
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            });
        })
    };

    /// Pan by delta in viewport pixels. Returns true if pan applied, false if at edge (caller may turn page).
    let try_pan = {
        let rs = rs.clone();
        let area = area.clone();
        let ensure_anim = ensure_anim.clone();
        let clamp_pan = clamp_pan.clone();
        Rc::new(move |dx: f64, dy: f64| -> bool {
            let alloc = area.allocation();
            let vw = alloc.width() as f64;
            let vh = alloc.height() as f64;
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return false };
            let (_, _, _, max_x, max_y) = layout(st, vw, vh);
            let before_x = st.target_x;
            let before_y = st.target_y;
            st.target_x = (st.target_x + dx).clamp(0.0, max_x);
            st.target_y = (st.target_y + dy).clamp(0.0, max_y);
            let moved = (st.target_x - before_x).abs() > 0.5 || (st.target_y - before_y).abs() > 0.5;
            drop(borrow);
            if moved {
                clamp_pan();
                ensure_anim();
            }
            moved
        })
    };

    let turn_or_pan = {
        let rs = rs.clone();
        let try_pan = try_pan.clone();
        let goto_page = goto_page.clone();
        let area = area.clone();
        Rc::new(move |dir_x: i32, dir_y: i32| {
            // dir: -1 left/up, +1 right/down
            let alloc = area.allocation();
            let vw = alloc.width() as f64;
            let vh = alloc.height() as f64;
            let step = {
                let borrow = rs.borrow();
                let st = borrow.as_ref();
                st.map(|s| s.settings.pan_step).unwrap_or(0.18)
            };
            let dx = dir_x as f64 * vw * step;
            let dy = dir_y as f64 * vh * step;
            if try_pan(dx, dy) {
                return;
            }
            // At edge → turn page (respect RTL for horizontal)
            let rtl = {
                let borrow = rs.borrow();
                borrow
                    .as_ref()
                    .map(|s| matches!(s.settings.reading_order, ReadingOrder::Rtl))
                    .unwrap_or(false)
            };
            let (page_delta, _) = if dir_x != 0 {
                let d = if rtl { -dir_x } else { dir_x };
                (d, 0)
            } else {
                (dir_y, 0)
            };
            let next = {
                let borrow = rs.borrow();
                let Some(st) = borrow.as_ref() else { return };
                st.page_index as i32 + page_delta
            };
            if next >= 0 {
                goto_page(next as usize);
            }
        })
    };

    prev_btn.connect_clicked(clone!(@strong goto_page, @strong rs => move |_| {
        let i = rs.borrow().as_ref().map(|s| s.page_index.saturating_sub(1)).unwrap_or(0);
        goto_page(i);
    }));
    next_btn.connect_clicked(clone!(@strong goto_page, @strong rs => move |_| {
        let i = rs.borrow().as_ref().map(|s| s.page_index + 1).unwrap_or(0);
        goto_page(i);
    }));

    {
        let rs = rs.clone();
        let save_progress = save_progress.clone();
        order_btn.connect_toggled(move |btn| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.settings.reading_order = if btn.is_active() {
                    btn.set_label("RTL");
                    ReadingOrder::Rtl
                } else {
                    btn.set_label("LTR");
                    ReadingOrder::Ltr
                };
            }
            save_progress();
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
        let save_progress = save_progress.clone();
        let clamp_pan = clamp_pan.clone();
        fit_drop.connect_selected_notify(move |drop| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.settings.fit = match drop.selected() {
                    1 => FitMode::Height,
                    2 => FitMode::Contain,
                    _ => FitMode::Width,
                };
                st.pan_x = 0.0;
                st.pan_y = 0.0;
                st.target_x = 0.0;
                st.target_y = 0.0;
            }
            clamp_pan();
            save_progress();
            redraw();
        });
    }

    // Drag to pan
    {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let clamp_pan = clamp_pan.clone();
        let drag = GestureDrag::new();
        drag.connect_drag_begin(clone!(@strong rs => move |_, _, _| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.drag_last = Some((st.pan_x, st.pan_y));
                st.target_x = st.pan_x;
                st.target_y = st.pan_y;
            }
        }));
        drag.connect_drag_update(clone!(@strong rs, @strong redraw, @strong clamp_pan => move |g, _, _| {
            let Some((dx, dy)) = g.offset() else { return };
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return };
            let Some((ox, oy)) = st.drag_last else { return };
            st.pan_x = ox - dx;
            st.pan_y = oy - dy;
            st.target_x = st.pan_x;
            st.target_y = st.pan_y;
            drop(borrow);
            clamp_pan();
            redraw();
        }));
        area.add_controller(drag);
    }

    // Keys
    {
        let turn_or_pan = turn_or_pan.clone();
        let window_keys = window.clone();
        let controller = EventControllerKey::new();
        controller.connect_key_pressed(move |_, key, _, mods| {
            if mods.contains(ModifierType::CONTROL_MASK) {
                return glib::Propagation::Proceed;
            }
            match key {
                Key::Right | Key::d | Key::D | Key::l | Key::L => {
                    turn_or_pan(1, 0);
                    glib::Propagation::Stop
                }
                Key::Left | Key::a | Key::A | Key::h | Key::H => {
                    turn_or_pan(-1, 0);
                    glib::Propagation::Stop
                }
                Key::Down | Key::j | Key::J | Key::space | Key::Page_Down => {
                    turn_or_pan(0, 1);
                    glib::Propagation::Stop
                }
                Key::Up | Key::k | Key::K | Key::Page_Up | Key::BackSpace => {
                    turn_or_pan(0, -1);
                    glib::Propagation::Stop
                }
                Key::F11 => {
                    if window_keys.is_fullscreen() {
                        window_keys.unfullscreen();
                    } else {
                        window_keys.fullscreen();
                    }
                    glib::Propagation::Stop
                }
                Key::Escape => {
                    if window_keys.is_fullscreen() {
                        window_keys.unfullscreen();
                        return glib::Propagation::Stop;
                    }
                    glib::Propagation::Proceed
                }
                _ => glib::Propagation::Proceed,
            }
        });
        window.add_controller(controller);
    }

    // Scroll wheel pans (smooth-ish via try_pan)
    {
        let try_pan = try_pan.clone();
        let turn_or_pan = turn_or_pan.clone();
        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(move |_, _dx, dy| {
            let amount = dy * 80.0;
            if !try_pan(0.0, amount) {
                if dy > 0.0 {
                    turn_or_pan(0, 1);
                } else if dy < 0.0 {
                    turn_or_pan(0, -1);
                }
            }
            glib::Propagation::Stop
        });
        area.add_controller(scroll);
    }

    window.connect_close_request(clone!(@strong save_progress => move |_| {
        save_progress();
        glib::Propagation::Proceed
    }));

    update_info();
    window.present();
    window.fullscreen();
    sync_chrome();
    area.grab_focus();
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
