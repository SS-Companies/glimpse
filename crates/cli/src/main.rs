//! `glimpse` CLI.
//!
//! Subcommands:
//!   - `glimpse capture` — one-shot OCR at the cursor, print to stdout.
//!   - `glimpse mcp`     — start the MCP stdio server (for `mcpServers.command`).
//!   - `glimpse version` — print version.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "glimpse", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// One-shot capture: OCR the region around the cursor, print text to stdout.
    Capture {
        /// Capture region width in DPI-independent pixels.
        #[arg(long, default_value_t = 400)]
        width: u32,
        /// Capture region height in DPI-independent pixels.
        #[arg(long, default_value_t = 100)]
        height: u32,
        /// BCP-47 OCR language tag. Defaults to the system language.
        #[arg(long)]
        language: Option<String>,
    },
    /// Run the MCP stdio server (for use under an MCP client like Claude Code).
    Mcp,
    /// Print version and exit.
    Version,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    match cli.command {
        Cmd::Capture { .. } => {
            anyhow::bail!("capture not yet implemented — see crates/core/src/capture.rs");
        }
        Cmd::Mcp => {
            // CLI invocation has no daemon to mediate permission; allow all.
            glimpse_mcp::run_stdio(|_| true).await?;
            Ok(())
        }
        Cmd::Version => {
            println!("glimpse {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let _ = fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .with_writer(std::io::stderr)
        .try_init();
}
