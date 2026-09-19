//! Panel detection v5 — Kumiko / cbxy-inspired.
//!
//! Classical CV without OpenCV: threshold gutters → morph-close panel mask →
//! flood-fill page margin → connected components → axis-aligned bounding boxes.
//! Full-width panels keep their full width; boxes get a small pad so borders
//! aren't clipped. Weak / unconventional pages fall back to the whole page.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PanelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

const CACHE_NOTE: u32 = 5;
#[allow(dead_code)]
pub const DETECT_VERSION: u32 = CACHE_NOTE;

/// Detect panel rectangles in normalized page coordinates (0..1).
pub fn detect_panels(rgba: &[u8], width: u32, height: u32) -> Vec<PanelRect> {
    if width < 8 || height < 8 || rgba.len() < (width * height * 4) as usize {
        return full_page();
    }

    let w = width as usize;
    let h = height as usize;
    // Working resolution: enough detail for gutters, fast enough for UI.
    let (sw, sh, lum) = downsample_luma(rgba, w, h, 1400);

    let white = detect_contour_panels(&lum, sw, sh, true);
    let dark = detect_contour_panels(&lum, sw, sh, false);
    let (best, score) = pick_better_scored(white, dark, sw, sh);

    if score >= 5.0 && looks_usable(&best, sw, sh) {
        return to_norm_panels(&best, sw, sh);
    }

    // Soft fallback: clear horizontal bands only (full-width slices).
    let rows_w = detect_row_slices(&lum, sw, sh, true);
    let rows_d = detect_row_slices(&lum, sw, sh, false);
    let (rows, row_score) = pick_better_scored(rows_w, rows_d, sw, sh);
    if row_score >= 3.5 && rows.len() >= 2 && rows.len() <= 10 {
        let panels = to_norm_panels(&rows, sw, sh);
        if panels.iter().all(|p| p.w >= 0.82) {
            return panels;
        }
    }

    full_page()
}

fn full_page() -> Vec<PanelRect> {
    vec![PanelRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    }]
}

fn downsample_luma(rgba: &[u8], w: usize, h: usize, max_side: usize) -> (usize, usize, Vec<u8>) {
    if w.max(h) <= max_side {
        let mut out = vec![0u8; w * h];
        for i in 0..(w * h) {
            let o = i * 4;
            let r = rgba[o] as u32;
            let g = rgba[o + 1] as u32;
            let b = rgba[o + 2] as u32;
            out[i] = ((r * 30 + g * 59 + b * 11) / 100) as u8;
        }
        return (w, h, out);
    }
    let scale = max_side as f32 / w.max(h) as f32;
    let nw = ((w as f32) * scale).round().max(8.0) as usize;
    let nh = ((h as f32) * scale).round().max(8.0) as usize;
    let mut out = vec![0u8; nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            let sx = x * w / nw;
            let sy = y * h / nh;
            let i = (sy * w + sx) * 4;
            let r = rgba[i] as u32;
            let g = rgba[i + 1] as u32;
            let b = rgba[i + 2] as u32;
            out[y * nw + x] = ((r * 30 + g * 59 + b * 11) / 100) as u8;
        }
    }
    (nw, nh, out)
}

