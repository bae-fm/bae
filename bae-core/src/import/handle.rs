use crate::cue_flac::CueFlacProcessor;
#[cfg(feature = "torrent")]
use crate::db::DbTorrent;
use crate::db::{Database, DbImport, ImageSource, ImportOperationStatus};
use crate::discogs::DiscogsRelease;
use crate::import::cover_art::download_cover_art_to_bae_folder;
#[cfg(feature = "cd-rip")]
use crate::import::discogs_parser::parse_discogs_release;
#[cfg(feature = "cd-rip")]
use crate::import::musicbrainz_parser::fetch_and_parse_mb_release;
use crate::import::progress::ImportProgressHandle;
use crate::import::track_to_file_mapper::map_tracks_to_files;
#[cfg(feature = "torrent")]
use crate::import::types::TorrentSource;
use crate::import::types::{
    DiscoveredFile, ImportCommand, ImportProgress, ImportRequest, PrepareStep, TrackFile,
};
use crate::library::{LibraryManager, SharedLibraryManager};
use crate::musicbrainz::MbRelease;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
/// Handle for sending import requests and subscribing to progress updates
#[derive(Clone)]
pub struct ImportServiceHandle {
    pub requests_tx: mpsc::UnboundedSender<ImportCommand>,
    pub progress_tx: mpsc::UnboundedSender<ImportProgress>,
    pub progress_handle: ImportProgressHandle,
    pub library_manager: SharedLibraryManager,
    pub database: Arc<Database>,
    pub runtime_handle: tokio::runtime::Handle,
}
/// Torrent-specific metadata for import
#[cfg(feature = "torrent")]
#[derive(Debug, Clone)]
pub struct TorrentImportMetadata {
    pub info_hash: String,
    pub magnet_link: Option<String>,
    pub torrent_name: String,
    pub total_size_bytes: i64,
    pub piece_length: i32,
    pub num_pieces: i32,
    pub seed_after_download: bool,
    pub file_list: Vec<TorrentFileMetadata>,
}
/// Metadata for a single file in a torrent
#[cfg(feature = "torrent")]
#[derive(Debug, Clone)]
pub struct TorrentFileMetadata {
    pub path: std::path::PathBuf,
    pub size: i64,
}
impl ImportServiceHandle {
    /// Create a new ImportHandle with the given dependencies
    pub fn new(
        requests_tx: mpsc::UnboundedSender<ImportCommand>,
        progress_tx: mpsc::UnboundedSender<ImportProgress>,
        progress_rx: mpsc::UnboundedReceiver<ImportProgress>,
        library_manager: SharedLibraryManager,
        database: Arc<Database>,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        let progress_handle = ImportProgressHandle::new(progress_rx, runtime_handle.clone());
        Self {
            requests_tx,
            progress_tx,
            progress_handle,
            library_manager,
            database,
            runtime_handle,
        }
    }
    /// Validate and queue an import request.
    ///
    /// Performs validation (track-to-file mapping) and DB insertion synchronously.
    /// If validation fails, returns error immediately with no side effects.
    /// If successful, album is inserted with status='queued' and an import
    /// request is sent to the import worker.
    ///
    /// Returns (album_id, release_id) for navigation and progress subscription.
    pub async fn send_request(&self, request: ImportRequest) -> Result<(String, String), String> {
        match request {
            ImportRequest::Folder {
                import_id,
                discogs_release,
                mb_release,
                folder,
                master_year,
                cover_art_url,
                storage_profile_id,
                selected_cover_filename,
            } => {
                self.send_folder_request(
                    import_id,
                    discogs_release,
                    mb_release,
                    folder,
                    master_year,
                    cover_art_url,
                    storage_profile_id,
                    selected_cover_filename,
                )
                .await
            }
            #[cfg(feature = "torrent")]
            ImportRequest::Torrent {
                torrent_source,
                discogs_release,
                mb_release,
                master_year,
                seed_after_download,
                torrent_metadata,
                cover_art_url,
                storage_profile_id,
                selected_cover_filename,
            } => {
                self.send_torrent_request(
                    torrent_source,
                    discogs_release,
                    mb_release,
                    master_year,
                    seed_after_download,
                    torrent_metadata,
                    cover_art_url,
                    storage_profile_id,
                    selected_cover_filename,
                )
                .await
            }
            #[cfg(feature = "cd-rip")]
            ImportRequest::CD {
                discogs_release,
                mb_release,
                drive_path,
                master_year,
                cover_art_url,
                storage_profile_id,
                selected_cover_filename,
            } => {
                self.send_cd_request(
                    discogs_release,
                    mb_release,
                    drive_path,
                    master_year,
                    cover_art_url,
                    storage_profile_id,
                    selected_cover_filename,
                )
                .await
            }
        }
    }
    async fn send_folder_request(
        &self,
        import_id: String,
        discogs_release: Option<DiscogsRelease>,
        mb_release: Option<MbRelease>,
        folder: std::path::PathBuf,
        master_year: u32,
        cover_art_url: Option<String>,
        storage_profile_id: Option<String>,
        selected_cover_filename: Option<String>,
    ) -> Result<(String, String), String> {
        if discogs_release.is_none() && mb_release.is_none() {
            return Err("Either discogs_release or mb_release must be provided".to_string());
        }
        let library_manager = self.library_manager.get();
        let (album_title, artist_name) = if let Some(ref discogs_rel) = discogs_release {
            let artist = discogs_rel
                .artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            (discogs_rel.title.clone(), artist)
        } else if let Some(ref mb_rel) = mb_release {
            (mb_rel.title.clone(), mb_rel.artist.clone())
        } else {
            return Err("No release provided".to_string());
        };
        let db_import = DbImport::new(
            &import_id,
            &album_title,
            &artist_name,
            folder.to_str().unwrap_or(""),
        );
        self.database
            .insert_import(&db_import)
            .await
            .map_err(|e| format!("Failed to create import record: {}", e))?;
        let emit_preparing = {
            let import_id = import_id.clone();
            let album_title = album_title.clone();
            let artist_name = artist_name.clone();
            let cover_art_url = cover_art_url.clone();
            let progress_tx = self.progress_tx.clone();
            move |step: PrepareStep| {
                let _ = progress_tx.send(ImportProgress::Preparing {
                    import_id: import_id.clone(),
                    step,
                    album_title: album_title.clone(),
                    artist_name: artist_name.clone(),
                    cover_art_url: cover_art_url.clone(),
                });
            }
        };
        emit_preparing(PrepareStep::ParsingMetadata);
        let (db_album, db_release, db_tracks, artists, album_artists) =
            if let Some(ref discogs_rel) = discogs_release {
                use crate::import::discogs_parser::parse_discogs_release;
                parse_discogs_release(discogs_rel, master_year, cover_art_url.clone())?
            } else if let Some(ref mb_rel) = mb_release {
                use crate::import::musicbrainz_parser::fetch_and_parse_mb_release;
                fetch_and_parse_mb_release(&mb_rel.release_id, master_year, cover_art_url.clone())
                    .await?
            } else {
                return Err("No release provided".to_string());
            };
        if let Some(ref url) = cover_art_url {
            emit_preparing(PrepareStep::DownloadingCoverArt);
            let source = if mb_release.is_some() {
                ImageSource::MusicBrainz
            } else {
                ImageSource::Discogs
            };
            match download_cover_art_to_bae_folder(url, &folder, source).await {
                Ok(downloaded) => {
                    info!("Downloaded cover art to {:?}", downloaded.path);
                }
                Err(e) => {
                    warn!("Failed to download cover art: {}", e);
                }
            }
        }
        emit_preparing(PrepareStep::DiscoveringFiles);
        let discovered_files = discover_folder_files(&folder)?;
        emit_preparing(PrepareStep::ValidatingTracks);
        let mapping_result = map_tracks_to_files(&db_tracks, &discovered_files).await?;
        let tracks_to_files = mapping_result.track_files.clone();
        let cue_flac_metadata = mapping_result.cue_flac_metadata.clone();
        emit_preparing(PrepareStep::SavingToDatabase);
        let mut artist_id_map = std::collections::HashMap::new();
        for artist in &artists {
            let parsed_id = artist.id.clone();
            let existing = if let Some(ref discogs_id) = artist.discogs_artist_id {
                library_manager
                    .get_artist_by_discogs_id(discogs_id)
                    .await
                    .map_err(|e| format!("Database error: {}", e))?
            } else {
                None
            };
            let actual_id = if let Some(existing_artist) = existing {
                existing_artist.id
            } else {
                library_manager
                    .insert_artist(artist)
                    .await
                    .map_err(|e| format!("Failed to insert artist: {}", e))?;
                artist.id.clone()
            };
            artist_id_map.insert(parsed_id, actual_id);
        }
        library_manager
            .insert_album_with_release_and_tracks(&db_album, &db_release, &db_tracks)
            .await
            .map_err(|e| format!("Database error: {}", e))?;
        self.database
            .link_import_to_release(&import_id, &db_release.id)
            .await
            .map_err(|e| format!("Failed to link import to release: {}", e))?;
        for album_artist in &album_artists {
            let actual_artist_id = artist_id_map.get(&album_artist.artist_id).ok_or_else(|| {
                format!(
                    "Artist ID {} not found in artist map",
                    album_artist.artist_id,
                )
            })?;
            let mut updated_album_artist = album_artist.clone();
            updated_album_artist.artist_id = actual_artist_id.clone();
            library_manager
                .insert_album_artist(&updated_album_artist)
                .await
                .map_err(|e| format!("Failed to insert album-artist relationship: {}", e))?;
        }
        emit_preparing(PrepareStep::ExtractingDurations);
        extract_and_store_durations(library_manager, &tracks_to_files).await?;
        tracing::info!(
            "Validated and queued album '{}' (release: {}) with {} tracks",
            db_album.title,
            db_release.id,
            db_tracks.len()
        );
        self.database
            .update_import_status(&import_id, ImportOperationStatus::Importing)
            .await
            .map_err(|e| format!("Failed to update import status: {}", e))?;
        let album_id = db_album.id.clone();
        let release_id = db_release.id.clone();
        self.requests_tx
            .send(ImportCommand::Folder {
                db_album,
                db_release,
                tracks_to_files,
                discovered_files,
                cue_flac_metadata,
                storage_profile_id,
                selected_cover_filename,
                import_id,
            })
            .map_err(|_| "Failed to queue validated album for import".to_string())?;
        Ok((album_id, release_id))
    }
    #[cfg(feature = "torrent")]
    async fn send_torrent_request(
        &self,
        torrent_source: TorrentSource,
        discogs_release: Option<DiscogsRelease>,
        mb_release: Option<MbRelease>,
        master_year: u32,
        seed_after_download: bool,
        torrent_metadata: TorrentImportMetadata,
        cover_art_url: Option<String>,
        storage_profile_id: Option<String>,
        selected_cover_filename: Option<String>,
    ) -> Result<(String, String), String> {
        if discogs_release.is_none() && mb_release.is_none() {
            return Err("Either discogs_release or mb_release must be provided".to_string());
        }
        let library_manager = self.library_manager.get();
        let torrent_source_for_request = torrent_source.clone();
        info!(
            "Torrent import: {} ({} pieces, {} bytes)",
            torrent_metadata.torrent_name,
            torrent_metadata.num_pieces,
            torrent_metadata.total_size_bytes
        );
        let (db_album, db_release, db_tracks, artists, album_artists) =
            if let Some(ref discogs_rel) = discogs_release {
                use crate::import::discogs_parser::parse_discogs_release;
                parse_discogs_release(discogs_rel, master_year, cover_art_url.clone())?
            } else if let Some(ref mb_rel) = mb_release {
                use crate::import::musicbrainz_parser::fetch_and_parse_mb_release;
                fetch_and_parse_mb_release(&mb_rel.release_id, master_year, cover_art_url.clone())
                    .await?
            } else {
                return Err("No release provided".to_string());
            };
        let temp_dir = std::env::temp_dir();
        let discovered_files: Vec<DiscoveredFile> = torrent_metadata
            .file_list
            .iter()
            .map(|tf| DiscoveredFile {
                path: temp_dir.join(&tf.path),
                size: tf.size as u64,
            })
            .collect();
        let mapping_result = map_tracks_to_files(&db_tracks, &discovered_files).await?;
        let tracks_to_files = mapping_result.track_files.clone();
        let mut artist_id_map = std::collections::HashMap::new();
        for artist in &artists {
            let parsed_id = artist.id.clone();
            let existing = if let Some(ref discogs_id) = artist.discogs_artist_id {
                library_manager
                    .get_artist_by_discogs_id(discogs_id)
                    .await
                    .map_err(|e| format!("Database error: {}", e))?
            } else {
                None
            };
            let actual_id = if let Some(existing_artist) = existing {
                existing_artist.id
            } else {
                library_manager
                    .insert_artist(artist)
                    .await
                    .map_err(|e| format!("Failed to insert artist: {}", e))?;
                artist.id.clone()
            };
            artist_id_map.insert(parsed_id, actual_id);
        }
        library_manager
            .insert_album_with_release_and_tracks(&db_album, &db_release, &db_tracks)
            .await
            .map_err(|e| format!("Database error: {}", e))?;
        extract_and_store_durations(library_manager, &tracks_to_files).await?;
        for album_artist in &album_artists {
            let actual_artist_id = artist_id_map.get(&album_artist.artist_id).ok_or_else(|| {
                format!(
                    "Artist ID {} not found in artist map",
                    album_artist.artist_id,
                )
            })?;
            let mut updated_album_artist = album_artist.clone();
            updated_album_artist.artist_id = actual_artist_id.clone();
            library_manager
                .insert_album_artist(&updated_album_artist)
                .await
                .map_err(|e| format!("Failed to insert album-artist relationship: {}", e))?;
        }
        let db_torrent = DbTorrent::new(
            &db_release.id,
            &torrent_metadata.info_hash,
            torrent_metadata.magnet_link.clone(),
            &torrent_metadata.torrent_name,
            torrent_metadata.total_size_bytes,
            torrent_metadata.piece_length,
            torrent_metadata.num_pieces,
        );
        library_manager
            .insert_torrent(&db_torrent)
            .await
            .map_err(|e| format!("Failed to save torrent metadata: {}", e))?;
        tracing::info!(
            "Validated and queued torrent import '{}' (release: {}) with {} tracks",
            db_album.title,
            db_release.id,
            db_tracks.len()
        );
        let album_id = db_album.id.clone();
        let release_id = db_release.id.clone();
        self.requests_tx
            .send(ImportCommand::Torrent {
                db_album,
                db_release,
                tracks_to_files,
                torrent_source: torrent_source_for_request,
                torrent_metadata,
                seed_after_download,
                cover_art_url,
                storage_profile_id,
                selected_cover_filename,
            })
            .map_err(|_| "Failed to queue validated torrent for import".to_string())?;
        Ok((album_id, release_id))
    }
    #[cfg(feature = "cd-rip")]
    async fn send_cd_request(
        &self,
        discogs_release: Option<DiscogsRelease>,
        mb_release: Option<MbRelease>,
        drive_path: std::path::PathBuf,
        master_year: u32,
        cover_art_url: Option<String>,
        storage_profile_id: Option<String>,
        selected_cover_filename: Option<String>,
    ) -> Result<(String, String), String> {
        if discogs_release.is_none() && mb_release.is_none() {
            return Err("Either discogs_release or mb_release must be provided".to_string());
        }
        let library_manager = self.library_manager.get();
        use crate::cd::CdDrive;
        let drive = CdDrive {
            device_path: drive_path.clone(),
            name: drive_path.to_str().unwrap_or("Unknown").to_string(),
        };
        let toc = drive
            .read_toc()
            .map_err(|e| format!("Failed to read CD TOC: {}", e))?;
        let (db_album, db_release, db_tracks, artists, album_artists) =
            if let Some(ref discogs_rel) = discogs_release {
                parse_discogs_release(discogs_rel, master_year, cover_art_url.clone())?
            } else if let Some(ref mb_rel) = mb_release {
                fetch_and_parse_mb_release(&mb_rel.release_id, master_year, cover_art_url.clone())
                    .await?
            } else {
                return Err("No release provided".to_string());
            };
        let mut artist_id_map = std::collections::HashMap::new();
        for artist in &artists {
            let parsed_id = artist.id.clone();
            let existing = if let Some(ref discogs_id) = artist.discogs_artist_id {
                library_manager
                    .get_artist_by_discogs_id(discogs_id)
                    .await
                    .map_err(|e| format!("Database error: {}", e))?
            } else {
                None
            };
            let actual_id = if let Some(existing_artist) = existing {
                existing_artist.id
            } else {
                library_manager
                    .insert_artist(artist)
                    .await
                    .map_err(|e| format!("Failed to insert artist: {}", e))?;
                artist.id.clone()
            };
            artist_id_map.insert(parsed_id, actual_id);
        }
        library_manager
            .insert_album_with_release_and_tracks(&db_album, &db_release, &db_tracks)
            .await
            .map_err(|e| format!("Database error: {}", e))?;
        for album_artist in &album_artists {
            let actual_artist_id = artist_id_map.get(&album_artist.artist_id).ok_or_else(|| {
                format!(
                    "Artist ID {} not found in artist map",
                    album_artist.artist_id,
                )
            })?;
            let mut updated_album_artist = album_artist.clone();
            updated_album_artist.artist_id = actual_artist_id.clone();
            library_manager
                .insert_album_artist(&updated_album_artist)
                .await
                .map_err(|e| format!("Failed to insert album-artist relationship: {}", e))?;
        }
        let album_id = db_album.id.clone();
        let release_id = db_release.id.clone();
        self.requests_tx
            .send(ImportCommand::CD {
                db_album,
                db_release,
                db_tracks,
                drive_path: drive.device_path,
                toc,
                storage_profile_id,
                selected_cover_filename,
            })
            .map_err(|_| "Failed to queue validated CD import".to_string())?;
        Ok((album_id, release_id))
    }
    /// Subscribe to progress updates for a specific release
    /// Returns a filtered receiver that yields only updates for the specified release
    pub fn subscribe_release(
        &self,
        release_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        self.progress_handle.subscribe_release(release_id)
    }
    /// Subscribe to progress updates for a specific track
    /// Returns a filtered receiver that yields only updates for the specified track
    pub fn subscribe_track(
        &self,
        track_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        self.progress_handle.subscribe_track(track_id)
    }
    /// Subscribe to progress updates for a specific import operation
    /// Returns Preparing events and any event with matching import_id
    pub fn subscribe_import(
        &self,
        import_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        self.progress_handle.subscribe_import(import_id)
    }
    /// Subscribe to progress updates for ALL import operations
    /// Returns any event that has an import_id (for toolbar dropdown)
    pub fn subscribe_all_imports(&self) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        self.progress_handle.subscribe_all_imports()
    }
}
/// Extract durations from audio files and update database immediately
pub async fn extract_and_store_durations(
    library_manager: &LibraryManager,
    tracks_to_files: &[TrackFile],
) -> Result<(), String> {
    use std::collections::HashMap;
    use std::path::Path;
    let mut file_groups: HashMap<&Path, Vec<&TrackFile>> = HashMap::new();
    for mapping in tracks_to_files {
        file_groups
            .entry(mapping.file_path.as_path())
            .or_default()
            .push(mapping);
    }
    for (file_path, mappings) in file_groups {
        let is_cue_flac = mappings.len() > 1
            && file_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase())
                == Some("flac".to_string());
        if is_cue_flac {
            let cue_path = file_path.with_extension("cue");
            if cue_path.exists() {
                match CueFlacProcessor::parse_cue_sheet(&cue_path) {
                    Ok(cue_sheet) => {
                        for (mapping, cue_track) in mappings.iter().zip(cue_sheet.tracks.iter()) {
                            let duration_ms =
                                cue_track.track_duration_ms().map(|d| d as i64).or_else(|| {
                                    // Last track: calculate from file duration
                                    // Use start_time_ms (INDEX 01) not audio_start_ms (INDEX 00) to exclude pregap
                                    extract_duration_from_file(file_path).map(|file_duration_ms| {
                                        file_duration_ms - cue_track.start_time_ms as i64
                                    })
                                });
                            library_manager
                                .update_track_duration(&mapping.db_track_id, duration_ms)
                                .await
                                .map_err(|e| format!("Failed to update track duration: {}", e))?;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse CUE sheet for duration extraction: {:?}", e);
                    }
                }
            }
        } else {
            for mapping in mappings {
                let duration_ms = extract_duration_from_file(&mapping.file_path);
                library_manager
                    .update_track_duration(&mapping.db_track_id, duration_ms)
                    .await
                    .map_err(|e| format!("Failed to update track duration: {}", e))?;
            }
        }
    }
    Ok(())
}
/// Extract duration from an audio file (FLAC only)
fn extract_duration_from_file(file_path: &Path) -> Option<i64> {
    debug!("Extracting duration from file: {}", file_path.display());
    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if extension.eq_ignore_ascii_case("flac") {
        return extract_flac_duration(file_path);
    }
    warn!(
        "Duration extraction not supported for non-FLAC file: {}",
        file_path.display()
    );
    None
}
/// Extract duration from a FLAC file using libFLAC
fn extract_flac_duration(file_path: &Path) -> Option<i64> {
    use crate::cue_flac::CueFlacProcessor;
    match CueFlacProcessor::analyze_flac(file_path) {
        Ok(flac_info) => {
            let duration_ms = flac_info.duration_ms() as i64;
            debug!(
                "Extracted FLAC duration via libFLAC: {} ms from {}",
                duration_ms,
                file_path.display()
            );
            Some(duration_ms)
        }
        Err(e) => {
            warn!(
                "Failed to extract FLAC duration via libFLAC: {} for {}",
                e,
                file_path.display()
            );
            None
        }
    }
}
/// Discover all files in folder with metadata.
///
/// Recursively scans the folder using the folder_scanner module to support:
/// - Single release (flat) - audio files in root, optional artwork subfolders
/// - Single release (multi-disc) - disc subfolders with audio, optional artwork
/// - Collections - recursive tree where leaves are single releases
///
/// Files are sorted by path for consistent ordering across runs.
fn discover_folder_files(folder: &Path) -> Result<Vec<DiscoveredFile>, String> {
    use crate::import::folder_scanner::{self, AudioContent};
    let categorized = folder_scanner::collect_release_files(folder)?;
    let mut files: Vec<DiscoveredFile> = Vec::new();
    match categorized.audio {
        AudioContent::CueFlacPairs(pairs) => {
            for pair in pairs {
                files.push(DiscoveredFile {
                    path: pair.cue_file.path,
                    size: pair.cue_file.size,
                });
                files.push(DiscoveredFile {
                    path: pair.audio_file.path,
                    size: pair.audio_file.size,
                });
            }
        }
        AudioContent::TrackFiles(tracks) => {
            for f in tracks {
                files.push(DiscoveredFile {
                    path: f.path,
                    size: f.size,
                });
            }
        }
    }
    for f in categorized.artwork {
        files.push(DiscoveredFile {
            path: f.path,
            size: f.size,
        });
    }
    for f in categorized.documents {
        files.push(DiscoveredFile {
            path: f.path,
            size: f.size,
        });
    }
    for f in categorized.other {
        files.push(DiscoveredFile {
            path: f.path,
            size: f.size,
        });
    }
    Ok(files)
}
