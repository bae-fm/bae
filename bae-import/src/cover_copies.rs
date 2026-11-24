//! The two copies a cover keeps, and the format they share. The import and a
//! library release's Change cover both make them.

use bae_pictures::{CopyFormat, PictureCopy, PictureStep};
use bae_util::content_type::ContentType;

use crate::ImportError;

/// A cover's browse copy and large copy, made from one picture, so both are
/// in one format.
#[derive(Debug, Clone)]
pub struct CoverCopies {
    pub browse: Vec<u8>,
    pub large: Vec<u8>,
    /// The format of both copies.
    pub content_type: ContentType,
}

impl CoverCopies {
    /// The copies of the picture `bytes` hold, made on a blocking thread:
    /// libvips decodes and shrinks them, CPU work.
    pub async fn of_picture(
        bytes: Vec<u8>,
        diagnostics: &bae_diagnostics::Diagnostics,
    ) -> Result<Self, ImportError> {
        let (browse, large) = diagnostics
            .blocking("make a cover's copies", move || {
                Ok::<_, bae_pictures::PicturesError>((
                    bae_pictures::make_copy_of_bytes(bytes.clone(), PictureStep::S768)?,
                    bae_pictures::make_copy_of_bytes(bytes, PictureStep::S1536)?,
                ))
            })
            .await?
            .map_err(|error| ImportError::CoverArt { detail: error.to_string() })?;
        Self::of_copies(browse, large)
    }

    /// The cover's copies from the two copies of its picture.
    pub fn of_copies(browse: PictureCopy, large: PictureCopy) -> Result<Self, ImportError> {
        if browse.format != large.format {
            return Err(ImportError::Internal {
                detail: "a cover's browse and large copies are in different formats".to_string(),
            });
        }
        Ok(Self {
            content_type: content_type_of(browse.format),
            browse: browse.bytes,
            large: large.bytes,
        })
    }
}

/// The content type of a copy in `format`.
pub fn content_type_of(format: CopyFormat) -> ContentType {
    match format {
        CopyFormat::Jpeg => ContentType::Jpeg,
        CopyFormat::Png => ContentType::Png,
    }
}
