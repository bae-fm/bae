use crate::cache::CacheManager;
use crate::cloud_storage::CloudStorage;
use crate::encryption::EncryptionService;
use crate::library::LibraryManager;
use crate::playback::track_loader::load_track_audio;
use crate::storage::create_storage_reader;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};

/// Export service for exporting files and tracks
pub struct ExportService;

impl ExportService {
    /// Export all files for a release to a directory
    ///
    /// Copies files from storage to the target directory.
    /// Files are written with their original filenames.
    pub async fn export_release(
        release_id: &str,
        target_dir: &Path,
        library_manager: &LibraryManager,
        _cache: &CacheManager,
        encryption_service: &EncryptionService,
    ) -> Result<(), String> {
        info!(
            "Exporting release {} to {}",
            release_id,
            target_dir.display()
        );

        // Get storage profile for this release
        let storage_profile = library_manager
            .get_storage_profile_for_release(release_id)
            .await
            .map_err(|e| format!("Failed to get storage profile: {}", e))?
            .ok_or_else(|| "No storage profile found for release".to_string())?;

        let storage = create_storage_reader(&storage_profile)
            .await
            .map_err(|e| format!("Failed to create storage reader: {}", e))?;

        let files = library_manager
            .get_files_for_release(release_id)
            .await
            .map_err(|e| format!("Failed to get files: {}", e))?;

        if files.is_empty() {
            return Err("No files found for release".to_string());
        }

        for file in &files {
            let file_data = if let Some(ref source_path) = file.source_path {
                // Read from storage
                debug!("Reading file from storage: {}", source_path);
                let data = storage
                    .download(source_path)
                    .await
                    .map_err(|e| format!("Failed to read file {}: {}", source_path, e))?;

                // Decrypt if profile has encryption enabled
                if storage_profile.encrypted {
                    let enc_service = encryption_service.clone();
                    tokio::task::spawn_blocking(move || {
                        enc_service
                            .decrypt(&data)
                            .map_err(|e| format!("Failed to decrypt file: {}", e))
                    })
                    .await
                    .map_err(|e| format!("Decryption task failed: {}", e))??
                } else {
                    data
                }
            } else {
                return Err(format!(
                    "File {} has no source path",
                    file.original_filename
                ));
            };

            // Ensure subdirectories exist for nested filenames
            let file_path = target_dir.join(&file.original_filename);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            }

            std::fs::write(&file_path, &file_data)
                .map_err(|e| format!("Failed to write file {}: {}", file.original_filename, e))?;

            debug!(
                "Exported file {} ({} bytes)",
                file.original_filename,
                file_data.len()
            );
        }

        info!(
            "Successfully exported {} files to {}",
            files.len(),
            target_dir.display()
        );
        Ok(())
    }

    /// Export a single track as a FLAC file
    ///
    /// For one-file-per-track: extracts the original file.
    /// For CUE/FLAC: extracts and re-encodes as a standalone FLAC.
    pub async fn export_track(
        track_id: &str,
        output_path: &Path,
        library_manager: &LibraryManager,
        storage: Arc<dyn CloudStorage>,
        cache: &CacheManager,
        encryption_service: &EncryptionService,
    ) -> Result<(), String> {
        info!("Exporting track {} to {}", track_id, output_path.display());

        let pcm_source = load_track_audio(
            track_id,
            library_manager,
            Some(storage),
            cache,
            encryption_service,
        )
        .await
        .map_err(|e| e.to_string())?;

        let flac_data = crate::audio_codec::encode_to_flac(
            pcm_source.raw_samples(),
            pcm_source.sample_rate(),
            pcm_source.channels(),
            pcm_source.bits_per_sample(),
        )
        .map_err(|e| format!("Failed to encode FLAC: {}", e))?;

        std::fs::write(output_path, &flac_data)
            .map_err(|e| format!("Failed to write track file: {}", e))?;

        info!(
            "Successfully exported track {} ({} bytes)",
            track_id,
            flac_data.len()
        );
        Ok(())
    }
}
