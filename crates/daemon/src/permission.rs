//! Per-session permission state for MCP agent captures.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Denied,
    OneShot,
    Session,
}

#[derive(Default)]
pub struct Permissions {
    by_session: Mutex<HashMap<String, Scope>>,
}

impl Permissions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check whether `session_id` may capture.
    ///
    /// Returns:
    ///   - `Some(Scope::Session)` if the user previously granted session-wide.
    ///   - `Some(Scope::OneShot)` if granted once; this call decrements it.
    ///   - `Some(Scope::Denied)` if explicitly denied.
    ///   - `None` if no decision yet — caller must prompt the user.
    pub fn check(&self, _session_id: &str) -> Option<Scope> {
        // TODO: read the map, decrement one-shot grants.
        unimplemented!("permission::check (#13 in v1 build order)")
    }

    pub fn set(&self, _session_id: &str, _scope: Scope) {
        // TODO: write the map.
        unimplemented!("permission::set")
    }
}