fn detect_contour_panels(
    lum: &[u8],
    w: usize,
    h: usize,
    white_gutters: bool,
) -> Vec<(usize, usize, usize, usize)> {
    let page = (w * h) as f32;
    let gutter_thr: u8 = if white_gutters { 225 } else { 40 };

    // Gutter mask: true = gutter / page background.
    let mut gutter = vec![false; w * h];
    for i in 0..lum.len() {
        gutter[i] = if white_gutters {
            lum[i] >= gutter_thr
        } else {
            lum[i] <= gutter_thr
        };
    }

    // Panel mask = not gutter, then morphological close to bridge halftone holes.
    let mut panel = vec![false; w * h];
    for i in 0..(w * h) {
        panel[i] = !gutter[i];
    }
    let k = ((w.min(h) as f32) * 0.004).max(2.0) as usize | 1;
    morph_close(&mut panel, w, h, k, 2);

    // Flood-fill gutter from corners → mark page margin background.
    let mut bg = vec![false; w * h];
    for &(sx, sy) in &[(0usize, 0usize), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
        if gutter[sy * w + sx] {
            flood_fill_bool(&gutter, &mut bg, w, h, sx, sy);
        }
    }
    for i in 0..(w * h) {
        if bg[i] {
            panel[i] = false;
        }
    }

    // Connected components on panel mask → bounding boxes.
    let labels = label_components(&panel, w, h);
    let mut boxes: Vec<(usize, usize, usize, usize, usize)> = Vec::new(); // x,y,w,h,area
    let nlab = *labels.iter().max().unwrap_or(&0);
    if nlab == 0 {
        return vec![(0, 0, w, h)];
    }
    let mut min_x = vec![w; nlab + 1];
    let mut min_y = vec![h; nlab + 1];
    let mut max_x = vec![0usize; nlab + 1];
    let mut max_y = vec![0usize; nlab + 1];
    let mut area = vec![0usize; nlab + 1];
    for y in 0..h {
        for x in 0..w {
            let id = labels[y * w + x];
            if id == 0 {
                continue;
            }
            min_x[id] = min_x[id].min(x);
            min_y[id] = min_y[id].min(y);
            max_x[id] = max_x[id].max(x);
            max_y[id] = max_y[id].max(y);
            area[id] += 1;
        }
    }

    let min_area = (page * 0.03) as usize;
    let max_area = (page * 0.88) as usize;
    for id in 1..=nlab {
        let a = area[id];
        if a < min_area || a > max_area {
            continue;
        }
        let bw = max_x[id] + 1 - min_x[id];
        let bh = max_y[id] + 1 - min_y[id];
        let aspect = bw as f32 / bh.max(1) as f32;
        if !(0.12..=8.0).contains(&aspect) {
            continue;
        }
        if bw < (w as f32 * 0.07) as usize || bh < (h as f32 * 0.045) as usize {
            continue;
        }
        boxes.push((min_x[id], min_y[id], bw, bh, a));
    }

    // Pad boxes slightly so black borders aren't clipped; snap near-full spans.
    let pad_x = ((w as f32) * 0.006).max(1.0) as usize;
    let pad_y = ((h as f32) * 0.006).max(1.0) as usize;
    let mut rects: Vec<(usize, usize, usize, usize)> = boxes
        .into_iter()
        .map(|(x, y, bw, bh, _)| {
            let mut x0 = x.saturating_sub(pad_x);
            let mut y0 = y.saturating_sub(pad_y);
            let mut x1 = (x + bw + pad_x).min(w);
            let mut y1 = (y + bh + pad_y).min(h);
            // Full-bleed / full-width: snap to page edges when nearly there.
            if x0 as f32 <= w as f32 * 0.03 {
                x0 = 0;
            }
            if (w - x1) as f32 <= w as f32 * 0.03 {
                x1 = w;
            }
            if y0 as f32 <= h as f32 * 0.02 {
                y0 = 0;
            }
            if (h - y1) as f32 <= h as f32 * 0.02 {
                y1 = h;
            }
            // If the component already spans most of the width, force full width.
            if (x1 - x0) as f32 >= w as f32 * 0.88 {
                x0 = 0;
                x1 = w;
            }
            (x0, y0, x1 - x0, y1 - y0)
        })
        .collect();

    rects = suppress_nested(rects, w, h);
    if rects.is_empty() {
        return vec![(0, 0, w, h)];
    }
    rects.sort_by(|a, b| {
        let ay = a.1 + a.3 / 2;
        let by = b.1 + b.3 / 2;
        let tol = (h as f32 * 0.05) as usize;
        if ay.abs_diff(by) <= tol {
            a.0.cmp(&b.0)
        } else {
            ay.cmp(&by)
        }
    });
    rects
}

fn morph_close(mask: &mut [bool], w: usize, h: usize, radius: usize, iterations: usize) {
    for _ in 0..iterations {
        dilate(mask, w, h, radius);
        erode(mask, w, h, radius);
    }
}

fn dilate(mask: &mut [bool], w: usize, h: usize, radius: usize) {
    let orig = mask.to_vec();
    for y in 0..h {
        for x in 0..w {
            let mut on = false;
            let y0 = y.saturating_sub(radius);
            let y1 = (y + radius + 1).min(h);
            let x0 = x.saturating_sub(radius);
            let x1 = (x + radius + 1).min(w);
            'search: for yy in y0..y1 {
                for xx in x0..x1 {
                    if orig[yy * w + xx] {
                        on = true;
                        break 'search;
                    }
                }
            }
            mask[y * w + x] = on;
        }
    }
}

