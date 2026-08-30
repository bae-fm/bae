//! Pick an import's cover image from the folder's own files.

use super::ImportService;
use crate::import::folder_scanner::ScannedFile;

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
    /// Read the chosen cover file's bytes. Nothing is written here, and no row is
    /// built: the caller resizes the winning candidate and records the result.
    pub(super) fn pick_folder_cover(
        &self,
        discovered_files: &[ScannedFile],
        selected_cover_path: Option<&str>,
    ) -> Result<Option<CoverCandidate>, crate::import::ImportError> {
        use crate::import::ImportError;

        let selected_cover = if let Some(selected_path) = selected_cover_path {
            Some(discovered_files.iter().find(|file| {
                file.relative_path == selected_path
                    && crate::util::content_type_hint::ContentTypeHint::path_is_raster_image(
                        &file.path,
                    )
            }).ok_or_else(|| ImportError::CoverArt {
                detail: format!(
                    "Selected cover {} not found among discovered images",
                    selected_path
                ),
            })?)
        } else {
            crate::import::local_artwork::default_local_cover_file(discovered_files)
        };

        let Some(cover_file) = selected_cover else {
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
            source_url: Some(format!("release://{}", cover_file.relative_path)),
        }))
    }
}
