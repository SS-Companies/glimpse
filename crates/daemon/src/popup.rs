//! Editable preview popup that appears after a Fire.
//!
//! Small bordered Win32 window with a single-line EDIT control, anchored
//! near the cursor. OCR'd text is pre-loaded; the user can correct mistakes
//! before committing.
//!
//! ## Flow
//!
//! 1. `on_fire` silently copies the OCR text to the clipboard immediately
//!    (so paste works even if the user never interacts with the popup).
//! 2. `popup::show_editable(text, x, y)` is then called. The popup appears
//!    near the cursor with the text selected.
//! 3. User actions:
//!    - **Enter**: read the (possibly edited) text from the EDIT control,
//!      overwrite the clipboard, hide the popup.
//!    - **Esc**: hide the popup. Clipboard stays at the silent-copy value.
//!    - **Click outside (kill-focus)**: same as Esc.
//!
//! ## Threading
//!
//! Dedicated `glimpse-popup` thread with its own Win32 message pump. The
//! main loop talks to it via a static pending-text mutex + `PostMessageW`.
//!
//! `HWND` contains a raw pointer and is therefore `!Send`. We cross thread
//! boundaries by storing handle values as `AtomicIsize`s and reconstructing
//! the `HWND` on the reading thread.

use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

use anyhow::Context;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, GetParent,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, PostMessageW, PostQuitMessage,
    RegisterClassExW, SendMessageW, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
    TranslateMessage, GWLP_USERDATA, GWLP_WNDPROC, HMENU, HWND_TOPMOST, MSG, SWP_NOSIZE, SW_HIDE,
    SW_SHOW, WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_KILLFOCUS, WNDCLASSEXW,
    WS_BORDER, WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

// ---------------- shared state ----------------

const WM_APP_SHOW: u32 = WM_APP + 1;
const POPUP_W: i32 = 360;
const POPUP_H: i32 = 40;
const EDIT_PADDING: i32 = 4;
const ID_EDIT: isize = 100;
const EM_SETSEL: u32 = 0x00B1;

static PENDING_TEXT: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static PENDING_X: AtomicI32 = AtomicI32::new(0);
static PENDING_Y: AtomicI32 = AtomicI32::new(0);

/// `HWND` of the popup window, stored as the raw pointer value so it can
/// live in a static across threads.
static POPUP_HWND_PTR: AtomicIsize = AtomicIsize::new(0);

/// Subclass chain: previous EDIT WndProc, stored as a function pointer cast
/// to `isize`. 0 means "no previous proc — use DefWindowProc".
static OLD_EDIT_PROC: AtomicIsize = AtomicIsize::new(0);

// ---------------- public API ----------------