fn erode(mask: &mut [bool], w: usize, h: usize, radius: usize) {
    let orig = mask.to_vec();
    for y in 0..h {
        for x in 0..w {
            let mut on = true;
            let y0 = y.saturating_sub(radius);
            let y1 = (y + radius + 1).min(h);
            let x0 = x.saturating_sub(radius);
            let x1 = (x + radius + 1).min(w);
            'search: for yy in y0..y1 {
                for xx in x0..x1 {
                    if !orig[yy * w + xx] {
                        on = false;
                        break 'search;
                    }
                }
            }
            mask[y * w + x] = on;
        }
    }
}

fn flood_fill_bool(allowed: &[bool], out: &mut [bool], w: usize, h: usize, sx: usize, sy: usize) {
    if out[sy * w + sx] || !allowed[sy * w + sx] {
        return;
    }
    let mut stack = vec![(sx, sy)];
    while let Some((x, y)) = stack.pop() {
        let i = y * w + x;
        if out[i] || !allowed[i] {
            continue;
        }
        out[i] = true;
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }
}

fn label_components(mask: &[bool], w: usize, h: usize) -> Vec<usize> {
    let mut labels = vec![0usize; w * h];
    let mut next = 1usize;
    let mut stack = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if !mask[i] || labels[i] != 0 {
                continue;
            }
            let id = next;
            next += 1;
            stack.clear();
            stack.push((x, y));
            while let Some((cx, cy)) = stack.pop() {
                let ci = cy * w + cx;
                if !mask[ci] || labels[ci] != 0 {
                    continue;
                }
                labels[ci] = id;
                if cx > 0 {
                    stack.push((cx - 1, cy));
                }
                if cx + 1 < w {
                    stack.push((cx + 1, cy));
                }
                if cy > 0 {
                    stack.push((cx, cy - 1));
                }
                if cy + 1 < h {
                    stack.push((cx, cy + 1));
                }
            }
        }
    }
    labels
}

fn suppress_nested(
    rects: Vec<(usize, usize, usize, usize)>,
    _w: usize,
    _h: usize,
) -> Vec<(usize, usize, usize, usize)> {
    let mut keep = vec![true; rects.len()];
    for i in 0..rects.len() {
        if !keep[i] {
            continue;
        }
        let a = rects[i];
        let aa = (a.2 * a.3) as f32;
        for j in 0..rects.len() {
            if i == j || !keep[j] {
                continue;
            }
            let b = rects[j];
            let bb = (b.2 * b.3) as f32;
            let inter = overlap_area(a, b) as f32;
            // Drop the smaller if mostly inside the larger (speech-bubble / figure bits).
            if aa >= bb && inter / bb.max(1.0) > 0.65 {
                keep[j] = false;
            } else if bb > aa && inter / aa.max(1.0) > 0.65 {
                keep[i] = false;
                break;
            }
        }
    }
    rects
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, r)| r)
        .collect()
}

fn overlap_area(a: (usize, usize, usize, usize), b: (usize, usize, usize, usize)) -> usize {
    let ax1 = a.0 + a.2;
    let ay1 = a.1 + a.3;
    let bx1 = b.0 + b.2;
    let by1 = b.1 + b.3;
    let ix0 = a.0.max(b.0);
    let iy0 = a.1.max(b.1);
    let ix1 = ax1.min(bx1);
    let iy1 = ay1.min(by1);
    if ix1 <= ix0 || iy1 <= iy0 {
        0
    } else {
        (ix1 - ix0) * (iy1 - iy0)
    }
}

fn detect_row_slices(lum: &[u8], w: usize, h: usize, white: bool) -> Vec<(usize, usize, usize, usize)> {
    let thr: u8 = if white { 230 } else { 35 };
    let mut gutter_row = vec![false; h];
    for y in 0..h {
        let mut g = 0u32;
        for x in 0..w {
            let v = lum[y * w + x];
            if white {
                if v >= thr {
                    g += 1;
                }
            } else if v <= thr {
                g += 1;
            }
        }
        gutter_row[y] = g as f32 / w as f32 >= 0.80;
    }
    // thicken
    let orig = gutter_row.clone();
    for y in 0..h {
        let lo = y.saturating_sub(1);
        let hi = (y + 2).min(h);
        gutter_row[y] = orig[lo..hi].iter().any(|&v| v);
    }
    let mut bands = Vec::new();
    let mut y = 0;
    let min_h = ((h as f32) * 0.06).max(12.0) as usize;
    while y < h {
        while y < h && gutter_row[y] {
            y += 1;
        }
        if y >= h {
            break;
        }
        let y0 = y;
        while y < h && !gutter_row[y] {
            y += 1;
        }
        if y - y0 >= min_h {
            bands.push((0, y0, w, y - y0));
        }
    }
    if bands.is_empty() {
        vec![(0, 0, w, h)]
    } else {
        bands
    }
}

