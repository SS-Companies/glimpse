//! Win32 clipboard get/set for CF_UNICODETEXT.
//!
//! Spec, in order:
//! 1. `OpenClipboard(NULL)`        — acquire the clipboard for the calling thread.
//! 2. `EmptyClipboard()`           — discard prior contents (set only).
//! 3. `GlobalAlloc(GMEM_MOVEABLE, n)` — allocate a movable HGLOBAL big enough
//!    for the UTF-16 text + null terminator.
//! 4. `GlobalLock`                 — get a pointer into the HGLOBAL.
//! 5. memcpy UTF-16 + null         — write the text.
//! 6. `GlobalUnlock`               — release the lock (does not free).
//! 7. `SetClipboardData(CF_UNICODETEXT, h)` — ownership of h transfers to the OS.
//! 8. `CloseClipboard()`           — release the clipboard.
//!
//! On the read path: open, `GetClipboardData(CF_UNICODETEXT)`, lock, copy,
//! unlock, close. Ownership stays with the clipboard — we never free.

use crate::{Error, Result};
use std::ptr;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Set the system clipboard to `text` (CF_UNICODETEXT).
pub fn set_text(text: &str) -> Result<()> {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = utf16.len() * std::mem::size_of::<u16>();

    unsafe {
        OpenClipboard(HWND(ptr::null_mut()))
            .map_err(|e| Error::Clipboard(format!("OpenClipboard: {e}")))?;
        let _close = ClipboardGuard;

        EmptyClipboard().map_err(|e| Error::Clipboard(format!("EmptyClipboard: {e}")))?;

        let hmem: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, byte_len)
            .map_err(|e| Error::Clipboard(format!("GlobalAlloc({byte_len}): {e}")))?;
        if hmem.is_invalid() {
            return Err(Error::Clipboard("GlobalAlloc returned null".into()));
        }

        let dst = GlobalLock(hmem) as *mut u16;
        if dst.is_null() {
            return Err(Error::Clipboard("GlobalLock returned null".into()));
        }
        ptr::copy_nonoverlapping(utf16.as_ptr(), dst, utf16.len());
        let _ = GlobalUnlock(hmem); // returns 0 on success per docs

        // SetClipboardData transfers ownership of hmem to the system on success.
        // If it FAILS we must free hmem ourselves; the windows crate's HGLOBAL
        // does not have a Drop impl, so this is a real concern.
        let h = HANDLE(hmem.0);
        SetClipboardData(CF_UNICODETEXT.0 as u32, h).map_err(|e| {
            // Free the buffer we still own.
            let _ = GlobalFree(hmem);
            Error::Clipboard(format!("SetClipboardData: {e}"))
        })?;
    }
    Ok(())
}

/// Read the current clipboard as UTF-8 text. Empty string if the clipboard
/// has no CF_UNICODETEXT format set.
pub fn get_text() -> Result<String> {
    unsafe {
        OpenClipboard(HWND(ptr::null_mut()))
            .map_err(|e| Error::Clipboard(format!("OpenClipboard: {e}")))?;
        let _close = ClipboardGuard;

        let h = match GetClipboardData(CF_UNICODETEXT.0 as u32) {
            Ok(h) if !h.is_invalid() => h,
            Ok(_) => return Ok(String::new()),
            // No CF_UNICODETEXT data set on the clipboard — return empty
            // rather than an error.
            Err(_) => return Ok(String::new()),
        };

        let hmem = HGLOBAL(h.0);
        let src = GlobalLock(hmem) as *const u16;
        if src.is_null() {
            return Err(Error::Clipboard("GlobalLock(get) returned null".into()));
        }

        // Find the null terminator. Cap the scan at 64 MiB / 2 to avoid
        // walking off the end of a malformed clipboard payload.
        let mut len = 0usize;
        const MAX_CHARS: usize = 32 * 1024 * 1024;
        while len < MAX_CHARS && *src.add(len) != 0 {
            len += 1;
        }

        let slice = std::slice::from_raw_parts(src, len);
        let text = String::from_utf16_lossy(slice);
        let _ = GlobalUnlock(hmem);
        Ok(text)
    }
}

/// Drop guard so `CloseClipboard` runs on every exit path, including panics.
struct ClipboardGuard;
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Touching the system clipboard from a unit test is flaky in CI (multiple
    // tests can race for the global resource, and CI windows runners sometimes
    // do not provide a usable clipboard). The roundtrip test is therefore
    // gated behind `#[ignore]` so it only runs on demand via:
    //     cargo test -p glimpse-core --lib -- --ignored
    #[test]
    #[ignore]
    fn roundtrip_ascii() {
        set_text("hello glimpse").unwrap();
        assert_eq!(get_text().unwrap(), "hello glimpse");
    }

    #[test]
    #[ignore]
    fn roundtrip_unicode() {
        let s = "héllo — 世界 🌍";
        set_text(s).unwrap();
        assert_eq!(get_text().unwrap(), s);
    }

    #[test]
    #[ignore]
    fn roundtrip_empty() {
        set_text("").unwrap();
        assert_eq!(get_text().unwrap(), "");
    }
}