pub fn show_editable(text: String, x: i32, y: i32) {
    let slot = PENDING_TEXT.get_or_init(|| Mutex::new(None));
    *slot.lock().unwrap() = Some(text);
    PENDING_X.store(x, Ordering::SeqCst);
    PENDING_Y.store(y, Ordering::SeqCst);

    let raw = POPUP_HWND_PTR.load(Ordering::SeqCst);
    if raw != 0 {
        let hwnd = HWND(raw as *mut _);
        unsafe {
            let _ = PostMessageW(hwnd, WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn spawn() -> anyhow::Result<()> {
    PENDING_TEXT.get_or_init(|| Mutex::new(None));
    thread::Builder::new()
        .name("glimpse-popup".into())
        .spawn(|| {
            if let Err(e) = unsafe { run() } {
                tracing::error!(error = ?e, "popup thread exited with error");
            }
        })
        .context("spawn popup thread")?;
    Ok(())
}

// ---------------- implementation ----------------

unsafe fn run() -> anyhow::Result<()> {
    let hmodule: HMODULE = GetModuleHandleW(None).context("GetModuleHandleW")?;
    let hinstance = windows::Win32::Foundation::HINSTANCE(hmodule.0);

    let class_name = w!("GlimpsePopupClass");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(parent_proc),
        hInstance: hinstance,
        lpszClassName: class_name,
        ..Default::default()
    };
    if RegisterClassExW(&wc) == 0 {
        anyhow::bail!("RegisterClassExW failed");
    }

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        class_name,
        w!("Glimpse Preview"),
        WS_POPUP | WS_BORDER,
        0,
        0,
        POPUP_W,
        POPUP_H,
        HWND::default(),
        HMENU::default(),
        hinstance,
        None,
    )
    .context("CreateWindowExW popup")?;

    // Child EDIT control. The `EDIT` window class is registered by USER32.
    let edit = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("EDIT"),
        PCWSTR::null(),
        WS_CHILD | WS_VISIBLE | WS_BORDER,
        EDIT_PADDING,
        EDIT_PADDING,
        POPUP_W - 2 * EDIT_PADDING,
        POPUP_H - 2 * EDIT_PADDING,
        hwnd,
        HMENU(ID_EDIT as *mut _),
        hinstance,
        None,
    )
    .context("CreateWindowExW edit")?;

    // Subclass the EDIT control. Save the previous WndProc address so the
    // subclass can chain to it. SetWindowLongPtrW takes a numeric address;
    // the cast from a fn pointer is intentional for the Win32 ABI.
    #[allow(clippy::fn_to_numeric_cast)]
    let new_proc = edit_subproc as isize;
    let prev_raw = SetWindowLongPtrW(edit, GWLP_WNDPROC, new_proc);
    OLD_EDIT_PROC.store(prev_raw, Ordering::SeqCst);

    // Stash the edit hwnd on the parent for easy retrieval in parent_proc.
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, edit.0 as isize);

    // Publish the parent hwnd so `show_editable` can target it.
    POPUP_HWND_PTR.store(hwnd.0 as isize, Ordering::SeqCst);

    tracing::info!(?hwnd, "popup window created");

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

unsafe extern "system" fn parent_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_APP_SHOW => {
            let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if raw == 0 {
                return LRESULT(0);
            }
            let edit = HWND(raw as *mut _);

            let text = PENDING_TEXT
                .get()
                .and_then(|m| m.lock().unwrap().take())
                .unwrap_or_default();
            let ax = PENDING_X.load(Ordering::SeqCst);
            let ay = PENDING_Y.load(Ordering::SeqCst);

            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = SetWindowTextW(edit, PCWSTR(wide.as_ptr()));

            let pos_x = ax + 12;
            let pos_y = ay + 12;
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, pos_x, pos_y, 0, 0, SWP_NOSIZE);

            let _ = ShowWindow(hwnd, SW_SHOW);

            // Select the whole EDIT contents so a single keystroke replaces.
            SendMessageW(edit, EM_SETSEL, WPARAM(0), LPARAM(-1));
            let _ = SetFocus(edit);

            LRESULT(0)
        }
        WM_CLOSE | WM_KILLFOCUS => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn edit_subproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_KEYDOWN {
        let vk = wparam.0 as u16;
        if vk == VK_RETURN.0 {
            commit(hwnd);
            return LRESULT(0);
        }
        if vk == VK_ESCAPE.0 {
            cancel(hwnd);
            return LRESULT(0);
        }
    }

    let raw = OLD_EDIT_PROC.load(Ordering::SeqCst);
    if raw != 0 {
        let old_proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT =
            std::mem::transmute(raw);
        CallWindowProcW(Some(old_proc), hwnd, msg, wparam, lparam)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

unsafe fn commit(edit: HWND) {
    let text = read_edit_text(edit);
    let parent = GetParent(edit).unwrap_or_default();
    let _ = ShowWindow(parent, SW_HIDE);

    if let Err(e) = glimpse_core::clipboard::set_text(&text) {
        tracing::warn!(error = ?e, "popup commit: clipboard set failed");
    } else {
        tracing::info!(
            chars = text.chars().count(),
            "popup commit → clipboard updated"
        );
    }
}

unsafe fn cancel(edit: HWND) {
    let parent = GetParent(edit).unwrap_or_default();
    let _ = ShowWindow(parent, SW_HIDE);
    tracing::debug!("popup cancelled (clipboard unchanged)");
}

unsafe fn read_edit_text(edit: HWND) -> String {
    let len = GetWindowTextLengthW(edit);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; (len as usize) + 1];
    let n = GetWindowTextW(edit, &mut buf);
    String::from_utf16_lossy(&buf[..n as usize])
}
