//! Cursor-ring overlay.
//!
//! A small layered, click-through, top-most Win32 window centred on the
//! cursor that draws a ring filling clockwise from the top as the L+R hold
//! progresses. It serves as the gesture's visual confirmation: until the
//! ring completes, the user knows the gesture has not yet fired.
//!
//! ## Architecture
//!
//! - Owned by a dedicated thread (`glimpse-ring`) with its own Win32 message
//!   pump — a Win32 window can only be touched from the thread that created
//!   it, so cross-thread show/hide goes through process-wide atomics.
//! - A 16 ms `SetTimer` drives redraws at ~60 Hz.
//! - The window is `WS_EX_LAYERED | WS_EX_TRANSPARENT`, so it accepts no
//!   input and the user's cursor and clicks pass through to whatever is
//!   behind it.
//! - Pixels are pre-rendered into a 32-bpp DIB section as **pre-multiplied
//!   BGRA**, then pushed to the desktop via `UpdateLayeredWindow` with
//!   `AC_SRC_OVER + AC_SRC_ALPHA`.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HMODULE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetWindowLongPtrW,
    PostQuitMessage, RegisterClassExW, SetTimer, SetWindowLongPtrW, ShowWindow,
    TranslateMessage, UpdateLayeredWindow, GWLP_USERDATA, HMENU, MSG, SW_HIDE,
    SW_SHOWNOACTIVATE, ULW_ALPHA, WM_DESTROY, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

// ---------------- shared state ----------------

static SHOWING: AtomicBool = AtomicBool::new(false);
static CURSOR_X: AtomicI32 = AtomicI32::new(0);
static CURSOR_Y: AtomicI32 = AtomicI32::new(0);
static BEGAN_AT_MS: AtomicU64 = AtomicU64::new(0);
static THRESHOLD_MS: AtomicU64 = AtomicU64::new(250);
static EPOCH: OnceLock<Instant> = OnceLock::new();

const RING_SIZE: u32 = 48;
const RING_THICKNESS: f32 = 4.0;
const RING_PADDING: f32 = 2.0;

const FILLED_RGB: [u8; 3] = [59, 130, 246]; // tailwind blue-500
const TRACK_RGB: [u8; 3] = [51, 65, 85]; //   tailwind slate-700
const TRACK_ALPHA: u8 = 140;

const TIMER_ID: usize = 1;

// ---------------- public API ----------------

pub fn show(x: i32, y: i32, hold_threshold: Duration) {
    let epoch = *EPOCH.get_or_init(Instant::now);
    let elapsed_ms = epoch.elapsed().as_millis() as u64;
    CURSOR_X.store(x, Ordering::SeqCst);
    CURSOR_Y.store(y, Ordering::SeqCst);
    BEGAN_AT_MS.store(elapsed_ms, Ordering::SeqCst);
    THRESHOLD_MS.store(hold_threshold.as_millis().max(1) as u64, Ordering::SeqCst);
    SHOWING.store(true, Ordering::SeqCst);
}

pub fn hide() {
    SHOWING.store(false, Ordering::SeqCst);
}

pub fn spawn() -> anyhow::Result<()> {
    EPOCH.get_or_init(Instant::now);
    thread::Builder::new()
        .name("glimpse-ring".into())
        .spawn(|| {
            if let Err(e) = unsafe { run() } {
                tracing::error!(error = ?e, "ring thread exited with error");
            }
        })
        .context("spawn ring thread")?;
    Ok(())
}

// ---------------- implementation ----------------

struct RingState {
    mem_dc: HDC,
    hbmp: HBITMAP,
    bits: *mut c_void,
    width: u32,
    height: u32,
    visible: bool,
}

