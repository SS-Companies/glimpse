//! Glimpse MCP server.
//!
//! Exposes Glimpse's OCR + clipboard capabilities to MCP clients (Claude Code,
//! Cursor, Cline, …). v1 ships stdio transport.
//!
//! The actual rmcp wiring is intentionally kept tiny here so the daemon can
//! embed this lib and run the server on the same process as the tray app.

pub mod tools;

/// Run the MCP stdio server.
///
/// `permission_check` is a callback the server invokes before any capture tool
/// fires. It returns `true` if the call is allowed for the current session.
pub async fn run_stdio<F>(_permission_check: F) -> anyhow::Result<()>
where
    F: Fn(&str) -> bool + Send + Sync + 'static,
{
    // TODO: depend on the `rmcp` crate (the Rust MCP SDK), register the three
    // tools from `tools.rs`, and start serving on stdin/stdout.
    unimplemented!("glimpse_mcp::run_stdio (#12 in v1 build order)")
}
