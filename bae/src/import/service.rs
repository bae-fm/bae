// # Import Service
//
// Single-instance queue-based service that imports albums.
// Runs on dedicated thread with own tokio runtime, handles import requests sequentially.
//
// Two-phase import model:
// 1. Acquire Phase: Get data ready (folder: no-op, torrent: download, CD: rip)
// 2. Chunk Phase: Upload and encrypt (same for all types)
//
// Flow:
// 1. Validation & Queueing (in ImportHandle, synchronous):
//    - Validate track-to-file mapping
//    - Insert album/tracks with status='queued'
//    - Send ImportCommand to service
//
// 2. Acquire Phase (async, in ImportService):
//    - Folder: Instant (no work)
//    - Torrent: Download torrent, emit progress with ImportPhase::Acquire
//    - CD: Rip tracks, emit progress with ImportPhase::Acquire
//
// 3. Chunk Phase (async, in ImportService::run_chunk_phase):
//    - Mark album as 'importing'
//    - Streaming pipeline: read → encrypt → upload → persist (bounded parallelism)
//    - Emit progress with ImportPhase::Chunk
//    - Mark album/tracks as 'complete'
//
// Architecture:
// - ImportHandle: Validates requests, inserts DB records, sends commands
// - ImportService: Executes acquire + chunk phases on dedicated thread
// - ImportProgressTracker: Tracks chunk completion, emits progress events
// - MetadataPersister: Saves file/chunk metadata to DB

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::cd::drive::CdToc;
use crate::cd::RipProgress;
use crate::cloud_storage::CloudStorageManager;
use crate::db::{
    Database, DbAlbum, DbFile, DbRelease, DbStorageProfile, DbTrack, ImportOperationStatus,
};
use crate::encryption::EncryptionService;
use crate::import::album_chunk_layout::AlbumChunkLayout;
use crate::import::album_chunk_layout::{build_seektable, find_track_byte_range};
use crate::import::handle::{ImportServiceHandle, TorrentImportMetadata};
use crate::import::metadata_persister::MetadataPersister;
use crate::import::pipeline;
use crate::import::progress::ImportProgressTracker;
use crate::import::types::{
    CueFlacLayoutData, CueFlacMetadata, DiscoveredFile, FileToChunks, ImportCommand, ImportPhase,
    ImportProgress, TorrentSource, TrackFile,
};
use crate::library::{LibraryManager, SharedLibraryManager};
use crate::storage::{ReleaseStorage, ReleaseStorageImpl};
use crate::torrent::TorrentManagerHandle;
use futures::stream::StreamExt;
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

/// Configuration for import service
#[derive(Clone)]
pub struct ImportConfig {
    /// Size of each chunk in bytes
    pub chunk_size_bytes: usize,
    /// Number of parallel encryption workers (CPU-bound, typically 2x CPU cores)
    pub max_encrypt_workers: usize,
    /// Number of parallel upload workers (I/O-bound)
    pub max_upload_workers: usize,
    /// Number of parallel DB write workers (I/O-bound)
    pub max_db_write_workers: usize,
}

