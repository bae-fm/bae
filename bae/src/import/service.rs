use crate::cd::drive::CdToc;
use crate::cd::RipProgress;
use crate::db::{
    Database, DbAlbum, DbFile, DbRelease, DbStorageProfile, DbTrack, ImportOperationStatus,
};
use crate::encryption::EncryptionService;
use crate::import::handle::{ImportServiceHandle, TorrentImportMetadata};
use crate::import::types::{
    CueFlacMetadata, DiscoveredFile, ImportCommand, ImportPhase, ImportProgress, TorrentSource,
    TrackFile,
};
use crate::library::{LibraryManager, SharedLibraryManager};
use crate::storage::{ReleaseStorage, ReleaseStorageImpl};
use crate::torrent::TorrentManagerHandle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Calculate track progress percentage based on bytes written.
///
/// For CUE/FLAC: tracks have specific byte ranges within the file.
/// For one-file-per-track: track spans entire file (start=0, end=file_size).
fn calculate_track_percent(bytes_written: usize, start_byte: i64, end_byte: i64) -> u8 {
    let bytes_written = bytes_written as i64;
    if bytes_written >= end_byte {
        100
    } else if bytes_written <= start_byte {
        0
    } else {
        let written_for_track = bytes_written - start_byte;
        let track_size = end_byte - start_byte;
        if track_size <= 0 {
            100
        } else {
            ((written_for_track * 100) / track_size) as u8
        }
    }
}

/// Import service that orchestrates the album import workflow
pub struct ImportService {
    /// Channel for receiving import commands from clients
    commands_rx: mpsc::UnboundedReceiver<ImportCommand>,
    /// Channel for sending progress updates to subscribers
    progress_tx: mpsc::UnboundedSender<ImportProgress>,
    /// Service for encrypting files before upload
    encryption_service: EncryptionService,
    /// Shared manager for library database operations
    library_manager: SharedLibraryManager,
    /// Handle to torrent manager service for torrent operations
    torrent_handle: TorrentManagerHandle,
    /// Database for storage operations
    database: Arc<Database>,
}

