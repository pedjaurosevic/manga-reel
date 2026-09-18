//! Italian-style comic panel detection (v4).
//!
//! Confident gutter grids → edge-to-edge panels (full height on screen).
//! Uncertain / unconventional pages → whole page, or clear full-width row
//! slices — never invent tight crops with fake margins.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PanelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

const CACHE_NOTE: u32 = 4;
#[allow(dead_code)]
pub const DETECT_VERSION: u32 = CACHE_NOTE;

/// Detect panel rectangles in normalized page coordinates (0..1).
pub fn detect_panels(rgba: &[u8], width: u32, height: u32) -> Vec<PanelRect> {
    if width < 8 || height < 8 || rgba.len() < (width * height * 4) as usize {
        return full_page();
    }

    let w = width as usize;
    let h = height as usize;
    let (sw, sh, lum) = downsample_luma(rgba, w, h, 1200);

    let white = detect_grid(&lum, sw, sh, true);
    let dark = detect_grid(&lum, sw, sh, false);
    let (best, best_score) = pick_better_scored(white, dark, sw, sh);

    // High confidence grid → edge-to-edge panel rects (no inset/margin pad).
    if best_score >= 6.5 && looks_like_clean_grid(&best, sw, sh) {
        let panels = to_norm_panels(&best, sw, sh);
        if !panels.is_empty() {
            return panels;
        }
    }

    // Medium: clear horizontal rows only → full-width page slices (safer than
    // inventing vertical cuts on unconventional art).
    let row_white = detect_row_slices(&lum, sw, sh, true);
    let row_dark = detect_row_slices(&lum, sw, sh, false);
    let (rows, row_score) = pick_better_scored(row_white, row_dark, sw, sh);
    if row_score >= 4.0 && rows.len() >= 2 && rows.len() <= 8 {
        let panels = to_norm_panels(&rows, sw, sh);
        if panels.iter().all(|p| p.w >= 0.85) {
            return panels;
        }
    }

    // Low confidence / weird page → whole page as one panel.
    let _ = best;
    full_page()
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
        .filter(|p| p.w * p.h >= 0.04)
        .collect();
    // Snap near-full spans to true edges so PC fullscreen has no fake margin.
    for p in &mut panels {
        if p.x <= 0.02 {
            p.w = (p.w + p.x).min(1.0);
            p.x = 0.0;
        }
        if p.y <= 0.02 {
            p.h = (p.h + p.y).min(1.0);
            p.y = 0.0;
        }
        if p.x + p.w >= 0.98 {
            p.w = 1.0 - p.x;
        }
        if p.y + p.h >= 0.98 {
            p.h = 1.0 - p.y;
        }
    }
    if panels.len() > 1 {
        let mut areas: Vec<f64> = panels.iter().map(|p| p.w * p.h).collect();
        areas.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        let median_area = areas[areas.len() / 2];
        panels.retain(|p| p.w * p.h >= median_area * 0.40);
    }
    panels
}

fn looks_like_clean_grid(rects: &[(usize, usize, usize, usize)], w: usize, h: usize) -> bool {
    let n = rects.len();
    if n < 2 || n > 12 {
        return false;
    }
    let page = (w * h) as f32;
    let mut area = 0f32;
    for &(_, _, pw, ph) in rects {
        let a = (pw * ph) as f32;
        if a / page < 0.045 {
            return false;
        }
        let ar = pw as f32 / ph.max(1) as f32;
        if !(0.4..=3.2).contains(&ar) {
            return false;
        }
        area += a;
    }
    let coverage = area / page;
    (0.55..=1.15).contains(&coverage)
}

