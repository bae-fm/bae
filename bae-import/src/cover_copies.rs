//! Making the two copies a cover keeps, and the format they share. The import and a
//! library release's Change cover both make them.

use bae_pictures::{CopyFormat, PictureCopy, PictureStep};
use bae_util::content_type::ContentType;

use bae_records::CoverCopies;

use crate::ImportError;

/// The copies of the picture `bytes` hold, made on a blocking thread:
/// libvips decodes and shrinks them, CPU work.
pub async fn make_cover_copies(
    bytes: Vec<u8>,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<CoverCopies, ImportError> {
    let (browse, large) = diagnostics
        .blocking("make a cover's copies", move || {
            Ok::<_, bae_pictures::PicturesError>((
                bae_pictures::make_copy_of_bytes(bytes.clone(), PictureStep::S768)?,
                bae_pictures::make_copy_of_bytes(bytes, PictureStep::S1536)?,
            ))
        })
        .await?
        .map_err(|error| ImportError::CoverArt { detail: error.to_string() })?;
    cover_copies_of(browse, large)
}

/// The cover's copies from the two copies of its picture, which share a
/// format.
pub fn cover_copies_of(
    browse: PictureCopy,
    large: PictureCopy,
) -> Result<CoverCopies, ImportError> {
    if browse.format != large.format {
        return Err(ImportError::Internal {
            detail: "a cover's browse and large copies are in different formats".to_string(),
        });
    }
    Ok(CoverCopies {
        content_type: content_type_of(browse.format),
        browse: browse.bytes,
        large: large.bytes,
    })
}

/// The content type of a copy in `format`.
pub fn content_type_of(format: CopyFormat) -> ContentType {
    match format {
        CopyFormat::Jpeg => ContentType::Jpeg,
        CopyFormat::Png => ContentType::Png,
    }
}
