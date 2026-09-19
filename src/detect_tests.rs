use super::*;

fn framed_grid(paper: [u8; 4], ink: [u8; 4]) -> Vec<u8> {
    let mut pixels = paper.repeat(400 * 400);
    for (x0, y0) in [(10, 10), (210, 10), (10, 210), (210, 210)] {
        for y in y0..y0 + 180 {
            for x in x0..x0 + 180 {
                if x < x0 + 3 || x >= x0 + 177 || y < y0 + 3 || y >= y0 + 177 {
                    pixels[(y * 400 + x) * 4..(y * 400 + x) * 4 + 4].copy_from_slice(&ink);
                }
            }
        }
    }
    pixels
}

#[test]
fn monochrome_frames_keep_borders_and_empty_interiors() {
    let p = detect_panels(&framed_grid([255; 4], [0, 0, 0, 255]), 400, 400);
    assert_eq!(p.len(), 4, "{p:?}");
    assert!(p[0].x <= 0.025 && p[0].x + p[0].w >= 0.475, "{p:?}");
}

#[test]
fn cream_and_black_gutters() {
    for (paper, ink) in [
        ([240, 228, 200, 255], [0, 0, 0, 255]),
        ([0, 0, 0, 255], [255; 4]),
    ] {
        assert_eq!(detect_panels(&framed_grid(paper, ink), 400, 400).len(), 4);
    }
}

#[test]
fn invalid_and_blank_inputs_are_safe() {
    for (pixels, w, h) in [
        (vec![], u32::MAX, u32::MAX),
        (vec![255; 40000], 100, 100),
        (vec![], 0, 0),
    ] {
        assert_eq!(detect_panels(&pixels, w, h), full_page());
    }
}

#[test]
fn wide_panel_fits_with_black_margin() {
    let (x, y, w, h) = panel_layout(1200.0, 300.0, 800.0, 600.0);
    assert!(x >= 8.0 && y >= 8.0 && x + w <= 792.0 && y + h <= 592.0);
    assert!((w / h - 4.0).abs() < 1e-9);
}

#[test]
fn rtl_reorders_rows_without_mixing_heights() {
    let p = vec![
        PanelRect {
            x: 0.0,
            y: 0.0,
            w: 0.45,
            h: 0.4,
        },
        PanelRect {
            x: 0.5,
            y: 0.01,
            w: 0.45,
            h: 0.25,
        },
        PanelRect {
            x: 0.0,
            y: 0.5,
            w: 1.0,
            h: 0.4,
        },
    ];
    let rtl = order_panels(&p, true);
    assert_eq!(rtl, vec![p[1], p[0], p[2]]);
    assert_eq!(order_panels(&rtl, false), p);
}
