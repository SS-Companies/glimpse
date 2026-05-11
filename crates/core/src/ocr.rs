//! Windows.Media.Ocr wrapper.
//!
//! Builds a `SoftwareBitmap` from a captured BGRA8 frame and runs the WinRT
//! OCR engine over it.

use crate::{capture::CapturedFrame, Error, Result};

use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;

#[derive(Debug, Clone)]
pub struct OcrResult {
    pub text: String,
    /// BCP-47 tag returned by the OCR engine (e.g. `"en-US"`).
    pub language: String,
}

/// Run Windows OCR over a captured BGRA frame.
///
/// `language_tag` is a BCP-47 tag like `"en-US"` or `"ja"`. Pass `None` to use
/// the system's user-profile language list.
pub fn ocr_frame(frame: &CapturedFrame, language_tag: Option<&str>) -> Result<OcrResult> {
    if frame.width == 0 || frame.height == 0 {
        return Err(Error::Ocr("empty frame".into()));
    }
    let expected = (frame.width as usize) * (frame.height as usize) * 4;
    if frame.pixels.len() != expected {
        return Err(Error::Ocr(format!(
            "pixel buffer length {} does not match {}x{}x4 = {}",
            frame.pixels.len(),
            frame.width,
            frame.height,
            expected
        )));
    }

    // 1. Wrap the BGRA bytes in a WinRT IBuffer via DataWriter.
    let writer =
        DataWriter::new().map_err(|e| Error::Ocr(format!("DataWriter::new: {e}")))?;
    writer
        .WriteBytes(&frame.pixels)
        .map_err(|e| Error::Ocr(format!("WriteBytes: {e}")))?;
    let buffer = writer
        .DetachBuffer()
        .map_err(|e| Error::Ocr(format!("DetachBuffer: {e}")))?;

    // 2. Build a SoftwareBitmap.
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        frame.width as i32,
        frame.height as i32,
    )
    .map_err(|e| Error::Ocr(format!("CreateCopyFromBuffer: {e}")))?;

    // 3. Pick / build an OcrEngine.
    let engine = match language_tag {
        Some(tag) => {
            let lang = Language::CreateLanguage(&windows::core::HSTRING::from(tag))
                .map_err(|e| Error::Ocr(format!("CreateLanguage({tag}): {e}")))?;
            OcrEngine::TryCreateFromLanguage(&lang).map_err(|e| {
                Error::Ocr(format!(
                    "TryCreateFromLanguage({tag}): {e} \
                     (install the language pack in Windows Settings → Time & Language → Language)"
                ))
            })?
        }
        None => OcrEngine::TryCreateFromUserProfileLanguages().map_err(|e| {
            Error::Ocr(format!(
                "TryCreateFromUserProfileLanguages: {e} \
                 (no OCR-capable language is installed for the current user)"
            ))
        })?,
    };

    // 4. Recognize. RecognizeAsync returns an IAsyncOperation; .get() blocks.
    let op = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| Error::Ocr(format!("RecognizeAsync: {e}")))?;
    let result = op
        .get()
        .map_err(|e| Error::Ocr(format!("RecognizeAsync await: {e}")))?;

    let text = result
        .Text()
        .map_err(|e| Error::Ocr(format!("OcrResult::Text: {e}")))?
        .to_string();

    let language = engine
        .RecognizerLanguage()
        .and_then(|l| l.LanguageTag())
        .map(|t| t.to_string())
        .unwrap_or_else(|_| "und".to_string());

    Ok(OcrResult { text, language })
}

/// List BCP-47 tags supported by the local Windows OCR engine.
pub fn available_languages() -> Result<Vec<String>> {
    let langs = OcrEngine::AvailableRecognizerLanguages()
        .map_err(|e| Error::Ocr(format!("AvailableRecognizerLanguages: {e}")))?;
    let mut out = Vec::new();
    for lang in langs {
        if let Ok(tag) = lang.LanguageTag() {
            out.push(tag.to_string());
        }
    }
    Ok(out)
}

