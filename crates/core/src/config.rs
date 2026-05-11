//! Glimpse on-disk config (`%APPDATA%\glimpse\config.json`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Gesture hold threshold in milliseconds.
    pub hold_ms: u64,

    /// Maximum cursor drift in physical pixels during hold before cancel.
    pub drift_limit_px: i32,

    /// Capture region width in DPI-independent pixels.
    pub capture_logical_w: u32,

    /// Capture region height in DPI-independent pixels.
    pub capture_logical_h: u32,

    /// BCP-47 language tag for OCR, or `None` to use the system default.
    pub ocr_language: Option<String>,

    /// Whether to enable the Ctrl+Shift+C fallback hotkey.
    pub fallback_hotkey_enabled: bool,

    /// Whether to check GitHub Releases for updates once a day.
    pub auto_update_check: bool,

    /// Whether to show the editable preview popup after each OCR. If false,
    /// text is silently copied with only a tray-icon flash.
    pub show_preview_popup: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hold_ms: 250,
            drift_limit_px: 5,
            capture_logical_w: 400,
            capture_logical_h: 100,
            ocr_language: None,
            fallback_hotkey_enabled: true,
            auto_update_check: true,
            show_preview_popup: true,
        }
    }
}

impl Config {
    /// `%APPDATA%\glimpse\config.json` on Windows.
    pub fn path() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "Glimpse", "glimpse")
            .map(|d| d.config_dir().join("config.json"))
    }

    pub fn load() -> crate::Result<Self> {
        let Some(path) = Self::path() else {
            return Ok(Self::default());
        };
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(&path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save(&self) -> crate::Result<()> {
        let Some(path) = Self::path() else {
            return Err(crate::Error::Config("no config dir".into()));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let pretty = serde_json::to_vec_pretty(self)?;
        std::fs::write(&path, pretty)?;
        Ok(())
    }
}