/// Full-width horizontal bands only (no vertical splits).
fn detect_row_slices(lum: &[u8], w: usize, h: usize, white: bool) -> Vec<(usize, usize, usize, usize)> {
    let mut row_gutter = vec![false; h];
    let mut row_buf = vec![0u8; w];
    for y in 0..h {
        for x in 0..w {
            row_buf[x] = lum[y * w + x];
        }
        row_gutter[y] = line_is_gutter(&row_buf, white);
    }
    thicken_mask(&mut row_gutter, 1);
    let bands = content_bands(&row_gutter, ((h as f32) * 0.05).max(12.0) as usize);
    let min_h = ((h as f32) * 0.08).max(16.0) as usize;
    let mut rects = Vec::new();
    for &(y0, y1) in &bands {
        let bh = y1.saturating_sub(y0);
        if bh < min_h {
            continue;
        }
        if cell_mostly_empty(lum, w, 0, y0, w, y1, white) {
            continue;
        }
        // Edge-to-edge horizontally — full page width slice.
        rects.push((0, y0, w, bh));
    }
    if rects.is_empty() {
        vec![(0, 0, w, h)]
    } else {
        rects
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

/// True when this line is a gutter across most of its length (not just average).
fn line_is_gutter(samples: &[u8], white: bool) -> bool {
    if samples.is_empty() {
        return false;
    }
    let n = samples.len() as f32;
    let (hi, lo) = if white {
        (235u8, 210u8)
    } else {
        (45u8, 25u8)
    };
    let mut strong = 0u32;
    let mut soft = 0u32;
    for &v in samples {
        if white {
            if v >= hi {
                strong += 1;
            } else if v >= lo {
                soft += 1;
            }
        } else if v <= lo {
            strong += 1;
        } else if v <= hi {
            soft += 1;
        }
    }
    // Require a clear majority of true gutter pixels so inked figures don't
    // create fake splits inside a panel.
    let ratio = (strong as f32 + soft as f32 * 0.45) / n;
    ratio >= 0.78 && strong as f32 / n >= 0.45
}

fn detect_grid(lum: &[u8], w: usize, h: usize, white: bool) -> Vec<(usize, usize, usize, usize)> {
    // Horizontal gutters: rows that are gutter-colored across most of the width.
    let mut row_gutter = vec![false; h];
    let mut row_buf = vec![0u8; w];
    for y in 0..h {
        for x in 0..w {
            row_buf[x] = lum[y * w + x];
        }
        row_gutter[y] = line_is_gutter(&row_buf, white);
    }
    thicken_mask(&mut row_gutter, 1);
    let row_bands = content_bands(&row_gutter, ((h as f32) * 0.04).max(10.0) as usize);

    let min_w = ((w as f32) * 0.14).max(16.0) as usize;
    let min_h = ((h as f32) * 0.08).max(16.0) as usize;

    let mut rects = Vec::new();
    for &(y0, y1) in &row_bands {
        let band_h = y1.saturating_sub(y0);
        if band_h < min_h {
            continue;
        }

        let mut col_gutter = vec![false; w];
        let mut col_buf = vec![0u8; band_h];
        for x in 0..w {
            for (i, y) in (y0..y1).enumerate() {
                col_buf[i] = lum[y * w + x];
            }
            col_gutter[x] = line_is_gutter(&col_buf, white);
        }
        // Ignore outer margins as "gutters" for splitting — keep as page edge.
        let margin = ((w as f32) * 0.03) as usize;
        for i in 0..margin.min(w) {
            col_gutter[i] = false;
            col_gutter[w - 1 - i] = false;
        }
        thicken_mask(&mut col_gutter, 1);

        let mut col_bands = content_bands(&col_gutter, ((w as f32) * 0.08).max(12.0) as usize);
        // Italian pages: prefer at most 3 panels per row; if we got too many,
        // keep the widest bands only (false gutters from figures).
        if col_bands.len() > 3 {
            col_bands.sort_by_key(|&(a, b)| std::cmp::Reverse(b.saturating_sub(a)));
            col_bands.truncate(3);
            col_bands.sort_by_key(|&(a, _)| a);
        }

        let mut row_rects = Vec::new();
        for &(x0, x1) in &col_bands {
            let pw = x1.saturating_sub(x0);
            if pw < min_w {
                continue;
            }
            if cell_mostly_empty(lum, w, x0, y0, x1, y1, white) {
                continue;
            }
            // Edge-to-edge within the gutter cell — no inset/margin pad.
            let tw = x1.saturating_sub(x0);
            let th = y1.saturating_sub(y0);
            if tw >= min_w && th >= min_h {
                row_rects.push((x0, y0, tw, th));
            }
        }

        if row_rects.is_empty() {
            if !cell_mostly_empty(lum, w, 0, y0, w, y1, white) {
                row_rects.push((0, y0, w, band_h));
            }
        }

        // If one very wide cell remains and a strong mid gutter exists, split once.
        let mut refined = Vec::new();
        for (x, y, pw, ph) in row_rects {
            if pw as f32 > w as f32 * 0.70 && ph as f32 > h as f32 * 0.10 {
                if let Some((left, right)) = try_binary_vertical_split(lum, w, x, y, x + pw, y + ph, white, min_w)
                {
                    refined.push(left);
                    refined.push(right);
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
        let row_tol = (h as f32 * 0.05) as usize;
        if ay.abs_diff(by) <= row_tol {
            a.0.cmp(&b.0)
        } else {
            ay.cmp(&by)
        }
    });

    merge_overlaps(rects)
}

fn try_binary_vertical_split(
    lum: &[u8],
    stride: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    white: bool,
    min_w: usize,
) -> Option<((usize, usize, usize, usize), (usize, usize, usize, usize))> {
    let bw = x1.saturating_sub(x0);
    let bh = y1.saturating_sub(y0);
    if bw < min_w * 2 || bh < 8 {
        return None;
    }
    let margin = ((bw as f32) * 0.18) as usize;
    let mut best: Option<(usize, f32)> = None;
    let mut col_buf = vec![0u8; bh];
    for i in margin..(bw.saturating_sub(margin)) {
        for (k, y) in (y0..y1).enumerate() {
            col_buf[k] = lum[y * stride + (x0 + i)];
        }
        if !line_is_gutter(&col_buf, white) {
            continue;
        }
        // Prefer gutters near the horizontal center for 2-up Italian rows.
        let center = bw as f32 / 2.0;
        let dist = (i as f32 - center).abs() / center;
        let score = 1.0 - dist;
        if best.map(|(_, s)| score > s).unwrap_or(true) {
            best = Some((i, score));
        }
    }
    let (cut, _) = best?;
    // Expand cut to full gutter run
    let mut a = cut;
    let mut b = cut + 1;
    while a > margin {
        for (k, y) in (y0..y1).enumerate() {
            col_buf[k] = lum[y * stride + (x0 + a - 1)];
        }
        if line_is_gutter(&col_buf, white) {
            a -= 1;
        } else {
            break;
        }
    }
    while b < bw.saturating_sub(margin) {
        for (k, y) in (y0..y1).enumerate() {
            col_buf[k] = lum[y * stride + (x0 + b)];
        }
        if line_is_gutter(&col_buf, white) {
            b += 1;
        } else {
            break;
        }
    }
    let left_w = a;
    let right_x = b;
    let right_w = bw.saturating_sub(right_x);
    if left_w < min_w || right_w < min_w {
        return None;
    }
    Some((
        (x0, y0, left_w, bh),
        (x0 + right_x, y0, right_w, bh),
    ))
}

fn thicken_mask(mask: &mut [bool], radius: usize) {
    let n = mask.len();
    let orig = mask.to_vec();
    for i in 0..n {
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(n);
        mask[i] = orig[lo..hi].iter().any(|&v| v);
    }
}

fn content_bands(gutter: &[bool], min_len: usize) -> Vec<(usize, usize)> {
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

fn cell_mostly_empty(
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
            sum += lum[y * w + x] as u64;
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
    if std < 10.0 {
        if white_gutter {
            return mean > 235.0;
        } else {
            return mean < 22.0;
        }
    }
    false
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
    let mut tiny_pen = 0f32;
    for &(x, y, pw, ph) in rects {
        let _ = (x, y);
        let a = (pw * ph) as f32;
        area += a;
        let ar = pw as f32 / ph as f32;
        // Italian panels are usually wider or roughly square — not ultra-thin.
        if (0.45..=2.8).contains(&ar) {
            shape += 1.0;
        } else {
            shape -= 0.5;
        }
        if a / page < 0.04 {
            tiny_pen += 1.5;
        }
    }
    let n = rects.len() as f32;
    let coverage = (area / page).clamp(0.0, 1.2);
    // Sweet spot: 2–12 panels, good coverage, few tiny fragments.
    let count_score = if (2.0..=12.0).contains(&n) {
        3.0 + (1.0 - (n - 6.0).abs() / 6.0)
    } else if n == 1.0 {
        0.8
    } else {
        0.2
    };
    count_score + shape + coverage * 2.5 - tiny_pen
}

fn merge_overlaps(rects: Vec<(usize, usize, usize, usize)>) -> Vec<(usize, usize, usize, usize)> {
    let mut out: Vec<(usize, usize, usize, usize)> = Vec::new();
    for r in rects {
        let mut merged = false;
        for o in out.iter_mut() {
            if overlap_ratio(r, *o) > 0.55 {
                let x0 = r.0.min(o.0);
                let y0 = r.1.min(o.1);
                let x1 = (r.0 + r.2).max(o.0 + o.2);
                let y1 = (r.1 + r.3).max(o.1 + o.3);
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
    let union = (a.2 * a.3 + b.2 * b.3) as f32 - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
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
    fn full_page_fallback_on_tiny() {
        let rgba = vec![0u8; 4];
        let p = detect_panels(&rgba, 1, 1);
        assert_eq!(p.len(), 1);
    }
}
