use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PanelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

const CACHE_NOTE: u32 = 7;
#[allow(dead_code)]
pub const DETECT_VERSION: u32 = CACHE_NOTE;

/// Min leaf area as fraction of page (Comic Trim ≈ page/70).
const MIN_LEAF_FRAC: f32 = 1.0 / 70.0;
/// Max BSP recursion depth / split budget.
const MAX_DEPTH: u32 = 16;
const MAX_SPLITS: u32 = 48;
/// Row/col is a gutter band if ≥ this fraction of pixels are gutter.
const BAND_GUTTER_FRAC: f32 = 0.88;
/// Minimum empty-band thickness (fraction of the *region* span).
const MIN_BAND_FRAC: f32 = 0.012;
const MIN_BAND_PX: usize = 2;
/// Paper / black proximity (luma 0..255).
const PAPER_TOL: u8 = 38;
const BLACK_TOL: u8 = 28;
/// Extreme gutter ratio → treat as single page.
const EXTREME_GUTTER_LO: f32 = 0.04;
const EXTREME_GUTTER_HI: f32 = 0.96;

/// Detect panel rectangles in normalized page coordinates (0..1).
pub fn detect_panels(rgba: &[u8], width: u32, height: u32) -> Vec<PanelRect> {
    if width < 8 || height < 8 || rgba.len() < (width * height * 4) as usize {
        return full_page();
    }

    let w = width as usize;
    let h = height as usize;
    let (sw, sh, lum) = downsample_luma(rgba, w, h, 1400);
    let paper = sample_paper_luma(&lum, sw, sh);
    let gutter = binarize_gutter(&lum, paper);

    let gutter_ratio = gutter.iter().filter(|&&g| g).count() as f32 / (sw * sh) as f32;
    if gutter_ratio < EXTREME_GUTTER_LO || gutter_ratio > EXTREME_GUTTER_HI {
        return full_page();
    }

    let page_area = (sw * sh) as f32;
    let min_leaf = (page_area * MIN_LEAF_FRAC).max(64.0) as usize;
    let mut splits = 0u32;
    let mut rects = Vec::new();
    bsp_split(
        &gutter,
        sw,
        sh,
        Region {
            x0: 0,
            y0: 0,
            x1: sw,
            y1: sh,
        },
        0,
        min_leaf,
        &mut splits,
        &mut rects,
    );

    if rects.is_empty() {
        return fallback_rows_or_page(&gutter, sw, sh);
    }

    // Prefer BSP result when we got a plausible multi-panel layout.
    if looks_usable(&rects, sw, sh) {
        return to_norm_panels(&rects, sw, sh);
    }

    // Uncertain: try clean full-width row slices, else whole page.
    fallback_rows_or_page(&gutter, sw, sh)
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

/// Sample ~12 margin points (corners, mid-edges, ¼/¾ height) and cluster luma.
fn sample_paper_luma(lum: &[u8], w: usize, h: usize) -> u8 {
    let pts: [(usize, usize); 12] = [
        (0, 0),
        (w / 2, 0),
        (w.saturating_sub(1), 0),
        (0, h / 4),
        (w.saturating_sub(1), h / 4),
        (0, h / 2),
        (w.saturating_sub(1), h / 2),
        (0, (3 * h) / 4),
        (w.saturating_sub(1), (3 * h) / 4),
        (0, h.saturating_sub(1)),
        (w / 2, h.saturating_sub(1)),
        (w.saturating_sub(1), h.saturating_sub(1)),
    ];
    let mut samples: Vec<u8> = pts
        .iter()
        .map(|&(x, y)| lum[y.min(h - 1) * w + x.min(w - 1)])
        .collect();
    samples.sort_unstable();

    // Prefer near-white or near-black margin clusters (paper), not mid-gray art.
    let mut best = samples[samples.len() / 2];
    let mut best_score = -1i32;
    // Simple 1D clustering: try each sample as center, score near neighbors that
    // look like paper (very light or very dark).
    for &c in &samples {
        let paperish = c >= 170 || c <= 45;
        if !paperish {
            continue;
        }
        let mut score = 0i32;
        for &s in &samples {
            let d = s.abs_diff(c) as i32;
            if d <= PAPER_TOL as i32 {
                score += 10 - d / 3;
            }
        }
        if score > best_score {
            best_score = score;
            best = c;
        }
    }
    if best_score < 0 {
        // Fallback: median of margin samples.
        best = samples[samples.len() / 2];
    }
    best
}

fn is_gutter_luma(v: u8, paper: u8) -> bool {
    // Near black always counts as gutter (common comic gutters / borders).
    if v <= BLACK_TOL {
        return true;
    }
    // Near sampled paper (usually white / off-white).
    v.abs_diff(paper) <= PAPER_TOL
}

fn binarize_gutter(lum: &[u8], paper: u8) -> Vec<bool> {
    lum.iter().map(|&v| is_gutter_luma(v, paper)).collect()
}
