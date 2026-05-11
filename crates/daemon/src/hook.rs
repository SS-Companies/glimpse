//! Low-level Windows mouse hook (`WH_MOUSE_LL`).

use glimpse_core::gesture::GestureEvent;

/// Install the low-level mouse hook and forward translated events to `tx`.
///
/// Returns once the daemon is shutting down.
pub fn run(_tx: std::sync::mpsc::Sender<GestureEvent>) -> anyhow::Result<()> {
    // TODO:
    //   - SetWindowsHookExW(WH_MOUSE_LL, ...)
    //   - GetMessageW loop on this thread
    //   - In the callback, translate WM_LBUTTONDOWN/UP, WM_RBUTTONDOWN/UP,
    //     WM_MOUSEMOVE into GestureEvent and tx.send(...)
    //   - When the gesture is in the "fired" state, return 1 from the
    //     callback to swallow the eventual right-click context menu.
    unimplemented!("hook::run (#5 in v1 build order)")
}
