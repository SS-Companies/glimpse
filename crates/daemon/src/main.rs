//! Glimpse daemon entry point.
//!
//! Owns:
//! - the global mouse hook ([`hook`]),
//! - the gesture state machine ([`glimpse_core::gesture::Gesture`]),
//! - (eventually) the tray icon, the cursor ring overlay, the editable
//!   preview popup, the per-session agent permission state, and the
//!   embedded MCP server.
//!
//! In this milestone, `main` runs the gesture loop and dispatches Fire to
//! capture+OCR+clipboard. The visual layers (ring, popup, tray) are still
//! `unimplemented!()` stubs.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hook;
mod permission;
mod popup;
mod ring;
mod tray;
mod updater;

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use glimpse_core::gesture::{Gesture, GestureEvent, GestureOutcome};

fn main() -> Result<()> {
    init_tracing()?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "glimpse-daemon starting");

    let config = glimpse_core::Config::load()?;
    glimpse_core::capture::init_dpi_awareness();

    // Spawn the cursor-ring overlay thread up front so it's ready when the
    // first hold starts.
    ring::spawn()?;

    // Spawn the editable-preview popup thread (always alive, hidden by default).
    if let Err(e) = popup::spawn() {
        tracing::warn!(error = ?e, "popup window could not be created; continuing without preview");
    }

    // Spawn the system tray icon. Failure here is non-fatal — users on
    // systems without a taskbar (rare) still get the gesture, just no UI.
    if let Err(e) = tray::spawn() {
        tracing::warn!(error = ?e, "tray icon could not be installed; continuing headless");
    }

    // Channel: hook thread → main loop.
    let (tx, rx) = mpsc::channel::<GestureEvent>();

    // Hook thread: blocks for the process lifetime.
    let hook_tx = tx.clone();
    let hook_handle = thread::Builder::new()
        .name("glimpse-mouse-hook".into())
        .spawn(move || {
            if let Err(e) = hook::run(hook_tx) {
                tracing::error!(error = ?e, "mouse hook thread terminated with error");
            }
        })?;

    // Tick thread: drips `Tick` events into the gesture every 16 ms so the
    // state machine can detect the hold threshold without depending on
    // wall-clock polling inside `process`.
    let tick_tx = tx.clone();
    let _tick_handle = thread::Builder::new()
        .name("glimpse-tick".into())
        .spawn(move || loop {
            if tick_tx
                .send(GestureEvent::Tick { now: Instant::now() })
                .is_err()
            {
                break;
            }
            thread::sleep(Duration::from_millis(16));
        })?;

    let hold_threshold = Duration::from_millis(config.hold_ms);
    let mut gesture = Gesture::new(hold_threshold, config.drift_limit_px);

    // Main event loop.
    while let Ok(event) = rx.recv() {
        match gesture.process(event) {
            GestureOutcome::Idle => {}
            GestureOutcome::HoldStarted { began_at } => {
                tracing::debug!(?began_at, "hold started");
                // Cursor position is fetched lazily so the ring lands where the
                // gesture actually began, not where the last mouse event was.
                if let Ok((cx, cy)) = glimpse_core::capture::cursor_position() {
                    ring::show(cx, cy, hold_threshold);
                }
            }
            GestureOutcome::HoldCancelled => {
                tracing::debug!("hold cancelled");
                ring::hide();
            }
            GestureOutcome::Fire => {
                tracing::info!("gesture fired");
                ring::hide();
                hook::suppress_until_release();
                if let Err(e) = on_fire(&config) {
                    tracing::error!(error = ?e, "Fire handler failed");
                }
            }
        }
    }

    drop(hook_handle); // detached
    Ok(())
}

fn on_fire(config: &glimpse_core::Config) -> Result<()> {
    let (cx, cy) = glimpse_core::capture::cursor_position()?;
    let rect = glimpse_core::capture::Rect::centred_on(
        cx,
        cy,
        config.capture_logical_w,
        config.capture_logical_h,
    )?;
    let frame = glimpse_core::capture::capture_region(rect)?;
    let ocr =
        glimpse_core::ocr::ocr_frame(&frame, config.ocr_language.as_deref())?;
    let cleaned = glimpse_core::cleanup::clean(&ocr.text);

    if cleaned.is_empty() {
        tracing::info!(lang = %ocr.language, "no text recognised");
        // TODO: ring should flash red or popup say "no text"
        return Ok(());
    }

    glimpse_core::clipboard::set_text(&cleaned)?;
    tracing::info!(
        lang = %ocr.language,
        chars = cleaned.chars().count(),
        preview = %truncate(&cleaned, 60),
        "captured + copied"
    );

    // Show the editable preview gated by config. The clipboard already
    // holds the text; the popup only updates it if the user edits + Enter.
    if config.show_preview_popup {
        popup::show_editable(cleaned, cx, cy);
    }
    Ok(())
}

fn truncate(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str("…");
    }
    out
}

fn init_tracing() -> Result<()> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let log_dir = directories::ProjectDirs::from("dev", "Glimpse", "glimpse")
        .map(|d| d.data_dir().join("logs"))
        .unwrap_or_else(|| std::path::PathBuf::from("./logs"));
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "daemon.log");
    let (nb, guard) = tracing_appender::non_blocking(file_appender);
    Box::leak(Box::new(guard)); // keep flushing for the process lifetime

    let console = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false);
    let file = fmt::layer().with_writer(nb).with_ansi(false);

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(console)
        .with(file)
        .init();

    Ok(())
}
