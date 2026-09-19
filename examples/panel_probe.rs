//! Render detection bounds and isolated panels for visual regression review.
#![allow(dead_code)]
#[path = "../src/detect.rs"]
mod detect;
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(args.len() == 3, "usage: panel_probe IMAGE OUTPUT_DIRECTORY");
    let img = image::open(&args[1])?.to_rgba8();
    let (w, h) = img.dimensions();
    let panels = detect::detect_panels(img.as_raw(), w, h);
    std::fs::create_dir_all(&args[2])?;
    println!("{}", serde_json::to_string_pretty(&panels)?);
    let mut annotated = img.clone();
    for (i, p) in panels.iter().enumerate() {
        let x = (p.x * w as f64).floor() as u32;
        let y = (p.y * h as f64).floor() as u32;
        let pw = ((p.w * w as f64).ceil() as u32).min(w - x);
        let ph = ((p.h * h as f64).ceil() as u32).min(h - y);
        for xx in x..x + pw {
            for yy in [y, y + ph - 1] {
                annotated.put_pixel(xx, yy, image::Rgba([255, 0, 0, 255]));
            }
        }
        for yy in y..y + ph {
            for xx in [x, x + pw - 1] {
                annotated.put_pixel(xx, yy, image::Rgba([255, 0, 0, 255]));
            }
        }
        let crop = image::imageops::crop_imm(&img, x, y, pw, ph).to_image();
        let (ox, oy, dw, dh) = detect::panel_layout(pw as f64, ph as f64, 960.0, 640.0);
        let scaled = image::imageops::resize(
            &crop,
            dw as u32,
            dh as u32,
            image::imageops::FilterType::Lanczos3,
        );
        let mut canvas = image::RgbaImage::from_pixel(960, 640, image::Rgba([0, 0, 0, 255]));
        image::imageops::overlay(&mut canvas, &scaled, ox as i64, oy as i64);
        canvas.save(format!("{}/panel-{i}.png", args[2]))?;
    }
    annotated.save(format!("{}/bounds.png", args[2]))?;
    Ok(())
}
