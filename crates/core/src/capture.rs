//! Screen capture via GDI BitBlt.
//!
//! Captures a rectangular region of the desktop and returns it as a raw BGRA8
//! buffer. v1 uses GDI for portability across display configurations; v1.5
//! will switch to DXGI Desktop Duplication for lower latency.

use crate::{Error, Result};
use std::mem::size_of;

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC,
    MonitorFromPoint, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ, MONITOR_DEFAULTTONEAREST, SRCCOPY,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// A captured screen region as raw BGRA8 pixels.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed BGRA8 (B,G,R,A,B,G,R,A,...), top-down rows.
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

/// Call once per process before any capture, so cursor + monitor coords are
/// reported in physical pixels and DPI scaling is honoured.
pub fn init_dpi_awareness() {
    // Best-effort; ignore the error if it was already set by the host.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// Read the current cursor position in physical pixels.
///
/// Requires [`init_dpi_awareness`] to have been called for the result to be
/// in physical pixels on high-DPI monitors.
pub fn cursor_position() -> Result<(i32, i32)> {
    let mut pt = POINT::default();
    unsafe {
        GetCursorPos(&mut pt).map_err(|e| Error::Capture(format!("GetCursorPos: {e}")))?;
    }
    Ok((pt.x, pt.y))
}

impl Rect {
    /// Build a DPI-aware rectangle centred on (`cx`, `cy`), where the size is
    /// given in logical (DPI-independent) pixels.
    ///
    /// The returned rect's `width`/`height` are scaled to physical pixels
    /// using the DPI of the monitor under (`cx`, `cy`).
    pub fn centred_on(cx: i32, cy: i32, logical_w: u32, logical_h: u32) -> Result<Self> {
        let (dpi_x, dpi_y) = monitor_dpi_at(cx, cy)?;
        let scale_x = dpi_x as f32 / 96.0;
        let scale_y = dpi_y as f32 / 96.0;
        let width = ((logical_w as f32) * scale_x).round() as u32;
        let height = ((logical_h as f32) * scale_y).round() as u32;
        Ok(Rect {
            x: cx - (width as i32) / 2,
            y: cy - (height as i32) / 2,
            width,
            height,
        })
    }
}

/// Query the effective DPI of the monitor containing the given screen point.
fn monitor_dpi_at(x: i32, y: i32) -> Result<(u32, u32)> {
    let pt = POINT { x, y };
    unsafe {
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if hmon.is_invalid() {
            return Err(Error::Capture("MonitorFromPoint returned null".into()));
        }
        let mut dpi_x: u32 = 96;
        let mut dpi_y: u32 = 96;
        GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y)
            .map_err(|e| Error::Capture(format!("GetDpiForMonitor: {e}")))?;
        Ok((dpi_x, dpi_y))
    }
}

/// Capture a screen region into a top-down BGRA8 buffer.
pub fn capture_region(rect: Rect) -> Result<CapturedFrame> {
    if rect.width == 0 || rect.height == 0 {
        return Err(Error::Capture("zero-area rect".into()));
    }

    unsafe {
        let hdc_screen = GetDC(HWND(std::ptr::null_mut()));
        if hdc_screen.is_invalid() {
            return Err(Error::Capture("GetDC(NULL) failed".into()));
        }
        // RAII-ish cleanup via a guard.
        let _screen_dc_guard = ScreenDcGuard(hdc_screen);

        let hdc_mem = CreateCompatibleDC(hdc_screen);
        if hdc_mem.is_invalid() {
            return Err(Error::Capture("CreateCompatibleDC failed".into()));
        }
        let _mem_dc_guard = MemDcGuard(hdc_mem);

        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: rect.width as i32,
                // Negative height = top-down rows, which matches what we want.
                biHeight: -(rect.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbmp = CreateDIBSection(
            hdc_mem,
            &bi,
            DIB_RGB_COLORS,
            &mut bits_ptr,
            None,
            0,
        )
        .map_err(|e| Error::Capture(format!("CreateDIBSection: {e}")))?;
        if bits_ptr.is_null() {
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            return Err(Error::Capture("CreateDIBSection returned null bits".into()));
        }
        let _bmp_guard = BitmapGuard(hbmp);

        let prev = SelectObject(hdc_mem, HGDIOBJ(hbmp.0));
        if prev.is_invalid() {
            return Err(Error::Capture("SelectObject failed".into()));
        }

        BitBlt(
            hdc_mem,
            0,
            0,
            rect.width as i32,
            rect.height as i32,
            hdc_screen,
            rect.x,
            rect.y,
            SRCCOPY,
        )
        .map_err(|e| Error::Capture(format!("BitBlt: {e}")))?;

        // Copy DIB bits into an owned Vec. Alpha bytes from GDI are
        // undefined; force them to 0xFF so downstream OCR doesn't treat
        // pixels as fully transparent.
        let byte_count = (rect.width as usize) * (rect.height as usize) * 4;
        let src = std::slice::from_raw_parts(bits_ptr as *const u8, byte_count);
        let mut pixels = src.to_vec();
        for px in pixels.chunks_exact_mut(4) {
            px[3] = 0xFF;
        }

        // Restore previous bitmap in mem DC before guards drop it.
        let _ = SelectObject(hdc_mem, prev);

        Ok(CapturedFrame {
            width: rect.width,
            height: rect.height,
            pixels,
        })
    }
}

// Drop guards so we don't leak GDI objects on the error path.

struct ScreenDcGuard(windows::Win32::Graphics::Gdi::HDC);
impl Drop for ScreenDcGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(HWND(std::ptr::null_mut()), self.0);
        }
    }
}

struct MemDcGuard(windows::Win32::Graphics::Gdi::HDC);
impl Drop for MemDcGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

struct BitmapGuard(windows::Win32::Graphics::Gdi::HBITMAP);
impl Drop for BitmapGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0 .0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_area_rejected() {
        let r = Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 100,
        };
        assert!(capture_region(r).is_err());
    }
}
