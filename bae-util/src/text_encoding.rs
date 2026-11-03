//! Text encoding detection and decoding.

use std::path::Path;

pub struct DecodedText {
    pub text: String,
    /// The encoding detected or used, e.g. "UTF-8", "Shift_JIS", "windows-1252".
    pub encoding: String,
}

/// Decode raw bytes, detecting the encoding:
///
/// 1. BOM: UTF-8 (EF BB BF), UTF-16 LE (FF FE), UTF-16 BE (FE FF)
/// 2. No BOM: try UTF-8
/// 3. Not valid UTF-8: chardetng detection (Windows-1252, Shift-JIS, …)
pub fn decode_text(bytes: &[u8]) -> DecodedText {
    if let Some((encoding, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (decoded, _, _) = encoding.decode(&bytes[bom_len..]);
        return DecodedText { text: decoded.into_owned(), encoding: encoding.name().to_string() };
    }

    if let Ok(s) = std::str::from_utf8(bytes) {
        return DecodedText { text: s.to_owned(), encoding: "UTF-8".to_string() };
    }

    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (decoded, _, _) = encoding.decode(bytes);
    DecodedText { text: decoded.into_owned(), encoding: encoding.name().to_string() }
}

/// Read a file and decode its contents, detecting encoding automatically.
pub fn read_text_file(path: &Path) -> std::io::Result<DecodedText> {
    let bytes = std::fs::read(path)?;
    Ok(decode_text(&bytes))
}

#[cfg(test)]
#[path = "text_encoding_tests.rs"]
mod tests;
