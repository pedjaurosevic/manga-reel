//! Real Paper 2 — faithful port of ~/screen/shaders/fx/paper/real-paper-2.frag
//! Baked into page pixels so paper + ink scroll as one sheet (no screen parallax).

use rayon::prelude::*;

/// Apply Real Paper 2 in-place on straight RGBA8 page pixels.
pub fn apply_real_paper_2(rgba: &mut [u8], width: u32, height: u32) {
    if width < 2 || height < 2 {
        return;
    }
    let w = width as usize;
    let h = height as usize;
    if rgba.len() < w * h * 4 {
        return;
    }

    let src = rgba.to_vec();
    let luma_w = (0.2126_f32, 0.7152_f32, 0.0722_f32);
    let fdir = normalize2(0.906, 0.422);
    let pdir = normalize2(-0.422, 0.906);
    // Still lighter / yellower cream stock for comics reading.
    let dark_paper = (0.22_f32, 0.185, 0.118);
    let cream_ink = (0.98_f32, 0.94, 0.82);
    let light_dir = normalize2(-0.6, -0.8);

    let mut out = vec![0u8; src.len()];
    out.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let si = (y * w + x) * 4;
            let i = x * 4;
            let r0 = src[si] as f32 / 255.0;
            let g0 = src[si + 1] as f32 / 255.0;
            let b0 = src[si + 2] as f32 / 255.0;
            let a0 = src[si + 3];

            let (r_a, g_a, b_a) =
                sample_rgb(&src, w, h, x as f32 + fdir.0 * 0.65, y as f32 + fdir.1 * 0.65);
            let (r_b, g_b, b_b) =
                sample_rgb(&src, w, h, x as f32 + pdir.0 * 0.65, y as f32 + pdir.1 * 0.65);

            let lum = dot3(r0, g0, b0, luma_w);
            let lum_a = dot3(r_a, g_a, b_a, luma_w);
            let lum_b = dot3(r_b, g_b, b_b, luma_w);
            let edge = (lum - lum_a).abs().max((lum - lum_b).abs());
            let wick_amt = (edge * 0.30).clamp(0.0, 0.16);
            let wicked = (
                mixf(r0, 0.5 * (r_a + r_b), wick_amt),
                mixf(g0, 0.5 * (g_a + g_b), wick_amt),
                mixf(b0, 0.5 * (b_a + b_b), wick_amt),
            );

            let coord = (x as f32, y as f32);
            let hgt = paper_height(coord);
            let h_light = paper_height((coord.0 - light_dir.0 * 1.5, coord.1 - light_dir.1 * 1.5));
            let relief = (hgt - h_light) * 1.2;

            let print_mask = smoothstep(0.22, 0.82, lum);
            let field_mask = 1.0 - print_mask;
            // Unsoftened — same mix as real-paper-2.frag
            let diffuse = 1.0 + relief * mixf(0.08, 0.145, field_mask);

            let glow = smoothstep(0.62, 0.98, lum);
            let mut matte = (
                wicked.0 * mixf(1.0, 0.90, glow),
                wicked.1 * mixf(1.0, 0.90, glow),
                wicked.2 * mixf(1.0, 0.90, glow),
            );
            matte = (
                mixf(matte.0, cream_ink.0, glow * 0.22),
                mixf(matte.1, cream_ink.1, glow * 0.22),
                mixf(matte.2, cream_ink.2, glow * 0.22),
            );

            let mx = matte.0.max(matte.1.max(matte.2));
            let mn = matte.0.min(matte.1.min(matte.2));
            let sat = if mx > 0.001 { (mx - mn) / mx } else { 0.0 };
            let neon = smoothstep(0.70, 1.0, sat) * smoothstep(0.45, 0.95, mx);
            let ylum = dot3(matte.0, matte.1, matte.2, luma_w);
            let chroma = (matte.0 - ylum, matte.1 - ylum, matte.2 - ylum);
            let chroma_scale = mixf(1.0, 0.86, neon);
            matte = (
                ylum + chroma.0 * chroma_scale,
                ylum + chroma.1 * chroma_scale,
                ylum + chroma.2 * chroma_scale,
            );

            let press = (0.5 - hgt).max(0.0) * 0.10 * print_mask;
            let holdout = hgt * 0.045 * print_mask;
            let printed = (
                mixf(matte.0 * (1.0 - press), dark_paper.0, holdout),
                mixf(matte.1 * (1.0 - press), dark_paper.1, holdout),
                mixf(matte.2 * (1.0 - press), dark_paper.2, holdout),
            );

            let mottle = (vnoise((coord.0 * 0.48, coord.1 * 0.48)) - 0.5) * 0.055 * print_mask;
            let printed = (
                printed.0 * (1.0 + mottle),
                printed.1 * (1.0 + mottle),
                printed.2 * (1.0 + mottle),
            );

            let stock = (
                dark_paper.0 * diffuse,
                dark_paper.1 * diffuse,
                dark_paper.2 * diffuse,
            );
            let floor_mix = field_mask * 0.22 * (1.0 - smoothstep(0.0, 0.25, lum));
            let base = (
                mixf(printed.0, printed.0 + stock.0, floor_mix),
                mixf(printed.1, printed.1 + stock.1, floor_mix),
                mixf(printed.2, printed.2 + stock.2, floor_mix),
            );

            let textured = (base.0 * diffuse, base.1 * diffuse, base.2 * diffuse);

            let u = (x as f32 + 0.5) / w as f32;
            let v = (y as f32 + 0.5) / h as f32;
            let vig = (u * v * (1.0 - u) * (1.0 - v)).max(0.0001);
            let vignette = (16.0 * vig).powf(0.035).clamp(0.0, 1.0);

            // Warm paper cast: lift + slight yellow without crushing blacks in ink.
            let warm = (
                (textured.0 * 1.12 + 0.06).min(1.0),
                (textured.1 * 1.08 + 0.045).min(1.0),
                (textured.2 * 0.92 + 0.015).min(1.0),
            );
            row[i] = ((warm.0 * vignette).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            row[i + 1] = ((warm.1 * vignette).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            row[i + 2] = ((warm.2 * vignette).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            row[i + 3] = a0;
        }
    });
    rgba.copy_from_slice(&out);
}

