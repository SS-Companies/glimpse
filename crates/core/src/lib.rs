//! Glimpse core.
//!
//! Pure logic shared by the daemon, MCP server, and CLI: screen capture,
//! Windows OCR, the gesture state machine, clipboard access, the post-OCR
//! cleanup pipeline, and config (de)serialization.

pub mod capture;
pub mod clipboard;
pub mod cleanup;
pub mod config;
pub mod gesture;
pub mod ocr;

pub use config::Config;
pub use gesture::{Gesture, GestureEvent, GestureOutcome};

/// Crate-wide error type.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("capture failed: {0}")]
    Capture(String),

    #[error("OCR failed: {0}")]
    Ocr(String),

    #[error("clipboard error: {0}")]
    Clipboard(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
