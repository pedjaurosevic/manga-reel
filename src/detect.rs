//! Classical gutter / projection panel detection (no ML).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PanelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Detect panel rectangles in normalized page coordinates (0..1) using
/// luminance projection gutters. Prefers white gutters (manga) then dark.
pub fn detect_panels(rgba: &[u8], width: u32, height: u32) -> Vec<PanelRect> {
    if width < 8 || height < 8 || rgba.len() < (width * height * 4) as usize {
        return full_page();
    }

    let w = width as usize;
    let h = height as usize;

    // Downscale for speed if huge
    let max_side = 1200usize;
    let (sw, sh, lum) = if w.max(h) > max_side {
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
    } else {
        let mut out = vec![0u8; w * h];
        for i in 0..(w * h) {
            let o = i * 4;
            let r = rgba[o] as u32;
            let g = rgba[o + 1] as u32;
            let b = rgba[o + 2] as u32;
            out[i] = ((r * 30 + g * 59 + b * 11) / 100) as u8;
        }
        (w, h, out)
    };

    let mut panels = detect_with_gutter_mode(&lum, sw, sh, true);
    if panels.len() < 2 {
        panels = detect_with_gutter_mode(&lum, sw, sh, false);
    }
    if panels.is_empty() {
        return full_page();
    }

    // Convert from downscaled pixel space to normalized 0..1
    panels
        .into_iter()
        .map(|(x, y, pw, ph)| PanelRect {
            x: x as f64 / sw as f64,
            y: y as f64 / sh as f64,
            w: (pw as f64 / sw as f64).max(0.02),
            h: (ph as f64 / sh as f64).max(0.02),
        })
        .filter(|p| p.w * p.h > 0.01) // drop tiny noise
        .collect::<Vec<_>>()
        .pipe_or_full()
}

trait PipeOrFull {
    fn pipe_or_full(self) -> Vec<PanelRect>;
}
impl PipeOrFull for Vec<PanelRect> {
    fn pipe_or_full(self) -> Vec<PanelRect> {
        if self.is_empty() {
            full_page()
        } else {
            self
        }
    }
}

fn full_page() -> Vec<PanelRect> {
    vec![PanelRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    }]
}

fn detect_with_gutter_mode(lum: &[u8], w: usize, h: usize, white_gutters: bool) -> Vec<(usize, usize, usize, usize)> {
    // Row / column mean luminance
    let mut row_mean = vec![0f32; h];
    let mut col_mean = vec![0f32; w];
    for y in 0..h {
        let mut s = 0u32;
        for x in 0..w {
            let v = lum[y * w + x] as u32;
            s += v;
            col_mean[x] += v as f32;
        }
        row_mean[y] = s as f32 / w as f32;
    }
    for x in 0..w {
        col_mean[x] /= h as f32;
    }

    let row_gutter = mark_gutters(&row_mean, white_gutters);
    let col_gutter = mark_gutters(&col_mean, white_gutters);

    let row_bands = bands_from_mask(&row_gutter);
    let col_bands = bands_from_mask(&col_gutter);

    if row_bands.is_empty() || col_bands.is_empty() {
        return vec![(0, 0, w, h)];
    }

    let min_w = (w as f32 * 0.08) as usize;
    let min_h = (h as f32 * 0.06) as usize;

    let mut rects = Vec::new();
    for &(y0, y1) in &row_bands {
        for &(x0, x1) in &col_bands {
            let pw = x1.saturating_sub(x0);
            let ph = y1.saturating_sub(y0);
            if pw < min_w || ph < min_h {
                continue;
            }
            // Reject mostly-gutter empty cells: check content variance
            if cell_is_empty(lum, w, x0, y0, x1, y1, white_gutters) {
                continue;
            }
            rects.push((x0, y0, pw, ph));
        }
    }

    if rects.is_empty() {
        // Fall back to row strips only
        for &(y0, y1) in &row_bands {
            let ph = y1.saturating_sub(y0);
            if ph >= min_h {
                rects.push((0, y0, w, ph));
            }
        }
    }

    // Reading order: top-to-bottom, then left-to-right (LTR). RTL reordering is done in UI.
    rects.sort_by(|a, b| {
        let ay = a.1 + a.3 / 2;
        let by = b.1 + b.3 / 2;
        let row_tol = (h as f32 * 0.05) as usize;
        if ay.abs_diff(by) <= row_tol {
            a.0.cmp(&b.0)
        } else {
            ay.cmp(&by)
        }
    });

    // Merge overlapping near-duplicates
    merge_overlaps(rects)
}

