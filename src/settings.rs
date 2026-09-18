//! App settings (reading order, letterbox, pan).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReadingOrder {
    #[default]
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Letterbox {
    #[default]
    Black,
    White,
}

/// How the page is scaled into the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FitMode {
    /// Page fills viewport width; pan vertically when taller.
    #[default]
    Width,
    /// Page fills viewport height; pan horizontally when wider.
    Height,
    /// Entire page visible (may letterbox); pan only if zoomed past contain.
    Contain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub reading_order: ReadingOrder,
    pub letterbox: Letterbox,
    pub fit: FitMode,
    /// Keyboard pan step as fraction of viewport (0.05–0.5).
    pub pan_step: f64,
    /// Autoscroll speed in pixels per second (slow continuous strip).
    #[serde(default = "default_autoscroll_pps")]
    pub autoscroll_pps: f64,
}

fn default_autoscroll_pps() -> f64 { 48.0 }


impl Default for Settings {
    fn default() -> Self {
        Self {
            reading_order: ReadingOrder::Ltr,
            letterbox: Letterbox::Black,
            fit: FitMode::Width,
            pan_step: 0.18,
            autoscroll_pps: 48.0,
        }
    }
}

fn settings_path() -> PathBuf {
    directories::ProjectDirs::from("app", "MangaReel", "manga-reel")
        .map(|d| d.config_dir().join("settings.json"))
        .unwrap_or_else(|| PathBuf::from(".").join("settings.json"))
}

pub fn load() -> Settings {
    let path = settings_path();
    let mut s = match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    };
    if !s.autoscroll_pps.is_finite() {
        s.autoscroll_pps = 48.0;
    }
    s.autoscroll_pps = s.autoscroll_pps.clamp(12.0, 240.0);
    s
}

pub fn save(settings: &Settings) -> Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}
