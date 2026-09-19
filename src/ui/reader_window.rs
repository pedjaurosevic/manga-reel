// Reader: fullscreen page pan + chrome toggle + seamless strip autoscroll.

use crate::archive::ComicArchive;
use crate::detect::{self, PanelRect};
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

struct PageImage { w: u32, h: u32, rgba: Vec<u8> }

struct ReaderState {
    archive: ComicArchive,
    settings: Settings,
    page_index: usize,
    current: PageImage,
    next: Option<PageImage>,
    prev: Option<PageImage>,
    pan_x: f64,
    pan_y: f64,
    target_x: f64,
    target_y: f64,
    drag_last: Option<(f64, f64)>,
    autoscroll: bool,
    /// When true, draw/navigate cropped panels instead of page strip.
    panel_mode: bool,
    panel_index: usize,
    panels: Vec<PanelRect>,
    /// Cached detections: page_index -> ordered panels
    panel_cache: std::collections::HashMap<usize, Vec<PanelRect>>,
}

const AUTOSCROLL_PPS_MIN: f64 = 12.0;
const AUTOSCROLL_PPS_MAX: f64 = 240.0;
const AUTOSCROLL_PPS_STEP: f64 = 12.0;

fn page_layout(fit: FitMode, pw: f64, ph: f64, vw: f64, vh: f64) -> (f64, f64, f64) {
    let scale = match fit {
        FitMode::Width => vw / pw,
        FitMode::Height => vh / ph,
        FitMode::Contain => (vw / pw).min(vh / ph),
    };
    (scale, pw * scale, ph * scale)
}

fn load_image(archive: &ComicArchive, index: usize) -> Option<PageImage> {
    archive.load_page_rgba(index).ok().map(|(w, h, mut rgba)| {
        // Bake Real Paper 2 into the page so paper and ink scroll as one sheet.
        crate::paper::apply_real_paper_2(&mut rgba, w, h);
        PageImage { w, h, rgba }
    })
}


fn paint_page(cr: &gtk4::cairo::Context, img: &PageImage, scale: f64, ox: f64, oy: f64) {
    if let Some(pixbuf) = rgba_to_pixbuf(&img.rgba, img.w, img.h) {
        cr.save().ok();
        cr.translate(ox, oy);
        cr.scale(scale, scale);
        cr.set_source_pixbuf(&pixbuf, 0.0, 0.0);
        cr.paint().ok();
        cr.restore().ok();
    }
}


fn detect_ordered_panels(img: &PageImage, rtl: bool) -> Vec<PanelRect> {
    let raw = detect::detect_panels(&img.rgba, img.w, img.h);
    detect::order_panels(&raw, rtl)
}

fn ensure_page_panels(st: &mut ReaderState) {
    let rtl = matches!(st.settings.reading_order, ReadingOrder::Rtl);
    if let Some(cached) = st.panel_cache.get(&st.page_index) {
        st.panels = cached.clone();
        if st.panel_index >= st.panels.len() {
            st.panel_index = st.panels.len().saturating_sub(1);
        }
        return;
    }
    let panels = detect_ordered_panels(&st.current, rtl);
    st.panel_cache.insert(st.page_index, panels.clone());
    st.panels = panels;
    if st.panel_index >= st.panels.len() {
        st.panel_index = st.panels.len().saturating_sub(1);
    }
}

/// Pick panel index from current vertical pan (page view) so entering Panel
/// starts at the "next" frame in the active strip.
fn panel_index_from_pan(st: &ReaderState, vw: f64, vh: f64) -> usize {
    if st.panels.is_empty() {
        return 0;
    }
    let (_, _, dh) = page_layout(st.settings.fit, st.current.w as f64, st.current.h as f64, vw, vh);
    if dh <= 1.0 {
        return 0;
    }
    let mid_norm = ((st.pan_y + vh * 0.35) / dh).clamp(0.0, 0.999);
    for (i, p) in st.panels.iter().enumerate() {
        let cy = p.y + p.h * 0.5;
        if cy + 0.02 >= mid_norm {
            return i;
        }
    }
    st.panels.len().saturating_sub(1)
}