fn sample_rgb(src: &[u8], w: usize, h: usize, fx: f32, fy: f32) -> (f32, f32, f32) {
    let x = fx.round().clamp(0.0, (w - 1) as f32) as usize;
    let y = fy.round().clamp(0.0, (h - 1) as f32) as usize;
    let i = (y * w + x) * 4;
    (
        src[i] as f32 / 255.0,
        src[i + 1] as f32 / 255.0,
        src[i + 2] as f32 / 255.0,
    )
}

fn hash21(p: (f32, f32)) -> f32 {
    let mut px = fract(p.0 * 0.1031);
    let mut py = fract(p.1 * 0.1031);
    let mut pz = fract(p.0 * 0.1031);
    let d = px * (py + 33.33) + py * (pz + 33.33) + pz * (px + 33.33);
    px += d;
    py += d;
    pz += d;
    fract((px + py) * pz)
}

fn vnoise(p: (f32, f32)) -> f32 {
    let i = (p.0.floor(), p.1.floor());
    let f = (fract(p.0), fract(p.1));
    let u = (f.0 * f.0 * (3.0 - 2.0 * f.0), f.1 * f.1 * (3.0 - 2.0 * f.1));
    let a = hash21(i);
    let b = hash21((i.0 + 1.0, i.1));
    let c = hash21((i.0, i.1 + 1.0));
    let d = hash21((i.0 + 1.0, i.1 + 1.0));
    mixf(mixf(a, b, u.0), mixf(c, d, u.0), u.1)
}

fn fiber(p: (f32, f32), dir: (f32, f32), scale: (f32, f32)) -> f32 {
    let perp = (-dir.1, dir.0);
    let rot = (
        (p.0 * dir.0 + p.1 * dir.1) * scale.0,
        (p.0 * perp.0 + p.1 * perp.1) * scale.1,
    );
    let n = vnoise(rot);
    1.0 - (n * 2.0 - 1.0).abs()
}

fn paper_height(p: (f32, f32)) -> f32 {
    let tooth = vnoise((p.0 * 0.35, p.1 * 0.35)) * 0.65 + vnoise((p.0 * 0.70, p.1 * 0.70)) * 0.35;
    let f1 = fiber(p, (0.906, 0.422), (0.07, 1.4));
    let f2 = fiber(p, (-0.422, 0.906), (1.4, 0.07));
    tooth * 0.65 + (f1 + f2) * 0.5 * 0.35
}

fn normalize2(x: f32, y: f32) -> (f32, f32) {
    let l = (x * x + y * y).sqrt().max(1e-6);
    (x / l, y / l)
}

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn mixf(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn dot3(r: f32, g: f32, b: f32, w: (f32, f32, f32)) -> f32 {
    r * w.0 + g * w.1 + b * w.2
}

/// Warm paper RGB for letterbox / panel margins (matches baked stock).
pub fn paper_stock_rgb() -> (f64, f64, f64) {
    (0.22 * 1.12 + 0.06, 0.185 * 1.08 + 0.045, 0.118 * 0.92 + 0.015)
}

/// Save current Hyprland screen_shader and clear it while reading.
pub fn suspend_hyprland_shader() -> Option<String> {
    let out = std::process::Command::new("hyprctl")
        .args(["getoption", "decoration:screen_shader"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let prev = text.lines().find_map(|l| {
        let l = l.trim();
        if let Some(rest) = l.strip_prefix("str:") {
            let s = rest.trim().trim_matches('"').to_string();
            if !s.is_empty() && s != "[[EMPTY]]" {
                return Some(s);
            }
        }
        // Fallback: first path-looking token
        if l.contains(".frag") {
            return l.split_whitespace().find(|t| t.contains(".frag")).map(|s| s.to_string());
        }
        None
    });
    let _ = std::process::Command::new("hyprctl")
        .args(["keyword", "decoration:screen_shader", "[[EMPTY]]"])
        .status();
    let _ = std::process::Command::new("hyprctl")
        .args(["keyword", "decoration:screen_shader", ""])
        .status();
    prev
}

pub fn restore_hyprland_shader(prev: Option<&str>) {
    let path = prev.unwrap_or("");
    let _ = std::process::Command::new("hyprctl")
        .args(["keyword", "decoration:screen_shader", path])
        .status();
}