impl ImportService {
    /// Start the import service worker.
    ///
    /// Creates one worker task that imports validated albums sequentially from a queue.
    /// Multiple imports will be queued and handled one at a time, not concurrently.
    /// Returns a handle that can be cloned and used throughout the app to submit import requests.
    pub fn start(
        _runtime_handle: tokio::runtime::Handle,
        library_manager: SharedLibraryManager,
        encryption_service: EncryptionService,
        torrent_handle: TorrentManagerHandle,
        database: Arc<Database>,
    ) -> ImportServiceHandle {
        let (commands_tx, commands_rx) = mpsc::unbounded_channel();
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();
        let progress_tx_for_handle = progress_tx.clone();
        let library_manager_for_worker = library_manager.clone();
        let database_for_handle = database.clone();
        let runtime_handle = tokio::runtime::Handle::current();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(async move {
                let mut service = ImportService {
                    commands_rx,
                    progress_tx,
                    library_manager: library_manager_for_worker,
                    encryption_service,
                    torrent_handle,
                    database,
                };

                info!("Worker started");
                loop {
                    match service.commands_rx.recv().await {
                        Some(command) => {
                            service.do_import(command).await;
                        }
                        None => {
                            info!("Worker receive channel closed");
                            break;
                        }
                    }
                }
            });
        });

        ImportServiceHandle::new(
            commands_tx,
            progress_tx_for_handle,
            progress_rx,
            library_manager,
            database_for_handle,
            runtime_handle,
        )
    }

    async fn do_import(&self, command: ImportCommand) {
        let (release_id_for_error, import_id_for_error) = match &command {
            ImportCommand::Folder {
                db_release,
                import_id,
                ..
            } => (db_release.id.clone(), Some(import_id.clone())),
            ImportCommand::Torrent { db_release, .. } => (db_release.id.clone(), None),
            ImportCommand::CD { db_release, .. } => (db_release.id.clone(), None),
        };

        let result = match command {
            ImportCommand::Folder {
                db_album,
                db_release,
                tracks_to_files,
                discovered_files,
                cue_flac_metadata,
                storage_profile_id,
                selected_cover_filename,
                import_id,
            } => {
                info!("Starting folder import for '{}'", db_album.title);
                match storage_profile_id {
                    Some(profile_id) => {
                        match self.database.get_storage_profile(&profile_id).await {
                            Ok(Some(profile)) => {
                                self.run_storage_import(
                                    &db_release,
                                    &discovered_files,
                                    &tracks_to_files,
                                    cue_flac_metadata,
                                    profile,
                                    selected_cover_filename,
                                    &import_id,
                                )
                                .await
                            }
                            Ok(None) => Err(format!("Storage profile not found: {}", profile_id)),
                            Err(e) => Err(format!("Failed to fetch storage profile: {}", e)),
                        }
                    }
                    None => {
                        self.run_none_import(
                            &db_release,
                            &discovered_files,
                            &tracks_to_files,
                            cue_flac_metadata,
                            selected_cover_filename,
                            &import_id,
                        )
                        .await
                    }
                }
            }
            ImportCommand::Torrent {
                db_album,
                db_release,
                tracks_to_files,
                torrent_source,
                torrent_metadata,
                seed_after_download,
                cover_art_url,
                storage_profile_id,
                selected_cover_filename,
            } => {
                info!("Starting torrent import for '{}'", db_album.title);
                match storage_profile_id {
                    Some(profile_id) => {
                        match self.database.get_storage_profile(&profile_id).await {
                            Ok(Some(profile)) => {
                                self.run_torrent_import(
                                    db_album,
                                    db_release,
                                    tracks_to_files,
                                    torrent_source,
                                    torrent_metadata,
                                    seed_after_download,
                                    cover_art_url,
                                    profile,
                                    selected_cover_filename,
                                )
                                .await
                            }
                            Ok(None) => Err(format!("Storage profile not found: {}", profile_id)),
                            Err(e) => Err(format!("Failed to fetch storage profile: {}", e)),
                        }
                    }
                    None => {
                        self.import_torrent_none_storage(
                            db_album,
                            db_release,
                            tracks_to_files,
                            torrent_source,
                            torrent_metadata,
                            cover_art_url,
                            selected_cover_filename,
                        )
                        .await
                    }
                }
            }
            ImportCommand::CD {
                db_album,
                db_release,
                db_tracks,
                drive_path,
                toc,
                storage_profile_id,
                selected_cover_filename,
            } => {
                info!("Starting CD import for '{}'", db_album.title);
                match storage_profile_id {
                    Some(profile_id) => {
                        match self.database.get_storage_profile(&profile_id).await {
                            Ok(Some(profile)) => {
                                self.run_cd_import(
                                    db_album,
                                    db_release,
                                    db_tracks,
                                    drive_path,
                                    toc,
                                    profile,
                                    selected_cover_filename,
                                )
                                .await
                            }
                            Ok(None) => Err(format!("Storage profile not found: {}", profile_id)),
                            Err(e) => Err(format!("Failed to fetch storage profile: {}", e)),
                        }
                    }
                    None => {
                        self.run_cd_import_none_storage(
                            db_album,
                            db_release,
                            db_tracks,
                            drive_path,
                            toc,
                            selected_cover_filename,
                        )
                        .await
                    }
                }
            }
        };

        if let Err(e) = result {
            error!("Import failed: {}", e);
            if let Err(db_err) = self
                .library_manager
                .mark_release_failed(&release_id_for_error)
                .await
            {
                error!("Failed to mark release as failed: {}", db_err);
            }
            let _ = self.progress_tx.send(ImportProgress::Failed {
                id: release_id_for_error,
                error: e,
                import_id: import_id_for_error,
            });
        }
    }

    /// Create a storage implementation from a profile
    async fn create_storage(
        &self,
        profile: DbStorageProfile,
    ) -> Result<ReleaseStorageImpl, String> {
        ReleaseStorageImpl::from_profile(
            profile,
            Some(self.encryption_service.clone()),
            self.database.clone(),
        )
        .await
        .map_err(|e| format!("Failed to create storage: {}", e))
    }

    /// Build a map from filename to track progress info for progress reporting.
    ///
    /// For CUE/FLAC: calculates byte ranges for each track within the shared FLAC file.
    /// For one-file-per-track: each track spans the entire file (0 to file_size).
    ///
    /// Returns: HashMap<filename, Vec<(track_id, start_byte, end_byte)>>
    async fn build_track_progress_map(
        &self,
        tracks_to_files: &[TrackFile],
        file_data: &[(String, Vec<u8>, PathBuf)],
        cue_flac_metadata: &Option<HashMap<PathBuf, CueFlacMetadata>>,
    ) -> Result<HashMap<String, Vec<(String, i64, i64)>>, String> {
        use crate::cue_flac::CueFlacProcessor;

        let mut result: HashMap<String, Vec<(String, i64, i64)>> = HashMap::new();
        let file_sizes: HashMap<&str, usize> = file_data
            .iter()
            .map(|(name, data, _)| (name.as_str(), data.len()))
            .collect();

        if let Some(ref cue_metadata) = cue_flac_metadata {
            for (flac_path, metadata) in cue_metadata {
                let filename = flac_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| format!("Invalid FLAC path: {:?}", flac_path))?
                    .to_string();

                let flac_info = CueFlacProcessor::analyze_flac(flac_path)
                    .map_err(|e| format!("Failed to analyze FLAC {:?}: {}", flac_path, e))?;

                let flac_tracks: Vec<_> = tracks_to_files
                    .iter()
                    .filter(|tf| &tf.file_path == flac_path)
                    .collect();

                let mut track_infos = Vec::new();
                for (i, cue_track) in metadata.cue_sheet.tracks.iter().enumerate() {
                    if let Some(track_file) = flac_tracks.get(i) {
                        let (start_byte, end_byte) = CueFlacProcessor::find_track_byte_range(
                            cue_track.start_time_ms,
                            cue_track.end_time_ms,
                            &flac_info.seektable,
                            flac_info.sample_rate,
                            flac_info.total_samples,
                            flac_info.audio_data_start,
                            flac_info.audio_data_end,
                        );
                        track_infos.push((track_file.db_track_id.clone(), start_byte, end_byte));
                    }
                }
                result.insert(filename, track_infos);
            }
        }

        for track_file in tracks_to_files {
            let filename = track_file
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("Invalid file path: {:?}", track_file.file_path))?
                .to_string();

            if result.contains_key(&filename) {
                continue;
            }

            let file_size = *file_sizes.get(filename.as_str()).unwrap_or(&0) as i64;
            result.entry(filename).or_default().push((
                track_file.db_track_id.clone(),
                0,
                file_size,
            ));
        }

        Ok(result)
    }

    /// Import files using the storage trait.
    ///
    /// Reads files and calls storage.write_file() for each.
    /// The storage layer handles encryption and cloud upload based on the profile.
    async fn run_storage_import(
        &self,
        db_release: &DbRelease,
        discovered_files: &[DiscoveredFile],
        tracks_to_files: &[TrackFile],
        cue_flac_metadata: Option<HashMap<PathBuf, CueFlacMetadata>>,
        storage_profile: DbStorageProfile,
        selected_cover_filename: Option<String>,
        import_id: &str,
    ) -> Result<(), String> {
        let library_manager = self.library_manager.get();
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: Some(import_id.to_string()),
        });

        let release_storage = crate::db::DbReleaseStorage::new(&db_release.id, &storage_profile.id);
        self.database
            .insert_release_storage(&release_storage)
            .await
            .map_err(|e| format!("Failed to link release to storage profile: {}", e))?;

        let storage = self.create_storage(storage_profile).await?;
        let total_files = discovered_files.len();

        info!(
            "Starting storage import for release {} ({} files)",
            db_release.id, total_files
        );

        let mut file_data: Vec<(String, Vec<u8>, PathBuf)> = Vec::with_capacity(total_files);
        for file in discovered_files.iter() {
            let filename = file
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("Invalid filename: {:?}", file.path))?
                .to_string();
            let data = tokio::fs::read(&file.path)
                .await
                .map_err(|e| format!("Failed to read file {:?}: {}", file.path, e))?;
            file_data.push((filename, data, file.path.clone()));
        }

        let file_to_tracks = self
            .build_track_progress_map(tracks_to_files, &file_data, &cue_flac_metadata)
            .await?;
        let release_total_bytes: usize = file_data.iter().map(|(_, data, _)| data.len()).sum();
        let mut release_bytes_written = 0usize;

        let import_id_owned = import_id.to_string();
        for (idx, (filename, data, _path)) in file_data.iter().enumerate() {
            let track_infos = file_to_tracks.get(filename).cloned().unwrap_or_default();
            let progress_tx = self.progress_tx.clone();
            let release_id = db_release.id.clone();
            let import_id_for_closure = import_id_owned.clone();
            let file_size = data.len();
            let base_bytes = release_bytes_written;

            storage
                .write_file(
                    &db_release.id,
                    filename,
                    data,
                    Box::new(move |file_bytes_written, _file_total| {
                        let bytes_written = file_bytes_written as i64;
                        for (track_id, start_byte, end_byte) in &track_infos {
                            if bytes_written > *start_byte {
                                let percent = calculate_track_percent(
                                    file_bytes_written,
                                    *start_byte,
                                    *end_byte,
                                );
                                let _ = progress_tx.send(ImportProgress::Progress {
                                    id: track_id.clone(),
                                    percent,
                                    phase: Some(ImportPhase::Chunk),
                                    import_id: Some(import_id_for_closure.clone()),
                                });
                            }
                        }

                        let total_written = base_bytes + file_bytes_written;
                        let release_percent =
                            (total_written * 100 / release_total_bytes.max(1)) as u8;
                        let _ = progress_tx.send(ImportProgress::Progress {
                            id: release_id.clone(),
                            percent: release_percent,
                            phase: Some(ImportPhase::Chunk),
                            import_id: Some(import_id_for_closure.clone()),
                        });
                    }),
                )
                .await
                .map_err(|e| format!("Failed to store file {}: {}", filename, e))?;

            release_bytes_written += file_size;
            info!(
                "Stored file {}/{}: {} ({} bytes)",
                idx + 1,
                total_files,
                filename,
                file_size
            );
        }

        // Persist track metadata for non-chunked storage
        self.persist_track_metadata(tracks_to_files, cue_flac_metadata)
            .await?;

        let cover_image_id = self
            .create_image_records(
                &db_release.id,
                &db_release.album_id,
                discovered_files,
                library_manager,
                selected_cover_filename,
            )
            .await?;

        for track_file in tracks_to_files {
            library_manager
                .mark_track_complete(&track_file.db_track_id)
                .await
                .map_err(|e| format!("Failed to mark track complete: {}", e))?;
            let _ = self.progress_tx.send(ImportProgress::Complete {
                id: track_file.db_track_id.clone(),
                release_id: Some(db_release.id.clone()),
                cover_image_id: None,
                import_id: Some(import_id.to_string()),
            });
        }

        library_manager
            .mark_release_complete(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release complete: {}", e))?;

        let _ = self
            .database
            .update_import_status(import_id, ImportOperationStatus::Complete)
            .await;

        let _ = self.progress_tx.send(ImportProgress::Complete {
            id: db_release.id.clone(),
            release_id: None,
            cover_image_id,
            import_id: Some(import_id.to_string()),
        });

        info!("Storage import complete for release {}", db_release.id);
        Ok(())
    }

    /// Create DbImage records for image files in the discovered files.
    async fn create_image_records(
        &self,
        release_id: &str,
        album_id: &str,
        discovered_files: &[DiscoveredFile],
        library_manager: &LibraryManager,
        selected_cover_filename: Option<String>,
    ) -> Result<Option<String>, String> {
        use crate::db::{DbImage, ImageSource};
        let image_extensions = ["jpg", "jpeg", "png", "gif", "webp"];
        let mut image_files: Vec<(&DiscoveredFile, String)> = discovered_files
            .iter()
            .filter_map(|f| {
                let ext = f.path.extension()?.to_str()?.to_lowercase();
                if image_extensions.contains(&ext.as_str()) {
                    let relative_path = self.get_relative_image_path(&f.path);
                    Some((f, relative_path))
                } else {
                    None
                }
            })
            .collect();

        if image_files.is_empty() {
            return Ok(None);
        }

        let cover_filename = if let Some(ref selected) = selected_cover_filename {
            if image_files.iter().any(|(_, path)| path == selected) {
                Some(selected.clone())
            } else {
                info!(
                    "Selected cover '{}' not found among images, using priority",
                    selected
                );
                None
            }
        } else {
            None
        };

        let cover_filename = cover_filename.unwrap_or_else(|| {
            image_files.sort_by(|(_, a), (_, b)| {
                let a_priority = Self::image_cover_priority(a);
                let b_priority = Self::image_cover_priority(b);
                a_priority.cmp(&b_priority)
            });
            image_files.first().map(|(_, path)| path.clone()).unwrap()
        });

        let mut cover_image_id: Option<String> = None;
        for (_file, relative_path) in &image_files {
            let source = if relative_path.starts_with(".bae/") {
                let filename_lower = relative_path.to_lowercase();
                if filename_lower.contains("-mb") || filename_lower.contains("musicbrainz") {
                    ImageSource::MusicBrainz
                } else if filename_lower.contains("-discogs") || filename_lower.contains("discogs")
                {
                    ImageSource::Discogs
                } else {
                    ImageSource::Local
                }
            } else {
                ImageSource::Local
            };

            let is_cover = relative_path == &cover_filename;
            let db_image = DbImage::new(release_id, relative_path, is_cover, source);
            let image_id = db_image.id.clone();

            library_manager
                .add_image(&db_image)
                .await
                .map_err(|e| format!("Failed to add image record: {}", e))?;

            if is_cover {
                library_manager
                    .set_album_cover_image(album_id, &image_id)
                    .await
                    .map_err(|e| format!("Failed to set album cover image: {}", e))?;
                cover_image_id = Some(image_id.clone());
            }

            info!(
                "Created DbImage: {} (cover={}, source={:?})",
                relative_path, is_cover, source
            );
        }

        Ok(cover_image_id)
    }

    fn image_cover_priority(filename: &str) -> u8 {
        let lower = filename.to_lowercase();
        if lower.starts_with(".bae/") {
            return 0;
        }
        if lower.contains("cover") || lower.contains("front") {
            return 1;
        }
        2
    }

    fn get_relative_image_path(&self, path: &std::path::Path) -> String {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some(parent) = path.parent() {
            if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
                if parent_name == ".bae"
                    || parent_name == "scans"
                    || parent_name == "artwork"
                    || parent_name == "images"
                {
                    return format!("{}/{}", parent_name, filename);
                }
            }
        }
        filename.to_string()
    }

    /// Persist track metadata (audio format info for playback).
    async fn persist_track_metadata(
        &self,
        tracks_to_files: &[TrackFile],
        cue_flac_metadata: Option<HashMap<PathBuf, CueFlacMetadata>>,
    ) -> Result<(), String> {
        use crate::cue_flac::{CueFlacProcessor, FlacInfo};
        use crate::db::DbAudioFormat;

        let library_manager = self.library_manager.get();

        // Build CUE/FLAC data if present (metadata, flac headers, flac info for byte offsets)
        #[allow(clippy::type_complexity)]
        let cue_flac_data: HashMap<PathBuf, (CueFlacMetadata, Vec<u8>, FlacInfo)> =
            if let Some(ref cue_metadata) = cue_flac_metadata {
                let mut data = HashMap::new();
                for (flac_path, metadata) in cue_metadata {
                    let flac_headers = CueFlacProcessor::extract_flac_headers(flac_path)
                        .map_err(|e| format!("Failed to extract FLAC headers: {}", e))?;
                    let flac_info = CueFlacProcessor::analyze_flac(flac_path)
                        .map_err(|e| format!("Failed to analyze FLAC: {}", e))?;
                    data.insert(
                        flac_path.clone(),
                        (metadata.clone(), flac_headers.headers, flac_info),
                    );
                }
                data
            } else {
                HashMap::new()
            };

        // Track which CUE track index we're on for each FLAC file
        let mut track_indices: HashMap<PathBuf, usize> = HashMap::new();

        for track_file in tracks_to_files {
            let format = track_file
                .file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("unknown")
                .to_lowercase();

            if let Some((metadata, flac_headers, flac_info)) =
                cue_flac_data.get(&track_file.file_path)
            {
                // Get current track index for this FLAC file
                let track_idx = *track_indices.get(&track_file.file_path).unwrap_or(&0);
                track_indices.insert(track_file.file_path.clone(), track_idx + 1);

                // Look up CUE track timing
                let cue_track = metadata.cue_sheet.tracks.get(track_idx).ok_or_else(|| {
                    format!(
                        "CUE track index {} out of bounds for {}",
                        track_idx,
                        track_file.file_path.display()
                    )
                })?;

                // Calculate byte offsets using seektable
                let (start_byte, end_byte) = CueFlacProcessor::find_track_byte_range(
                    cue_track.start_time_ms,
                    cue_track.end_time_ms,
                    &flac_info.seektable,
                    flac_info.sample_rate,
                    flac_info.total_samples,
                    flac_info.audio_data_start,
                    flac_info.audio_data_end,
                );

                // Serialize seektable for storage
                let seektable_tuples: Vec<(u64, u64)> = flac_info
                    .seektable
                    .iter()
                    .map(|sp| (sp.sample_number, sp.stream_offset))
                    .collect();
                let flac_seektable = bincode::serialize(&seektable_tuples)
                    .map_err(|e| format!("Failed to serialize seektable: {}", e))?;

                let audio_format = DbAudioFormat::new_with_byte_offsets(
                    &track_file.db_track_id,
                    "flac",
                    Some(flac_headers.clone()),
                    Some(flac_seektable),
                    true, // needs_headers for CUE/FLAC
                    start_byte,
                    end_byte,
                );
                library_manager
                    .add_audio_format(&audio_format)
                    .await
                    .map_err(|e| format!("Failed to insert audio format: {}", e))?;
            } else {
                let audio_format =
                    DbAudioFormat::new(&track_file.db_track_id, &format, None, false);
                library_manager
                    .add_audio_format(&audio_format)
                    .await
                    .map_err(|e| format!("Failed to insert audio format: {}", e))?;
            }
        }

        Ok(())
    }

    /// Import for None storage: just record file paths, no storage management.
    async fn run_none_import(
        &self,
        db_release: &DbRelease,
        discovered_files: &[DiscoveredFile],
        tracks_to_files: &[TrackFile],
        cue_flac_metadata: Option<HashMap<PathBuf, CueFlacMetadata>>,
        selected_cover_filename: Option<String>,
        import_id: &str,
    ) -> Result<(), String> {
        let library_manager = self.library_manager.get();
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: Some(import_id.to_string()),
        });

        let total_files = discovered_files.len();
        info!(
            "Starting None storage import for release {} ({} files)",
            db_release.id, total_files
        );

        let file_to_tracks: HashMap<String, Vec<String>> = {
            let mut map: HashMap<String, Vec<String>> = HashMap::new();
            for tf in tracks_to_files {
                if let Some(filename) = tf.file_path.file_name().and_then(|n| n.to_str()) {
                    map.entry(filename.to_string())
                        .or_default()
                        .push(tf.db_track_id.clone());
                }
            }
            map
        };

        for (idx, file) in discovered_files.iter().enumerate() {
            let filename = file
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("Invalid filename: {:?}", file.path))?;
            let format = file
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("bin")
                .to_lowercase();
            let source_path = file
                .path
                .to_str()
                .ok_or_else(|| format!("Cannot convert path to string: {:?}", file.path))?;

            let db_file = DbFile::new(&db_release.id, filename, file.size as i64, &format)
                .with_source_path(source_path);
            library_manager
                .add_file(&db_file)
                .await
                .map_err(|e| format!("Failed to add file record: {}", e))?;

            if let Some(track_ids) = file_to_tracks.get(filename) {
                for track_id in track_ids {
                    let _ = self.progress_tx.send(ImportProgress::Progress {
                        id: track_id.clone(),
                        percent: 100,
                        phase: Some(ImportPhase::Chunk),
                        import_id: Some(import_id.to_string()),
                    });
                }
            }

            let release_percent = ((idx + 1) * 100 / total_files.max(1)) as u8;
            let _ = self.progress_tx.send(ImportProgress::Progress {
                id: db_release.id.clone(),
                percent: release_percent,
                phase: Some(ImportPhase::Chunk),
                import_id: Some(import_id.to_string()),
            });

            info!(
                "Recorded file {}/{}: {} -> {}",
                idx + 1,
                total_files,
                filename,
                source_path
            );
        }

        if cue_flac_metadata.is_some() {
            self.persist_track_metadata(tracks_to_files, cue_flac_metadata)
                .await?;
        }

        for track_file in tracks_to_files {
            library_manager
                .mark_track_complete(&track_file.db_track_id)
                .await
                .map_err(|e| format!("Failed to mark track complete: {}", e))?;
            let _ = self.progress_tx.send(ImportProgress::Complete {
                id: track_file.db_track_id.clone(),
                release_id: Some(db_release.id.clone()),
                cover_image_id: None,
                import_id: Some(import_id.to_string()),
            });
        }

        let cover_image_id = self
            .create_image_records(
                &db_release.id,
                &db_release.album_id,
                discovered_files,
                library_manager,
                selected_cover_filename,
            )
            .await?;

        library_manager
            .mark_release_complete(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release complete: {}", e))?;

        let _ = self
            .database
            .update_import_status(import_id, ImportOperationStatus::Complete)
            .await;

        let _ = self.progress_tx.send(ImportProgress::Complete {
            id: db_release.id.clone(),
            release_id: None,
            cover_image_id,
            import_id: Some(import_id.to_string()),
        });

        info!("None storage import complete for release {}", db_release.id);
        Ok(())
    }

    /// Torrent import for None storage
    #[allow(clippy::too_many_arguments)]
    async fn import_torrent_none_storage(
        &self,
        db_album: DbAlbum,
        db_release: DbRelease,
        tracks_to_files: Vec<TrackFile>,
        torrent_source: TorrentSource,
        torrent_metadata: TorrentImportMetadata,
        cover_art_url: Option<String>,
        selected_cover_filename: Option<String>,
    ) -> Result<(), String> {
        let library_manager = self.library_manager.get();
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        info!(
            "Starting torrent import with None storage for '{}'",
            db_album.title
        );

        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: None,
        });

        info!("Starting torrent download (acquire phase)");
        let torrent_handle = self
            .torrent_handle
            .add_torrent(torrent_source.clone())
            .await
            .map_err(|e| format!("Failed to add torrent: {}", e))?;

        torrent_handle
            .wait_for_metadata()
            .await
            .map_err(|e| format!("Failed to wait for metadata: {}", e))?;

        loop {
            let progress = torrent_handle
                .progress()
                .await
                .map_err(|e| format!("Failed to check torrent progress: {}", e))?;
            let percent = (progress * 100.0) as u8;
            let _ = self.progress_tx.send(ImportProgress::Progress {
                id: db_release.id.clone(),
                percent,
                phase: Some(ImportPhase::Acquire),
                import_id: None,
            });
            if progress >= 1.0 {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        info!("Torrent download complete");

        let torrent_files = torrent_handle
            .get_file_list()
            .await
            .map_err(|e| format!("Failed to get torrent file list: {}", e))?;

        let temp_dir = std::env::temp_dir();
        let torrent_save_dir = temp_dir.join(&torrent_metadata.torrent_name);

        let mut discovered_files: Vec<DiscoveredFile> = torrent_files
            .iter()
            .map(|tf| DiscoveredFile {
                path: temp_dir.join(&tf.path),
                size: tf.size as u64,
            })
            .collect();

        if let Some(ref url) = cover_art_url {
            use crate::db::ImageSource;
            use crate::import::cover_art::download_cover_art_to_bae_folder;
            let source = if url.contains("coverartarchive.org") || url.contains("musicbrainz") {
                ImageSource::MusicBrainz
            } else {
                ImageSource::Discogs
            };
            match download_cover_art_to_bae_folder(url, &torrent_save_dir, source).await {
                Ok(downloaded) => {
                    info!("Downloaded cover art to {:?}", downloaded.path);
                    if let Ok(metadata) = tokio::fs::metadata(&downloaded.path).await {
                        discovered_files.push(DiscoveredFile {
                            path: downloaded.path,
                            size: metadata.len(),
                        });
                    }
                }
                Err(e) => {
                    warn!("Failed to download cover art: {}", e);
                }
            }
        }

        let _ = self
            .torrent_handle
            .remove_torrent(torrent_handle, false)
            .await;

        let total_files = discovered_files.len();
        info!(
            "Recording {} files in temp folder for None storage",
            total_files
        );

        for (idx, file) in discovered_files.iter().enumerate() {
            let filename = file
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("Invalid filename: {:?}", file.path))?;
            let format = file
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("bin")
                .to_lowercase();
            let source_path = file
                .path
                .to_str()
                .ok_or_else(|| format!("Cannot convert path to string: {:?}", file.path))?;

            let db_file = DbFile::new(&db_release.id, filename, file.size as i64, &format)
                .with_source_path(source_path);
            library_manager
                .add_file(&db_file)
                .await
                .map_err(|e| format!("Failed to add file record: {}", e))?;

            info!(
                "Recorded temp file {}/{}: {} -> {}",
                idx + 1,
                total_files,
                filename,
                source_path
            );

            let release_percent = ((idx + 1) * 100 / total_files.max(1)) as u8;
            let _ = self.progress_tx.send(ImportProgress::Progress {
                id: db_release.id.clone(),
                percent: release_percent,
                phase: Some(ImportPhase::Chunk),
                import_id: None,
            });
        }

        for track_file in &tracks_to_files {
            library_manager
                .mark_track_complete(&track_file.db_track_id)
                .await
                .map_err(|e| format!("Failed to mark track complete: {}", e))?;
            let _ = self.progress_tx.send(ImportProgress::Complete {
                id: track_file.db_track_id.clone(),
                release_id: Some(db_release.id.clone()),
                cover_image_id: None,
                import_id: None,
            });
        }

        let cover_image_id = self
            .create_image_records(
                &db_release.id,
                &db_release.album_id,
                &discovered_files,
                library_manager,
                selected_cover_filename,
            )
            .await?;

        library_manager
            .mark_release_complete(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release complete: {}", e))?;

        let _ = self.progress_tx.send(ImportProgress::Complete {
            id: db_release.id.clone(),
            release_id: None,
            cover_image_id,
            import_id: None,
        });

        info!(
            "Torrent None storage import complete for '{}' (files in temp: {:?})",
            db_album.title, torrent_save_dir
        );
        Ok(())
    }

    /// Torrent import using storage profile.
    #[allow(clippy::too_many_arguments)]
    async fn run_torrent_import(
        &self,
        db_album: DbAlbum,
        db_release: DbRelease,
        tracks_to_files: Vec<TrackFile>,
        torrent_source: TorrentSource,
        torrent_metadata: TorrentImportMetadata,
        seed_after_download: bool,
        cover_art_url: Option<String>,
        storage_profile: DbStorageProfile,
        selected_cover_filename: Option<String>,
    ) -> Result<(), String> {
        let library_manager = self.library_manager.get();
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        info!("Starting torrent import for '{}'", db_album.title);

        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: None,
        });

        // Download torrent
        let torrent_handle = self
            .torrent_handle
            .add_torrent(torrent_source.clone())
            .await
            .map_err(|e| format!("Failed to add torrent: {}", e))?;

        torrent_handle
            .wait_for_metadata()
            .await
            .map_err(|e| format!("Failed to wait for metadata: {}", e))?;

        loop {
            let progress = torrent_handle
                .progress()
                .await
                .map_err(|e| format!("Failed to check torrent progress: {}", e))?;
            let percent = (progress * 100.0) as u8;
            let _ = self.progress_tx.send(ImportProgress::Progress {
                id: db_release.id.clone(),
                percent,
                phase: Some(ImportPhase::Acquire),
                import_id: None,
            });
            if progress >= 1.0 {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        info!("Torrent download complete");

        let torrent_files = torrent_handle
            .get_file_list()
            .await
            .map_err(|e| format!("Failed to get torrent file list: {}", e))?;

        let temp_dir = std::env::temp_dir();
        let torrent_save_dir = temp_dir.join(&torrent_metadata.torrent_name);

        let mut discovered_files: Vec<DiscoveredFile> = torrent_files
            .iter()
            .map(|tf| DiscoveredFile {
                path: temp_dir.join(&tf.path),
                size: tf.size as u64,
            })
            .collect();

        if let Some(ref url) = cover_art_url {
            use crate::db::ImageSource;
            use crate::import::cover_art::download_cover_art_to_bae_folder;
            let source = if url.contains("coverartarchive.org") || url.contains("musicbrainz") {
                ImageSource::MusicBrainz
            } else {
                ImageSource::Discogs
            };
            match download_cover_art_to_bae_folder(url, &torrent_save_dir, source).await {
                Ok(downloaded) => {
                    info!("Downloaded cover art to {:?}", downloaded.path);
                    if let Ok(metadata) = tokio::fs::metadata(&downloaded.path).await {
                        discovered_files.push(DiscoveredFile {
                            path: downloaded.path,
                            size: metadata.len(),
                        });
                    }
                }
                Err(e) => {
                    warn!("Failed to download cover art: {}", e);
                }
            }
        }

        // Detect CUE/FLAC
        let file_paths: Vec<PathBuf> = discovered_files.iter().map(|f| f.path.clone()).collect();
        let cue_flac_pairs =
            crate::cue_flac::CueFlacProcessor::detect_cue_flac_from_paths(&file_paths)
                .map_err(|e| format!("Failed to detect CUE/FLAC files: {}", e))?;

        let mut cue_flac_metadata = HashMap::new();
        for pair in cue_flac_pairs {
            let flac_path = pair.flac_path.clone();
            let cue_sheet = crate::cue_flac::CueFlacProcessor::parse_cue_sheet(&pair.cue_path)
                .map_err(|e| format!("Failed to parse CUE sheet: {}", e))?;
            let metadata = CueFlacMetadata {
                cue_sheet,
                cue_path: pair.cue_path,
                flac_path: flac_path.clone(),
            };
            cue_flac_metadata.insert(flac_path, metadata);
        }

        let cue_flac_opt = if cue_flac_metadata.is_empty() {
            None
        } else {
            Some(cue_flac_metadata)
        };

        // Use the import ID as a placeholder since torrent imports don't have one
        let import_id = format!("torrent-{}", db_release.id);

        // Import using storage
        self.run_storage_import(
            &db_release,
            &discovered_files,
            &tracks_to_files,
            cue_flac_opt,
            storage_profile,
            selected_cover_filename,
            &import_id,
        )
        .await?;

        if seed_after_download {
            let _ = self
                .torrent_handle
                .start_seeding(db_release.id.clone())
                .await;
        }

        let _ = self
            .torrent_handle
            .remove_torrent(torrent_handle, true)
            .await;

        // Clean up temp files
        if torrent_save_dir.exists() {
            match tokio::fs::remove_dir_all(&torrent_save_dir).await {
                Ok(_) => {
                    info!("Cleaned up temporary torrent files: {:?}", torrent_save_dir);
                }
                Err(e) => {
                    warn!(
                        "Failed to clean up temporary torrent files {:?}: {}",
                        torrent_save_dir, e
                    );
                }
            }
        }

        info!(
            "Torrent import completed successfully for {}",
            db_album.title
        );
        Ok(())
    }

    /// CD import using storage profile.
    async fn run_cd_import(
        &self,
        db_album: DbAlbum,
        db_release: DbRelease,
        db_tracks: Vec<DbTrack>,
        drive_path: PathBuf,
        toc: CdToc,
        storage_profile: DbStorageProfile,
        selected_cover_filename: Option<String>,
    ) -> Result<(), String> {
        use crate::cd::{CdDrive, CdRipper, CueGenerator, LogGenerator};
        use crate::import::track_to_file_mapper::map_tracks_to_files;

        let library_manager = self.library_manager.get();
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        info!("Starting CD import for '{}'", db_album.title);

        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: None,
        });

        // Rip CD
        let temp_dir = std::env::temp_dir().join(format!("bae_cd_rip_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| format!("Failed to create temp directory: {}", e))?;

        let drive = CdDrive {
            device_path: drive_path.clone(),
            name: drive_path.to_str().unwrap_or("Unknown").to_string(),
        };

        let ripper = CdRipper::new(drive.clone(), toc.clone(), temp_dir.clone());
        let (rip_progress_tx, mut rip_progress_rx) = mpsc::unbounded_channel::<RipProgress>();

        let release_id_for_progress = db_release.id.clone();
        let progress_tx_for_ripping = self.progress_tx.clone();

        tokio::spawn(async move {
            while let Some(rip_progress) = rip_progress_rx.recv().await {
                let _ = progress_tx_for_ripping.send(ImportProgress::Progress {
                    id: release_id_for_progress.clone(),
                    percent: rip_progress.percent,
                    phase: Some(ImportPhase::Acquire),
                    import_id: None,
                });
            }
        });

        let rip_results = ripper
            .rip_all_tracks(Some(rip_progress_tx))
            .await
            .map_err(|e| format!("Failed to rip CD: {}", e))?;

        info!("CD ripping completed, {} tracks ripped", rip_results.len());

        // Generate CUE and log files
        let artist_name = "Unknown Artist".to_string();
        let flac_filename = format!("{}.flac", db_album.title.replace("/", "_"));

        let cue_sheet = CueGenerator::generate_cue_sheet(
            &toc,
            &rip_results,
            &flac_filename,
            &artist_name,
            &db_album.title,
        );

        let cue_path = temp_dir.join(format!("{}.cue", db_album.title.replace("/", "_")));
        CueGenerator::write_cue_file(&cue_sheet, &toc.disc_id, &flac_filename, &cue_path)
            .map_err(|e| format!("Failed to write CUE file: {}", e))?;

        let log_path = temp_dir.join(format!("{}.log", db_album.title.replace("/", "_")));
        LogGenerator::write_log_file(&toc, &rip_results, &drive.name, &log_path)
            .map_err(|e| format!("Failed to write log file: {}", e))?;

        // Build discovered files list
        let mut discovered_files = Vec::new();
        for result in &rip_results {
            let metadata = tokio::fs::metadata(&result.output_path)
                .await
                .map_err(|e| format!("Failed to get file size: {}", e))?;
            discovered_files.push(DiscoveredFile {
                path: result.output_path.clone(),
                size: metadata.len(),
            });
        }

        let cue_metadata = tokio::fs::metadata(&cue_path)
            .await
            .map_err(|e| format!("Failed to get CUE file size: {}", e))?;
        discovered_files.push(DiscoveredFile {
            path: cue_path.clone(),
            size: cue_metadata.len(),
        });

        let log_metadata = tokio::fs::metadata(&log_path)
            .await
            .map_err(|e| format!("Failed to get log file size: {}", e))?;
        discovered_files.push(DiscoveredFile {
            path: log_path.clone(),
            size: log_metadata.len(),
        });

        // Map tracks to files
        let mapping_result = map_tracks_to_files(&db_tracks, &discovered_files)
            .await
            .map_err(|e| format!("Failed to map tracks to files: {}", e))?;

        let tracks_to_files = mapping_result.track_files.clone();
        let cue_flac_metadata = mapping_result.cue_flac_metadata.clone();

        crate::import::handle::extract_and_store_durations(library_manager, &tracks_to_files)
            .await
            .map_err(|e| format!("Failed to extract durations: {}", e))?;

        // Use the import ID as a placeholder
        let import_id = format!("cd-{}", db_release.id);

        // Import using storage
        self.run_storage_import(
            &db_release,
            &discovered_files,
            &tracks_to_files,
            cue_flac_metadata,
            storage_profile,
            selected_cover_filename,
            &import_id,
        )
        .await?;

        // Clean up temp files
        if let Err(e) = tokio::fs::remove_dir_all(&temp_dir).await {
            warn!("Failed to remove temp directory {:?}: {}", temp_dir, e);
        } else {
            info!("Cleaned up temp directory: {:?}", temp_dir);
        }

        info!("CD import completed successfully for {}", db_album.title);
        Ok(())
    }

    /// CD import with no bae storage
    async fn run_cd_import_none_storage(
        &self,
        db_album: DbAlbum,
        db_release: DbRelease,
        db_tracks: Vec<DbTrack>,
        drive_path: PathBuf,
        toc: CdToc,
        _selected_cover_filename: Option<String>,
    ) -> Result<(), String> {
        use crate::cd::{CdDrive, CdRipper};

        let library_manager = self.library_manager.get();
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        info!(
            "Starting CD import with no storage for '{}' ({} tracks)",
            db_album.title,
            db_tracks.len()
        );

        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: None,
        });

        let temp_dir = std::env::temp_dir().join(format!("bae_cd_rip_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| format!("Failed to create temp directory: {}", e))?;

        let drive = CdDrive {
            device_path: drive_path.clone(),
            name: drive_path.to_str().unwrap_or("Unknown").to_string(),
        };

        let ripper = CdRipper::new(drive.clone(), toc.clone(), temp_dir.clone());
        let rip_results = ripper
            .rip_all_tracks(None)
            .await
            .map_err(|e| format!("Failed to rip CD: {}", e))?;

        info!("CD ripping completed, {} tracks ripped", rip_results.len());

        for (idx, result) in rip_results.iter().enumerate() {
            let filename = result
                .output_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown.flac");
            let file_size = tokio::fs::metadata(&result.output_path)
                .await
                .map(|m| m.len() as i64)
                .unwrap_or(0);
            let source_path = result.output_path.to_str().ok_or_else(|| {
                format!("Cannot convert path to string: {:?}", result.output_path)
            })?;

            let db_file = DbFile::new(&db_release.id, filename, file_size, "flac")
                .with_source_path(source_path);
            library_manager
                .add_file(&db_file)
                .await
                .map_err(|e| format!("Failed to add file record: {}", e))?;

            info!(
                "Recorded ripped file {}/{}: {} -> {}",
                idx + 1,
                rip_results.len(),
                filename,
                source_path
            );
        }

        for track in &db_tracks {
            library_manager
                .mark_track_complete(&track.id)
                .await
                .map_err(|e| format!("Failed to mark track complete: {}", e))?;
            let _ = self.progress_tx.send(ImportProgress::Complete {
                id: track.id.clone(),
                release_id: Some(db_release.id.clone()),
                cover_image_id: None,
                import_id: None,
            });
        }

        library_manager
            .mark_release_complete(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release complete: {}", e))?;

        let _ = self.progress_tx.send(ImportProgress::Complete {
            id: db_release.id.clone(),
            release_id: None,
            cover_image_id: None,
            import_id: None,
        });

        info!(
            "CD none-storage import complete for '{}'. Files at: {:?}",
            db_album.title, temp_dir
        );
        Ok(())
    }
}