unsafe fn run() -> anyhow::Result<()> {
    let hmodule: HMODULE = GetModuleHandleW(None).context("GetModuleHandleW")?;
    // CreateWindowExW expects HINSTANCE — same handle value as HMODULE.
    let hinstance = windows::Win32::Foundation::HINSTANCE(hmodule.0);

    let class_name = w!("GlimpseRingClass");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance,
        lpszClassName: class_name,
        ..Default::default()
    };
    if RegisterClassExW(&wc) == 0 {
        anyhow::bail!("RegisterClassExW failed");
    }

    let ex_style = WS_EX_LAYERED
        | WS_EX_TRANSPARENT
        | WS_EX_TOPMOST
        | WS_EX_TOOLWINDOW
        | WS_EX_NOACTIVATE;

    let hwnd = CreateWindowExW(
        ex_style,
        class_name,
        w!("Glimpse Ring"),
        WS_POPUP,
        0,
        0,
        RING_SIZE as i32,
        RING_SIZE as i32,
        HWND::default(),
        HMENU::default(),
        hinstance,
        None,
    )
    .context("CreateWindowExW")?;

    // Build a DIB-section bitmap we render into. Pass HDC::default() (null)
    // to CreateCompatibleDC — Win32 then makes it compatible with the screen.
    let null_dc = HDC::default();
    let mem_dc = CreateCompatibleDC(null_dc);

    let bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: RING_SIZE as i32,
            biHeight: -(RING_SIZE as i32), // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut c_void = std::ptr::null_mut();
    let hbmp = CreateDIBSection(
        mem_dc,
        &bi,
        DIB_RGB_COLORS,
        &mut bits,
        windows::Win32::Foundation::HANDLE::default(),
        0,
    )
    .context("CreateDIBSection")?;
    if bits.is_null() {
        anyhow::bail!("CreateDIBSection returned null bits");
    }
    SelectObject(mem_dc, HGDIOBJ(hbmp.0));

    let state = Box::new(RingState {
        mem_dc,
        hbmp,
        bits,
        width: RING_SIZE,
        height: RING_SIZE,
        visible: false,
    });
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

    SetTimer(hwnd, TIMER_ID, 16, None);
    tracing::info!(?hwnd, "ring overlay window created");

    let mut msg = MSG::default();
    loop {
        let got = GetMessageW(&mut msg, HWND::default(), 0, 0);
        if got.0 <= 0 {
            break;
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    Ok(())
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER => {
            let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if raw != 0 {
                let state = &mut *(raw as *mut RingState);
                tick(hwnd, state);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let raw = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if raw != 0 {
                let state: Box<RingState> = Box::from_raw(raw as *mut RingState);
                let _ = DeleteObject(HGDIOBJ(state.hbmp.0));
                let _ = DeleteDC(state.mem_dc);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn tick(hwnd: HWND, state: &mut RingState) {
    let showing = SHOWING.load(Ordering::SeqCst);
    if !showing {
        if state.visible {
            let _ = ShowWindow(hwnd, SW_HIDE);
            state.visible = false;
        }
        return;
    }

    let epoch = EPOCH.get().copied().unwrap_or_else(Instant::now);
    let now_ms = epoch.elapsed().as_millis() as u64;
    let began = BEGAN_AT_MS.load(Ordering::SeqCst);
    let threshold = THRESHOLD_MS.load(Ordering::SeqCst).max(1);
    let elapsed = now_ms.saturating_sub(began);
    let progress = (elapsed as f32 / threshold as f32).clamp(0.0, 1.0);

    let buf = std::slice::from_raw_parts_mut(
        state.bits as *mut u8,
        (state.width * state.height * 4) as usize,
    );
    render_ring(buf, state.width, state.height, progress);

    let cx = CURSOR_X.load(Ordering::SeqCst);
    let cy = CURSOR_Y.load(Ordering::SeqCst);
    let pos = POINT {
        x: cx - (state.width as i32) / 2,
        y: cy - (state.height as i32) / 2,
    };
    let size = SIZE {
        cx: state.width as i32,
        cy: state.height as i32,
    };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let screen_dc = GetDC(HWND::default());
    let _ = UpdateLayeredWindow(
        hwnd,
        screen_dc,
        Some(&pos),
        Some(&size),
        state.mem_dc,
        Some(&src),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );
    ReleaseDC(HWND::default(), screen_dc);

    if !state.visible {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        state.visible = true;
    }
}

/// Software-rasterize the ring into `buf` (premultiplied BGRA, top-down).
fn render_ring(buf: &mut [u8], width: u32, height: u32, progress: f32) {
    buf.fill(0);

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let outer = (width.min(height) as f32 / 2.0) - RING_PADDING;
    let inner = outer - RING_THICKNESS;
    let tau = std::f32::consts::TAU;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let r = (dx * dx + dy * dy).sqrt();
            if r < inner || r > outer {
                continue;
            }

            let mut theta = dx.atan2(-dy);
            if theta < 0.0 {
                theta += tau;
            }
            let frac = theta / tau;

            let idx = ((y * width + x) * 4) as usize;
            if frac <= progress {
                buf[idx] = FILLED_RGB[2];
                buf[idx + 1] = FILLED_RGB[1];
                buf[idx + 2] = FILLED_RGB[0];
                buf[idx + 3] = 255;
            } else {
                let a = TRACK_ALPHA as u32;
                buf[idx] = ((TRACK_RGB[2] as u32 * a) / 255) as u8;
                buf[idx + 1] = ((TRACK_RGB[1] as u32 * a) / 255) as u8;
                buf[idx + 2] = ((TRACK_RGB[0] as u32 * a) / 255) as u8;
                buf[idx + 3] = TRACK_ALPHA;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_zero_progress_has_only_track_pixels() {
        let mut buf = vec![0u8; (RING_SIZE * RING_SIZE * 4) as usize];
        render_ring(&mut buf, RING_SIZE, RING_SIZE, 0.0);
        let filled = buf
            .chunks_exact(4)
            .any(|px| px[3] == 255 && px[2] == FILLED_RGB[0]);
        assert!(!filled, "no pixels should be fully filled at progress=0");
        let any_visible = buf.chunks_exact(4).any(|px| px[3] > 0);
        assert!(any_visible);
    }

    #[test]
    fn render_full_progress_has_filled_pixels() {
        let mut buf = vec![0u8; (RING_SIZE * RING_SIZE * 4) as usize];
        render_ring(&mut buf, RING_SIZE, RING_SIZE, 1.0);
        let filled = buf
            .chunks_exact(4)
            .filter(|px| px[3] == 255 && px[2] == FILLED_RGB[0])
            .count();
        assert!(filled > 50, "expected many filled pixels at progress=1");
    }

    #[test]
    fn render_half_progress_is_between() {
        let mut buf = vec![0u8; (RING_SIZE * RING_SIZE * 4) as usize];
        render_ring(&mut buf, RING_SIZE, RING_SIZE, 0.5);
        let filled = buf
            .chunks_exact(4)
            .filter(|px| px[3] == 255 && px[2] == FILLED_RGB[0])
            .count();
        let total_band = buf.chunks_exact(4).filter(|px| px[3] > 0).count();
        let ratio = filled as f32 / total_band as f32;
        assert!(
            (0.35..=0.65).contains(&ratio),
            "expected ~50%% filled, got {:.2}%%",
            ratio * 100.0
        );
    }
}
