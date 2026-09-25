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
    /// Read the selected cover file's bytes. Nothing is written here, and no row
    /// is built: the caller resizes the winning candidate and records the result.
    ///
    /// The path is the candidate's stored selection, so a folder that no
    /// longer holds that image is a candidate whose selection no longer
    /// describes it, and that is stated rather than replaced by some other
    /// image.
    ///
    /// The read runs on a blocking thread, so a cover on a slow volume does not
    /// stall the import worker's runtime.
    pub(super) async fn pick_folder_cover(
        discovered_files: &[ScannedFile],
        selected_cover_path: &str,
    ) -> Result<Option<CoverCandidate>, crate::import::ImportError> {
        use crate::import::ImportError;

        let cover_file = discovered_files
            .iter()
            .find(|file| {
                file.relative_path == selected_cover_path
                    && crate::util::content_type_hint::ContentTypeHint::path_is_supported_cover(
                        &file.path,
                    )
            })
            .ok_or_else(|| ImportError::LocalCover {
                detail: format!(
                    "Selected cover {selected_cover_path} not found among discovered images"
                ),
            })?;

        let path = cover_file.path.clone();
        let bytes = tokio::task::spawn_blocking(move || std::fs::read(&path))
            .await
            .map_err(|e| ImportError::Internal {
                detail: format!("cover read task failed: {e}"),
            })?
            .map_err(|e| ImportError::LocalCover {
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
