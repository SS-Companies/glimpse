# Glimpse

> Hold both mouse buttons, read any text on screen. For humans and AI agents.

Glimpse is a Windows-native tool that turns any pixel of text — in videos, images, games, PDFs, browsers, anywhere — into copyable text. Hover over the text, press the left and right mouse buttons together for ~250ms, and the text is on your clipboard.

It also exposes the same capability as an **MCP server**, so AI agents like Claude Code, Cursor, and Cline can "see" your screen on demand.

## Status

🚧 **Pre-alpha.** Repo scaffolded. Code not yet written. See the v1 issue list below.

## Features (v1)

- **Hold-both gesture** — press L+R mouse buttons for 250ms over any text. Cancels if you move the cursor or release early.
- **Hotkey fallback** — `Ctrl+Shift+C` for accessibility / mice that don't support chord clicks.
- **Editable preview popup** — fixes OCR errors before they hit the clipboard.
- **System tray app** — runs in the background, ~20 MB RAM.
- **MCP server** — built-in. Any agent that speaks MCP can call `ocr_at_cursor`, `ocr_region`, `read_clipboard`.
- **CLI** — `glimpse capture` from any script.
- **Auto-update** — checks GitHub Releases once a day (toggle off in settings).

## Privacy

- **All OCR runs locally** via the built-in Windows OCR API. No image or text ever leaves your machine.
- **No telemetry. No analytics. No crash reports.**
- The only network call is the daily GitHub release check. Toggle off in `%APPDATA%\glimpse\config.json`.
- Local logs at `%APPDATA%\glimpse\logs\daemon.log` (rotating, 7-day retention, max 10 MB). Never auto-sent.

See [`docs/PRIVACY.md`](docs/PRIVACY.md) for the full statement.

## Install

> Pre-alpha. No releases yet. Once v1 ships:

```powershell
# Portable: download glimpse.exe from GitHub Releases, double-click to run.
# Right-click → Properties → Unblock on first run (SmartScreen workaround).
```

`winget` and `Scoop` manifests will follow in v1.1.

## MCP integration

See [`docs/MCP.md`](docs/MCP.md) for wiring Glimpse into Claude Code, Cursor, and other MCP-compatible agents.

Quick start for Claude Code:

```json
{
  "mcpServers": {
    "glimpse": {
      "command": "glimpse",
      "args": ["mcp"]
    }
  }
}
```

Then the agent can call:
- `ocr_at_cursor()` — read text under the cursor right now
- `ocr_region(x, y, w, h)` — read text in an arbitrary screen region
- `read_clipboard()` — get the current clipboard text

The first call in each session triggers a user permission prompt.

## Building from source

```powershell
git clone https://github.com/Such-a-user/glimpse
cd glimpse
cargo build --release
# Binaries at target\release\glimpse.exe and glimpse-daemon.exe
```

Requirements: Rust 1.80+, Windows 10 1903+ (for the Windows OCR API).

## Architecture

```
crates/
├── core/       # OCR, capture, gesture state machine (lib)
├── daemon/     # tray app, mouse hook, popup UI (bin: glimpse-daemon)
├── mcp/        # MCP server, embedded in daemon (lib + bin)
└── cli/        # glimpse CLI (bin: glimpse)
```

The daemon embeds the MCP server by default — one process runs the tray app, the mouse hook, and the MCP stdio transport.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

## Contributing

See [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md). v1 issue list is tracked in GitHub Issues.
