//! Tool definitions exposed over MCP.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OcrAtCursorArgs {}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OcrRegionArgs {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReadClipboardArgs {}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OcrResult {
    pub text: String,
}

pub async fn ocr_at_cursor(_args: OcrAtCursorArgs) -> anyhow::Result<OcrResult> {
    // TODO: query cursor pos, build capture::Rect, capture, OCR.
    unimplemented!("tools::ocr_at_cursor")
}

pub async fn ocr_region(_args: OcrRegionArgs) -> anyhow::Result<OcrResult> {
    // TODO: capture the requested rect, OCR.
    unimplemented!("tools::ocr_region")
}

pub async fn read_clipboard(_args: ReadClipboardArgs) -> anyhow::Result<OcrResult> {
    // TODO: glimpse_core::clipboard::get_text(); wrap as OcrResult.
    unimplemented!("tools::read_clipboard")
}
