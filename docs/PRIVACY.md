# Privacy

Glimpse is built around one principle: **your screen never leaves your machine.**

## What Glimpse does NOT do

- ❌ No telemetry of any kind
- ❌ No usage analytics
- ❌ No crash reports
- ❌ No "anonymous" data collection
- ❌ No cloud OCR (everything runs through the local Windows OCR API)
- ❌ No account, no login, no sync
- ❌ No bundled third-party SDKs that phone home

## The single network call

Glimpse makes exactly one outbound network request when this feature is enabled:

- `GET https://api.github.com/repos/<owner>/glimpse/releases/latest` — once every 24 hours, to check if a new version is available.

This call sends no telemetry. The GitHub API may log the request IP per its own privacy policy.

**Toggle off:** set `"auto_update_check": false` in `%APPDATA%\glimpse\config.json`.

## Local data

Glimpse stores data only on your machine, under `%APPDATA%\glimpse\`:

| File | Purpose | Sensitive? |
|---|---|---|
| `config.json` | Settings | No |
| `logs/daemon.log` (rotating) | Local diagnostics, never auto-sent | Possibly — contains gesture timing, OCR success/fail counts, error stack traces. May include file paths. Does NOT log OCR'd text. |
| `history.db` (v1.5) | Clipboard history of past OCR captures | **Yes — contains OCR'd text.** Plain SQLite. Delete anytime. Optional retention setting. |

You can delete the entire `%APPDATA%\glimpse\` folder at any time. Glimpse will recreate config defaults on next launch.

## MCP agent capture

When an AI agent calls `ocr_at_cursor()` or `ocr_region()` via Glimpse's MCP server, the agent triggers a screen-region OCR. By default:

- **First call per agent session** triggers a user permission prompt (Allow once / Allow this session / Deny).
- **Every subsequent call** flashes the Glimpse tray icon to indicate a capture.
- The captured region is OCR'd locally; only the resulting **text string** is returned to the agent.
- The raw screen pixels are never persisted to disk and never leave the machine.

The agent obviously sees the resulting text and may transmit it to whatever LLM endpoint it uses — that is governed by the agent's own privacy policy, not Glimpse's.

## Code signing

Glimpse releases are **not currently code-signed**. Windows SmartScreen will show "unrecognized publisher" on first launch. To verify a release came from this repo, compare the SHA-256 checksum published alongside each release.

Code signing may be added in a future release once funding / sustained adoption justify the recurring cert cost.

## Reporting a privacy concern

Open an issue on GitHub with the `privacy` label, or email the maintainer (see `Cargo.toml`).
