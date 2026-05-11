//! Daily auto-update poller against GitHub Releases.

/// Spawn a background task that checks for new releases once every 24 hours.
/// When a new release is found, surface a tray balloon; never silently replace
/// the running binary.
pub fn spawn(_config: &glimpse_core::Config) {
    // TODO: use `self_update` crate. Honour `config.auto_update_check`.
    // On found release, set a tray flag; user clicks "Update now" to apply.
}
