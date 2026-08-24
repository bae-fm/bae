//! Pick an import's cover image from the folder's own files. Folder images are
//! ranked so that `cover`/`front` wins when the user made no explicit pick.

use crate::import::folder_scanner::ScannedFile;
use crate::util::content_type_hint::ContentTypeHint;

use super::ImportService;

/// A candidate cover's bytes as they came from their source, with the provenance
/// the `covers` row records. The bytes here are NOT the stored ones: the import
/// funnel resizes the winning candidate and builds the row from that output, so
/// nothing describing a candidate can be mistaken for a description of the blob.
#[derive(Debug)]
pub(super) struct CoverCandidate {
    pub bytes: Vec<u8>,
    /// `covers.source`: "local" for a folder image, "embedded" for a picture
    /// pulled out of the audio, else the metadata source that supplied the URL.
    pub source: String,
    /// `covers.source_url`: "release://{path}" for a folder image, the download
    /// URL for a remote one, `None` for an embedded picture.
    pub source_url: Option<String>,
}

impl ImportService {
    /// How a folder's images rank as the cover when nothing has chosen one —
    /// no stored pick, and a source that offered no art of its own.
    ///
    /// A conventionally named image first, and among those the one at the
    /// release root: the scan lists files by relative path, which puts
    /// `Artwork/front.jpg` ahead of `cover.jpg`, so name alone would reach into
    /// the subfolder for a cover sitting at the top.
    pub(super) fn folder_cover_rank(file: &ScannedFile) -> (u8, u8) {
        (
            u8::from(!crate::import::folder_scanner::is_cover_name(&file.path)),
            u8::from(file.dir_prefix.is_some()),
        )
    }

    /// Read the chosen cover file's bytes. Nothing is written here, and no row is
    /// built: the caller resizes the winning candidate and records the result.
    pub(super) fn pick_folder_cover(
        &self,
        discovered_files: &[ScannedFile],
        selected_cover_path: Option<&str>,
    ) -> Result<Option<CoverCandidate>, crate::import::ImportError> {
        use crate::import::ImportError;

        let mut image_files: Vec<(&ScannedFile, &str)> = Vec::new();
        for f in discovered_files {
            if ContentTypeHint::path_is_raster_image(&f.path) {
                image_files.push((f, f.relative_path.as_str()));
            }
        }

        let selected_cover = if let Some(selected_path) = selected_cover_path {
            Some(
                image_files
                    .iter()
                    .find(|(f, _)| f.relative_path == selected_path)
                    .ok_or_else(|| ImportError::CoverArt {
                        detail: format!(
                            "Selected cover {} not found among discovered images",
                            selected_path
                        ),
                    })?,
            )
        } else {
            image_files
                .iter()
                .min_by_key(|(file, _)| Self::folder_cover_rank(file))
        };

        let Some((cover_file, relative_path)) = selected_cover else {
            return Ok(None);
        };

        let bytes = std::fs::read(&cover_file.path).map_err(|e| ImportError::CoverArt {
            detail: format!(
                "Failed to read cover art {}: {e}",
                cover_file.path.display()
            ),
        })?;

        Ok(Some(CoverCandidate {
            bytes,
            source: "local".to_string(),
            source_url: Some(format!("release://{}", relative_path)),
        }))
    }
}
