//! Screen capture.
//!
//! Captures a rectangular region of the desktop and returns it as a raw RGBA
//! buffer that the OCR layer can hand off to Windows.Media.Ocr.
//!
//! v1: GDI BitBlt (simplest, works on all Windows 10+ display configurations).
//! v1.5: switch to DXGI Desktop Duplication for lower latency.

use crate::Result;

/// A captured screen region as raw BGRA8 pixels.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed BGRA8 (B,G,R,A,B,G,R,A,...).
    pub pixels: Vec<u8>,
}

/// Virtual-desktop screen rectangle.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// DPI-aware box centred on (`cx`, `cy`).
    ///
    /// `logical_w` / `logical_h` are in DPI-independent pixels; this function
    /// scales them to physical pixels using the monitor under the cursor.
    pub fn centred_on(_cx: i32, _cy: i32, _logical_w: u32, _logical_h: u32) -> Self {
        // TODO: query GetDpiForWindow / GetDpiForMonitor and scale.
        unimplemented!("capture::Rect::centred_on (#1 in v1 build order)")
    }
}

/// Capture a screen region.
pub fn capture_region(_rect: Rect) -> Result<CapturedFrame> {
    // TODO: implement via Win32 GDI BitBlt then upgrade to DXGI later.
    unimplemented!("capture::capture_region (#2 in v1 build order)")
}
