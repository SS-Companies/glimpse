//! Post-OCR text cleanup pipeline.

/// Apply the v1 default cleanup pipeline to OCR output.
///
/// v1 only does two things, both safe:
/// - trim leading/trailing whitespace
/// - collapse runs of internal whitespace to single spaces
///
/// Toggles for smart-quote replacement, hyphenation joining, etc. land in v1.5.
pub fn clean(input: &str) -> String {
    let trimmed = input.trim();
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_collapses() {
        assert_eq!(clean("  hello   world  \n"), "hello world");
    }

    #[test]
    fn preserves_unicode() {
        assert_eq!(clean("  hé  llo  "), "hé llo");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(clean("   \t \n  "), "");
    }
}
