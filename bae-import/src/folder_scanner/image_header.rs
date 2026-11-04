//! Whether an image file opens the way its format does: the check that tells
//! a damaged image (a corrupt download) from one a release can show.
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
        checks.iter().all(|(offset, magic)| &header[*offset..*offset + magic.len()] == *magic)
    }))
}

/// Whether an image file's magic bytes match its extension. An extension with
/// no known magic is assumed valid — an unrecognized format is not evidence of
/// corruption, so it must not block the import.
pub(super) fn is_valid_image(path: &Path) -> io::Result<bool> {
    #[cfg(test)]
    crate::tests::reads::record(crate::tests::reads::FileRead::ImageCheck, path);
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
    path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default()
}

#[cfg(test)]
#[path = "image_header_tests.rs"]
mod tests;
