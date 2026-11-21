//! Text encoding detection and decoding.

use coven::LeasedFile;
use std::io::Read;

#[derive(Debug)]
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

/// The most bytes of a text document that bae reads whole, such as a CUE
/// sheet or a rip log.
pub const MAX_DOCUMENT_BYTES: u64 = 5 * 1024 * 1024;

/// Refuse a text document of `size` bytes that is over [`MAX_DOCUMENT_BYTES`],
/// with an error that names `name` and its size.
pub fn check_document_size(name: &str, size: u64) -> std::io::Result<()> {
    if size > MAX_DOCUMENT_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{name} is {size} bytes, and bae reads a text document of at most {MAX_DOCUMENT_BYTES} bytes"
            ),
        ));
    }
    Ok(())
}

/// Decode the text document `name` from its `bytes`, detecting the encoding
/// as [`decode_text`] does. A document over [`MAX_DOCUMENT_BYTES`] is refused.
pub fn decode_document(name: &str, bytes: &[u8]) -> std::io::Result<DecodedText> {
    check_document_size(name, bytes.len() as u64)?;
    Ok(decode_text(bytes))
}

/// Read the open text document `name` to its end and decode its contents,
/// detecting encoding automatically. A document over [`MAX_DOCUMENT_BYTES`]
/// is refused before any of it is read. read_text_file closes the file,
/// which gives its place back, when it returns.
pub fn read_text_file(
    name: &str,
    mut file: LeasedFile<std::fs::File>,
) -> std::io::Result<DecodedText> {
    check_document_size(name, file.metadata()?.len())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    decode_document(name, &bytes)
}

#[cfg(test)]
#[path = "text_encoding_tests.rs"]
mod tests;
