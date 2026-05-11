//! `glimpse` CLI.
//!
//! Subcommands:
//!   - `glimpse capture` — one-shot OCR at the cursor, print to stdout.
//!   - `glimpse mcp`     — start the MCP stdio server (for `mcpServers.command`).
//!   - `glimpse version` — print version.
//!   - `glimpse langs`   — list OCR languages installed locally.

mod permission;

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
        /// Override the centre point instead of using the cursor (mostly for testing).
        #[arg(long, value_names = ["X", "Y"], number_of_values = 2)]
        at: Option<Vec<i32>>,
        /// Skip the post-OCR cleanup pipeline and print raw OCR output.
        #[arg(long)]
        raw: bool,
        /// Do not push the OCR result to the system clipboard.
        #[arg(long)]
        no_copy: bool,
    },
    /// Read the system clipboard and print it to stdout.
    Clipboard,
    /// List OCR-capable languages installed on this machine.
    Langs,
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
        Cmd::Capture {
            width,
            height,
            language,
            at,
            raw,
            no_copy,
        } => {
            glimpse_core::capture::init_dpi_awareness();

            let (cx, cy) = match at {
                Some(xy) => (xy[0], xy[1]),
                None => glimpse_core::capture::cursor_position()?,
            };

            let rect = glimpse_core::capture::Rect::centred_on(cx, cy, width, height)?
                .clamp_to_monitor()?;
            tracing::info!(
                "capturing region at ({cx},{cy}) size {}x{} → physical {}x{} at ({},{})",
                width,
                height,
                rect.width,
                rect.height,
                rect.x,
                rect.y
            );

            let frame = glimpse_core::capture::capture_region(rect)?;
            let ocr = glimpse_core::ocr::ocr_frame(&frame, language.as_deref())?;

            let out = if raw {
                ocr.text
            } else {
                glimpse_core::cleanup::clean(&ocr.text)
            };

            if !no_copy && !out.is_empty() {
                glimpse_core::clipboard::set_text(&out)?;
            }

            // stderr: which language was used + copy state (diagnostic).
            // stdout: the text. Easy to pipe.
            let copied = if no_copy || out.is_empty() {
                ""
            } else {
                " [copied]"
            };
            eprintln!("[lang={}]{copied}", ocr.language);
            println!("{out}");
            Ok(())
        }
        Cmd::Clipboard => {
            let text = glimpse_core::clipboard::get_text()?;
            print!("{text}");
            Ok(())
        }
        Cmd::Langs => {
            for tag in glimpse_core::ocr::available_languages()? {
                println!("{tag}");
            }
            Ok(())
        }
        Cmd::Mcp => {
            // First MCP capture per client triggers a topmost Windows
            // permission prompt; the decision is cached for the rest of
            // this server's lifetime.
            glimpse_mcp::run_stdio(permission::check_with_prompt).await?;
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
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();
}
