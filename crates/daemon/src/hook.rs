//! Low-level Windows mouse hook (`WH_MOUSE_LL`).
//!
//! Installs a global mouse hook, translates the OS message stream into the
//! pure-Rust [`GestureEvent`]s consumed by `glimpse_core::gesture`, and
//! suppresses the imminent left/right button-up events when the gesture has
//! Fired (so the user's hold does not also trigger the OS context menu).
//!
//! ## Threading model
//!
//! `WH_MOUSE_LL` is a *global* hook: the OS marshals every mouse event in the
//! system onto the thread that installed the hook before invoking the
//! callback. That thread must therefore run a Win32 message pump. [`run`]
//! does both — install + pump — and blocks until either `UnhookWindowsHookEx`
//! is implicitly invoked at process exit or a `WM_QUIT` is posted to the
//! installer thread.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::OnceLock;

use anyhow::Context;
use glimpse_core::gesture::GestureEvent;

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow as _GetActiveWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetCursorPos, GetMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

// ---------- shared state (set once at install, mutated from the callback) ----------

/// Sender for translated gesture events. Set exactly once by [`run`].
static SENDER: OnceLock<Sender<GestureEvent>> = OnceLock::new();

/// When `true`, swallow `WM_LBUTTONUP` and `WM_RBUTTONUP` events so the OS
/// does not deliver the context menu / click that would otherwise follow a
/// fired gesture. Cleared automatically when both buttons are observed up.
static SUPPRESS_BUTTONS: AtomicBool = AtomicBool::new(false);

/// Track button state in the callback so we can clear `SUPPRESS_BUTTONS`
/// once the user has fully released the chord.
static LEFT_DOWN: AtomicBool = AtomicBool::new(false);
static RIGHT_DOWN: AtomicBool = AtomicBool::new(false);

/// Last observed cursor position (physical pixels). Seeded by [`run`] before
/// installing the hook so the first emitted `Move` event has a small delta.
static LAST_X: AtomicI32 = AtomicI32::new(0);
static LAST_Y: AtomicI32 = AtomicI32::new(0);
static POS_INITIALIZED: AtomicBool = AtomicBool::new(false);

// Silence unused-import lint on the alias.
#[allow(dead_code)]
fn _ensure_active_window_in_scope() {
    let _ = unsafe { _GetActiveWindow() };
}

// ----------------------------- public API -----------------------------

/// From the main loop: tell the hook to swallow the upcoming L+R button-ups.
///
/// Call this immediately after `GestureOutcome::Fire`, before the user has
/// had time to release the chord. The flag clears automatically once both
/// buttons have been observed up by the callback.
pub fn suppress_until_release() {
    SUPPRESS_BUTTONS.store(true, Ordering::SeqCst);
}

/// Install the low-level mouse hook and run the Win32 message pump.
///
/// Blocks the calling thread for the lifetime of the hook. Spawn a dedicated
/// thread for this.
pub fn run(tx: Sender<GestureEvent>) -> anyhow::Result<()> {
    SENDER
        .set(tx)
        .map_err(|_| anyhow::anyhow!("mouse hook already installed in this process"))?;

    // Seed last-known cursor position so the first move delta is small.
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_ok() {
            LAST_X.store(pt.x, Ordering::SeqCst);
            LAST_Y.store(pt.y, Ordering::SeqCst);
            POS_INITIALIZED.store(true, Ordering::SeqCst);
        }
    }

    unsafe {
        let hhook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), HINSTANCE::default(), 0)
            .context("SetWindowsHookExW(WH_MOUSE_LL)")?;

        tracing::info!(?hhook, "mouse hook installed");

        // Pump messages so the OS can deliver hook callbacks on this thread.
        let mut msg = MSG::default();
        loop {
            let got = GetMessageW(&mut msg, None, 0, 0);
            if got.0 <= 0 {
                // 0 = WM_QUIT, -1 = error.
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(hhook);
    }

    Ok(())
}

// ----------------------------- callback -----------------------------

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Per docs: if `code < 0`, the callback must immediately
    // CallNextHookEx and return its result without further processing.
    if code < 0 {
        return CallNextHookEx(HHOOK::default(), code, wparam, lparam);
    }

    let info_ptr = lparam.0 as *const MSLLHOOKSTRUCT;
    if info_ptr.is_null() {
        return CallNextHookEx(HHOOK::default(), code, wparam, lparam);
    }
    let info = &*info_ptr;
    let msg = wparam.0 as u32;

    // The events we want to translate. Everything else passes through.
    let event = match msg {
        WM_LBUTTONDOWN => {
            LEFT_DOWN.store(true, Ordering::SeqCst);
            Some(GestureEvent::LeftDown)
        }
        WM_RBUTTONDOWN => {
            RIGHT_DOWN.store(true, Ordering::SeqCst);
            Some(GestureEvent::RightDown)
        }
        WM_LBUTTONUP => {
            LEFT_DOWN.store(false, Ordering::SeqCst);
            let should_swallow = SUPPRESS_BUTTONS.load(Ordering::SeqCst);
            send(GestureEvent::LeftUp);
            // If both buttons are now up, the suppression window ends.
            if !RIGHT_DOWN.load(Ordering::SeqCst) {
                SUPPRESS_BUTTONS.store(false, Ordering::SeqCst);
            }
            if should_swallow {
                return LRESULT(1);
            }
            None
        }
        WM_RBUTTONUP => {
            RIGHT_DOWN.store(false, Ordering::SeqCst);
            let should_swallow = SUPPRESS_BUTTONS.load(Ordering::SeqCst);
            send(GestureEvent::RightUp);
            if !LEFT_DOWN.load(Ordering::SeqCst) {
                SUPPRESS_BUTTONS.store(false, Ordering::SeqCst);
            }
            if should_swallow {
                return LRESULT(1);
            }
            None
        }
        WM_MOUSEMOVE => {
            let prev_x = LAST_X.load(Ordering::SeqCst);
            let prev_y = LAST_Y.load(Ordering::SeqCst);
            LAST_X.store(info.pt.x, Ordering::SeqCst);
            LAST_Y.store(info.pt.y, Ordering::SeqCst);

            if POS_INITIALIZED.swap(true, Ordering::SeqCst) {
                Some(GestureEvent::Move {
                    dx: info.pt.x - prev_x,
                    dy: info.pt.y - prev_y,
                })
            } else {
                None
            }
        }
        _ => None,
    };

    if let Some(ev) = event {
        send(ev);
    }

    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

fn send(event: GestureEvent) {
    if let Some(tx) = SENDER.get() {
        // If the receiver is gone we silently drop — the daemon is shutting
        // down. Never panic inside a Win32 callback.
        let _ = tx.send(event);
    }
}
