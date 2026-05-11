//! Editable preview popup near the cursor.
//!
//! Pops up immediately after OCR fires with the recognised text in a single
//! line edit. Enter commits to clipboard, Esc cancels and clears the clipboard
//! back to its previous state.

pub fn show_editable(_initial_text: String, _anchor_x: i32, _anchor_y: i32) -> anyhow::Result<()> {
    // TODO: spawn a small egui/eframe window:
    //   - frameless, always-on-top, anchored near cursor (clamped to screen)
    //   - single-line text editor pre-filled with `initial_text`
    //   - "Copy" button + Enter shortcut
    //   - Esc / blur dismisses
    //   - 3-second auto-dismiss timer if untouched
    unimplemented!("popup::show_editable (#9 in v1 build order)")
}
