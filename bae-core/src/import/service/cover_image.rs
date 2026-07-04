//! Pick and read the cover image for an import from the folder's files.
//!
//! Builds the `DbLibraryImage` record (and its bytes) for a local cover,
//! ranking folder images so `cover`/`front` wins when the user made no pick.

use std::path::Path;

use tracing::{error, info};

use crate::import::types::DiscoveredFile;

use super::format_prep::resolve_file_content_type;
use super::ImportService;

impl ImportService {
    /// Build a cover image record from local files without writing to DB.
    /// Read the chosen cover file's bytes and build its `DbLibraryImage` record,
    /// returning `(record, bytes)`. The caller hands the bytes to coven's local
    /// store (the cover's home as a host-provided Local blob) and the record to
    /// finalize; nothing is written to a bae path here.
    #[allow(clippy::type_complexity)]
    pub(super) fn build_cover_image_record(
        &self,
        release_id: &str,
        discovered_files: &[DiscoveredFile],
        cover_image_path: Option<&Path>,
    ) -> Result<Option<(crate::db::DbLibraryImage, Vec<u8>)>, String> {
        use crate::db::{DbLibraryImage, LibraryImageType};

        let image_extensions = ["jpg", "jpeg", "png", "gif", "webp"];
        let mut image_files: Vec<(&DiscoveredFile, String)> = Vec::new();
        for f in discovered_files {
            let is_image = f
                .path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| image_extensions.contains(&e.to_lowercase().as_str()))
                .unwrap_or(false);
            if is_image {
                let relative_path = self.get_relative_image_path(&f.path)?;
                image_files.push((f, relative_path));
            }
        }

        if image_files.is_empty() {
            return Ok(None);
        }

        // Determine which file is the cover: match by absolute path if provided.
        let selected_cover = if let Some(selected_path) = cover_image_path {
            let found = image_files
                .iter()
                .find(|(f, _)| f.path.as_path() == selected_path);
            if found.is_none() {
                info!(
                    "Selected cover {:?} not found among images, using priority",
                    selected_path
                );
            }
            found
        } else {
            None
        };

        let (cover_file, relative_path) = selected_cover.unwrap_or_else(|| {
            image_files
                .iter()
                .min_by_key(|(_, relative_path)| Self::image_cover_priority(relative_path))
                .expect("image_files is non-empty after earlier check")
        });
        let content_type = resolve_file_content_type(&cover_file.path)?;
        let source_url = format!("release://{}", relative_path);

        // Read the cover bytes from the user's folder; the caller stores them in
        // coven's local store and writes the row.
        let bytes = match std::fs::read(&cover_file.path) {
            Ok(bytes) => bytes,
            Err(e) => {
                error!(
                    "Failed to read cover art {}: {e}",
                    cover_file.path.display()
                );
                return Ok(None);
            }
        };

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

    fn get_relative_image_path(&self, path: &std::path::Path) -> Result<String, String> {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("Invalid filename: {:?}", path))?;
        if let Some(parent) = path.parent() {
            if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
                if parent_name == "scans" || parent_name == "artwork" || parent_name == "images" {
                    return Ok(format!("{}/{}", parent_name, filename));
                }
            }
        }
        Ok(filename.to_string())
    }
}