fn to_norm_panels(rects: &[(usize, usize, usize, usize)], sw: usize, sh: usize) -> Vec<PanelRect> {
    let mut panels: Vec<PanelRect> = rects
        .iter()
        .map(|&(x, y, pw, ph)| PanelRect {
            x: (x as f64 / sw as f64).clamp(0.0, 1.0),
            y: (y as f64 / sh as f64).clamp(0.0, 1.0),
            w: (pw as f64 / sw as f64).clamp(0.04, 1.0),
            h: (ph as f64 / sh as f64).clamp(0.04, 1.0),
        })
        .filter(|p| p.w * p.h >= 0.028)
        .collect();
    for p in &mut panels {
        if p.x <= 0.02 {
            p.w = (p.w + p.x).min(1.0);
            p.x = 0.0;
        }
        if p.y <= 0.015 {
            p.h = (p.h + p.y).min(1.0);
            p.y = 0.0;
        }
        if p.x + p.w >= 0.98 {
            p.w = 1.0 - p.x;
        }
        if p.y + p.h >= 0.985 {
            p.h = 1.0 - p.y;
        }
        if p.w >= 0.88 {
            p.x = 0.0;
            p.w = 1.0;
        }
    }
    if panels.is_empty() {
        full_page()
    } else {
        panels
    }
}

fn looks_usable(rects: &[(usize, usize, usize, usize)], w: usize, h: usize) -> bool {
    let n = rects.len();
    if n == 0 || n > 16 {
        return false;
    }
    if n == 1 {
        // Single near-full page is ok (splash); tiny single crop is not.
        let a = (rects[0].2 * rects[0].3) as f32 / (w * h) as f32;
        return a >= 0.55;
    }
    true
}

fn pick_better_scored(
    a: Vec<(usize, usize, usize, usize)>,
    b: Vec<(usize, usize, usize, usize)>,
    w: usize,
    h: usize,
) -> (Vec<(usize, usize, usize, usize)>, f32) {
    let sa = score(&a, w, h);
    let sb = score(&b, w, h);
    if sa >= sb {
        (a, sa)
    } else {
        (b, sb)
    }
}

fn score(rects: &[(usize, usize, usize, usize)], w: usize, h: usize) -> f32 {
    if rects.is_empty() {
        return 0.0;
    }
    let page = (w * h) as f32;
    let mut area = 0f32;
    let mut shape = 0f32;
    let mut tiny = 0f32;
    for &(_, _, pw, ph) in rects {
        let a = (pw * ph) as f32;
        area += a;
        let ar = pw as f32 / ph.max(1) as f32;
        if (0.35..=3.5).contains(&ar) {
            shape += 1.0;
        }
        if a / page < 0.035 {
            tiny += 1.2;
        }
    }
    let n = rects.len() as f32;
    let coverage = (area / page).clamp(0.0, 1.25);
    let count = if (2.0..=12.0).contains(&n) {
        3.5
    } else if n == 1.0 {
        1.2
    } else {
        0.4
    };
    count + shape + coverage * 2.2 - tiny
}

/// Reorder panels for RTL (manga): top-to-bottom, right-to-left within rows.
pub fn order_panels(panels: &[PanelRect], rtl: bool) -> Vec<PanelRect> {
    if panels.is_empty() {
        return full_page();
    }
    let mut indexed: Vec<(usize, PanelRect)> = panels.iter().cloned().enumerate().collect();
    indexed.sort_by(|(_, a), (_, b)| {
        let ay = a.y + a.h / 2.0;
        let by = b.y + b.h / 2.0;
        if (ay - by).abs() < 0.05 {
            if rtl {
                b.x.partial_cmp(&a.x).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)
            }
        } else {
            ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
        }
    });
    indexed.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_falls_back() {
        assert_eq!(detect_panels(&[0; 4], 1, 1).len(), 1);
    }
}