/// Import service that orchestrates the album import workflow
pub struct ImportService {
    /// Configuration for the import service
    config: ImportConfig,
    /// Channel for receiving import commands from clients
    commands_rx: mpsc::UnboundedReceiver<ImportCommand>,
    /// Channel for sending progress updates to subscribers
    progress_tx: mpsc::UnboundedSender<ImportProgress>,
    /// Service for encrypting files before upload
    encryption_service: EncryptionService,
    /// Service for uploading encrypted chunks to cloud storage
    cloud_storage: CloudStorageManager,
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
        config: ImportConfig,
        runtime_handle: tokio::runtime::Handle,
        library_manager: SharedLibraryManager,
        encryption_service: EncryptionService,
        cloud_storage: CloudStorageManager,
        torrent_handle: TorrentManagerHandle,
        database: Arc<Database>,
    ) -> ImportServiceHandle {
        let (commands_tx, commands_rx) = mpsc::unbounded_channel();
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();

        // Clone progress_tx for the handle (before it's moved to worker)
        let progress_tx_for_handle = progress_tx.clone();

        // Clone library_manager and database for the thread
        let library_manager_for_worker = library_manager.clone();
        let database_for_handle = database.clone();

        // Spawn the service task on a dedicated thread
        std::thread::spawn(move || {
            // Create a new tokio runtime for this thread
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");

            rt.block_on(async move {
                let mut service = ImportService {
                    config,
                    commands_rx,
                    progress_tx,
                    library_manager: library_manager_for_worker,
                    encryption_service,
                    cloud_storage,
                    torrent_handle,
                    database,
                };

                info!("Worker started");

                // Import validated albums sequentially from the queue.
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
        // Extract release_id and import_id before consuming command
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
                        // No storage profile = files stay in place
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
                        // No storage profile = files stay in temp folder
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
                        // No storage profile = ripped files stay in temp folder
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

            // Mark release as failed in database
            if let Err(db_err) = self
                .library_manager
                .mark_release_failed(&release_id_for_error)
                .await
            {
                error!("Failed to mark release as failed: {}", db_err);
            }

            // Emit Failed progress event
            let _ = self.progress_tx.send(ImportProgress::Failed {
                id: release_id_for_error,
                error: e,
                import_id: import_id_for_error,
            });
        }
    }

    /// Executes the streaming import pipeline for a torrent-based import.
    ///
    /// Orchestrates the entire import workflow:
    /// 1. Marks the album as 'importing'
    /// 2. Streams torrent pieces → chunks → encrypts → uploads (no upfront layout computation)
    /// 3. After torrent completes: extracts FLAC headers, builds seektable, computes layout
    /// 4. Persists metadata and marks album complete.
    async fn import_album_from_torrent(
        &self,
        db_album: DbAlbum,
        db_release: DbRelease,
        tracks_to_files: Vec<TrackFile>,
        torrent_source: TorrentSource,
        torrent_metadata: TorrentImportMetadata,
        seed_after_download: bool,
        cover_art_url: Option<String>,
    ) -> Result<(), String> {
        let library_manager = self.library_manager.get();

        // Mark release as importing now that pipeline is starting
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        info!("Marked release as 'importing' - starting torrent pipeline");

        // Send started progress
        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: None,
        });

        // ========== ACQUIRE PHASE: TORRENT DOWNLOAD ==========

        info!("Starting torrent download (acquire phase)");

        // Add torrent via torrent manager
        let torrent_handle = self
            .torrent_handle
            .add_torrent(torrent_source.clone())
            .await
            .map_err(|e| format!("Failed to add torrent: {}", e))?;

        // Wait for metadata if needed
        torrent_handle
            .wait_for_metadata()
            .await
            .map_err(|e| format!("Failed to wait for metadata: {}", e))?;

        // Download torrent and emit progress (Acquire phase)
        loop {
            let progress = torrent_handle
                .progress()
                .await
                .map_err(|e| format!("Failed to check torrent progress: {}", e))?;

            let percent = (progress * 100.0) as u8;
            let _ = self.progress_tx.send(ImportProgress::Progress {
                id: db_release.id.clone(),
                percent,
                phase: Some(crate::import::types::ImportPhase::Acquire),
                import_id: None,
            });

            if progress >= 1.0 {
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Wait a bit for libtorrent to finish writing files to disk
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        info!("Torrent download (acquire phase) complete");

        // Get file list from torrent to construct discovered_files
        let torrent_files = torrent_handle
            .get_file_list()
            .await
            .map_err(|e| format!("Failed to get torrent file list: {}", e))?;

        // Convert torrent files to DiscoveredFile format
        let temp_dir = std::env::temp_dir();
        let torrent_save_dir = temp_dir.join(&torrent_metadata.torrent_name);
        let mut discovered_files: Vec<DiscoveredFile> = torrent_files
            .iter()
            .map(|tf| DiscoveredFile {
                path: temp_dir.join(&tf.path),
                size: tf.size as u64,
            })
            .collect();

        // Download cover art to .bae/ folder in the torrent's temp directory
        if let Some(ref url) = cover_art_url {
            use crate::db::ImageSource;
            use crate::import::cover_art::download_cover_art_to_bae_folder;

            // Determine source based on URL pattern
            let source = if url.contains("coverartarchive.org") || url.contains("musicbrainz") {
                ImageSource::MusicBrainz
            } else {
                ImageSource::Discogs
            };

            match download_cover_art_to_bae_folder(url, &torrent_save_dir, source).await {
                Ok(downloaded) => {
                    info!("Downloaded cover art to {:?}", downloaded.path);
                    // Add the downloaded cover art to discovered_files so it gets chunked
                    if let Ok(metadata) = tokio::fs::metadata(&downloaded.path).await {
                        discovered_files.push(DiscoveredFile {
                            path: downloaded.path,
                            size: metadata.len(),
                        });
                    }
                }
                Err(e) => {
                    // Non-fatal - continue import without cover art
                    warn!("Failed to download cover art: {}", e);
                }
            }
        }

        info!("Starting chunk phase");

        // Detect and parse CUE/FLAC files
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

        // ========== CHUNK PHASE ==========
        // Now that data is acquired, run chunk phase (same as folder import)

        self.run_chunk_phase(
            &db_release,
            &tracks_to_files,
            &discovered_files,
            Some(cue_flac_metadata),
        )
        .await?;

        // ========== HANDOFF TO SEEDER ==========
        // Hand off to seeder if requested (fire-and-forget)
        if seed_after_download {
            let _ = self
                .torrent_handle
                .start_seeding(db_release.id.clone())
                .await;
        }

        // Remove torrent from download client after import completes
        let _ = self
            .torrent_handle
            .remove_torrent(torrent_handle, true)
            .await;

        // ========== CLEANUP TEMPORARY FILES ==========

        // Clean up temporary downloaded files (torrent_save_dir was defined earlier)
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
                    // Don't fail the import if cleanup fails
                }
            }
        }

        // Send release completion event
        // Note: This old code path doesn't create image records, so cover_image_id is None
        let _ = self.progress_tx.send(ImportProgress::Complete {
            id: db_release.id,
            release_id: None,
            cover_image_id: None,
            import_id: None,
        });

        info!(
            "Torrent import completed successfully for {}",
            db_album.title
        );
        Ok(())
    }

    /// Executes the streaming import pipeline for a CD-based import.
    ///
    /// Orchestrates the entire import workflow:
    /// 1. Marks the album as 'importing'
    /// 2. **Acquire phase**: Rip CD tracks to FLAC files
    /// 3. **Chunk phase**: Stream ripped files → encrypts → uploads
    /// 4. After upload: persists metadata, marks complete
    /// 5. Cleans up temporary directory
    async fn import_album_from_cd(
        &self,
        db_album: DbAlbum,
        db_release: DbRelease,
        db_tracks: Vec<DbTrack>,
        drive_path: PathBuf,
        toc: CdToc,
    ) -> Result<(), String> {
        let library_manager = self.library_manager.get();

        // Mark release as importing now that pipeline is starting
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        info!("Marked release as 'importing' - starting CD import pipeline");

        // Send started progress
        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: None,
        });

        // ========== ACQUIRE PHASE: CD RIPPING ==========

        info!(
            "Starting CD ripping (acquire phase) for {} tracks",
            toc.last_track - toc.first_track + 1
        );

        // Create temporary directory for ripped files
        let temp_dir = std::env::temp_dir().join(format!("bae_cd_rip_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| format!("Failed to create temp directory: {}", e))?;

        // Create CD drive and ripper
        use crate::cd::{CdDrive, CdRipper, CueGenerator, LogGenerator};
        let drive = CdDrive {
            device_path: drive_path.clone(),
            name: drive_path.to_str().unwrap_or("Unknown").to_string(),
        };
        let ripper = CdRipper::new(drive.clone(), toc.clone(), temp_dir.clone());

        // Create progress channel for ripping
        let (rip_progress_tx, mut rip_progress_rx) = mpsc::unbounded_channel::<RipProgress>();

        // Map track numbers (1-indexed) to track IDs
        let track_number_to_id: HashMap<u8, String> = db_tracks
            .iter()
            .enumerate()
            .map(|(idx, track)| {
                // Track numbers are 1-indexed, enumerate is 0-indexed
                let track_num = toc.first_track + idx as u8;
                (track_num, track.id.clone())
            })
            .collect();

        // Spawn task to forward ripping progress to UI (Acquire phase)
        let release_id_for_progress = db_release.id.clone();
        let progress_tx_for_ripping = self.progress_tx.clone();
        let track_number_to_id_for_progress = track_number_to_id.clone();
        tokio::spawn(async move {
            while let Some(rip_progress) = rip_progress_rx.recv().await {
                use crate::import::types::ImportPhase;

                // Send release-level progress (Acquire phase)
                let _ = progress_tx_for_ripping.send(ImportProgress::Progress {
                    id: release_id_for_progress.clone(),
                    percent: rip_progress.percent,
                    phase: Some(ImportPhase::Acquire),
                    import_id: None,
                });

                // Send track-level progress (Acquire phase) for the current track
                if let Some(track_id) = track_number_to_id_for_progress.get(&rip_progress.track) {
                    let _ = progress_tx_for_ripping.send(ImportProgress::Progress {
                        id: track_id.clone(),
                        percent: rip_progress.track_percent,
                        phase: Some(ImportPhase::Acquire),
                        import_id: None,
                    });
                }
            }
        });

        // Rip all tracks
        let rip_results = ripper
            .rip_all_tracks(Some(rip_progress_tx))
            .await
            .map_err(|e| format!("Failed to rip CD: {}", e))?;

        info!("CD ripping completed, {} tracks ripped", rip_results.len());

        // Generate CUE sheet and log file
        // Note: Artist name is just for CUE metadata, use placeholder if not available
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

        // Discover files after ripping
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

        // Add CUE and log files
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
        use crate::import::track_to_file_mapper::map_tracks_to_files;
        let mapping_result = map_tracks_to_files(&db_tracks, &discovered_files)
            .await
            .map_err(|e| format!("Failed to map tracks to files: {}", e))?;
        let tracks_to_files = mapping_result.track_files.clone();
        let cue_flac_metadata = mapping_result.cue_flac_metadata.clone();

        // Extract and store durations
        crate::import::handle::extract_and_store_durations(library_manager, &tracks_to_files)
            .await
            .map_err(|e| format!("Failed to extract durations: {}", e))?;

        info!("CD ripping (acquire phase) complete, starting chunk phase");

        // ========== CHUNK PHASE ==========
        // Now that data is acquired, run chunk phase (same as folder import)

        self.run_chunk_phase(
            &db_release,
            &tracks_to_files,
            &discovered_files,
            cue_flac_metadata,
        )
        .await?;

        // ========== CLEANUP TEMP DIRECTORY ==========
        // Remove temporary directory with ripped files
        if let Err(e) = tokio::fs::remove_dir_all(&temp_dir).await {
            warn!("Failed to remove temp directory {:?}: {}", temp_dir, e);
            // Don't fail the import if cleanup fails
        } else {
            info!("Cleaned up temp directory: {:?}", temp_dir);
        }

        // Send release completion event
        // Note: This old code path doesn't create image records, so cover_image_id is None
        let _ = self.progress_tx.send(ImportProgress::Complete {
            id: db_release.id,
            release_id: None,
            cover_image_id: None,
            import_id: None,
        });

        info!("CD import completed successfully for {}", db_album.title);
        Ok(())
    }

    /// Run the chunk phase: compute layout, stream chunks, upload, and persist metadata.
    ///
    /// This is the common chunk upload phase used by all import types after data acquisition.
    /// For folder imports, this runs immediately (no acquire phase).
    /// For CD/torrent imports, this runs after the acquire phase completes.
    async fn run_chunk_phase(
        &self,
        db_release: &DbRelease,
        tracks_to_files: &[TrackFile],
        discovered_files: &[DiscoveredFile],
        cue_flac_metadata: Option<HashMap<PathBuf, CueFlacMetadata>>,
    ) -> Result<(), String> {
        let library_manager = self.library_manager.get();

        // ========== COMPUTE LAYOUT FIRST ==========
        // Compute the layout before streaming so we have accurate progress tracking

        let chunk_layout = AlbumChunkLayout::build(
            discovered_files.to_vec(),
            tracks_to_files,
            self.config.chunk_size_bytes,
            cue_flac_metadata.clone(),
        )?;

        // ========== STREAMING PIPELINE ==========
        // Stream chunks with accurate progress tracking

        let progress_tracker = ImportProgressTracker::new(
            db_release.id.clone(),
            chunk_layout.total_chunks,
            chunk_layout.chunk_to_track.clone(),
            chunk_layout.track_chunk_counts.clone(),
            self.progress_tx.clone(),
        );

        let (pipeline, chunk_tx) = pipeline::build_import_pipeline(
            self.config.clone(),
            db_release.id.clone(),
            self.encryption_service.clone(),
            self.cloud_storage.clone(),
            library_manager.clone(),
            progress_tracker,
            tracks_to_files.to_vec(),
            chunk_layout.files_to_chunks.clone(),
            self.config.chunk_size_bytes,
            chunk_layout.cue_flac_data.clone(),
        );

        // Use file producer
        let files_to_chunks_for_producer: Vec<FileToChunks> = discovered_files
            .iter()
            .map(|f| FileToChunks {
                file_path: f.path.clone(),
                start_chunk_index: 0, // Unused by producer
                end_chunk_index: 0,   // Unused by producer
                start_byte_offset: 0, // Unused by producer
                end_byte_offset: 0,   // Unused by producer
            })
            .collect();

        tokio::spawn(pipeline::chunk_producer::produce_chunk_stream_from_files(
            files_to_chunks_for_producer,
            self.config.chunk_size_bytes,
            chunk_tx,
        ));

        // Wait for the pipeline to complete
        let results: Vec<_> = pipeline.collect().await;

        // Check for errors
        for result in results {
            result?;
        }

        info!("All chunks uploaded successfully, persisting release metadata...");

        // ========== PERSIST RELEASE METADATA ==========
        // Track metadata was already persisted by the pipeline as tracks completed.
        // Now persist release-level metadata (files) and mark release complete.

        let persister = MetadataPersister::new(library_manager);
        persister
            .persist_release_metadata(
                &db_release.id,
                tracks_to_files,
                &chunk_layout.files_to_chunks,
                self.config.chunk_size_bytes,
            )
            .await?;

        // Mark release complete
        library_manager
            .mark_release_complete(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release complete: {}", e))?;

        Ok(())
    }

    /// Create a storage implementation from a profile
    fn create_storage(&self, profile: DbStorageProfile) -> ReleaseStorageImpl {
        ReleaseStorageImpl::new_full(
            profile,
            Some(self.encryption_service.clone()),
            Some(Arc::new(self.cloud_storage.clone())),
            self.database.clone(),
            self.config.chunk_size_bytes,
        )
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
        let mut result: HashMap<String, Vec<(String, i64, i64)>> = HashMap::new();

        // Build file size map for quick lookup
        let file_sizes: HashMap<&str, usize> = file_data
            .iter()
            .map(|(name, data, _)| (name.as_str(), data.len()))
            .collect();

        // Check if we have CUE/FLAC metadata (need to calculate byte ranges per track)
        if let Some(ref cue_metadata) = cue_flac_metadata {
            // For CUE/FLAC: calculate per-track byte ranges using seektable
            for (flac_path, metadata) in cue_metadata {
                let filename = flac_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| format!("Invalid FLAC path: {:?}", flac_path))?
                    .to_string();

                // Build seektable for this FLAC
                let flac_info = build_seektable(flac_path)
                    .map_err(|e| format!("Failed to build seektable for {:?}: {}", flac_path, e))?;

                // Get tracks that map to this FLAC file
                let flac_tracks: Vec<_> = tracks_to_files
                    .iter()
                    .filter(|tf| &tf.file_path == flac_path)
                    .collect();

                let mut track_infos = Vec::new();

                for (i, cue_track) in metadata.cue_sheet.tracks.iter().enumerate() {
                    if let Some(track_file) = flac_tracks.get(i) {
                        // Calculate byte range for this track
                        let (start_byte, end_byte) = find_track_byte_range(
                            flac_path,
                            cue_track.start_time_ms,
                            cue_track.end_time_ms,
                            &flac_info.seektable,
                            flac_info.sample_rate,
                        )?;

                        track_infos.push((track_file.db_track_id.clone(), start_byte, end_byte));
                    }
                }

                result.insert(filename, track_infos);
            }
        }

        // For one-file-per-track (or files not in CUE/FLAC)
        for track_file in tracks_to_files {
            let filename = track_file
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("Invalid file path: {:?}", track_file.file_path))?
                .to_string();

            // Skip if already handled by CUE/FLAC processing
            if result.contains_key(&filename) {
                continue;
            }

            // One-file-per-track: track spans entire file
            let file_size = *file_sizes.get(filename.as_str()).unwrap_or(&0) as i64;
            result.entry(filename).or_default().push((
                track_file.db_track_id.clone(),
                0,
                file_size,
            ));
        }

        Ok(result)
    }

    /// Build CueFlacLayoutData for CUE/FLAC imports.
    ///
    /// Extracts FLAC headers and calculates per-track byte/chunk ranges
    /// needed for accurate seeking during playback.
    async fn build_cue_flac_layout_data(
        &self,
        cue_metadata: &HashMap<PathBuf, CueFlacMetadata>,
        tracks_to_files: &[TrackFile],
    ) -> Result<HashMap<PathBuf, CueFlacLayoutData>, String> {
        use crate::cue_flac::CueFlacProcessor;
        use crate::import::album_chunk_layout::{build_seektable, find_track_byte_range};

        let mut result = HashMap::new();
        let chunk_size = self.config.chunk_size_bytes as i64;

        for (flac_path, metadata) in cue_metadata {
            // Extract FLAC headers
            let flac_headers = CueFlacProcessor::extract_flac_headers(flac_path)
                .map_err(|e| format!("Failed to extract FLAC headers: {}", e))?;

            // Build seektable from FLAC file
            let flac_info = build_seektable(flac_path)
                .map_err(|e| format!("Failed to build seektable: {}", e))?;

            // Get tracks that map to this FLAC file
            let flac_tracks: Vec<_> = tracks_to_files
                .iter()
                .filter(|tf| &tf.file_path == flac_path)
                .collect();

            // Calculate per-track byte and chunk ranges
            let mut track_chunk_ranges = HashMap::new();
            let mut track_byte_ranges = HashMap::new();

            for (i, cue_track) in metadata.cue_sheet.tracks.iter().enumerate() {
                // Find the DB track that corresponds to this CUE track
                let db_track = flac_tracks
                    .get(i)
                    .ok_or_else(|| format!("No track mapping for CUE track {}", cue_track.title))?;

                // Find exact byte positions using seektable and sample rate from libFLAC
                let (start_byte, end_byte) = find_track_byte_range(
                    flac_path,
                    cue_track.start_time_ms,
                    cue_track.end_time_ms,
                    &flac_info.seektable,
                    flac_info.sample_rate,
                )?;

                track_byte_ranges.insert(db_track.db_track_id.clone(), (start_byte, end_byte));

                // Calculate chunk ranges (relative to file's chunks)
                let start_chunk = (start_byte / chunk_size) as i32;
                let end_chunk = ((end_byte - 1) / chunk_size) as i32;

                // For per-file chunking, chunks start at 0 for each file
                track_chunk_ranges.insert(db_track.db_track_id.clone(), (start_chunk, end_chunk));
            }

            result.insert(
                flac_path.clone(),
                CueFlacLayoutData {
                    cue_sheet: metadata.cue_sheet.clone(),
                    flac_headers,
                    track_chunk_ranges,
                    track_byte_ranges,
                    seektable: Some(flac_info.seektable),
                },
            );
        }

        Ok(result)
    }

    /// Import files using the storage trait.
    ///
    /// Reads files and calls storage.write_file() for each.
    /// The storage layer handles chunking, encryption, and cloud upload based on the profile.
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

        // Mark release as importing
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        // Send started event
        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: Some(import_id.to_string()),
        });

        // Link release to storage profile
        let release_storage = crate::db::DbReleaseStorage::new(&db_release.id, &storage_profile.id);
        self.database
            .insert_release_storage(&release_storage)
            .await
            .map_err(|e| format!("Failed to link release to storage profile: {}", e))?;

        let is_chunked = storage_profile.chunked;
        let storage = self.create_storage(storage_profile);
        let total_files = discovered_files.len();

        info!(
            "Starting storage import for release {} ({} files)",
            db_release.id, total_files
        );

        // Read all files first
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

        // Build track progress info: filename → [(track_id, start_byte, end_byte)]
        let file_to_tracks = self
            .build_track_progress_map(tracks_to_files, &file_data, &cue_flac_metadata)
            .await?;

        // Calculate total bytes for release-level progress
        let release_total_bytes: usize = file_data.iter().map(|(_, data, _)| data.len()).sum();
        let mut release_bytes_written = 0usize;

        // Write files with progress reporting
        let mut chunk_index = 0i32;
        let import_id_owned = import_id.to_string();
        for (idx, (filename, data, _path)) in file_data.iter().enumerate() {
            let track_infos = file_to_tracks.get(filename).cloned().unwrap_or_default();
            let progress_tx = self.progress_tx.clone();
            let release_id = db_release.id.clone();
            let import_id_for_closure = import_id_owned.clone();
            let file_size = data.len();

            // Capture release_bytes_written for the closure
            let base_bytes = release_bytes_written;

            chunk_index = storage
                .write_file(
                    &db_release.id,
                    filename,
                    data,
                    chunk_index,
                    Box::new(move |file_bytes_written, _file_total| {
                        // Emit per-track progress only for tracks we've started writing
                        // This ensures sequential progress: track 1 completes before track 2 starts
                        let bytes_written = file_bytes_written as i64;
                        for (track_id, start_byte, end_byte) in &track_infos {
                            // Only emit progress once we've reached this track's byte range
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

                        // Emit release-level progress
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

        // Persist track metadata (audio format, chunk coords for playback)
        // For chunked storage: DbFile, DbChunk, DbFileChunk already created by storage layer
        // For non-chunked storage: only DbFile created, no chunks

        if is_chunked {
            // Chunked storage: build file-to-chunks mapping from DB records
            let mut files_to_chunks = Vec::new();
            for file in discovered_files {
                let filename = file.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Get file record from DB
                if let Ok(Some(db_file)) = library_manager
                    .get_file_by_release_and_filename(&db_release.id, filename)
                    .await
                {
                    // Get chunk mappings for this file
                    if let Ok(file_chunks) = library_manager.get_file_chunks(&db_file.id).await {
                        if !file_chunks.is_empty() {
                            let start_chunk = file_chunks.first().unwrap().chunk_index;
                            let end_chunk = file_chunks.last().unwrap().chunk_index;
                            let start_offset = file_chunks.first().unwrap().byte_offset;
                            let end_offset = file_chunks.last().unwrap().byte_offset
                                + file_chunks.last().unwrap().byte_length;

                            files_to_chunks.push(FileToChunks {
                                file_path: file.path.clone(),
                                start_chunk_index: start_chunk,
                                end_chunk_index: end_chunk,
                                start_byte_offset: start_offset,
                                end_byte_offset: end_offset,
                            });
                        }
                    }
                }
            }

            // Build CueFlacLayoutData if we have CUE/FLAC metadata
            let cue_flac_data = if let Some(ref cue_metadata) = cue_flac_metadata {
                self.build_cue_flac_layout_data(cue_metadata, tracks_to_files)
                    .await?
            } else {
                HashMap::new()
            };

            // Persist track metadata for each track
            let persister = MetadataPersister::new(library_manager);
            for track_file in tracks_to_files {
                persister
                    .persist_track_metadata(
                        &db_release.id,
                        &track_file.db_track_id,
                        tracks_to_files,
                        &files_to_chunks,
                        self.config.chunk_size_bytes,
                        &cue_flac_data,
                    )
                    .await?;
            }
        } else {
            // Non-chunked storage: create audio format records and byte ranges for seeking
            // Playback uses HTTP range requests for cloud or direct file access for local
            self.persist_non_chunked_track_metadata(tracks_to_files, cue_flac_metadata)
                .await?;
        }

        // Create DbImage records for image files and set album cover
        let cover_image_id = self
            .create_image_records(
                &db_release.id,
                &db_release.album_id,
                discovered_files,
                library_manager,
                selected_cover_filename,
            )
            .await?;

        // Mark all tracks complete and send completion events
        for track_file in tracks_to_files {
            library_manager
                .mark_track_complete(&track_file.db_track_id)
                .await
                .map_err(|e| format!("Failed to mark track complete: {}", e))?;

            let _ = self.progress_tx.send(ImportProgress::Complete {
                id: track_file.db_track_id.clone(),
                release_id: Some(db_release.id.clone()),
                cover_image_id: None, // Tracks don't have covers
                import_id: Some(import_id.to_string()),
            });
        }

        // Mark release complete
        library_manager
            .mark_release_complete(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release complete: {}", e))?;

        // Update import status to Complete
        let _ = self
            .database
            .update_import_status(import_id, ImportOperationStatus::Complete)
            .await;

        // Send release completion event with cover_image_id for reactive UI update
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
    ///
    /// Detects image files (.jpg, .jpeg, .png, .gif, .webp) and creates database records.
    /// Files in .bae/ folder are marked as MusicBrainz/Discogs source based on filename.
    /// The first image found (preferring .bae/ folder or cover/front named files) is marked as cover.
    /// Also sets the album's cover_image_id to the cover image.
    ///
    /// If `selected_cover_filename` is provided, that image will be used as cover.
    /// Otherwise, falls back to priority-based selection (.bae/ folder, cover/front in name).
    ///
    /// Returns the cover_image_id if one was set.
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

        // Collect image files with their relative paths
        let mut image_files: Vec<(&DiscoveredFile, String)> = discovered_files
            .iter()
            .filter_map(|f| {
                let ext = f.path.extension()?.to_str()?.to_lowercase();
                if image_extensions.contains(&ext.as_str()) {
                    // Build relative path preserving directory structure
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

        // Determine which image should be cover
        let cover_filename = if let Some(ref selected) = selected_cover_filename {
            // User explicitly selected a cover - use it if it exists
            if image_files.iter().any(|(_, path)| path == selected) {
                Some(selected.clone())
            } else {
                // Selected file not found, fall back to priority
                info!(
                    "Selected cover '{}' not found among images, using priority",
                    selected
                );
                None
            }
        } else {
            None
        };

        // If no valid selection, sort by priority and use first
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
            // Determine source based on path
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

            // Mark as cover if this is the selected/priority cover
            let is_cover = relative_path == &cover_filename;

            let db_image = DbImage::new(release_id, relative_path, is_cover, source);
            let image_id = db_image.id.clone();

            library_manager
                .add_image(&db_image)
                .await
                .map_err(|e| format!("Failed to add image record: {}", e))?;

            // Set album's cover_image_id for the cover image
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

    /// Get priority for cover image selection (lower = higher priority)
    fn image_cover_priority(filename: &str) -> u8 {
        let lower = filename.to_lowercase();

        // .bae/ folder files have highest priority (downloaded cover art)
        if lower.starts_with(".bae/") {
            return 0;
        }

        // Files named cover/front have second priority
        if lower.contains("cover") || lower.contains("front") {
            return 1;
        }

        // Everything else
        2
    }

    /// Get relative path for an image file, preserving subdirectory structure.
    fn get_relative_image_path(&self, path: &std::path::Path) -> String {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Check parent directory to preserve structure like ".bae/", "scans/", etc.
        if let Some(parent) = path.parent() {
            if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
                // Preserve common subdirectory patterns
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

    /// Persist track metadata for non-chunked storage.
    ///
    /// For non-chunked storage, we create:
    /// - DbAudioFormat for all tracks (with FLAC headers/seektable for CUE/FLAC)
    /// - DbTrackChunkCoords with absolute byte positions for CUE/FLAC tracks
    ///   (chunk indices are 0, byte offsets are absolute positions in file)
    ///
    /// Playback uses HTTP range requests for cloud or direct file seeking for local.
    async fn persist_non_chunked_track_metadata(
        &self,
        tracks_to_files: &[TrackFile],
        cue_flac_metadata: Option<HashMap<PathBuf, CueFlacMetadata>>,
    ) -> Result<(), String> {
        use crate::cue_flac::CueFlacProcessor;
        use crate::db::{DbAudioFormat, DbTrackChunkCoords};
        use crate::import::album_chunk_layout::{build_seektable, find_track_byte_range};

        let library_manager = self.library_manager.get();

        // Build CUE/FLAC layout data if present
        let cue_flac_data = if let Some(ref cue_metadata) = cue_flac_metadata {
            let mut data = HashMap::new();
            for (flac_path, metadata) in cue_metadata {
                let flac_headers = CueFlacProcessor::extract_flac_headers(flac_path)
                    .map_err(|e| format!("Failed to extract FLAC headers: {}", e))?;

                let flac_info = build_seektable(flac_path)
                    .map_err(|e| format!("Failed to build seektable: {}", e))?;

                // Get tracks mapping to this FLAC
                let flac_tracks: Vec<_> = tracks_to_files
                    .iter()
                    .filter(|tf| &tf.file_path == flac_path)
                    .collect();

                // Calculate byte ranges for each track
                let mut track_byte_ranges = HashMap::new();
                for (i, cue_track) in metadata.cue_sheet.tracks.iter().enumerate() {
                    if let Some(db_track) = flac_tracks.get(i) {
                        let (start_byte, end_byte) = find_track_byte_range(
                            flac_path,
                            cue_track.start_time_ms,
                            cue_track.end_time_ms,
                            &flac_info.seektable,
                            flac_info.sample_rate,
                        )?;
                        track_byte_ranges
                            .insert(db_track.db_track_id.clone(), (start_byte, end_byte));
                    }
                }

                data.insert(
                    flac_path.clone(),
                    (
                        metadata.clone(),
                        flac_headers,
                        flac_info.seektable,
                        track_byte_ranges,
                    ),
                );
            }
            data
        } else {
            HashMap::new()
        };

        for track_file in tracks_to_files {
            let format = track_file
                .file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("unknown")
                .to_lowercase();

            // Check if this is a CUE/FLAC track
            if let Some((metadata, flac_headers, seektable, track_byte_ranges)) =
                cue_flac_data.get(&track_file.file_path)
            {
                // CUE/FLAC track: store headers, seektable, and byte coords
                let flac_seektable = bincode::serialize(seektable)
                    .map_err(|e| format!("Failed to serialize seektable: {}", e))?;

                let audio_format = DbAudioFormat::new_with_seektable(
                    &track_file.db_track_id,
                    "flac",
                    Some(flac_headers.headers.clone()),
                    Some(flac_seektable),
                    true, // needs_headers = true for CUE/FLAC
                );

                library_manager
                    .add_audio_format(&audio_format)
                    .await
                    .map_err(|e| format!("Failed to insert audio format: {}", e))?;

                // Get byte range and time range for this track
                if let Some(&(start_byte, end_byte)) =
                    track_byte_ranges.get(&track_file.db_track_id)
                {
                    // Find the CUE track for time info
                    let track_index = tracks_to_files
                        .iter()
                        .filter(|tf| tf.file_path == track_file.file_path)
                        .position(|tf| tf.db_track_id == track_file.db_track_id)
                        .unwrap_or(0);

                    let cue_track = metadata.cue_sheet.tracks.get(track_index);
                    let (start_time_ms, end_time_ms) = cue_track
                        .map(|ct| (ct.start_time_ms as i64, ct.end_time_ms.unwrap_or(0) as i64))
                        .unwrap_or((0, 0));

                    // Store coords with chunk_index=-1 (sentinel for non-chunked) and absolute byte positions
                    let coords = DbTrackChunkCoords::new(
                        &track_file.db_track_id,
                        -1, // start_chunk_index (-1 = non-chunked, use byte offsets directly)
                        -1, // end_chunk_index
                        start_byte, // absolute byte offset in file
                        end_byte, // absolute byte offset in file
                        start_time_ms,
                        end_time_ms,
                    );

                    library_manager
                        .add_track_chunk_coords(&coords)
                        .await
                        .map_err(|e| format!("Failed to insert track chunk coords: {}", e))?;
                }
            } else {
                // Regular track: just audio format, no chunk coords needed
                let audio_format = DbAudioFormat::new(
                    &track_file.db_track_id,
                    &format,
                    None,  // No headers - they're in the stored file
                    false, // needs_headers = false
                );

                library_manager
                    .add_audio_format(&audio_format)
                    .await
                    .map_err(|e| format!("Failed to insert audio format: {}", e))?;
            }
        }

        Ok(())
    }

    /// Import for None storage: just record file paths, no chunking/encryption.
    ///
    /// For local file imports, records the original file path.
    /// For torrent imports, records the temp folder path (ephemeral).
    ///
    /// No chunks are created. Playback reads directly from source_path.
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

        // Mark release as importing
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        // Send started event
        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: Some(import_id.to_string()),
        });

        let total_files = discovered_files.len();

        info!(
            "Starting None storage import for release {} ({} files)",
            db_release.id, total_files
        );

        // Build file→tracks map for progress reporting
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

        // Create DbFile records with source_path (no chunks)
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

            // Create file record with source_path
            let db_file = DbFile::new(&db_release.id, filename, file.size as i64, &format)
                .with_source_path(source_path);

            library_manager
                .add_file(&db_file)
                .await
                .map_err(|e| format!("Failed to add file record: {}", e))?;

            // Emit track progress (100% since no actual I/O)
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

            // Emit release-level progress
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

        // For CUE/FLAC imports, persist track metadata (positions, audio format, seektable)
        // This is needed so playback knows where each track starts/ends in the FLAC file
        if cue_flac_metadata.is_some() {
            self.persist_non_chunked_track_metadata(tracks_to_files, cue_flac_metadata)
                .await?;
        }

        // Mark all tracks as complete and send completion events
        for track_file in tracks_to_files {
            library_manager
                .mark_track_complete(&track_file.db_track_id)
                .await
                .map_err(|e| format!("Failed to mark track complete: {}", e))?;

            let _ = self.progress_tx.send(ImportProgress::Complete {
                id: track_file.db_track_id.clone(),
                release_id: Some(db_release.id.clone()),
                cover_image_id: None, // Tracks don't have covers
                import_id: Some(import_id.to_string()),
            });
        }

        // Create DbImage records for image files
        let cover_image_id = self
            .create_image_records(
                &db_release.id,
                &db_release.album_id,
                discovered_files,
                library_manager,
                selected_cover_filename,
            )
            .await?;

        // Mark release complete
        library_manager
            .mark_release_complete(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release complete: {}", e))?;

        // Update import status to Complete
        let _ = self
            .database
            .update_import_status(import_id, ImportOperationStatus::Complete)
            .await;

        // Send release completion event with cover_image_id for reactive UI update
        let _ = self.progress_tx.send(ImportProgress::Complete {
            id: db_release.id.clone(),
            release_id: None,
            cover_image_id,
            import_id: Some(import_id.to_string()),
        });

        info!("None storage import complete for release {}", db_release.id);
        Ok(())
    }

    /// Torrent import for None storage: download to temp, record paths, skip cleanup.
    ///
    /// Files stay in the temp folder. This is ephemeral - files may be deleted
    /// at any time by the OS or user.
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

        // Mark release as importing
        library_manager
            .mark_release_importing(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release as importing: {}", e))?;

        info!(
            "Starting torrent import with None storage for '{}'",
            db_album.title
        );

        // Send started progress
        let _ = self.progress_tx.send(ImportProgress::Started {
            id: db_release.id.clone(),
            import_id: None,
        });

        // ========== ACQUIRE PHASE: TORRENT DOWNLOAD ==========

        info!("Starting torrent download (acquire phase)");

        // Add torrent via torrent manager
        let torrent_handle = self
            .torrent_handle
            .add_torrent(torrent_source.clone())
            .await
            .map_err(|e| format!("Failed to add torrent: {}", e))?;

        // Wait for metadata if needed
        torrent_handle
            .wait_for_metadata()
            .await
            .map_err(|e| format!("Failed to wait for metadata: {}", e))?;

        // Download torrent and emit progress (Acquire phase)
        loop {
            let progress = torrent_handle
                .progress()
                .await
                .map_err(|e| format!("Failed to check torrent progress: {}", e))?;

            let percent = (progress * 100.0) as u8;
            let _ = self.progress_tx.send(ImportProgress::Progress {
                id: db_release.id.clone(),
                percent,
                phase: Some(crate::import::types::ImportPhase::Acquire),
                import_id: None,
            });

            if progress >= 1.0 {
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Wait a bit for libtorrent to finish writing files to disk
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        info!("Torrent download complete");

        // Get file list from torrent to construct discovered_files
        let torrent_files = torrent_handle
            .get_file_list()
            .await
            .map_err(|e| format!("Failed to get torrent file list: {}", e))?;

        // Convert torrent files to DiscoveredFile format
        let temp_dir = std::env::temp_dir();
        let torrent_save_dir = temp_dir.join(&torrent_metadata.torrent_name);
        let mut discovered_files: Vec<DiscoveredFile> = torrent_files
            .iter()
            .map(|tf| DiscoveredFile {
                path: temp_dir.join(&tf.path),
                size: tf.size as u64,
            })
            .collect();

        // Download cover art to .bae/ folder in the torrent's temp directory
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

        // Remove torrent from download client (but DON'T delete files)
        let _ = self
            .torrent_handle
            .remove_torrent(torrent_handle, false) // false = don't delete files
            .await;

        // ========== RECORD FILE PATHS (NO CHUNKS) ==========

        // Use run_none_import to record file paths
        // Note: We call the inner logic directly since release is already marked as importing
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

            // Emit release-level progress
            let release_percent = ((idx + 1) * 100 / total_files.max(1)) as u8;
            let _ = self.progress_tx.send(ImportProgress::Progress {
                id: db_release.id.clone(),
                percent: release_percent,
                phase: Some(ImportPhase::Chunk),
                import_id: None,
            });
        }

        // Mark all tracks as complete and send completion events
        for track_file in &tracks_to_files {
            library_manager
                .mark_track_complete(&track_file.db_track_id)
                .await
                .map_err(|e| format!("Failed to mark track complete: {}", e))?;

            let _ = self.progress_tx.send(ImportProgress::Complete {
                id: track_file.db_track_id.clone(),
                release_id: Some(db_release.id.clone()),
                cover_image_id: None, // Tracks don't have covers
                import_id: None,
            });
        }

        // Create DbImage records for image files
        let cover_image_id = self
            .create_image_records(
                &db_release.id,
                &db_release.album_id,
                &discovered_files,
                library_manager,
                selected_cover_filename,
            )
            .await?;

        // Mark release complete
        library_manager
            .mark_release_complete(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release complete: {}", e))?;

        // Send release completion event with cover_image_id for reactive UI update
        let _ = self.progress_tx.send(ImportProgress::Complete {
            id: db_release.id.clone(),
            release_id: None,
            cover_image_id,
            import_id: None,
        });

        // NOTE: We intentionally skip cleanup - files stay in temp folder
        // This is ephemeral storage that may be deleted at any time
        info!(
            "Torrent None storage import complete for '{}' (files in temp: {:?})",
            db_album.title, torrent_save_dir
        );

        Ok(())
    }

    /// Torrent import using storage profile.
    ///
    /// Downloads torrent first, then imports files via run_storage_import.
    /// For None storage, files stay in temp folder (ephemeral).
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
        _storage_profile: DbStorageProfile,
        _selected_cover_filename: Option<String>,
    ) -> Result<(), String> {
        self.import_album_from_torrent(
            db_album,
            db_release,
            tracks_to_files,
            torrent_source,
            torrent_metadata,
            seed_after_download,
            cover_art_url,
        )
        .await
    }

    /// CD import using storage profile.
    ///
    /// Rips CD first, then imports files via run_storage_import.
    async fn run_cd_import(
        &self,
        db_album: DbAlbum,
        db_release: DbRelease,
        db_tracks: Vec<DbTrack>,
        drive_path: PathBuf,
        toc: CdToc,
        _storage_profile: DbStorageProfile,
        _selected_cover_filename: Option<String>,
    ) -> Result<(), String> {
        // TODO: Refactor to use storage profile for file storage
        // For now, delegate to existing implementation
        self.import_album_from_cd(db_album, db_release, db_tracks, drive_path, toc)
            .await
    }

    /// CD import with no bae storage: rips to temp folder and records paths.
    ///
    /// Files stay in temp folder. This is ephemeral - files may be deleted
    /// at any time by the OS or user.
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

        // Mark release as importing
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

        // Create temp directory for ripped files (will NOT be cleaned up)
        let temp_dir = std::env::temp_dir().join(format!("bae_cd_rip_{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| format!("Failed to create temp directory: {}", e))?;

        let drive = CdDrive {
            device_path: drive_path.clone(),
            name: drive_path.to_str().unwrap_or("Unknown").to_string(),
        };
        let ripper = CdRipper::new(drive.clone(), toc.clone(), temp_dir.clone());

        // Rip all tracks
        let rip_results = ripper
            .rip_all_tracks(None)
            .await
            .map_err(|e| format!("Failed to rip CD: {}", e))?;

        info!("CD ripping completed, {} tracks ripped", rip_results.len());

        // Record each ripped file with source_path
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

        // Mark all tracks complete and send completion events
        for track in &db_tracks {
            library_manager
                .mark_track_complete(&track.id)
                .await
                .map_err(|e| format!("Failed to mark track complete: {}", e))?;

            let _ = self.progress_tx.send(ImportProgress::Complete {
                id: track.id.clone(),
                release_id: Some(db_release.id.clone()),
                cover_image_id: None, // Tracks don't have covers
                import_id: None,
            });
        }

        // Mark release complete
        library_manager
            .mark_release_complete(&db_release.id)
            .await
            .map_err(|e| format!("Failed to mark release complete: {}", e))?;

        // Send release completion event
        // CD rips don't have image files, so cover_image_id is None
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
