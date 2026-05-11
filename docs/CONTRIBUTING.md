# Contributing to Glimpse

Thanks for your interest. Glimpse is small, opinionated, and Windows-only — please skim this before opening a PR.

## Getting started

```powershell
git clone https://github.com/CHANGE_ME/glimpse
cd glimpse
cargo build
cargo test
```

Requirements:
- Rust 1.80+ (pinned via `rust-toolchain.toml`)
- Windows 10 1903+ for runtime (older Windows lacks `Windows.Media.Ocr`)
- The `x86_64-pc-windows-msvc` target

## Project layout

See `README.md → Architecture`. Each crate has a single responsibility.

- New OCR/capture/gesture logic → `crates/core`
- New tray menus, popups, mouse-hook plumbing → `crates/daemon`
- New MCP tools → `crates/mcp`
- New CLI subcommands → `crates/cli`

## Style

- `cargo fmt` before committing.
- `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- Prefer small, focused PRs over megabranches.
- One commit per logical change. Squash on merge.

## Tests

- Unit tests live next to code in `#[cfg(test)] mod tests`.
- Pure logic (gesture state machine, cleanup pipeline, config parsing) should be testable without Win32. Mock the boundary.
- Integration tests that need a real Windows session go in `crates/<name>/tests/` and are gated behind `#[ignore]` if they require a desktop.

## What we accept

- Bug fixes
- Performance improvements
- Documentation improvements
- Test coverage
- Tools to make development faster

## What needs discussion first

- New MCP tools (open an issue)
- New gesture mechanics
- New OCR backends
- macOS/Linux ports — these are major scope expansions, talk to maintainers first

## What we don't accept

- Telemetry, analytics, or "anonymous" data collection, ever
- Cloud-based OCR backends as defaults
- Code-signed binaries from contributor forks pretending to be official
- Dependencies that phone home

## Reporting bugs

Open a GitHub issue with:
- Windows version (`winver` output)
- Glimpse version (`glimpse --version`)
- Reproduction steps
- The relevant section of `%APPDATA%\glimpse\logs\daemon.log`
