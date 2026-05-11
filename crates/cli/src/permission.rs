//! Permission prompt for the MCP server.
//!
//! When an MCP client (Claude Code, Cursor, …) makes its first capture call
//! in a session, we show a topmost Win32 MessageBox asking the user whether
//! to allow this client to read the screen for the rest of this server's
//! process lifetime. The decision is cached per `client_id`.
//!
//! v1 deliberately uses [`MessageBoxW`] for simplicity and trust — it's the
//! Windows-native, well-known modal dialog that users recognise as a
//! security prompt. A polished egui prompt is a v1.5 polish item.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use windows::core::HSTRING;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONQUESTION, MB_SYSTEMMODAL, MB_TOPMOST, MB_YESNO,
};

/// Cache of `client_id -> allowed?`. Lifetime is the MCP server process.
static DECISIONS: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();

/// MCP permission gate: prompts on first call per `client_id`, then caches.
pub fn check_with_prompt(client_id: &str) -> bool {
    let map = DECISIONS.get_or_init(|| Mutex::new(HashMap::new()));

    // Fast path: already decided.
    if let Some(&decision) = map.lock().unwrap().get(client_id) {
        return decision;
    }

    let decision = prompt(client_id);

    map.lock()
        .unwrap()
        .insert(client_id.to_string(), decision);

    tracing::info!(
        client = %client_id,
        allowed = decision,
        "screen-capture permission recorded for session"
    );
    decision
}

fn prompt(client_id: &str) -> bool {
    let title = HSTRING::from("Glimpse — screen access requested");
    let body = format!(
        "An AI client wants to read text from your screen using Glimpse.\n\n\
         Client: {client_id}\n\n\
         Allow this client to capture and OCR your screen for the rest of \
         this session?\n\n\
         You can revoke this in the Glimpse tray menu."
    );
    let body_w = HSTRING::from(body.as_str());

    let response = unsafe {
        MessageBoxW(
            HWND::default(),
            &body_w,
            &title,
            MB_YESNO | MB_ICONQUESTION | MB_SYSTEMMODAL | MB_TOPMOST,
        )
    };
    response == IDYES
}
