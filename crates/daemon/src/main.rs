//! Glimpse daemon entry point.
//!
//! Owns the global mouse hook, the tray icon, the cursor-ring overlay, the
//! editable preview popup, the per-session agent permission state, and the
//! embedded MCP server.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hook;
mod permission;
mod popup;
mod ring;
mod tray;
mod updater;

use anyhow::Result;

fn main() -> Result<()> {
    init_tracing()?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "glimpse-daemon starting");

    let _config = glimpse_core::Config::load()?;

    // TODO: wire up
    //   - tray::spawn()
    //   - hook::install() → channel<GestureEvent>
    //   - gesture::Gesture loop driving ring + popup + clipboard
    //   - mcp server task (stdio or local socket)
    //   - updater::spawn() daily poll

    Ok(())
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

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().with_writer(nb).with_ansi(false))
        .init();

    Ok(())
}
