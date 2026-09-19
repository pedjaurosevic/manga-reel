fn fallback_rows_or_page(gutter: &[bool], w: usize, h: usize) -> Vec<PanelRect> {
    let rows = detect_row_slices(gutter, w, h);
    if rows.len() >= 2 && rows.len() <= 10 && rows.iter().all(|r| r.2 as f32 >= w as f32 * 0.82) {
        let panels = to_norm_panels(&rows, w, h);
        if panels.len() >= 2 {
            return panels;
        }
    }
    full_page()
}

fn detect_row_slices(gutter: &[bool], w: usize, h: usize) -> Vec<(usize, usize, usize, usize)> {
    let mut gutter_row = vec![false; h];
    for y in 0..h {
        let mut g = 0u32;
        for x in 0..w {
            if gutter[y * w + x] {
                g += 1;
            }
        }
        gutter_row[y] = g as f32 / w as f32 >= 0.88;
    }
    // Slight thicken so thin gutters still separate.
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
        .filter(|p| p.w * p.h >= MIN_LEAF_FRAC as f64 * 0.85)
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

    // Reading order: top-to-bottom, LTR within row (order_panels handles RTL).
    panels.sort_by(|a, b| {
        let ay = a.y + a.h / 2.0;
        let by = b.y + b.h / 2.0;
        if (ay - by).abs() < 0.05 {
            a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
        }
    });

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
        let a = (rects[0].2 * rects[0].3) as f32 / (w * h) as f32;
        return a >= 0.45;
    }
    // Multi-panel: require reasonable coverage and not all tiny.
    let page = (w * h) as f32;
    let cover: f32 = rects
        .iter()
        .map(|r| (r.2 * r.3) as f32)
        .sum::<f32>()
        / page;
    cover >= 0.35 && cover <= 1.15
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

    #[test]
    fn version_is_seven() {
        assert_eq!(DETECT_VERSION, 7);
    }

    #[test]
    fn blank_white_page_is_full() {
        let w = 200u32;
        let h = 300u32;
        let mut rgba = vec![255u8; (w * h * 4) as usize];
        for i in (0..rgba.len()).step_by(4) {
            rgba[i + 3] = 255;
        }
        let panels = detect_panels(&rgba, w, h);
        assert_eq!(panels.len(), 1);
        assert!(panels[0].w > 0.9 && panels[0].h > 0.9);
    }

    #[test]
    fn two_by_two_gutter_grid() {
        // White page with four dark content blocks separated by white gutters.
        let w = 400usize;
        let h = 400usize;
        let mut rgba = vec![255u8; w * h * 4];
        for i in (0..rgba.len()).step_by(4) {
            rgba[i + 3] = 255;
        }
        // Mid-gray fills = content (not near-black gutters / not paper).
        let paint = |rgba: &mut [u8], x0: usize, y0: usize, x1: usize, y1: usize| {
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 4;
                    // Slight variation so interiors aren't mistaken for flat black bars.
                    let v = 110 + ((x + y) % 20) as u8;
                    rgba[i] = v;
                    rgba[i + 1] = v;
                    rgba[i + 2] = v;
                }
            }
        };
        // 2x2 panels with ~20px gutters through the middle.
        paint(&mut rgba, 10, 10, 185, 185);
        paint(&mut rgba, 215, 10, 390, 185);
        paint(&mut rgba, 10, 215, 185, 390);
        paint(&mut rgba, 215, 215, 390, 390);

        let panels = detect_panels(&rgba, w as u32, h as u32);
        assert!(
            panels.len() >= 3 && panels.len() <= 6,
            "expected ~4 panels, got {} {:?}",
            panels.len(),
            panels
        );
    }

    #[test]
    fn cream_paper_two_by_two_grid() {
        // Cream/yellow paper (Italian scan) with four dark content blocks.
        let w = 400usize;
        let h = 400usize;
        let cream = [240u8, 228, 200, 255];
        let mut rgba = vec![0u8; w * h * 4];
        for i in (0..rgba.len()).step_by(4) {
            rgba[i..i + 4].copy_from_slice(&cream);
        }
        let paint = |rgba: &mut [u8], x0: usize, y0: usize, x1: usize, y1: usize| {
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 4;
                    let v = 90 + ((x + y) % 25) as u8;
                    rgba[i] = v;
                    rgba[i + 1] = v.saturating_sub(5);
                    rgba[i + 2] = v.saturating_sub(10);
                    rgba[i + 3] = 255;
                }
            }
        };
        paint(&mut rgba, 12, 12, 188, 188);
        paint(&mut rgba, 212, 12, 388, 188);
        paint(&mut rgba, 12, 212, 188, 388);
        paint(&mut rgba, 212, 212, 388, 388);
        let panels = detect_panels(&rgba, w as u32, h as u32);
        assert!(
            panels.len() >= 3 && panels.len() <= 6,
            "cream paper expected ~4 panels, got {} {:?}",
            panels.len(),
            panels
        );
    }

    #[test]
    fn order_rtl_flips_within_row() {
        let panels = vec![
            PanelRect {
                x: 0.0,
                y: 0.0,
                w: 0.4,
                h: 0.4,
            },
            PanelRect {
                x: 0.5,
                y: 0.0,
                w: 0.4,
                h: 0.4,
            },
        ];
        let rtl = order_panels(&panels, true);
        assert!(rtl[0].x > rtl[1].x);
    }
}
