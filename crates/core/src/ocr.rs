//! Windows.Media.Ocr wrapper.
//!
//! Wraps the WinRT OCR engine. v1 uses the system default language picked by
//! the user's Windows display language; the config may override.

use crate::{capture::CapturedFrame, Result};

#[derive(Debug, Clone)]
pub struct OcrResult {
    pub text: String,
    /// Language tag returned by the OCR engine (e.g. `"en-US"`).
    pub language: String,
}

/// Run Windows OCR over a captured BGRA frame.
///
/// `language_tag` is a BCP-47 tag like `"en-US"` or `"ja"`. Pass `None` to use
/// the system default.
pub fn ocr_frame(_frame: &CapturedFrame, _language_tag: Option<&str>) -> Result<OcrResult> {
    // TODO: build SoftwareBitmap from BGRA bytes, call OcrEngine::TryCreateFromLanguage,
    // call RecognizeAsync, await.
    unimplemented!("ocr::ocr_frame (#3 in v1 build order)")
}

/// List BCP-47 tags supported by the local Windows OCR engine.
pub fn available_languages() -> Result<Vec<String>> {
    // TODO: OcrEngine::AvailableRecognizerLanguages.
    unimplemented!("ocr::available_languages")
}
