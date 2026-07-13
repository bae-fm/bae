//! Pick and read an import's cover image from the folder's own files, building
//! the `DbLibraryImage` record and its bytes. Folder images are ranked so that
//! `cover`/`front` wins when the user made no explicit pick.

use crate::import::folder_scanner::ScannedFile;
use crate::util::content_type_hint::ContentTypeHint;

use super::format_prep::resolve_file_content_type;
use super::ImportService;

impl ImportService {
    /// Read the chosen cover file's bytes and build its `DbLibraryImage` record.
    /// Nothing is written here: the caller hands the bytes to coven's local store
    /// and the record to finalize.
    #[allow(clippy::type_complexity)]
    pub(super) fn build_cover_image_record(
        &self,
        release_id: &str,
        discovered_files: &[ScannedFile],
        selected_cover_path: Option<&str>,
    ) -> Result<Option<(crate::db::DbLibraryImage, Vec<u8>)>, crate::import::ImportError> {
        use crate::db::{DbLibraryImage, LibraryImageType};
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
                .min_by_key(|(_, relative_path)| Self::image_cover_priority(relative_path))
        };

        let Some((cover_file, relative_path)) = selected_cover else {
            return Ok(None);
        };

        let content_type = resolve_file_content_type(&cover_file.path)?;
        let source_url = format!("release://{}", relative_path);

        let bytes = std::fs::read(&cover_file.path).map_err(|e| ImportError::CoverArt {
            detail: format!(
                "Failed to read cover art {}: {e}",
                cover_file.path.display()
            ),
        })?;

        let now = self.library_manager.clock().now();
        let db_image = DbLibraryImage {
            id: release_id.to_string(),
            image_type: LibraryImageType::Cover,
            content_type,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: Some(source_url),
            // Computed in the finalize transaction under a browsable home.
            cloud_path: None,
            content_hash: Some(crate::util::fs::hash_bytes(&bytes)),
            created_at: now,
        };

        Ok(Some((db_image, bytes)))
    }

    pub(super) fn image_cover_priority(filename: &str) -> u8 {
        let lower = filename.to_lowercase();
        if lower.contains("cover") || lower.contains("front") {
            return 0;
        }
        1
    }
}
