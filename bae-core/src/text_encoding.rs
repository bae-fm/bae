//! Text encoding detection and decoding.
//!
//! Handles BOM detection (UTF-8, UTF-16 LE/BE), validates UTF-8,
//! and falls back to chardetng for legacy encodings (Windows-1252, Shift-JIS, etc.).

use std::path::Path;

/// Result of decoding text with detected encoding info.
pub struct DecodedText {
    pub text: String,
    /// The encoding that was detected or used, e.g. "UTF-8", "Shift_JIS", "windows-1252".
    pub encoding: String,
}

/// Decode raw bytes to a String, detecting encoding automatically.
///
/// 1. BOM: UTF-8 (EF BB BF), UTF-16 LE (FF FE), UTF-16 BE (FE FF)
/// 2. No BOM: try UTF-8
/// 3. Not valid UTF-8: chardetng detection, decode via encoding_rs
pub fn decode_text(bytes: &[u8]) -> DecodedText {
    if let Some((encoding, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (decoded, _, _) = encoding.decode(&bytes[bom_len..]);
        return DecodedText {
            text: decoded.into_owned(),
            encoding: encoding.name().to_string(),
        };
    }

    // Try UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return DecodedText {
            text: s.to_owned(),
            encoding: "UTF-8".to_string(),
        };
    }

    // Fallback: detect legacy encoding
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (decoded, _, _) = encoding.decode(bytes);
    DecodedText {
        text: decoded.into_owned(),
        encoding: encoding.name().to_string(),
    }
}

/// Read a file and decode its contents, detecting encoding automatically.
pub fn read_text_file(path: &Path) -> std::io::Result<DecodedText> {
    let bytes = std::fs::read(path)?;
    Ok(decode_text(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_no_bom() {
        let input = "Hello, world!".as_bytes();
        let result = decode_text(input);
        assert_eq!(result.text, "Hello, world!");
        assert_eq!(result.encoding, "UTF-8");
    }

    #[test]
    fn utf8_with_bom() {
        let mut input = vec![0xEF, 0xBB, 0xBF];
        input.extend_from_slice("Hello, world!".as_bytes());
        let result = decode_text(&input);
        assert_eq!(result.text, "Hello, world!");
        assert_eq!(result.encoding, "UTF-8");
    }

    #[test]
    fn utf16_le_with_bom() {
        let text = "Hello";
        let mut input = vec![0xFF, 0xFE]; // BOM
        for ch in text.encode_utf16() {
            input.extend_from_slice(&ch.to_le_bytes());
        }
        let result = decode_text(&input);
        assert_eq!(result.text, "Hello");
        assert_eq!(result.encoding, "UTF-16LE");
    }

    #[test]
    fn utf16_be_with_bom() {
        let text = "Hello";
        let mut input = vec![0xFE, 0xFF]; // BOM
        for ch in text.encode_utf16() {
            input.extend_from_slice(&ch.to_be_bytes());
        }
        let result = decode_text(&input);
        assert_eq!(result.text, "Hello");
        assert_eq!(result.encoding, "UTF-16BE");
    }

    #[test]
    fn windows_1252_fallback() {
        // "caf\xe9" = "cafe" with e-acute in Windows-1252
        let input = b"caf\xe9";
        let result = decode_text(input);
        assert!(
            result.text.contains("caf"),
            "should decode the ASCII portion"
        );
        // chardetng should detect this as a Latin encoding and decode the e-acute
        assert!(
            result.text.contains('\u{00e9}') || result.text.len() == 4,
            "should decode e-acute"
        );
    }

    #[test]
    fn shift_jis_fallback() {
        // Encode a known Shift-JIS string: "テスト" (test)
        let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode("テスト");
        let result = decode_text(&encoded);
        assert_eq!(result.text, "テスト");
        assert_eq!(result.encoding, "Shift_JIS");
    }
}
