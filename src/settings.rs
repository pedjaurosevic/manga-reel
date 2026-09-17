//! App settings (reading order, letterbox, film-strip speed).

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReaderMode {
    #[default]
    Guided,
    FilmStrip,
    Vertical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub reading_order: ReadingOrder,
    pub letterbox: Letterbox,
    pub mode: ReaderMode,
    /// Film-strip advance interval in milliseconds.
    pub film_strip_ms: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            reading_order: ReadingOrder::Rtl,
            letterbox: Letterbox::Black,
            mode: ReaderMode::Guided,
            film_strip_ms: 2500,
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
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(settings: &Settings) -> Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}
