//! System tray icon + menu.
//!
//! Single menu surface for the daemon: a disabled "Glimpse vX.Y.Z" label, a
//! separator, and a Quit item. Tooltip says "Glimpse — hold L+R mouse buttons
//! to read text".
//!
//! Threading
//! =========
//!
//! `tray-icon` on Windows requires the thread that created the tray icon to
//! pump Win32 messages — its `Shell_NotifyIconW` plumbing receives clicks via
//! a hidden window owned by that thread. We therefore spawn a dedicated
//! thread that:
//!   1. Builds the tray icon and menu.
//!   2. Installs a `MenuEvent` callback that calls `std::process::exit` on
//!      Quit. The daemon has no critical shutdown work, so abrupt exit is
//!      fine — the OS cleans up our hooks, ring window, log buffers (via the
//!      `tracing_appender` guard), and child threads.
//!   3. Runs `GetMessageW` in a blocking loop.

use std::thread;

use anyhow::Context;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIconBuilder,
};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const TOOLTIP: &str = "Glimpse — hold L+R mouse buttons to read text";

/// Spawn the tray thread. Returns once the thread has been started; the
/// actual tray icon is created on the new thread and may take a few ms to
/// appear in the user's taskbar.
pub fn spawn() -> anyhow::Result<()> {
    thread::Builder::new()
        .name("glimpse-tray".into())
        .spawn(|| {
            if let Err(e) = run() {
                tracing::error!(error = ?e, "tray thread exited with error");
            }
        })
        .context("spawn tray thread")?;
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let menu = Menu::new();

    let label_text = format!("Glimpse v{VERSION}");
    let label = MenuItem::new(&label_text, false, None);
    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::new("Quit Glimpse", true, None);

    menu.append(&label).context("append label")?;
    menu.append(&separator).context("append separator")?;
    menu.append(&quit).context("append quit")?;

    let icon = build_icon().context("build tray icon")?;

    // Keep the TrayIcon alive for the lifetime of the thread; if it drops,
    // the icon vanishes from the taskbar.
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(TOOLTIP)
        .with_icon(icon)
        .with_title("Glimpse")
        .build()
        .context("TrayIconBuilder::build")?;

    let quit_id = quit.id().clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == quit_id {
            tracing::info!("user clicked Quit — exiting");
            // Daemon has no critical shutdown work; OS cleans up hooks,
            // overlay window, etc. Tracing's non-blocking guard flushes on
            // drop, which `process::exit` triggers via at-exit handlers.
            std::process::exit(0);
        }
    }));

    tracing::info!("tray icon installed");

    // Pump Win32 messages so tray-icon's hidden window can deliver events.
    unsafe {
        let mut msg = MSG::default();
        loop {
            let got = GetMessageW(&mut msg, HWND::default(), 0, 0);
            if got.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// Build a 32x32 RGBA icon: a tailwind-blue filled disc on transparent
/// background. We generate it procedurally so the binary has no embedded
/// asset and Cargo doesn't need an `include_bytes!`.
fn build_icon() -> anyhow::Result<Icon> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    let cx = SIZE as f32 / 2.0;
    let cy = SIZE as f32 / 2.0;
    let radius = (SIZE as f32 / 2.0) - 2.0;

    // Tailwind blue-500 / white "G" placeholder.
    const BLUE: [u8; 3] = [59, 130, 246];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let r = (dx * dx + dy * dy).sqrt();
            let idx = ((y * SIZE + x) * 4) as usize;
            if r <= radius {
                // RGBA, NOT premultiplied — tray-icon's `from_rgba` documents
                // unpremultiplied RGBA.
                rgba[idx] = BLUE[0];
                rgba[idx + 1] = BLUE[1];
                rgba[idx + 2] = BLUE[2];
                rgba[idx + 3] = 255;
            }
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).context("Icon::from_rgba")
}
