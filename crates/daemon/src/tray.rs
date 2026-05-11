//! System tray icon + menu.

pub fn run() -> anyhow::Result<()> {
    // TODO: tray-icon crate. Menu items:
    //   - Glimpse v0.1.0 (disabled label)
    //   - Last OCR: "Lorem ipsum..." (disabled, dynamic)
    //   - Open config…  (opens %APPDATA%\glimpse\config.json)
    //   - Show logs…   (opens %APPDATA%\glimpse\logs)
    //   - --- separator ---
    //   - Active agent sessions › (submenu of allowed MCP clients with Revoke buttons)
    //   - --- separator ---
    //   - About
    //   - Quit
    unimplemented!("tray::run (#7 in v1 build order)")
}
