//! Whether an image file opens the way its format does: the check that tells
//! a damaged image (a corrupt download) from one a release can show.
//!
//! Magic-byte and minimum-size checks, by the magic bytes bae-util's
//! `ContentType::opens_with` knows. No deep parsing, no heuristics.

use bae_util::content_type::ContentType;
use bae_util::content_type_hint::ContentTypeHint;
use coven::LeasePlace;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Whether an image file's magic bytes match its extension. An extension with
/// no known magic is assumed valid — an unrecognized format is not evidence of
/// corruption, so it must not block the import. The file opens in `place`.
pub(super) fn is_valid_image(path: &Path, place: LeasePlace) -> io::Result<bool> {
    #[cfg(test)]
    crate::tests::reads::record(crate::tests::reads::FileRead::ImageCheck, path);
    // Ahead of the dispatch: an empty file is corrupt whatever its extension,
    // so an unrecognized one must not pass it through.
    if fs::metadata(path)?.len() == 0 {
        return Ok(false);
    }
    let format = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(|extension| ContentTypeHint::from_extension(extension).image_content_type());
    let Some(format) = format else {
        return Ok(true);
    };
    let mut header = Vec::with_capacity(ContentType::IMAGE_HEADER_LEN);
    let file = place.open(path)?;
    (&*file).take(ContentType::IMAGE_HEADER_LEN as u64).read_to_end(&mut header)?;
    Ok(format.opens_with(&header).unwrap_or(true))
}

#[cfg(test)]
#[path = "image_header_tests.rs"]
mod tests;
