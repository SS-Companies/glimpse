//! Tool definitions and dispatch.
//!
//! MCP `tools/call` responses use the `{ content: [...], isError }` shape;
//! [`ok_response`] and [`error_response`] wrap a plain text string in that
//! envelope so the rest of the server stays terse.

use crate::protocol::{Id, Response};
use serde_json::{json, Value};

#[derive(thiserror::Error, Debug)]
pub enum ToolError {
    #[error("tool not found: {0}")]
    NotFound(String),

    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    #[error("core error: {0}")]
    Core(#[from] glimpse_core::Error),
}

/// Tool dispatch entry point.
pub struct Tool;

impl Tool {
    /// Look up `name` and invoke it with `arguments`, returning the OCR /
    /// clipboard text on success.
    pub async fn dispatch(name: &str, arguments: &Value) -> Result<String, ToolError> {
        match name {
            "ocr_at_cursor" => ocr_at_cursor(arguments).await,
            "ocr_region" => ocr_region(arguments).await,
            "read_clipboard" => read_clipboard(arguments).await,
            other => Err(ToolError::NotFound(other.to_string())),
        }
    }
}

pub fn all_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "ocr_at_cursor",
            "description": "Capture a small region of the screen centred on the current mouse cursor and return the OCR'd text. Use this to read whatever text the user is hovering over — including text inside videos, images, games, and other non-selectable surfaces.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "width":  { "type": "integer", "minimum": 32, "maximum": 4096, "default": 400, "description": "Capture width in DPI-independent pixels. Defaults to 400." },
                    "height": { "type": "integer", "minimum": 32, "maximum": 4096, "default": 100, "description": "Capture height in DPI-independent pixels. Defaults to 100." },
                    "language": { "type": "string", "description": "Optional BCP-47 OCR language tag (e.g. 'en-US', 'ja'). Defaults to the system language." }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "ocr_region",
            "description": "Capture an explicit screen rectangle in virtual-desktop coordinates and return the OCR'd text. Use this when you already know the on-screen location of the text you want to read.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "x":      { "type": "integer", "description": "Left edge in virtual-desktop screen coordinates (physical pixels)." },
                    "y":      { "type": "integer", "description": "Top edge in virtual-desktop screen coordinates (physical pixels)." },
                    "width":  { "type": "integer", "minimum": 1, "maximum": 8192 },
                    "height": { "type": "integer", "minimum": 1, "maximum": 8192 },
                    "language": { "type": "string", "description": "Optional BCP-47 OCR language tag." }
                },
                "required": ["x", "y", "width", "height"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "read_clipboard",
            "description": "Return the current system clipboard text. Useful for grounding follow-up reasoning on whatever the user just copied.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
    ]
}

pub fn ok_response(id: Id, text: String) -> Response {
    Response::ok(
        id,
        json!({
            "content": [{ "type": "text", "text": text }],
            "isError": false,
        }),
    )
}

pub fn error_response(id: Id, message: impl Into<String>) -> Response {
    Response::ok(
        id,
        json!({
            "content": [{ "type": "text", "text": message.into() }],
            "isError": true,
        }),
    )
}

// ----------------------------- tool impls -----------------------------

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct OcrAtCursorArgs {
    width: Option<u32>,
    height: Option<u32>,
    language: Option<String>,
}

async fn ocr_at_cursor(arguments: &Value) -> Result<String, ToolError> {
    let args: OcrAtCursorArgs = serde_json::from_value(arguments.clone())
        .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
    let w = args.width.unwrap_or(400).clamp(32, 4096);
    let h = args.height.unwrap_or(100).clamp(32, 4096);

    // Capture + OCR + cleanup happen on a blocking thread; cursor/GDI/WinRT
    // APIs are sync and we do not want to stall the tokio runtime.
    let lang = args.language;
    let text = tokio::task::spawn_blocking(move || -> Result<String, glimpse_core::Error> {
        glimpse_core::capture::init_dpi_awareness();
        let (cx, cy) = glimpse_core::capture::cursor_position()?;
        let rect = glimpse_core::capture::Rect::centred_on(cx, cy, w, h)?
            .clamp_to_monitor()?;
        let frame = glimpse_core::capture::capture_region(rect)?;
        let result = glimpse_core::ocr::ocr_frame(&frame, lang.as_deref())?;
        Ok(glimpse_core::cleanup::clean(&result.text))
    })
    .await
    .map_err(|e| ToolError::InvalidArgs(format!("blocking task: {e}")))??;

    Ok(text)
}

#[derive(serde::Deserialize)]
struct OcrRegionArgs {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    #[serde(default)]
    language: Option<String>,
}

async fn ocr_region(arguments: &Value) -> Result<String, ToolError> {
    let args: OcrRegionArgs = serde_json::from_value(arguments.clone())
        .map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
    if args.width == 0 || args.height == 0 {
        return Err(ToolError::InvalidArgs("zero-area rect".into()));
    }

    let lang = args.language;
    let text = tokio::task::spawn_blocking(move || -> Result<String, glimpse_core::Error> {
        glimpse_core::capture::init_dpi_awareness();
        let rect = glimpse_core::capture::Rect {
            x: args.x,
            y: args.y,
            width: args.width,
            height: args.height,
        }
        .clamp_to_monitor()?;
        let frame = glimpse_core::capture::capture_region(rect)?;
        let result = glimpse_core::ocr::ocr_frame(&frame, lang.as_deref())?;
        Ok(glimpse_core::cleanup::clean(&result.text))
    })
    .await
    .map_err(|e| ToolError::InvalidArgs(format!("blocking task: {e}")))??;

    Ok(text)
}

async fn read_clipboard(_arguments: &Value) -> Result<String, ToolError> {
    let text = tokio::task::spawn_blocking(glimpse_core::clipboard::get_text)
        .await
        .map_err(|e| ToolError::InvalidArgs(format!("blocking task: {e}")))??;
    Ok(text)
}