fn paint_panel_full_height(cr: &gtk4::cairo::Context, img: &PageImage, panel: &PanelRect, vw: f64, vh: f64) {
    // Always black side bars in panel mode (per product spec).
    cr.set_source_rgb(0.0, 0.0, 0.0);
    cr.paint().ok();
    let pw = (panel.w * img.w as f64).max(1.0);
    let ph = (panel.h * img.h as f64).max(1.0);
    let px = (panel.x * img.w as f64).max(0.0);
    let py = (panel.y * img.h as f64).max(0.0);
    let scale = vh / ph;
    let dw = pw * scale;
    let dh = vh;
    let ox = (vw - dw) / 2.0;
    let oy = 0.0;
    if let Some(full) = rgba_to_pixbuf(&img.rgba, img.w, img.h) {
        let sx = px.round().clamp(0.0, (img.w as f64 - 1.0).max(0.0)) as i32;
        let sy = py.round().clamp(0.0, (img.h as f64 - 1.0).max(0.0)) as i32;
        let sw = pw.round().clamp(1.0, (img.w as i32 - sx).max(1) as f64) as i32;
        let sh = ph.round().clamp(1.0, (img.h as i32 - sy).max(1) as f64) as i32;
        let sub = full.new_subpixbuf(sx, sy, sw, sh);
        cr.save().ok();
        cr.translate(ox, oy);
        cr.scale(scale, scale);
        cr.set_source_pixbuf(&sub, 0.0, 0.0);
        cr.paint().ok();
        cr.restore().ok();
        let _ = (dw, dh);
    }
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

    // Hyprland screen shaders fight page-locked paper; disable while reading.
    let saved_shader = Rc::new(RefCell::new(crate::paper::suspend_hyprland_shader()));

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
    let auto_btn = ToggleButton::with_label("Auto");
    auto_btn.set_tooltip_text(Some("Smooth continuous autoscroll (speed ↓/↑)"));
    let manual_btn = ToggleButton::with_label("Manual");
    manual_btn.set_tooltip_text(Some("Manual: Space jumps down by 1/3 of the screen"));
    let panel_btn = ToggleButton::with_label("Panel");
    panel_btn.set_tooltip_text(Some("Panel mode — full-height frames; toggle returns to page pan"));

    let speed_box = GtkBox::new(Orientation::Horizontal, 2);
    speed_box.set_tooltip_text(Some("Autoscroll speed (only while Auto is on)"));
    let slower_btn = Button::from_icon_name("go-down-symbolic");
    slower_btn.set_tooltip_text(Some("Slower autoscroll (−)"));
    let faster_btn = Button::from_icon_name("go-up-symbolic");
    faster_btn.set_tooltip_text(Some("Faster autoscroll (+)"));
    let speed_label = Label::new(Some("48 px/s"));
    speed_label.add_css_class("dim-label");
    speed_label.set_width_chars(8);
    speed_box.append(&slower_btn);
    speed_box.append(&speed_label);
    speed_box.append(&faster_btn);
    speed_box.set_visible(false);

    let fit_drop = DropDown::from_strings(&["Fit width", "Fit height", "Fit page"]);
    fit_drop.set_tooltip_text(Some("How the page fills the screen"));

    let info = Label::new(Some("Loading…"));
    info.add_css_class("dim-label");

    header.pack_start(&prev_btn);
    header.pack_start(&next_btn);
    header.pack_end(&letter_btn);
    header.pack_end(&order_btn);
    header.pack_end(&speed_box);
    header.pack_end(&panel_btn);
    header.pack_end(&manual_btn);
    header.pack_end(&auto_btn);
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

    let chrome_visible = Rc::new(Cell::new(false));
    let sync_chrome = Rc::new({
        let toolbar = toolbar.clone();
        let header = header.clone();
        let status_bar = status_bar.clone();
        let chrome_visible = chrome_visible.clone();
        move || {
            let show = chrome_visible.get();
            toolbar.set_reveal_top_bars(show);
            header.set_visible(show);
            status_bar.set_visible(show);
        }
    });
    sync_chrome();
    {
        let chrome_visible = chrome_visible.clone();
        let sync_chrome = sync_chrome.clone();
        window.connect_fullscreened_notify(move |win| {
            if !win.is_fullscreen() {
                chrome_visible.set(true);
            }
            sync_chrome();
        });
    }

    let settings = settings::load();
    order_btn.set_active(matches!(settings.reading_order, ReadingOrder::Rtl));
    order_btn.set_label(if matches!(settings.reading_order, ReadingOrder::Rtl) { "RTL" } else { "LTR" });
    letter_btn.set_active(matches!(settings.letterbox, Letterbox::White));
    letter_btn.set_label(if matches!(settings.letterbox, Letterbox::White) { "White" } else { "Black" });
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

    let Some(current) = load_image(&archive, start_page) else {
        info.set_text("Failed to load page");
        window.present();
        return;
    };
    let next = if start_page + 1 < page_count { load_image(&archive, start_page + 1) } else { None };
    let prev = if start_page > 0 { load_image(&archive, start_page - 1) } else { None };

    let rs = Rc::new(RefCell::new(Some(ReaderState {
        archive,
        settings,
        page_index: start_page,
        current,
        next,
        prev,
        pan_x: 0.0,
        pan_y: 0.0,
        target_x: 0.0,
        target_y: 0.0,
        drag_last: None,
        autoscroll: false,
        panel_mode: false,
        panel_index: 0,
        panels: Vec::new(),
        panel_cache: std::collections::HashMap::new(),
    })));

    let animating = Rc::new(Cell::new(false));
    let autoscroll_on = Rc::new(Cell::new(false));
    let manual_on = Rc::new(Cell::new(false));
    let panel_mode_on = Rc::new(Cell::new(false));

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
            let vw = width as f64;
            let vh = height as f64;
            if st.panel_mode {
                if let Some(panel) = st.panels.get(st.panel_index) {
                    paint_panel_full_height(cr, &st.current, panel, vw, vh);
                } else {
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                    cr.paint().ok();
                }
                return;
            }
            cr.set_source_rgb(bg.0, bg.1, bg.2);
            cr.paint().ok();
            let fit = st.settings.fit;
            let (scale, dw, dh) = page_layout(fit, st.current.w as f64, st.current.h as f64, vw, vh);
            let max_x = (dw - vw).max(0.0);
            let pan_x = st.pan_x.clamp(0.0, max_x);
            let ox = if dw <= vw { (vw - dw) / 2.0 } else { -pan_x };
            let oy = -st.pan_y;
            paint_page(cr, &st.current, scale, ox, oy);
            if let Some(next) = st.next.as_ref() {
                let (nscale, ndw, _) = page_layout(fit, next.w as f64, next.h as f64, vw, vh);
                let nox = if ndw <= vw { (vw - ndw) / 2.0 } else { -pan_x.clamp(0.0, (ndw - vw).max(0.0)) };
                paint_page(cr, next, nscale, nox, oy + dh);
            }
            if let Some(prev) = st.prev.as_ref() {
                let (pscale, pdw, pdh) = page_layout(fit, prev.w as f64, prev.h as f64, vw, vh);
                let pox = if pdw <= vw { (vw - pdw) / 2.0 } else { -pan_x.clamp(0.0, (pdw - vw).max(0.0)) };
                paint_page(cr, prev, pscale, pox, oy - pdh);
            }
        });
    }

    let update_info = {
        let info = info.clone();
        let rs = rs.clone();
        let autoscroll_on = autoscroll_on.clone();
        let manual_on = manual_on.clone();
        Rc::new(move || {
            let borrow = rs.borrow();
            let Some(st) = borrow.as_ref() else { info.set_text("…"); return; };
            let fit = match st.settings.fit {
                FitMode::Width => "width",
                FitMode::Height => "height",
                FitMode::Contain => "page",
            };
            let auto = if autoscroll_on.get() {
                " · AUTO"
            } else if manual_on.get() {
                " · MANUAL"
            } else {
                ""
            };
            if st.panel_mode {
                let pc = st.panels.len().max(1);
                info.set_text(&format!(
                    "Page {}/{} · panel {}/{} · Panel mode · Space next",
                    st.page_index + 1, st.archive.page_count(), st.panel_index + 1, pc
                ));
            } else {
                info.set_text(&format!(
                    "Page {}/{} · fit {}{} · Space: Manual ⅓ / Auto",
                    st.page_index + 1, st.archive.page_count(), fit, auto
                ));
            }
        })
    };

    let redraw = {
        let area = area.clone();
        let update_info = update_info.clone();
        Rc::new(move || { update_info(); area.queue_draw(); })
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

    let refresh_neighbors = {
        let rs = rs.clone();
        Rc::new(move || {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return; };
            let count = st.archive.page_count();
            st.next = if st.page_index + 1 < count { load_image(&st.archive, st.page_index + 1) } else { None };
            st.prev = if st.page_index > 0 { load_image(&st.archive, st.page_index - 1) } else { None };
        })
    };

    let normalize_strip = {
        let rs = rs.clone();
        let area = area.clone();
        let save_progress = save_progress.clone();
        Rc::new(move || {
            let alloc = area.allocation();
            let vw = alloc.width().max(1) as f64;
            let vh = alloc.height().max(1) as f64;
            let mut advanced = false;
            loop {
                let mut borrow = rs.borrow_mut();
                let Some(st) = borrow.as_mut() else { return; };
                let fit = st.settings.fit;
                let (_, _, dh) = page_layout(fit, st.current.w as f64, st.current.h as f64, vw, vh);
                if st.pan_y >= dh - 0.01 {
                    if st.next.is_none() {
                        st.pan_y = (dh - vh).max(0.0);
                        st.target_y = st.pan_y;
                        break;
                    }
                    let Some(next_img) = st.next.take() else { break; };
                    st.prev = Some(std::mem::replace(&mut st.current, next_img));
                    st.page_index += 1;
                    st.pan_y -= dh;
                    st.target_y -= dh;
                    st.next = if st.page_index + 1 < st.archive.page_count() {
                        load_image(&st.archive, st.page_index + 1)
                    } else { None };
                    advanced = true;
                    continue;
                }
                if st.pan_y < -0.01 {
                    if st.prev.is_none() {
                        st.pan_y = 0.0;
                        st.target_y = 0.0;
                        break;
                    }
                    let Some(prev_img) = st.prev.take() else { break; };
                    let (_, _, pdh) = page_layout(fit, prev_img.w as f64, prev_img.h as f64, vw, vh);
                    st.next = Some(std::mem::replace(&mut st.current, prev_img));
                    st.page_index = st.page_index.saturating_sub(1);
                    st.pan_y += pdh;
                    st.target_y += pdh;
                    st.prev = if st.page_index > 0 {
                        load_image(&st.archive, st.page_index - 1)
                    } else { None };
                    advanced = true;
                    continue;
                }
                break;
            }
            if advanced { save_progress(); }
        })
    };

    let strip_limits = {
        let rs = rs.clone();
        let area = area.clone();
        Rc::new(move || -> (f64, f64, f64) {
            let alloc = area.allocation();
            let vw = alloc.width().max(1) as f64;
            let vh = alloc.height().max(1) as f64;
            let borrow = rs.borrow();
            let Some(st) = borrow.as_ref() else { return (0.0, 0.0, 0.0); };
            let fit = st.settings.fit;
            let (_, dw, dh) = page_layout(fit, st.current.w as f64, st.current.h as f64, vw, vh);
            let max_x = (dw - vw).max(0.0);
            let next_dh = st.next.as_ref().map(|n| page_layout(fit, n.w as f64, n.h as f64, vw, vh).2).unwrap_or(0.0);
            let prev_dh = st.prev.as_ref().map(|p| page_layout(fit, p.w as f64, p.h as f64, vw, vh).2).unwrap_or(0.0);
            let max_y = if st.next.is_some() { (dh + next_dh - vh).max(0.0).max(dh - 0.5) } else { (dh - vh).max(0.0) };
            let min_y = if st.prev.is_some() { -prev_dh + 0.5 } else { 0.0 };
            (max_x, min_y, max_y)
        })
    };

    let goto_page = {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let save_progress = save_progress.clone();
        let refresh_neighbors = refresh_neighbors.clone();
        Rc::new(move |index: usize| {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return; };
            if index >= st.archive.page_count() { return; }
            let Some(img) = load_image(&st.archive, index) else { return; };
            st.page_index = index;
            st.current = img;
            st.next = None;
            st.prev = None;
            st.pan_x = 0.0;
            st.pan_y = 0.0;
            st.target_x = 0.0;
            st.target_y = 0.0;
            st.panels.clear();
            st.panel_index = 0;
            if st.panel_mode {
                ensure_page_panels(st);
            }
            drop(borrow);
            refresh_neighbors();
            save_progress();
            redraw();
        })
    };

    let tick_anim = {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let animating = animating.clone();
        let normalize_strip = normalize_strip.clone();
        Rc::new(move || {
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { animating.set(false); return; };
            let dx = st.target_x - st.pan_x;
            let dy = st.target_y - st.pan_y;
            if dx.abs() < 0.5 && dy.abs() < 0.5 {
                st.pan_x = st.target_x;
                st.pan_y = st.target_y;
                animating.set(false);
                drop(borrow);
                normalize_strip();
                redraw();
                return;
            }
            st.pan_x += dx * 0.22;
            st.pan_y += dy * 0.22;
            drop(borrow);
            normalize_strip();
            redraw();
        })
    };

    let ensure_anim = {
        let animating = animating.clone();
        let tick_anim = tick_anim.clone();
        Rc::new(move || {
            if animating.get() { return; }
            animating.set(true);
            let tick_anim = tick_anim.clone();
            let animating = animating.clone();
            glib::timeout_add_local(Duration::from_millis(16), move || {
                tick_anim();
                if animating.get() { glib::ControlFlow::Continue } else { glib::ControlFlow::Break }
            });
        })
    };

    let pause_autoscroll = {
        let rs = rs.clone();
        let autoscroll_on = autoscroll_on.clone();
        let auto_btn = auto_btn.clone();
        let redraw = redraw.clone();
        Rc::new(move || {
            if !autoscroll_on.get() { return; }
            autoscroll_on.set(false);
            if let Some(st) = rs.borrow_mut().as_mut() { st.autoscroll = false; }
            auto_btn.set_active(false);
            redraw();
        })
    };

    let try_pan = {
        let rs = rs.clone();
        let ensure_anim = ensure_anim.clone();
        let strip_limits = strip_limits.clone();
        let normalize_strip = normalize_strip.clone();
        Rc::new(move |dx: f64, dy: f64| -> bool {
            let (max_x, min_y, max_y) = strip_limits();
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return false; };
            let before_x = st.target_x;
            let before_y = st.target_y;
            st.target_x = (st.target_x + dx).clamp(0.0, max_x);
            st.target_y = (st.target_y + dy).clamp(min_y, max_y);
            let moved = (st.target_x - before_x).abs() > 0.5 || (st.target_y - before_y).abs() > 0.5;
            drop(borrow);
            if moved { ensure_anim(); normalize_strip(); }
            moved
        })
    };

    let turn_or_pan = {
        let rs = rs.clone();
        let try_pan = try_pan.clone();
        let goto_page = goto_page.clone();
        let area = area.clone();
        let pause_autoscroll = pause_autoscroll.clone();
        Rc::new(move |dir_x: i32, dir_y: i32| {
            pause_autoscroll();
            let alloc = area.allocation();
            let vw = alloc.width() as f64;
            let vh = alloc.height() as f64;
            let step = rs.borrow().as_ref().map(|s| s.settings.pan_step).unwrap_or(0.18);
            if try_pan(dir_x as f64 * vw * step, dir_y as f64 * vh * step) { return; }
            let rtl = rs.borrow().as_ref().map(|s| matches!(s.settings.reading_order, ReadingOrder::Rtl)).unwrap_or(false);
            let page_delta = if dir_x != 0 { if rtl { -dir_x } else { dir_x } } else { dir_y };
            let next = rs.borrow().as_ref().map(|s| s.page_index as i32 + page_delta).unwrap_or(0);
            if next >= 0 { goto_page(next as usize); }
        })
    };

    let sync_speed_ui = {
        let rs = rs.clone();
        let speed_label = speed_label.clone();
        let speed_box = speed_box.clone();
        let autoscroll_on = autoscroll_on.clone();
        Rc::new(move || {
            speed_box.set_visible(autoscroll_on.get());
            let pps = rs
                .borrow()
                .as_ref()
                .map(|s| s.settings.autoscroll_pps)
                .unwrap_or(48.0);
            speed_label.set_text(&format!("{:.0} px/s", pps));
        })
    };
    sync_speed_ui();

    let bump_autoscroll_speed = {
        let rs = rs.clone();
        let sync_speed_ui = sync_speed_ui.clone();
        Rc::new(move |delta: f64| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                let next = (st.settings.autoscroll_pps + delta)
                    .clamp(AUTOSCROLL_PPS_MIN, AUTOSCROLL_PPS_MAX);
                st.settings.autoscroll_pps = next;
                let _ = settings::save(&st.settings);
            }
            sync_speed_ui();
        })
    };

    let set_autoscroll = {
        let rs = rs.clone();
        let autoscroll_on = autoscroll_on.clone();
        let auto_btn = auto_btn.clone();
        let manual_btn = manual_btn.clone();
        let manual_on = manual_on.clone();
        let redraw = redraw.clone();
        let refresh_neighbors = refresh_neighbors.clone();
        let sync_speed_ui = sync_speed_ui.clone();
        Rc::new(move |on: bool| {
            if on {
                manual_on.set(false);
                if manual_btn.is_active() { manual_btn.set_active(false); }
            }
            autoscroll_on.set(on);
            if let Some(st) = rs.borrow_mut().as_mut() { st.autoscroll = on; }
            if auto_btn.is_active() != on { auto_btn.set_active(on); }
            if on { refresh_neighbors(); }
            sync_speed_ui();
            redraw();
        })
    };

    let set_manual = {
        let manual_on = manual_on.clone();
        let manual_btn = manual_btn.clone();
        let set_autoscroll = set_autoscroll.clone();
        let redraw = redraw.clone();
        Rc::new(move |on: bool| {
            if on {
                set_autoscroll(false);
            }
            manual_on.set(on);
            if manual_btn.is_active() != on {
                manual_btn.set_active(on);
            }
            redraw();
        })
    };

    let step_third = {
        let try_pan = try_pan.clone();
        let turn_or_pan = turn_or_pan.clone();
        let area = area.clone();
        let pause_autoscroll = pause_autoscroll.clone();
        Rc::new(move |dir: i32| {
            // dir > 0 = down, < 0 = up — one third of the viewport.
            pause_autoscroll();
            let alloc = area.allocation();
            let vh = alloc.height().max(1) as f64;
            let amount = vh / 3.0;
            if !try_pan(0.0, dir as f64 * amount) {
                turn_or_pan(0, dir);
            }
        })
    };

    let set_panel_mode = {
        let rs = rs.clone();
        let panel_mode_on = panel_mode_on.clone();
        let panel_btn = panel_btn.clone();
        let pause_autoscroll = pause_autoscroll.clone();
        let set_manual = set_manual.clone();
        let redraw = redraw.clone();
        let area = area.clone();
        Rc::new(move |on: bool| {
            if on {
                pause_autoscroll();
                set_manual(false);
            }
            panel_mode_on.set(on);
            if panel_btn.is_active() != on {
                panel_btn.set_active(on);
            }
            let alloc = area.allocation();
            let vw = alloc.width().max(1) as f64;
            let vh = alloc.height().max(1) as f64;
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.panel_mode = on;
                if on {
                    ensure_page_panels(st);
                    st.panel_index = panel_index_from_pan(st, vw, vh);
                }
            }
            redraw();
        })
    };

    let step_panel = {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let save_progress = save_progress.clone();
        let refresh_neighbors = refresh_neighbors.clone();
        Rc::new(move |delta: i32| {
            // Advance/retreat panel; page boundary uses same full-height paint.
            loop {
                let mut borrow = rs.borrow_mut();
                let Some(st) = borrow.as_mut() else { return; };
                if !st.panel_mode {
                    return;
                }
                ensure_page_panels(st);
                if st.panels.is_empty() {
                    st.panels = vec![PanelRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }];
                }
                let next_i = st.panel_index as i32 + delta;
                if next_i >= 0 && (next_i as usize) < st.panels.len() {
                    st.panel_index = next_i as usize;
                    drop(borrow);
                    redraw();
                    return;
                }
                if delta > 0 {
                    let next_page = st.page_index + 1;
                    if next_page >= st.archive.page_count() {
                        return;
                    }
                    let Some(img) = load_image(&st.archive, next_page) else { return; };
                    st.page_index = next_page;
                    st.current = img;
                    st.next = None;
                    st.prev = None;
                    st.pan_x = 0.0;
                    st.pan_y = 0.0;
                    st.target_x = 0.0;
                    st.target_y = 0.0;
                    st.panel_index = 0;
                    st.panels.clear();
                    ensure_page_panels(st);
                    drop(borrow);
                    refresh_neighbors();
                    save_progress();
                    redraw();
                    return;
                } else {
                    if st.page_index == 0 {
                        return;
                    }
                    let prev_page = st.page_index - 1;
                    let Some(img) = load_image(&st.archive, prev_page) else { return; };
                    st.page_index = prev_page;
                    st.current = img;
                    st.next = None;
                    st.prev = None;
                    st.pan_x = 0.0;
                    st.pan_y = 0.0;
                    st.target_x = 0.0;
                    st.target_y = 0.0;
                    st.panels.clear();
                    ensure_page_panels(st);
                    st.panel_index = st.panels.len().saturating_sub(1);
                    drop(borrow);
                    refresh_neighbors();
                    save_progress();
                    redraw();
                    return;
                }
            }
        })
    };

    {
        let rs = rs.clone();
        let autoscroll_on = autoscroll_on.clone();
        let redraw = redraw.clone();
        let normalize_strip = normalize_strip.clone();
        let strip_limits = strip_limits.clone();
        let set_autoscroll = set_autoscroll.clone();
        let panel_mode_on = panel_mode_on.clone();
        let last = std::cell::Cell::new(None::<std::time::Instant>);
        glib::timeout_add_local(Duration::from_millis(16), move || {
            if panel_mode_on.get() {
                last.set(None);
                return glib::ControlFlow::Continue;
            }
            if !autoscroll_on.get() {
                last.set(None);
                return glib::ControlFlow::Continue;
            }
            let now = std::time::Instant::now();
            let dt = match last.get() {
                Some(t) => now.duration_since(t).as_secs_f64().min(0.05),
                None => 1.0 / 60.0,
            };
            last.set(Some(now));
            let pps = rs.borrow().as_ref().map(|s| s.settings.autoscroll_pps).unwrap_or(48.0);
            let dy = pps * dt;
            let (_max_x, min_y, max_y) = strip_limits();
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return glib::ControlFlow::Continue; };
            if st.next.is_none() && st.pan_y >= max_y - 0.5 {
                drop(borrow);
                set_autoscroll(false);
                return glib::ControlFlow::Continue;
            }
            st.pan_y = (st.pan_y + dy).clamp(min_y, max_y + dy);
            st.target_y = st.pan_y;
            drop(borrow);
            normalize_strip();
            redraw();
            glib::ControlFlow::Continue
        });
    }

    prev_btn.connect_clicked(clone!(@strong goto_page, @strong rs, @strong pause_autoscroll, @strong step_panel, @strong panel_mode_on => move |_| {
        pause_autoscroll();
        if panel_mode_on.get() {
            step_panel(-1);
            return;
        }
        let i = rs.borrow().as_ref().map(|s| s.page_index.saturating_sub(1)).unwrap_or(0);
        goto_page(i);
    }));
    next_btn.connect_clicked(clone!(@strong goto_page, @strong rs, @strong pause_autoscroll, @strong step_panel, @strong panel_mode_on => move |_| {
        pause_autoscroll();
        if panel_mode_on.get() {
            step_panel(1);
            return;
        }
        let i = rs.borrow().as_ref().map(|s| s.page_index + 1).unwrap_or(0);
        goto_page(i);
    }));
    {
        let set_autoscroll = set_autoscroll.clone();
        let panel_mode_on = panel_mode_on.clone();
        auto_btn.connect_toggled(move |btn| {
            if panel_mode_on.get() {
                btn.set_active(false);
                return;
            }
            set_autoscroll(btn.is_active());
        });
    }
    {
        let set_manual = set_manual.clone();
        let panel_mode_on = panel_mode_on.clone();
        manual_btn.connect_toggled(move |btn| {
            if panel_mode_on.get() {
                btn.set_active(false);
                return;
            }
            set_manual(btn.is_active());
        });
    }
    {
        let set_panel_mode = set_panel_mode.clone();
        panel_btn.connect_toggled(move |btn| set_panel_mode(btn.is_active()));
    }
    {
        let bump = bump_autoscroll_speed.clone();
        slower_btn.connect_clicked(move |_| bump(-AUTOSCROLL_PPS_STEP));
    }
    {
        let bump = bump_autoscroll_speed.clone();
        faster_btn.connect_clicked(move |_| bump(AUTOSCROLL_PPS_STEP));
    }
    {
        let rs = rs.clone();
        let save_progress = save_progress.clone();
        order_btn.connect_toggled(move |btn| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.settings.reading_order = if btn.is_active() {
                    btn.set_label("RTL"); ReadingOrder::Rtl
                } else {
                    btn.set_label("LTR"); ReadingOrder::Ltr
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
                if btn.is_active() { btn.set_label("White"); st.settings.letterbox = Letterbox::White; }
                else { btn.set_label("Black"); st.settings.letterbox = Letterbox::Black; }
            }
            save_progress();
            redraw();
        });
    }
    {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let save_progress = save_progress.clone();
        let refresh_neighbors = refresh_neighbors.clone();
        fit_drop.connect_selected_notify(move |drop| {
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.settings.fit = match drop.selected() {
                    1 => FitMode::Height,
                    2 => FitMode::Contain,
                    _ => FitMode::Width,
                };
                st.pan_x = 0.0; st.pan_y = 0.0; st.target_x = 0.0; st.target_y = 0.0;
            }
            refresh_neighbors();
            save_progress();
            redraw();
        });
    }

    {
        let chrome_visible = chrome_visible.clone();
        let sync_chrome = sync_chrome.clone();
        let click = GestureClick::new();
        click.set_button(1);
        click.connect_pressed(move |_g, n_press, _x, _y| {
            if n_press == 2 {
                chrome_visible.set(!chrome_visible.get());
                sync_chrome();
            }
        });
        area.add_controller(click);
    }

    {
        let rs = rs.clone();
        let redraw = redraw.clone();
        let normalize_strip = normalize_strip.clone();
        let strip_limits = strip_limits.clone();
        let pause_autoscroll = pause_autoscroll.clone();
        let drag = GestureDrag::new();
        drag.connect_drag_begin(clone!(@strong rs, @strong pause_autoscroll, @strong panel_mode_on => move |_, _, _| {
            if panel_mode_on.get() { return; }
            pause_autoscroll();
            if let Some(st) = rs.borrow_mut().as_mut() {
                st.drag_last = Some((st.pan_x, st.pan_y));
                st.target_x = st.pan_x;
                st.target_y = st.pan_y;
            }
        }));
        drag.connect_drag_update(clone!(@strong rs, @strong redraw, @strong normalize_strip, @strong strip_limits, @strong panel_mode_on => move |g, _, _| {
            if panel_mode_on.get() { return; }
            let Some((dx, dy)) = g.offset() else { return; };
            let (max_x, min_y, max_y) = strip_limits();
            let mut borrow = rs.borrow_mut();
            let Some(st) = borrow.as_mut() else { return; };
            let Some((ox, oy)) = st.drag_last else { return; };
            st.pan_x = (ox - dx).clamp(0.0, max_x);
            st.pan_y = (oy - dy).clamp(min_y, max_y);
            st.target_x = st.pan_x;
            st.target_y = st.pan_y;
            drop(borrow);
            normalize_strip();
            redraw();
        }));
        area.add_controller(drag);
    }

    {
        let turn_or_pan = turn_or_pan.clone();
        let window_keys = window.clone();
        let set_autoscroll = set_autoscroll.clone();
        let autoscroll_on = autoscroll_on.clone();
        let manual_on = manual_on.clone();
        let set_manual = set_manual.clone();
        let step_third = step_third.clone();
        let bump_autoscroll_speed = bump_autoscroll_speed.clone();
        let chrome_visible = chrome_visible.clone();
        let sync_chrome = sync_chrome.clone();
        let panel_mode_on = panel_mode_on.clone();
        let step_panel = step_panel.clone();
        let set_panel_mode = set_panel_mode.clone();
        let controller = EventControllerKey::new();
        controller.connect_key_pressed(move |_, key, _, mods| {
            if mods.contains(ModifierType::CONTROL_MASK) {
                return glib::Propagation::Proceed;
            }
            if panel_mode_on.get() {
                match key {
                    Key::Right | Key::d | Key::D | Key::l | Key::L | Key::Down | Key::j | Key::J | Key::Page_Down | Key::space => {
                        step_panel(1); return glib::Propagation::Stop;
                    }
                    Key::Left | Key::a | Key::A | Key::h | Key::H | Key::Up | Key::k | Key::K | Key::Page_Up | Key::BackSpace => {
                        step_panel(-1); return glib::Propagation::Stop;
                    }
                    Key::p | Key::P => {
                        set_panel_mode(false); return glib::Propagation::Stop;
                    }
                    Key::t | Key::T => {
                        chrome_visible.set(!chrome_visible.get());
                        sync_chrome();
                        return glib::Propagation::Stop;
                    }
                    Key::F11 => {
                        if window_keys.is_fullscreen() { window_keys.unfullscreen(); } else { window_keys.fullscreen(); }
                        return glib::Propagation::Stop;
                    }
                    Key::Escape => {
                        // Spec: leave Panel first conceptually via toggle; Esc still only exits fullscreen.
                        if window_keys.is_fullscreen() {
                            window_keys.unfullscreen();
                            return glib::Propagation::Stop;
                        }
                        return glib::Propagation::Proceed;
                    }
                    _ => return glib::Propagation::Proceed,
                }
            }
            match key {
                Key::Right | Key::d | Key::D | Key::l | Key::L => { turn_or_pan(1, 0); glib::Propagation::Stop }
                Key::Left | Key::a | Key::A | Key::h | Key::H => { turn_or_pan(-1, 0); glib::Propagation::Stop }
                Key::Down | Key::j | Key::J | Key::Page_Down => { turn_or_pan(0, 1); glib::Propagation::Stop }
                Key::Up | Key::k | Key::K | Key::Page_Up | Key::BackSpace => { turn_or_pan(0, -1); glib::Propagation::Stop }
                Key::space => {
                    if manual_on.get() {
                        let up = mods.contains(ModifierType::SHIFT_MASK);
                        step_third(if up { -1 } else { 1 });
                    } else if autoscroll_on.get() {
                        set_autoscroll(false);
                    } else {
                        // Default: enter Manual and jump one third (Space = page through).
                        set_manual(true);
                        step_third(1);
                    }
                    glib::Propagation::Stop
                }
                Key::p | Key::P => { set_panel_mode(true); glib::Propagation::Stop }
                Key::minus | Key::KP_Subtract => {
                    if autoscroll_on.get() { bump_autoscroll_speed(-AUTOSCROLL_PPS_STEP); }
                    glib::Propagation::Stop
                }
                Key::equal | Key::plus | Key::KP_Add => {
                    if autoscroll_on.get() { bump_autoscroll_speed(AUTOSCROLL_PPS_STEP); }
                    glib::Propagation::Stop
                }
                Key::t | Key::T => {
                    chrome_visible.set(!chrome_visible.get());
                    sync_chrome();
                    glib::Propagation::Stop
                }
                Key::F11 => {
                    if window_keys.is_fullscreen() { window_keys.unfullscreen(); } else { window_keys.fullscreen(); }
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

    {
        let try_pan = try_pan.clone();
        let turn_or_pan = turn_or_pan.clone();
        let pause_autoscroll = pause_autoscroll.clone();
        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        let panel_mode_on = panel_mode_on.clone();
        let step_panel = step_panel.clone();
        scroll.connect_scroll(move |_, _dx, dy| {
            if panel_mode_on.get() {
                if dy > 0.0 { step_panel(1); }
                else if dy < 0.0 { step_panel(-1); }
                return glib::Propagation::Stop;
            }
            pause_autoscroll();
            let amount = dy * 80.0;
            if !try_pan(0.0, amount) {
                if dy > 0.0 { turn_or_pan(0, 1); }
                else if dy < 0.0 { turn_or_pan(0, -1); }
            }
            glib::Propagation::Stop
        });
        area.add_controller(scroll);
    }

    window.connect_close_request(clone!(@strong save_progress, @strong saved_shader => move |_| {
        save_progress();
        let prev = saved_shader.borrow_mut().take();
        crate::paper::restore_hyprland_shader(prev.as_deref());
        glib::Propagation::Proceed
    }));

    {
        let saved_shader = saved_shader.clone();
        window.connect_destroy(move |_| {
            let prev = saved_shader.borrow_mut().take();
            if prev.is_some() {
                crate::paper::restore_hyprland_shader(prev.as_deref());
            }
        });
    }

    refresh_neighbors();
    update_info();
    window.present();
    window.fullscreen();
    sync_chrome();
    area.grab_focus();
}
