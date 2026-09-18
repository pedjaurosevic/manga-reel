//! Classical panel detection: projection gutters + per-row splits + content tighten.
//! Version 2 — better than the global grid approach for irregular comic layouts.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PanelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

const CACHE_NOTE: u32 = 2;
#[allow(dead_code)]
pub const DETECT_VERSION: u32 = CACHE_NOTE;

/// Detect panel rectangles in normalized page coordinates (0..1).
pub fn detect_panels(rgba: &[u8], width: u32, height: u32) -> Vec<PanelRect> {
    if width < 8 || height < 8 || rgba.len() < (width * height * 4) as usize {
        return full_page();
    }

    let w = width as usize;
    let h = height as usize;

    let max_side = 1400usize;
    let (sw, sh, lum) = downsample_luma(rgba, w, h, max_side);

    let white = detect_layout(&lum, sw, sh, true);
    let dark = detect_layout(&lum, sw, sh, false);
    let mut best = pick_better(white, dark, sw, sh);

    if best.len() < 2 {
        // Last resort: recursive median-split on content
        let split = recursive_split(&lum, sw, sh, 0, 0, sw, sh, 0);
        if split.len() > best.len() {
            best = split;
        }
    }

    let panels: Vec<PanelRect> = best
        .into_iter()
        .map(|(x, y, pw, ph)| PanelRect {
            x: x as f64 / sw as f64,
            y: y as f64 / sh as f64,
            w: (pw as f64 / sw as f64).clamp(0.02, 1.0),
            h: (ph as f64 / sh as f64).clamp(0.02, 1.0),
        })
        .filter(|p| p.w * p.h >= 0.008)
        .collect();

    if panels.is_empty() {
        full_page()
    } else {
        panels
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

fn detect_layout(lum: &[u8], w: usize, h: usize, white_gutters: bool) -> Vec<(usize, usize, usize, usize)> {
    // Full-page row means → horizontal gutters → content bands
    let mut row_mean = vec![0f32; h];
    for y in 0..h {
        let mut s = 0u32;
        for x in 0..w {
            s += lum[y * w + x] as u32;
        }
        row_mean[y] = s as f32 / w as f32;
    }

    let row_gutter = mark_gutters_adaptive(&row_mean, white_gutters);
    let row_bands = bands_from_mask(&row_gutter, (h as f32 * 0.03) as usize);

    let min_w = ((w as f32) * 0.06).max(8.0) as usize;
    let min_h = ((h as f32) * 0.045).max(8.0) as usize;

    let mut rects = Vec::new();

    for &(y0, y1) in &row_bands {
        let band_h = y1.saturating_sub(y0);
        if band_h < min_h {
            continue;
        }

        // Per-row column projection (key improvement vs global grid)
        let mut col_mean = vec![0f32; w];
        for x in 0..w {
            let mut s = 0u32;
            for y in y0..y1 {
                s += lum[y * w + x] as u32;
            }
            col_mean[x] = s as f32 / band_h as f32;
        }
        let col_gutter = mark_gutters_adaptive(&col_mean, white_gutters);
        let col_bands = bands_from_mask(&col_gutter, (w as f32 * 0.02) as usize);

        let mut row_rects = Vec::new();
        for &(x0, x1) in &col_bands {
            let pw = x1.saturating_sub(x0);
            if pw < min_w {
                continue;
            }
            if cell_is_empty(lum, w, x0, y0, x1, y1, white_gutters) {
                continue;
            }
            // Tighten to actual ink/content
            if let Some((tx, ty, tw, th)) = tighten(lum, w, h, x0, y0, x1, y1, white_gutters) {
                if tw >= min_w && th >= min_h {
                    row_rects.push((tx, ty, tw, th));
                }
            } else {
                row_rects.push((x0, y0, pw, band_h));
            }
        }

        if row_rects.is_empty() {
            // Whole band as one panel if it has content
            if !cell_is_empty(lum, w, 0, y0, w, y1, white_gutters) {
                if let Some((tx, ty, tw, th)) = tighten(lum, w, h, 0, y0, w, y1, white_gutters) {
                    if tw >= min_w && th >= min_h {
                        row_rects.push((tx, ty, tw, th));
                    }
                } else {
                    row_rects.push((0, y0, w, band_h));
                }
            }
        }

        // If a "panel" is still very wide, try one more vertical split inside it
        let mut refined = Vec::new();
        for (x, y, pw, ph) in row_rects {
            if pw as f32 > w as f32 * 0.72 && ph as f32 > h as f32 * 0.12 {
                let extra = split_vertically(lum, w, h, x, y, x + pw, y + ph, white_gutters, min_w);
                if extra.len() >= 2 {
                    refined.extend(extra);
                    continue;
                }
            }
            refined.push((x, y, pw, ph));
        }
        rects.extend(refined);
    }

    if rects.is_empty() {
        return vec![(0, 0, w, h)];
    }

    rects.sort_by(|a, b| {
        let ay = a.1 + a.3 / 2;
        let by = b.1 + b.3 / 2;
        let row_tol = (h as f32 * 0.04) as usize;
        if ay.abs_diff(by) <= row_tol {
            a.0.cmp(&b.0)
        } else {
            ay.cmp(&by)
        }
    });

    merge_overlaps(rects)
}

fn split_vertically(
    lum: &[u8],
    stride: usize,
    page_h: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    white: bool,
    min_w: usize,
) -> Vec<(usize, usize, usize, usize)> {
    let bw = x1.saturating_sub(x0);
    let bh = y1.saturating_sub(y0);
    if bw < min_w * 2 || bh < 8 {
        return vec![];
    }
    let mut col_mean = vec![0f32; bw];
    for i in 0..bw {
        let mut s = 0u32;
        for y in y0..y1 {
            s += lum[y * stride + (x0 + i)] as u32;
        }
        col_mean[i] = s as f32 / bh as f32;
    }
    let mut g = mark_gutters_adaptive(&col_mean, white);
    let margin = ((bw as f32) * 0.08) as usize;
    for i in 0..margin.min(bw) {
        g[i] = false;
        g[bw - 1 - i] = false;
    }
    let bands = bands_from_mask(&g, ((bw as f32) * 0.03) as usize);
    let mut out = Vec::new();
    for &(a, b) in &bands {
        let pw = b.saturating_sub(a);
        if pw < min_w {
            continue;
        }
        let gx0 = x0 + a;
        let gx1 = x0 + b;
        if cell_is_empty(lum, stride, gx0, y0, gx1, y1, white) {
            continue;
        }
        if let Some(t) = tighten(lum, stride, page_h, gx0, y0, gx1, y1, white) {
            out.push(t);
        } else {
            out.push((gx0, y0, pw, bh));
        }
    }
    out
}

fn mark_gutters_adaptive(means: &[f32], white: bool) -> Vec<bool> {
    let n = means.len();
    let mut mask = vec![false; n];
    if n == 0 {
        return mask;
    }
    let mut sorted = means.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p10 = sorted[(n as f32 * 0.10) as usize];
    let p50 = sorted[n / 2];
    let p90 = sorted[(n as f32 * 0.90) as usize];

    let threshold = if white {
        // Gutters near the bright end; require separation from midtones
        let t = (p90 * 0.55 + p50 * 0.45).max(p50 + 18.0);
        t.clamp(170.0, 248.0)
    } else {
        let t = (p10 * 0.55 + p50 * 0.45).min(p50 - 18.0);
        t.clamp(8.0, 90.0)
    };

    for (i, &m) in means.iter().enumerate() {
        mask[i] = if white { m >= threshold } else { m <= threshold };
    }

    // Soften: fill tiny non-gutter holes inside gutters and vice versa
    dilate_erode(&mut mask, 1);
    // Expand gutters slightly so thin lines count
    let eroded = mask.clone();
    let radius = 1usize;
    for i in 0..n {
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(n);
        mask[i] = eroded[lo..hi].iter().any(|&v| v);
    }
    mask
}

fn dilate_erode(mask: &mut [bool], radius: usize) {
    let n = mask.len();
    let orig = mask.to_vec();
    for i in 0..n {
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(n);
        mask[i] = orig[lo..hi].iter().all(|&v| v);
    }
    let eroded = mask.to_vec();
    for i in 0..n {
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(n);
        mask[i] = eroded[lo..hi].iter().any(|&v| v);
    }
}

fn bands_from_mask(gutter: &[bool], min_len: usize) -> Vec<(usize, usize)> {
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
        if i.saturating_sub(start) >= min_len.max(1) {
            bands.push((start, i));
        }
    }
    if bands.is_empty() {
        bands.push((0, n));
    }
    bands
}

fn cell_is_empty(
    lum: &[u8],
    w: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    white_gutter: bool,
) -> bool {
    let mut sum = 0u64;
    let mut count = 0u64;
    let mut var_acc = 0f64;
    let step = 3usize;
    for y in (y0..y1).step_by(step) {
        for x in (x0..x1).step_by(step) {
            let v = lum[y * w + x] as u64;
            sum += v;
            count += 1;
        }
    }
    if count == 0 {
        return true;
    }
    let mean = sum as f32 / count as f32;
    for y in (y0..y1).step_by(step) {
        for x in (x0..x1).step_by(step) {
            let d = lum[y * w + x] as f64 - mean as f64;
            var_acc += d * d;
        }
    }
    let std = (var_acc / count as f64).sqrt();
    // Empty gutter-like cells: uniform and extreme luminance
    if std < 8.0 {
        if white_gutter {
            return mean > 238.0;
        } else {
            return mean < 18.0;
        }
    }
    false
}

/// Shrink rect to bounding box of non-gutter content pixels.
fn tighten(
    lum: &[u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    white_gutter: bool,
) -> Option<(usize, usize, usize, usize)> {
    let x1 = x1.min(w);
    let y1 = y1.min(h);
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let mut min_x = x1;
    let mut min_y = y1;
    let mut max_x = x0;
    let mut max_y = y0;
    let step = 2usize;
    for y in (y0..y1).step_by(step) {
        for x in (x0..x1).step_by(step) {
            let v = lum[y * w + x];
            let is_gutter = if white_gutter { v >= 245 } else { v <= 12 };
            if !is_gutter {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if max_x <= min_x || max_y <= min_y {
        return None;
    }
    // Small pad
    let pad = 2usize;
    let tx = min_x.saturating_sub(pad).max(x0);
    let ty = min_y.saturating_sub(pad).max(y0);
    let tx1 = (max_x + pad + 1).min(x1);
    let ty1 = (max_y + pad + 1).min(y1);
    Some((tx, ty, tx1 - tx, ty1 - ty))
}

fn recursive_split(
    lum: &[u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    depth: usize,
) -> Vec<(usize, usize, usize, usize)> {
    let pw = x1.saturating_sub(x0);
    let ph = y1.saturating_sub(y0);
    if depth > 4 || pw < (w as f32 * 0.12) as usize || ph < (h as f32 * 0.10) as usize {
        return vec![(x0, y0, pw, ph)];
    }

    // Prefer splitting along the stronger gutter axis
    let mut row_mean = vec![0f32; ph];
    for i in 0..ph {
        let mut s = 0u32;
        for x in x0..x1 {
            s += lum[(y0 + i) * w + x] as u32;
        }
        row_mean[i] = s as f32 / pw as f32;
    }
    let mut col_mean = vec![0f32; pw];
    for i in 0..pw {
        let mut s = 0u32;
        for y in y0..y1 {
            s += lum[y * w + (x0 + i)] as u32;
        }
        col_mean[i] = s as f32 / ph as f32;
    }

    let (split_row, row_score) = best_gutter_cut(&row_mean);
    let (split_col, col_score) = best_gutter_cut(&col_mean);

    if row_score < 12.0 && col_score < 12.0 {
        return vec![(x0, y0, pw, ph)];
    }

    if row_score >= col_score {
        let mid = y0 + split_row;
        if mid <= y0 + 4 || mid >= y1.saturating_sub(4) {
            return vec![(x0, y0, pw, ph)];
        }
        let mut a = recursive_split(lum, w, h, x0, y0, x1, mid, depth + 1);
        let b = recursive_split(lum, w, h, x0, mid, x1, y1, depth + 1);
        a.extend(b);
        a
    } else {
        let mid = x0 + split_col;
        if mid <= x0 + 4 || mid >= x1.saturating_sub(4) {
            return vec![(x0, y0, pw, ph)];
        }
        let mut a = recursive_split(lum, w, h, x0, y0, mid, y1, depth + 1);
        let b = recursive_split(lum, w, h, mid, y0, x1, y1, depth + 1);
        a.extend(b);
        a
    }
}

fn best_gutter_cut(means: &[f32]) -> (usize, f32) {
    let n = means.len();
    if n < 16 {
        return (n / 2, 0.0);
    }
    let avg = means.iter().sum::<f32>() / n as f32;
    let mut best_i = n / 2;
    let mut best_score = 0f32;
    let lo = n / 5;
    let hi = n - n / 5;
    for i in lo..hi {
        // Bright or dark spike relative to neighbors = gutter
        let m = means[i];
        let local = (means[i.saturating_sub(2)] + means[(i + 2).min(n - 1)]) * 0.5;
        let bright = m - local;
        let dark = local - m;
        let score = bright.max(dark) + (m - avg).abs() * 0.15;
        if score > best_score {
            best_score = score;
            best_i = i;
        }
    }
    (best_i, best_score)
}

fn pick_better(
    a: Vec<(usize, usize, usize, usize)>,
    b: Vec<(usize, usize, usize, usize)>,
    w: usize,
    h: usize,
) -> Vec<(usize, usize, usize, usize)> {
    if score(&a, w, h) >= score(&b, w, h) {
        a
    } else {
        b
    }
}

fn score(rects: &[(usize, usize, usize, usize)], w: usize, h: usize) -> f32 {
    if rects.is_empty() {
        return 0.0;
    }
    let page = (w * h) as f32;
    let mut cover = 0f32;
    let mut good = 0f32;
    for &(x, y, pw, ph) in rects {
        let area = (pw * ph) as f32;
        cover += area;
        let ar = pw as f32 / ph.max(1) as f32;
        // Prefer comic-like aspect ratios
        if (0.35..=3.5).contains(&ar) && area / page >= 0.02 && area / page <= 0.85 {
            good += 1.0;
        }
    }
    let n = rects.len() as f32;
    // Reward multiple panels, good shapes, and reasonable coverage (not 1 huge page)
    let cover_ratio = (cover / page).min(1.2);
    let cover_score = if cover_ratio < 0.35 {
        cover_ratio
    } else if cover_ratio > 1.05 {
        0.5
    } else {
        1.0
    };
    good * 2.0 + n.min(12.0) + cover_score * 3.0 - if n <= 1.0 { 4.0 } else { 0.0 }
}

fn merge_overlaps(rects: Vec<(usize, usize, usize, usize)>) -> Vec<(usize, usize, usize, usize)> {
    let mut out: Vec<(usize, usize, usize, usize)> = Vec::new();
    for r in rects {
        let mut merged = false;
        for o in out.iter_mut() {
            if overlap_ratio(r, *o) > 0.65 {
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
