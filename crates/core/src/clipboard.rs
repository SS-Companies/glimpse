//! Clipboard get/set via Win32 OpenClipboard / SetClipboardData.

use crate::Result;

pub fn set_text(_text: &str) -> Result<()> {
    // TODO: OpenClipboard / EmptyClipboard / GlobalAlloc(GMEM_MOVEABLE) /
    // SetClipboardData(CF_UNICODETEXT) / CloseClipboard.
    unimplemented!("clipboard::set_text")
}

pub fn get_text() -> Result<String> {
    // TODO: OpenClipboard / GetClipboardData(CF_UNICODETEXT) / CloseClipboard.
    unimplemented!("clipboard::get_text")
}