fn mark_gutters(means: &[f32], white: bool) -> Vec<bool> {
    let n = means.len();
    let mut mask = vec![false; n];
    if n == 0 {
        return mask;
    }
    let avg: f32 = means.iter().sum::<f32>() / n as f32;
    let threshold = if white {
        // bright = gutter
        (avg + 30.0).min(245.0).max(180.0)
    } else {
        // dark = gutter
        (avg - 30.0).max(10.0).min(80.0)
    };
    for (i, &m) in means.iter().enumerate() {
        mask[i] = if white { m >= threshold } else { m <= threshold };
    }
    // Morphological open/close: remove tiny gaps
    dilate_erode(&mut mask, 2);
    mask
}

fn dilate_erode(mask: &mut [bool], radius: usize) {
    let n = mask.len();
    let orig = mask.to_vec();
    // erode gutters (require neighborhood)
    for i in 0..n {
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(n);
        mask[i] = orig[lo..hi].iter().all(|&v| v);
    }
    let eroded = mask.to_vec();
    // dilate
    for i in 0..n {
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(n);
        mask[i] = eroded[lo..hi].iter().any(|&v| v);
    }
}

/// Content bands = runs where mask is false (not gutter).
fn bands_from_mask(gutter: &[bool]) -> Vec<(usize, usize)> {
    let n = gutter.len();
    let mut bands = Vec::new();
    let mut i = 0;
    while i < n {
        while i < n && gutter[i] {
            i += 1;
        }
        if i >= n {
            break;
        }
        let start = i;
        while i < n && !gutter[i] {
            i += 1;
        }
        bands.push((start, i));
    }
    if bands.is_empty() {
        bands.push((0, n));
    }
    bands
}

fn cell_is_empty(lum: &[u8], w: usize, x0: usize, y0: usize, x1: usize, y1: usize, white_gutter: bool) -> bool {
    let mut sum = 0u64;
    let mut count = 0u64;
    let step = 4usize;
    for y in (y0..y1).step_by(step) {
        for x in (x0..x1).step_by(step) {
            sum += lum[y * w + x] as u64;
            count += 1;
        }
    }
    if count == 0 {
        return true;
    }
    let mean = sum as f32 / count as f32;
    if white_gutter {
        mean > 245.0
    } else {
        mean < 12.0
    }
}

fn merge_overlaps(rects: Vec<(usize, usize, usize, usize)>) -> Vec<(usize, usize, usize, usize)> {
    let mut out: Vec<(usize, usize, usize, usize)> = Vec::new();
    for r in rects {
        let mut merged = false;
        for o in out.iter_mut() {
            if overlap_ratio(r, *o) > 0.7 {
                let x0 = o.0.min(r.0);
                let y0 = o.1.min(r.1);
                let x1 = (o.0 + o.2).max(r.0 + r.2);
                let y1 = (o.1 + o.3).max(r.1 + r.3);
                *o = (x0, y0, x1 - x0, y1 - y0);
                merged = true;
                break;
            }
        }
        if !merged {
            out.push(r);
        }
    }
    out
}

fn overlap_ratio(a: (usize, usize, usize, usize), b: (usize, usize, usize, usize)) -> f32 {
    let ax1 = a.0 + a.2;
    let ay1 = a.1 + a.3;
    let bx1 = b.0 + b.2;
    let by1 = b.1 + b.3;
    let ix0 = a.0.max(b.0);
    let iy0 = a.1.max(b.1);
    let ix1 = ax1.min(bx1);
    let iy1 = ay1.min(by1);
    if ix1 <= ix0 || iy1 <= iy0 {
        return 0.0;
    }
    let inter = ((ix1 - ix0) * (iy1 - iy0)) as f32;
    let area_a = (a.2 * a.3) as f32;
    let area_b = (b.2 * b.3) as f32;
    inter / area_a.min(area_b).max(1.0)
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
