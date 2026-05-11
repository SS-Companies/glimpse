//! Cursor-ring overlay: a translucent, click-through window drawn at the
//! cursor that fills as the hold progresses, then dissolves on Fire or Cancel.

pub fn show(_x: i32, _y: i32) -> anyhow::Result<()> {
    // TODO: create a layered transparent window with WS_EX_LAYERED |
    // WS_EX_TRANSPARENT | WS_EX_TOPMOST. Draw arc via Direct2D or GDI+.
    unimplemented!("ring::show (#8 in v1 build order)")
}

pub fn update_progress(_fraction: f32) {
    // 0.0 → 1.0 over the hold window.
}

pub fn hide() {
    // Tear down the overlay.
}
