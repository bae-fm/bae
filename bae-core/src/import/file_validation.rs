//! File header validation for detecting corrupt downloads.
//!
//! Magic-byte and minimum-size checks. No deep parsing, no heuristics.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// One accepted header shape: every `(offset, magic)` in it must match.
type Magic = &'static [(usize, &'static [u8])];

const JPEG: Magic = &[(0, &[0xFF, 0xD8, 0xFF])];
const PNG: Magic = &[(0, &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])];
const WEBP: Magic = &[(0, b"RIFF"), (8, b"WEBP")];
const GIF: Magic = &[(0, b"GIF8")]; // GIF87a and GIF89a share the prefix.
const BMP: Magic = &[(0, b"BM")];

/// The file's first `len` bytes, or `None` if it is empty or shorter than `len`
/// — either way it cannot be carrying the header in question.
fn read_header(path: &Path, len: usize) -> io::Result<Option<Vec<u8>>> {
    if fs::metadata(path)?.len() == 0 {
        return Ok(None);
    }
    let mut buf = vec![0u8; len];
    let read = fs::File::open(path)?.read(&mut buf)?;
    Ok((read >= len).then_some(buf))
}

/// Whether the file opens with any one of `alternatives`.
fn matches_any(path: &Path, alternatives: &[Magic]) -> io::Result<bool> {
    let len = alternatives
        .iter()
        .flat_map(|checks| checks.iter())
        .map(|(offset, magic)| offset + magic.len())
        .max()
        .unwrap_or(0);
    let Some(header) = read_header(path, len)? else {
        return Ok(false);
    };
    Ok(alternatives.iter().any(|checks| {
        checks
            .iter()
            .all(|(offset, magic)| &header[*offset..*offset + magic.len()] == *magic)
    }))
}

/// Whether an image file's magic bytes match its extension. An extension with
/// no known magic is assumed valid — an unrecognized format is not evidence of
/// corruption, so it must not block the import.
pub(crate) fn is_valid_image(path: &Path) -> io::Result<bool> {
    // Ahead of the dispatch: an empty file is corrupt whatever its extension,
    // so an unrecognized one must not pass it through.
    if fs::metadata(path)?.len() == 0 {
        return Ok(false);
    }
    match extension(path).as_str() {
        "jpg" | "jpeg" => matches_any(path, &[JPEG]),
        "png" => matches_any(path, &[PNG]),
        "webp" => matches_any(path, &[WEBP]),
        "gif" => matches_any(path, &[GIF]),
        "bmp" => matches_any(path, &[BMP]),
        _ => Ok(true),
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_file(extension: &str, data: &[u8]) -> NamedTempFile {
        let mut file = tempfile::Builder::new()
            .suffix(&format!(".{}", extension))
            .tempfile()
            .unwrap();
        file.write_all(data).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_valid_jpeg_magic() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let file = write_temp_file("jpg", &data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_valid_png_magic() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        let file = write_temp_file("png", &data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_valid_webp_magic() {
        let data = b"RIFF\x00\x00\x00\x00WEBP";
        let file = write_temp_file("webp", data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_valid_gif_magic() {
        let data = b"GIF89a\x00\x00";
        let file = write_temp_file("gif", data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_valid_bmp_magic() {
        let data = b"BM\x00\x00\x00\x00";
        let file = write_temp_file("bmp", data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_invalid_image_magic() {
        // Random bytes that don't match JPEG magic
        let data = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        let file = write_temp_file("jpg", &data);
        assert!(!is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn test_invalid_png_magic() {
        let data = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let file = write_temp_file("png", &data);
        assert!(!is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn truncated_image_headers_are_rejected() {
        // Each image magic is checked against `bytes_read`, so a file shorter
        // than its magic (webp=12, gif=4, bmp=2) is rejected without an
        // out-of-bounds read.
        for (ext, bytes) in [
            ("webp", &b"RIFF\x00\x00\x00\x00WEB"[..]), // 11 bytes, needs 12
            ("gif", &b"GIF"[..]),                      // 3 bytes, needs 4
            ("bmp", &b"B"[..]),                        // 1 byte, needs 2
        ] {
            let file = write_temp_file(ext, bytes);
            assert!(
                !is_valid_image(file.path()).unwrap(),
                ".{ext} with a truncated magic must be rejected",
            );
        }
    }

    #[test]
    fn test_unknown_image_extension_assumed_valid() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let file = write_temp_file("tiff", &data);
        assert!(is_valid_image(file.path()).unwrap());
    }

    #[test]
    fn zero_byte_images_are_rejected() {
        for ext in ["jpg", "png", "webp", "gif", "bmp"] {
            let file = write_temp_file(ext, &[]);
            assert!(
                !is_valid_image(file.path()).unwrap(),
                "zero-byte .{ext} image must be rejected",
            );
        }
    }
}
