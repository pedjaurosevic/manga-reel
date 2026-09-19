//! Paper-color gutter segmentation on original page pixels.
//! Independent Rust implementation: sample the margin color, trim exterior paper,
//! then divide along uninterrupted paper bands, preserving printed frame edges.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PanelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
pub const DETECT_VERSION: u32 = 8;

fn full_page() -> Vec<PanelRect> {
    vec![PanelRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    }]
}

#[derive(Clone, Copy)]
struct Region {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}
impl Region {
    fn area(self) -> usize {
        (self.x1 - self.x0) * (self.y1 - self.y0)
    }
}

struct Mask {
    width: usize,
    paper: Vec<bool>,
    min_area: usize,
}
impl Mask {
    fn clear_line(&self, r: Region, horizontal: bool, at: usize) -> bool {
        let (start, end) = if horizontal {
            (r.x0, r.x1)
        } else {
            (r.y0, r.y1)
        };
        // Ink at both edges is a panel frame, even if its interior is white.
        // A percentage-only threshold incorrectly cuts through these frames.
        let mut ink = 0;
        let mut first = None;
        for p in start..end {
            let index = if horizontal {
                at * self.width + p
            } else {
                p * self.width + at
            };
            if !self.paper[index] {
                ink += 1;
                let first = *first.get_or_insert(p);
                if ink > (end - start) / 500 || p - first > 2 {
                    return false;
                }
            }
        }
        true
    }
    fn trim(&self, mut r: Region) -> Region {
        while r.y0 < r.y1 && self.clear_line(r, true, r.y0) {
            r.y0 += 1;
        }
        while r.y1 > r.y0 && self.clear_line(r, true, r.y1 - 1) {
            r.y1 -= 1;
        }
        while r.x0 < r.x1 && self.clear_line(r, false, r.x0) {
            r.x0 += 1;
        }
        while r.x1 > r.x0 && self.clear_line(r, false, r.x1 - 1) {
            r.x1 -= 1;
        }
        r
    }
    fn split(&self, region: Region, depth: usize, out: &mut Vec<Region>) {
        let r = self.trim(region);
        if r.area() < self.min_area {
            return;
        }
        if depth < 16 && out.len() < 48 {
            // Horizontal precedence preserves row reading order on comic grids.
            for horizontal in [true, false] {
                let (start, end) = if horizontal {
                    (r.y0, r.y1)
                } else {
                    (r.x0, r.x1)
                };
                let mut p = start + 1;
                while p + 1 < end {
                    if !self.clear_line(r, horizontal, p) {
                        p += 1;
                        continue;
                    }
                    let begin = p;
                    while p < end && self.clear_line(r, horizontal, p) {
                        p += 1;
                    }
                    if p - begin < 2 {
                        continue;
                    }
                    let (mut a, mut b) = (r, r);
                    if horizontal {
                        a.y1 = begin;
                        b.y0 = p;
                    } else {
                        a.x1 = begin;
                        b.x0 = p;
                    }
                    a = self.trim(a);
                    b = self.trim(b);
                    if a.area() >= self.min_area && b.area() >= self.min_area {
                        self.split(a, depth + 1, out);
                        self.split(b, depth + 1, out);
                        return;
                    }
                }
            }
        }
        out.push(r);
    }
}

pub fn detect_panels(rgba: &[u8], width: u32, height: u32) -> Vec<PanelRect> {
    let (w, h) = (width as usize, height as usize);
    let Some(bytes) = w.checked_mul(h).and_then(|n| n.checked_mul(4)) else {
        return full_page();
    };
    if w < 8 || h < 8 || rgba.len() < bytes {
        return full_page();
    }
    let ratio = 1600.0 / w.max(h) as f64;
    let sw = (w as f64 * ratio.min(1.0)).round().max(8.0) as usize;
    let sh = (h as f64 * ratio.min(1.0)).round().max(8.0) as usize;
    let pixel = |x: usize, y: usize| -> [u8; 3] {
        let i = ((y * h / sh) * w + x * w / sw) * 4;
        let alpha = rgba[i + 3] as u32;
        std::array::from_fn(|c| ((rgba[i + c] as u32 * alpha + 255 * (255 - alpha)) / 255) as u8)
    };
    let mut samples = Vec::new();
    for n in 0..32 {
        let x = n * (sw - 1) / 31;
        let y = n * (sh - 1) / 31;
        for inset in [0, 2] {
            samples.extend([
                pixel(x, inset),
                pixel(x, sh - 1 - inset),
                pixel(inset, y),
                pixel(sw - 1 - inset, y),
            ]);
        }
    }
    let close =
        |a: [u8; 3], b: [u8; 3], tolerance: u8| (0..3).all(|c| a[c].abs_diff(b[c]) <= tolerance);
    let background = *samples
        .iter()
        .max_by_key(|&&candidate| samples.iter().filter(|&&s| close(s, candidate, 18)).count())
        .unwrap();
    let mut paper = Vec::with_capacity(sw * sh);
    for y in 0..sh {
        for x in 0..sw {
            paper.push(close(pixel(x, y), background, 24));
        }
    }
    let mask = Mask {
        width: sw,
        paper,
        min_area: (sw * sh / 100).max(16),
    };
    let mut regions = Vec::new();
    mask.split(
        Region {
            x0: 0,
            y0: 0,
            x1: sw,
            y1: sh,
        },
        0,
        &mut regions,
    );
    if regions.is_empty() {
        return full_page();
    }
    let panels: Vec<_> = regions
        .into_iter()
        .map(|r| {
            // One analysis pixel of breathing room keeps antialiased frame edges.
            let x = r.x0.saturating_sub(1);
            let y = r.y0.saturating_sub(1);
            PanelRect {
                x: x as f64 / sw as f64,
                y: y as f64 / sh as f64,
                w: ((r.x1 + 1).min(sw) - x) as f64 / sw as f64,
                h: ((r.y1 + 1).min(sh) - y) as f64 / sh as f64,
            }
        })
        .collect();
    order_panels(&panels, false)
}

pub fn order_panels(panels: &[PanelRect], rtl: bool) -> Vec<PanelRect> {
    if panels.is_empty() {
        return full_page();
    }
    let mut remaining = panels.to_vec();
    remaining.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut result = Vec::new();
    while !remaining.is_empty() {
        let anchor = remaining[0];
        let mut row = Vec::new();
        remaining.retain(|p| {
            if (p.y - anchor.y).abs() <= 0.04_f64.min(anchor.h.min(p.h) * 0.2) {
                row.push(*p);
                false
            } else {
                true
            }
        });
        row.sort_by(|a, b| {
            if rtl {
                b.x.total_cmp(&a.x)
            } else {
                a.x.total_cmp(&b.x)
            }
        });
        result.extend(row);
    }
    result
}

/// Contain the entire panel, centered, with a black inset on all four sides.
pub fn panel_layout(pw: f64, ph: f64, vw: f64, vh: f64) -> (f64, f64, f64, f64) {
    let border = 8.0_f64.min(vw.min(vh).max(0.0) * 0.02);
    let scale = ((vw - 2.0 * border).max(0.0) / pw.max(1.0))
        .min((vh - 2.0 * border).max(0.0) / ph.max(1.0));
    let (dw, dh) = (pw * scale, ph * scale);
    ((vw - dw) / 2.0, (vh - dh) / 2.0, dw, dh)
}

#[cfg(test)]
#[path = "detect_tests.rs"]
mod tests;
