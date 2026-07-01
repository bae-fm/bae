use crate::import::handle::ImportServiceHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::handle::{ScanEvent, WatcherCommand};
use crate::import::types::{
    ImportCommand, ImportProgress, MetadataRef, MetadataSource, StorageMode,
};
use crate::library::LibraryManager;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use {
    crate::db::{
        DbAlbum, DbAlbumArtist, DbFile, DbRelease, DbReleaseArtistRole, DbReleaseMetadata, DbTrack,
        DbTrackArtist, DbTrackArtistRole, ImportOperationStatus,
    },
    crate::import::folder_registry::ImportFolderRegistry,
    crate::import::folder_scanner::{
        scan_for_candidates_with_callback, InvalidCandidate, ScanItem,
    },
    crate::import::track_to_file_mapper::map_tracks_to_files,
    crate::import::types::{CoverSelection, DiscoveredFile, ImportPhase, PrepareStep, TrackFile},
    crate::import::ParsedWorkGraph,
    notify::RecursiveMode,
    notify_debouncer_full::{new_debouncer, DebounceEventResult},
    std::collections::{HashMap, HashSet},
    std::path::{Path, PathBuf},
    std::sync::{Arc, Mutex},
    std::time::Duration,
};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tokio::sync::{broadcast, mpsc};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tracing::{error, info, warn};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod cover_image;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod format_prep;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod progress;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod reconcile;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use format_prep::resolve_file_content_type;

/// Metadata-prep output for a folder import.
///
/// Resolves a release against MB/Discogs, matches it to an existing album,
/// records the DbImport row, and remaps parsed artist IDs to their actual
/// DB IDs. The caller takes what it needs.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct PreparedMetadata {
    db_album: DbAlbum,
    db_release: DbRelease,
    db_tracks: Vec<DbTrack>,
    /// Raw (source, json) pairs from the metadata resolver, wrapped into
    /// DbReleaseMetadata at commit.
    resolved_metadata: Vec<(String, String)>,
    existing_album_id: Option<String>,
    remapped_track_artists: Vec<DbTrackArtist>,
    remapped_album_artists: Vec<DbAlbumArtist>,
    work_graph: ParsedWorkGraph,
    remapped_release_artist_roles: Vec<DbReleaseArtistRole>,
    remapped_track_artist_roles: Vec<DbTrackArtistRole>,
    /// Per-source identity rows for the release. Empty for Unknown.
    /// Commit writes one `release_identities` row per element.
    identities: Vec<crate::import::types::ReleaseIdentity>,
    album_title: String,
    artist_name: String,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn storage_mode_label(mode: &StorageMode) -> &'static str {
    match mode {
        StorageMode::Remote => "remote",
        StorageMode::Local => "local",
    }
}

/// Send an import event on the broadcast bus, logging on send failure.
use crate::import::handle::send_event;

pub struct ImportService {
    commands_rx: mpsc::UnboundedReceiver<ImportCommand>,
    event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
    library_manager: LibraryManager,
}

/// The watched roots that contain at least one of the `changed` paths, in
/// `roots` order and without duplicates.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn affected_roots(changed: &[&Path], roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .filter(|root| changed.iter().any(|path| path.starts_with(root)))
        .cloned()
        .collect()
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ImportService {
    /// The folder watcher. Owns a debouncing filesystem watcher and, per watched
    /// folder, the set of candidate keys it last emitted. A `Watch` command
    /// starts watching and scanning a folder, a debounced filesystem change
    /// under a watched folder re-scans it, and `Unwatch` stops. Every re-scan
    /// reconciles against the last known keys, emitting `FolderCandidate` for
    /// what's on disk and `CandidateRemoved` for what's gone, so changes
    /// propagate beyond the first scan.
    fn start_watcher(
        runtime_handle: &tokio::runtime::Handle,
        mut cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
        event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: LibraryManager,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    ) {
        runtime_handle.spawn(async move {
            let (fs_tx, mut fs_rx) = mpsc::unbounded_channel::<DebounceEventResult>();
            let mut debouncer = match new_debouncer(
                Duration::from_secs(1),
                None,
                move |result: DebounceEventResult| {
                    // Runs on the debouncer's own thread; hand the batch to the
                    // task. A send error means the task's receiver is gone (the
                    // service is shutting down); the debouncer is dropped right
                    // after, so this is benign but worth a line.
                    if fs_tx.send(result).is_err() {
                        warn!("folder watcher event dropped: task receiver gone");
                    }
                },
            ) {
                Ok(debouncer) => debouncer,
                Err(e) => {
                    error!("failed to start folder watcher: {e}");
                    return;
                }
            };

            let mut roots: Vec<PathBuf> = Vec::new();
            let mut last_keys: HashMap<PathBuf, HashSet<String>> = HashMap::new();

            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        let Some(cmd) = cmd else { break };
                        match cmd {
                            WatcherCommand::Watch(path) => {
                                if !roots.contains(&path) {
                                    if let Err(e) =
                                        debouncer.watch(&path, RecursiveMode::Recursive)
                                    {
                                        warn!("failed to watch {}: {e}", path.display());
                                    }
                                    roots.push(path.clone());
                                }
                                Self::rescan_and_reconcile(
                                    &path,
                                    &mut last_keys,
                                    &event_tx,
                                    &library_manager,
                                    &folder_registry,
                                )
                                .await;
                            }
                            WatcherCommand::Unwatch(path) => {
                                if let Err(e) = debouncer.unwatch(&path) {
                                    warn!("failed to unwatch {}: {e}", path.display());
                                }
                                roots.retain(|p| p != &path);
                                last_keys.remove(&path);
                            }
                        }
                    }
                    Some(result) = fs_rx.recv() => {
                        let events = match result {
                            Ok(events) => events,
                            Err(errors) => {
                                for e in errors {
                                    warn!("folder watcher error: {e}");
                                }
                                continue;
                            }
                        };
                        let changed: Vec<&Path> = events
                            .iter()
                            .flat_map(|e| e.paths.iter().map(PathBuf::as_path))
                            .collect();
                        for root in affected_roots(&changed, &roots) {
                            Self::rescan_and_reconcile(
                                &root,
                                &mut last_keys,
                                &event_tx,
                                &library_manager,
                                &folder_registry,
                            )
                            .await;
                        }
                    }
                }
            }
        });
    }

    /// Re-scan `root` and reconcile against the candidate keys last emitted for
    /// it: emit every current candidate (the reducer keeps in-progress state for
    /// ones it already holds) and `CandidateRemoved` for any that vanished. A
    /// scan error (folder unreadable/deleted) reconciles to empty, removing all
    /// of the folder's candidates.
    async fn rescan_and_reconcile(
        root: &Path,
        last_keys: &mut HashMap<PathBuf, HashSet<String>>,
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: &LibraryManager,
        folder_registry: &Arc<Mutex<ImportFolderRegistry>>,
    ) {
        let root_buf = root.to_path_buf();
        let scanned = tokio::task::spawn_blocking(move || {
            let mut valid = Vec::new();
            let mut invalid = Vec::new();
            scan_for_candidates_with_callback(root_buf, |item| match item {
                ScanItem::Valid(c) => valid.push(c),
                ScanItem::Invalid(c) => invalid.push(c),
            })
            .map(|()| (valid, invalid))
        })
        .await;

        let (mut candidates, invalid_candidates): (_, Vec<InvalidCandidate>) = match scanned {
            Ok(Ok(split)) => split,
            Ok(Err(e)) => {
                warn!(
                    "re-scan of {} failed ({e}); removing its candidates",
                    root.display()
                );
                (Vec::new(), Vec::new())
            }
            Err(e) => {
                error!("folder scan task panicked for {}: {e}", root.display());
                return;
            }
        };

        // The blocking walk left `skipped`/`is_added` at their defaults (it has
        // neither the registry nor the DB). Stamp the real values now, before
        // reconciling, so every emitted candidate carries its tab state. Invalid
        // candidates carry no files or tab state, so they need no stamping.
        for candidate in &mut candidates {
            let path = candidate.path.to_string_lossy();
            candidate.skipped = folder_registry.lock().unwrap().is_skipped(&path);
            candidate.is_added = match library_manager
                .is_content_hash_imported(&candidate.files.content_hash())
                .await
            {
                Ok(is_added) => is_added,
                Err(e) => {
                    // A DB read fault leaves the candidate as "not added" rather
                    // than dropping it — the user still sees it under "New" and
                    // can act on it; the next re-scan retries the lookup.
                    warn!(
                        "added-state lookup failed for {}; treating as not added: {e}",
                        candidate.path.display()
                    );
                    false
                }
            };
        }

        // Reconciliation keys span both lists: a folder that flipped valid →
        // invalid (or vice versa) keeps the same path key, so a removal is only
        // emitted when the path drops out of the scan entirely.
        let new_keys: HashSet<String> = candidates
            .iter()
            .map(|c| c.path.to_string_lossy().into_owned())
            .chain(
                invalid_candidates
                    .iter()
                    .map(|c| c.path.to_string_lossy().into_owned()),
            )
            .collect();

        for candidate in candidates {
            send_event(
                event_tx,
                crate::import::handle::ImportEvent::Scan(ScanEvent::FolderCandidate(candidate)),
            );
        }

        for invalid in invalid_candidates {
            send_event(
                event_tx,
                crate::import::handle::ImportEvent::Scan(ScanEvent::InvalidCandidate(invalid)),
            );
        }

        if let Some(previous) = last_keys.get(root) {
            for gone in previous.difference(&new_keys) {
                send_event(
                    event_tx,
                    crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                        candidate_key: gone.clone(),
                    }),
                );
            }
        }
        last_keys.insert(root.to_path_buf(), new_keys);

        send_event(
            event_tx,
            crate::import::handle::ImportEvent::Scan(ScanEvent::Finished),
        );
    }

    /// Start the import service worker.
    ///
    /// Creates one worker task that imports validated albums sequentially from a queue.
    /// Multiple imports will be queued and handled one at a time, not concurrently.
    /// Returns a handle that can be cloned and used throughout the app to submit import requests.
    pub fn start(
        runtime_handle: tokio::runtime::Handle,
        library_manager: LibraryManager,
        cover_art_archive: crate::import::cover_art::CoverArtArchiveClient,
    ) -> ImportServiceHandle {
        let (commands_tx, commands_rx) = mpsc::unbounded_channel();
        let (watcher_tx, watcher_rx) = mpsc::unbounded_channel();
        let (event_tx, _) = broadcast::channel(1024);
        let event_tx_for_worker = event_tx.clone();
        let library_manager_for_handle = library_manager.clone();

        // The watched-folder list is durable per-library appdata; `load` warns
        // and starts empty if the file is corrupt, so app start never fails on it.
        // The same `Arc` is shared by the watcher (which reads the skip set while
        // stamping candidates) and the handle (which mutates it on add/remove/skip).
        let folder_registry = Arc::new(Mutex::new(ImportFolderRegistry::load(
            library_manager_for_handle.library_dir(),
        )));

        ImportService::start_watcher(
            &runtime_handle,
            watcher_rx,
            event_tx.clone(),
            library_manager_for_handle.clone(),
            folder_registry.clone(),
        );

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create import runtime");
            rt.block_on(async move {
                let mut service = ImportService {
                    commands_rx,
                    event_tx: event_tx_for_worker,
                    library_manager,
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
            library_manager_for_handle,
            runtime_handle,
            watcher_tx,
            event_tx,
            folder_registry,
            cover_art_archive,
        )
    }

    async fn do_import(&self, command: ImportCommand) {
        let ImportCommand::Folder {
            import_id,
            candidate_key,
            folder,
            selected_cover,
            storage_mode,
            pin,
            identity_choice,
            user_edit,
        } = command;

        let result = self
            .prepare_and_run_folder_import(
                import_id.clone(),
                candidate_key.clone(),
                folder,
                selected_cover,
                storage_mode,
                pin,
                identity_choice,
                user_edit,
            )
            .await;

        if let Err(e) = result {
            error!("Import failed: {}", e);

            // No release to mark failed -- if import fails before the atomic
            // finalize, there's no release in the DB. Update the import record.
            if let Err(db_err) = self
                .library_manager
                .update_import_error(&import_id, &e)
                .await
            {
                error!("Failed to update import error: {}", db_err);
            }

            send_event(
                &self.event_tx,
                crate::import::handle::ImportEvent::ImportProgress {
                    candidate_key,
                    progress: ImportProgress::Failed {
                        id: import_id.clone(),
                        error: e,
                        import_id: Some(import_id),
                    },
                },
            );
        }
    }

    /// Prepare and run a folder import.
    ///
    /// For Exact / Approximate calls `prepare_release` to fetch and map
    /// the release — this hits the network LRU caches that the UI's
    /// prefetch warmed, so the normal case is a cache hit. For Unknown
    /// reads embedded tags via `map_file_tags_to_db` from the
    /// candidate's audio files instead. Either way: walks the folder
    /// to discover files, then runs track mapping, storage, and the
    /// atomic DB transaction. Remote cover bytes are pulled through
    /// `download_cover_art_bytes`, which has its own LRU cache.
    async fn prepare_and_run_folder_import(
        &self,
        import_id: String,
        candidate_key: String,
        folder: PathBuf,
        selected_cover: Option<CoverSelection>,
        storage_mode: StorageMode,
        pin: bool,
        identity_choice: crate::import::IdentityChoice,
        user_edit: Option<crate::import::ReleaseUserEdit>,
    ) -> Result<(), String> {
        let library_manager = &self.library_manager;

        let import_start = std::time::Instant::now();
        let mut step_times: Vec<(&str, std::time::Duration)> = Vec::new();
        let mut last_step_start = import_start;

        // Walk the folder to discover files. Scan and commit are two
        // points in time separated by user interaction; reality can
        // shift in that gap — the user can move, rename, or reorganize
        // in the same window. The worker treats the disk at commit as
        // the source of truth.
        let folder_buf = folder.clone();
        let categorized = tokio::task::spawn_blocking(move || {
            crate::import::folder_scanner::collect_release_candidate_files(&folder_buf)
        })
        .await
        .map_err(|e| format!("Folder scan task failed: {e}"))??;

        // Content fingerprint of the folder tree. Used below to overwrite a
        // prior import of the same files, then stamped onto the new release row.
        let content_hash = categorized.content_hash();

        // Phase 0a: Reconcile the prepared release with existing library state
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.clone(),
                progress: ImportProgress::Preparing {
                    import_id: import_id.clone(),
                    step: PrepareStep::ParsingMetadata,
                    album_title: String::new(),
                    artist_name: String::new(),
                },
            },
        );

        // Source the parsed album. Exact / Approximate fetch from
        // MB / Discogs; Unknown reads embedded tags. The UI's prefetch
        // warmed the network LRU cache for the source-release path,
        // so that case is a cache-hit; cold cache costs one round-trip.
        let (parsed, metadata_pairs) = match &identity_choice {
            crate::import::IdentityChoice::Exact { release_ref }
            | crate::import::IdentityChoice::Approximate { release_ref } => {
                let release = prepare_release(library_manager, release_ref).await?;
                (release.parsed, release.metadata_pairs)
            }
            crate::import::IdentityChoice::Unknown => {
                let folder_name = folder
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());
                let clock = library_manager.clock().clone();
                let ids = library_manager.ids().clone();
                // Track structure for a CUE image comes from the CUE, not the
                // single audio file: one DbTrack per CUE TRACK, titles from the
                // sheet. Per-track rips seed one DbTrack per file. Both feed the
                // same assembly in file_tag_mapper.
                let parsed = match &categorized.audio {
                    crate::import::folder_scanner::AudioContent::CueFlacPairs { pairs, .. } => {
                        // Sort pairs the same way `map_tracks_to_files` slices
                        // them so seed order lines up, then hand the parsed
                        // sheets + audio paths to the blocking task.
                        let mut sorted: Vec<&crate::import::folder_scanner::ScannedCueFlacPair> =
                            pairs.iter().collect();
                        sorted.sort_by(|a, b| {
                            natord::compare(&a.cue_file.relative_path, &b.cue_file.relative_path)
                        });
                        let mut owned: Vec<(crate::cue_flac::CueSheet, PathBuf)> =
                            Vec::with_capacity(sorted.len());
                        for p in sorted {
                            let sheet = p.cue_sheet.clone().ok_or_else(|| {
                                format!("CUE sheet not parsed for pair {:?}", p.cue_file.path)
                            })?;
                            owned.push((sheet, p.audio_file.path.clone()));
                        }
                        tokio::task::spawn_blocking(move || {
                            let sheets: Vec<&crate::cue_flac::CueSheet> =
                                owned.iter().map(|(s, _)| s).collect();
                            let audio: Vec<&Path> =
                                owned.iter().map(|(_, p)| p.as_path()).collect();
                            crate::import::file_tag_mapper::map_cue_sheets_to_db(
                                &sheets,
                                &audio,
                                folder_name.as_deref(),
                                clock.as_ref(),
                                ids.as_ref(),
                            )
                        })
                        .await
                        .map_err(|e| format!("cue-seed mapping task failed: {e}"))??
                    }
                    crate::import::folder_scanner::AudioContent::TrackFiles { .. } => {
                        let audio_paths =
                            crate::import::handle::categorized_audio_paths(&categorized);
                        tokio::task::spawn_blocking(move || {
                            crate::import::file_tag_mapper::map_file_tags_to_db(
                                &audio_paths,
                                folder_name.as_deref(),
                                clock.as_ref(),
                                ids.as_ref(),
                            )
                        })
                        .await
                        .map_err(|e| format!("file-tag mapping task failed: {e}"))??
                    }
                };
                (parsed, Vec::new())
            }
        };

        // Overwrite a prior import of the same folder tree: re-importing
        // replaces rather than duplicates. Remove the existing release(s)
        // carrying this content hash (and their remote files, via the library
        // remove path) before reconciling, so the new import builds against
        // post-removal state — a single-release album is freshly recreated; a
        // multi-release album keeps its siblings and reassigns its primary. The
        // source-metadata fetch above already ran, so the window between this
        // removal and the new insert holds no fallible network step.
        library_manager
            .delete_releases_with_content_hash(&content_hash)
            .await
            .map_err(|e| format!("Failed to overwrite prior import: {e}"))?;

        let PreparedMetadata {
            db_album,
            mut db_release,
            db_tracks,
            resolved_metadata,
            existing_album_id,
            remapped_track_artists,
            remapped_album_artists,
            work_graph,
            remapped_release_artist_roles,
            remapped_track_artist_roles,
            identities,
            album_title,
            artist_name,
        } = self
            .reconcile_prepared_release(
                parsed,
                metadata_pairs,
                &import_id,
                folder
                    .to_str()
                    .ok_or_else(|| format!("Non-UTF-8 folder path: {:?}", folder))?,
                &identity_choice,
                user_edit,
            )
            .await?;

        let emit_preparing = {
            let import_id = import_id.clone();
            let candidate_key = candidate_key.clone();
            let album_title = album_title.clone();
            let artist_name = artist_name.clone();
            let event_tx = self.event_tx.clone();
            move |step: PrepareStep| {
                send_event(
                    &event_tx,
                    crate::import::handle::ImportEvent::ImportProgress {
                        candidate_key: candidate_key.clone(),
                        progress: ImportProgress::Preparing {
                            import_id: import_id.clone(),
                            step,
                            album_title: album_title.clone(),
                            artist_name: artist_name.clone(),
                        },
                    },
                );
            }
        };

        step_times.push(("resolve_metadata", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        db_release.source_folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        db_release.content_hash = Some(content_hash);

        // Phase 0b: Remote cover art is downloaded through the
        // session-wide LRU cache. The UI may have pre-fetched the URL
        // at cover-select time; if so this is a cache hit. The download
        // function returns the content type from the HTTP response,
        // so no magic-byte sniffing is required here.
        let remote_cover_data =
            if let Some(CoverSelection::Remote(ref url, source)) = selected_cover {
                emit_preparing(PrepareStep::WritingCoverArt);
                let (bytes, content_type) =
                    crate::import::cover_art::download_cover_art_bytes(url).await?;
                if matches!(
                    content_type,
                    crate::util::content_type::ContentType::OctetStream
                ) {
                    return Err(
                        "Cover bytes aren't a recognized image format (PNG/JPEG/GIF/WebP/BMP)"
                            .to_string(),
                    );
                }
                Some((
                    std::sync::Arc::new(bytes),
                    content_type,
                    url.clone(),
                    source,
                ))
            } else {
                None
            };

        step_times.push(("write_cover_art", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        // Phase 0c: The folder walk that happened up front produced the
        // `categorized` set. Flatten it into the discovered-files list
        // the downstream pipeline consumes.
        emit_preparing(PrepareStep::DiscoveringFiles);
        let discovered_files = crate::import::handle::categorized_to_discovered_files(&categorized);

        step_times.push(("discover_files", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        // Phase 0d: Map tracks to files. The mapper consumes the `db_tracks`
        // Vec, moves each DbTrack into its TrackFile variant, and populates
        // `duration_ms` from the CUE sheet or a standalone-file probe. After
        // this point the DbTracks live inside `tracks_to_files`.
        emit_preparing(PrepareStep::ValidatingTracks);
        let tracks_to_files = map_tracks_to_files(db_tracks, &categorized).await?;

        // Resolve local cover path from discovered files
        let cover_image_path = match &selected_cover {
            Some(CoverSelection::Local(filename)) => discovered_files.iter().find_map(|f| {
                let path_str = f.path.to_string_lossy();
                if path_str.ends_with(filename) {
                    Some(f.path.clone())
                } else {
                    None
                }
            }),
            _ => None,
        };

        // Embedded cover art is the lowest-priority source: read it only
        // when the user made no explicit selection. `run_import` writes it
        // solely when no folder image is found either, so it never beats a
        // Remote/Local pick or a folder image. The picture is only present
        // on tagged rips, which is the Unknown path; Exact/Approximate
        // imports skip the read.
        let embedded_cover = if selected_cover.is_none()
            && matches!(identity_choice, crate::import::IdentityChoice::Unknown)
        {
            let audio_paths = crate::import::handle::categorized_audio_paths(&categorized);
            tokio::task::spawn_blocking(move || {
                crate::import::file_tag_mapper::read_embedded_cover(&audio_paths)
            })
            .await
            .map_err(|e| format!("embedded-cover read task failed: {e}"))?
        } else {
            None
        };

        step_times.push(("validate_tracks", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        emit_preparing(PrepareStep::SavingToDatabase);

        let metadata_now = library_manager.clock().now();
        let db_metadata: Vec<DbReleaseMetadata> = resolved_metadata
            .into_iter()
            .map(|(src, json)| {
                DbReleaseMetadata::new(
                    &db_release.id,
                    &src,
                    json,
                    library_manager.ids().new_id(),
                    metadata_now,
                )
            })
            .collect();

        let album_id = existing_album_id
            .as_deref()
            .unwrap_or(&db_album.id)
            .to_string();

        // Build the remote cover record + its bytes (no storage yet — the winning
        // cover's bytes are handed to coven's local store below, and its row is
        // written by finalize).
        let remote_cover_image: Option<(crate::db::DbLibraryImage, Vec<u8>)> =
            if let Some((bytes, content_type, url, source)) = remote_cover_data {
                let now = library_manager.clock().now();
                let bytes = bytes.to_vec();
                Some((
                    crate::db::DbLibraryImage {
                        id: db_release.id.clone(),
                        image_type: crate::db::LibraryImageType::Cover,
                        content_type,
                        file_size: bytes.len() as i64,
                        width: None,
                        height: None,
                        source: source.as_str().to_string(),
                        source_url: Some(url),
                        // finalize computes the readable cloud_path on a browsable
                        // home; NULL (hashed) on an opaque one.
                        cloud_path: None,
                        created_at: now,
                    },
                    bytes,
                ))
            } else {
                None
            };
        let remote_cover_set = remote_cover_image.is_some();

        step_times.push(("save_to_database", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        info!(
            "Prepared album '{}' (release: {}) with {} tracks",
            db_album.title,
            db_release.id,
            tracks_to_files.len()
        );

        // Phase 1+2: Storage (writes files to disk/cloud, builds DB records in memory)
        self.run_import(
            &storage_mode,
            pin,
            &mut db_release,
            &discovered_files,
            &tracks_to_files,
            cover_image_path.as_deref(),
            &import_id,
            &candidate_key,
            remote_cover_set,
            remote_cover_image,
            embedded_cover,
            &db_metadata,
            &remapped_track_artists,
            &remapped_album_artists,
            &work_graph,
            &remapped_release_artist_roles,
            &remapped_track_artist_roles,
            existing_album_id.is_none().then_some(&db_album),
            &album_id,
            &identities,
        )
        .await?;

        step_times.push(("storage", last_step_start.elapsed()));

        let total_duration = import_start.elapsed();
        let step_summary: Vec<String> = step_times
            .iter()
            .map(|(name, dur)| format!("{}={:.0?}", name, dur))
            .collect();
        info!(
            "Import timing for '{}': total={:.0?} [{}]",
            album_title,
            total_duration,
            step_summary.join(", ")
        );

        // Write trace to JSONL file when BAE_IMPORT_TRACE=1
        if std::env::var("BAE_IMPORT_TRACE").is_ok_and(|v| v == "1") {
            if let Some(home) = std::env::var_os("HOME") {
                let trace_dir = PathBuf::from(home).join(".bae-traces");
                if let Err(e) = std::fs::create_dir_all(&trace_dir) {
                    warn!("import trace dir {:?}: {}", trace_dir, e);
                }
                let trace_path = trace_dir.join("imports.jsonl");
                let steps: Vec<String> = step_times
                    .iter()
                    .map(|(name, dur)| format!("\"{}\":{}", name, dur.as_millis()))
                    .collect();
                let line = format!(
                    "{{\"ts\":\"{}\",\"import_id\":\"{}\",\"album\":\"{}\",\"artist\":\"{}\",\"total_ms\":{},\"steps\":{{{}}}}}\n",
                    library_manager.clock().now().to_rfc3339(),
                    import_id,
                    album_title.replace('\"', "\\\""),
                    artist_name.replace('\"', "\\\""),
                    total_duration.as_millis(),
                    steps.join(","),
                );
                match std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&trace_path)
                {
                    Ok(mut f) => {
                        use std::io::Write;
                        if let Err(e) = f.write_all(line.as_bytes()) {
                            warn!("import trace write {:?}: {}", trace_path, e);
                        }
                    }
                    Err(e) => warn!("import trace open {:?}: {}", trace_path, e),
                }
            }
        }

        Ok(())
    }

    /// Run an import. ONE path regardless of storage mode: read metadata, build
    /// DbFile + audio-format records, reference the files in place, finalize
    /// atomically as a LOCAL release (playable immediately), emit events.
    /// No bytes move here. If `storage_mode` is `Remote`, the release then
    /// transitions to the cloud via `coven_make_remote` (the same flow the
    /// "Manage" action runs), carrying `pin` as the upload's retain-pinned intent;
    /// coven flips `remote` true and deletes the in-place source once the last
    /// upload lands. `pin` is ignored for an `Local` import.
    ///
    /// All DB writes happen in one atomic transaction at the end. DbTracks —
    /// including their populated `duration_ms` — live inside `tracks_to_files`.
    #[allow(clippy::too_many_arguments)]
    async fn run_import(
        &self,
        storage_mode: &StorageMode,
        pin: bool,
        db_release: &mut DbRelease,
        discovered_files: &[DiscoveredFile],
        tracks_to_files: &[TrackFile],
        cover_image_path: Option<&Path>,
        import_id: &str,
        candidate_key: &str,
        remote_cover_set: bool,
        remote_cover_image: Option<(crate::db::DbLibraryImage, Vec<u8>)>,
        embedded_cover: Option<(Vec<u8>, crate::util::content_type::ContentType)>,
        db_metadata: &[DbReleaseMetadata],
        remapped_track_artists: &[crate::db::DbTrackArtist],
        remapped_album_artists: &[crate::db::DbAlbumArtist],
        work_graph: &ParsedWorkGraph,
        remapped_release_artist_roles: &[crate::db::DbReleaseArtistRole],
        remapped_track_artist_roles: &[crate::db::DbTrackArtistRole],
        new_album: Option<&crate::db::DbAlbum>,
        album_id: &str,
        identities: &[crate::import::types::ReleaseIdentity],
    ) -> Result<(), String> {
        let library_manager = &self.library_manager;
        let total_files = discovered_files.len();

        self.emit_started(candidate_key, &db_release.id, import_id);
        info!(
            "Starting {} import for release {} ({} files)",
            storage_mode_label(storage_mode),
            db_release.id,
            total_files,
        );

        // Build DbFile records. Keyed by absolute path (same key TrackFile uses)
        // so disc-subfolder siblings with identical bare filenames stay distinct.
        let files_now = library_manager.clock().now();
        let mut db_files: Vec<DbFile> = Vec::with_capacity(total_files);
        let mut file_ids: HashMap<PathBuf, String> = HashMap::new();
        for file in discovered_files.iter() {
            let db_file = DbFile::new(
                &db_release.id,
                &file.relative_path,
                file.size as i64,
                resolve_file_content_type(&file.path)?,
                library_manager.ids().new_id(),
                files_now,
            );
            file_ids.insert(file.path.clone(), db_file.id.clone());
            db_files.push(db_file);
        }

        // Every import lands LOCAL: reference the files in place and record
        // their common-ancestor folder as the release's local source. No bytes
        // move here in any mode. A Remote import then transitions to the cloud
        // (`coven_make_remote`, below); coven flips `remote` true once the upload
        // lands; until then it is a valid, playable local release, so another
        // device never sees a release before its audio is in the cloud.
        let local_root = {
            let mut ancestor: Option<&Path> = None;
            for file in discovered_files.iter() {
                let parent = file
                    .path
                    .parent()
                    .ok_or_else(|| format!("File has no parent: {:?}", file.path))?;
                ancestor = Some(match ancestor {
                    None => parent,
                    Some(a) => common_ancestor(a, parent),
                });
            }
            ancestor.ok_or_else(|| "No files to determine local path".to_string())?
        };
        let local_path = local_root
            .to_str()
            .ok_or_else(|| format!("Cannot convert path to string: {:?}", local_root))?
            .to_string();

        // Per-track progress jumps to 100% immediately — files are referenced in
        // place, no bytes move.
        let file_to_tracks: HashMap<PathBuf, Vec<String>> = {
            let mut map: HashMap<PathBuf, Vec<String>> = HashMap::new();
            for tf in tracks_to_files {
                map.entry(tf.file_path().to_path_buf())
                    .or_default()
                    .push(tf.db_track().id.clone());
            }
            map
        };
        for (idx, file) in discovered_files.iter().enumerate() {
            if let Some(track_ids) = file_to_tracks.get(&file.path) {
                for track_id in track_ids {
                    self.emit_phase_progress(
                        candidate_key,
                        track_id,
                        100,
                        ImportPhase::ReferencingFiles,
                        import_id,
                    );
                }
            }
            let release_percent = ((idx + 1) * 100 / total_files.max(1)) as u8;
            self.emit_phase_progress(
                candidate_key,
                &db_release.id,
                release_percent,
                ImportPhase::ReferencingFiles,
                import_id,
            );
            info!(
                "Recorded file {}/{}: {}",
                idx + 1,
                total_files,
                file.relative_path,
            );
        }

        // Audio formats, cover image, and finalize are identical across strategies.
        let mut audio_formats = Self::build_audio_formats(
            tracks_to_files,
            &file_ids,
            self.library_manager.clock().as_ref(),
            self.library_manager.ids().as_ref(),
        )?;

        // Measure loudness from the source decode — bae stores originals verbatim
        // (no transcode), so source samples == stored samples. The source files
        // are present in place (every import references them and lands local);
        // a remote import's uploads queue only after finalize. Per-track NULLs and
        // an album NULL are legitimate "not measured" results, each logged at the
        // skip point inside `measure_loudness`.
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        {
            self.emit_phase_progress(
                candidate_key,
                &db_release.id,
                0,
                ImportPhase::MeasuringLoudness,
                import_id,
            );
            let loudness = crate::import::loudness::measure_loudness(
                &self.event_tx,
                &mut audio_formats,
                tracks_to_files,
                candidate_key,
            )
            .await;
            db_release.album_loudness_lufs = loudness.album_loudness_lufs;
            db_release.album_peak_linear = loudness.album_peak_linear;

            // A track that failed to decode fully (fatal errors / truncated body)
            // would import fine but fail at play time. When verify is on, fail the
            // import now instead — before finalize commits anything to the library.
            if self.library_manager.get_config().verify_decode_on_import
                && !loudness.broken.is_empty()
            {
                return Err(format!(
                    "decode verification failed for {} track(s): {}",
                    loudness.broken.len(),
                    loudness.broken.join("; ")
                ));
            }
        }

        let local_cover_image = if !remote_cover_set {
            self.build_cover_image_record(&db_release.id, discovered_files, cover_image_path)?
        } else {
            None
        };

        // Embedded cover art is the last resort: use it only when no remote
        // selection and no folder image produced a cover. This keeps the priority
        // Remote/Local > folder image > embedded — a tagged rip with embedded art
        // but no sidecar image still gets a cover, but a folder image always wins.
        let embedded_cover_image: Option<(crate::db::DbLibraryImage, Vec<u8>)> =
            match embedded_cover {
                Some((bytes, content_type)) if !remote_cover_set && local_cover_image.is_none() => {
                    let now = library_manager.clock().now();
                    Some((
                        crate::db::DbLibraryImage {
                            id: db_release.id.clone(),
                            image_type: crate::db::LibraryImageType::Cover,
                            content_type,
                            file_size: bytes.len() as i64,
                            width: None,
                            height: None,
                            source: "embedded".to_string(),
                            source_url: None,
                            cloud_path: None,
                            created_at: now,
                        },
                        bytes,
                    ))
                }
                _ => None,
            };

        // The winning cover (Remote > Local folder image > embedded): finalize
        // writes its bytes and row in one coven batch.
        let cover_winner = remote_cover_image
            .or(local_cover_image)
            .or(embedded_cover_image);
        let library_image = cover_winner
            .as_ref()
            .map(|(image, bytes)| (image, bytes.as_slice()));
        let cover_rel_id = Some((album_id, db_release.id.as_str()));

        self.emit_phase_progress(
            candidate_key,
            &db_release.id,
            0,
            ImportPhase::Finalizing,
            import_id,
        );

        let remote_intent = matches!(storage_mode, StorageMode::Remote);
        let finalized_import_status = if remote_intent {
            ImportOperationStatus::Importing
        } else {
            ImportOperationStatus::Complete
        };
        library_manager
            .finalize_import_atomic(
                new_album,
                db_release,
                tracks_to_files,
                db_metadata,
                remapped_track_artists,
                remapped_album_artists,
                &work_graph.works,
                &work_graph.work_artists,
                &work_graph.work_parts,
                &work_graph.track_works,
                remapped_release_artist_roles,
                remapped_track_artist_roles,
                &db_files,
                &audio_formats,
                library_image,
                cover_rel_id,
                import_id,
                finalized_import_status,
                identities,
                &local_path,
            )
            .await
            .map_err(|e| format!("Failed to finalize import: {}", e))?;

        // A Remote import transitions to the cloud in the background — the same
        // flow the "Make Remote" action runs: coven uploads each file from its
        // external (in-place) source, and on the last flips `remote` true, drops
        // the external refs, and re-emits the subtree (the cover rides along). The
        // user's original files are referenced in place and left untouched — coven
        // never deletes a user-provided source. This runs BEFORE the events below so
        // the outbox already holds the upload by the time any consumer observes the
        // release or `Complete`.
        //
        if remote_intent {
            if let Err(e) = library_manager.coven_make_remote(&db_release.id, pin).await {
                let remote_error = format!(
                    "Remote import of {} could not start its cloud upload: {e}",
                    db_release.id
                );
                if let Err(import_error) = library_manager
                    .fail_import_and_delete_release(import_id, &db_release.id, &remote_error)
                    .await
                {
                    return Err(format!(
                        "{remote_error}; removing release and marking import failed failed: {import_error}"
                    ));
                }
                return Err(remote_error);
            }
        }

        if new_album.is_some() {
            library_manager.emit_album_added(album_id).await;
        } else {
            library_manager
                .emit_release_added(album_id, &db_release.id)
                .await;
        }

        let progress = if remote_intent {
            ImportProgress::RemoteUploadQueued {
                id: db_release.id.to_string(),
                import_id: import_id.to_string(),
                album_id: album_id.to_string(),
            }
        } else {
            ImportProgress::Complete {
                id: db_release.id.to_string(),
                import_id: import_id.to_string(),
                album_id: album_id.to_string(),
            }
        };
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.to_string(),
                progress,
            },
        );

        info!("Import complete for release {}", db_release.id);
        Ok(())
    }
}

/// Project the user's identity choice onto the mapper's identity vec.
///
/// The MB / Discogs mappers always emit Exact rows
/// (`source_release_id = Some`); the file-tag mapper emits an empty
/// vec, since Unknown imports make no identity claim.
///
/// For Approximate, NULL out `source_release_id` on every row — the
/// primary AND any cross-source row from MB↔Discogs url-rels mirror
/// the user's choice. A cross-source row's `source_release_id` follows
/// the same rule as the primary: present for Exact, NULL for
/// Approximate. The claim is at the group level for every row.
///
/// For Unknown, the mapper output is empty; this function returns an
/// empty vec straight through.
///
/// Lives at module level so the test below can exercise it directly.
pub(crate) fn apply_identity_choice(
    mapper_output: &[crate::import::ReleaseIdentity],
    choice: &crate::import::IdentityChoice,
) -> Vec<crate::import::ReleaseIdentity> {
    match choice {
        crate::import::IdentityChoice::Exact { .. } | crate::import::IdentityChoice::Unknown => {
            mapper_output.to_vec()
        }
        crate::import::IdentityChoice::Approximate { .. } => mapper_output
            .iter()
            .map(|id| crate::import::ReleaseIdentity {
                source: id.source,
                source_group_id: id.source_group_id.clone(),
                source_release_id: None,
            })
            .collect(),
    }
}

/// Apply the editor's overlay onto the seeded album/release/tracks.
///
/// Overwrites: album title (`db_album.title`), pressing fields on the
/// release (year/format/label/catalog_number/country/barcode), per-track
/// title/side/track_number.
///
/// Artist credits (`album_artists`, `track_artists`) are only rebuilt
/// when the edit's name strings differ from the seed's. When the user
/// hasn't touched the artist field, the original mapper-emitted rows
/// stay intact — preserving the source-id linkage (e.g.
/// `musicbrainz_artist_id`) that the mapper writes onto each `DbArtist`.
/// Comparison uses the same form-shape the editor renders: an empty
/// per-track list means "track shares the album artist," so a seeded
/// track whose credits exactly match the album credits (positionally,
/// case-insensitive) compares equal to an empty edit list.
///
/// Rebuilds resolve names against the existing `artists` vec, inserting
/// fresh `DbArtist` rows for previously-unseen names; the import
/// pipeline's `find_or_create_artists` canonicalizes them at DB-write
/// time. Inserted rows leave `musicbrainz_artist_id` /
/// `discogs_artist_id` as `None` — the user-introduced name has no
/// source binding to record.
///
/// Length mismatch on `tracks` is a structural error — the editor binds
/// to the seeded track list and never adds or removes rows.
fn apply_user_edit_to_seed(
    edit: &crate::import::ReleaseUserEdit,
    db_album: &mut crate::db::DbAlbum,
    db_release: &mut crate::db::DbRelease,
    db_tracks: &mut [crate::db::DbTrack],
    artists: &mut Vec<crate::db::DbArtist>,
    album_artists: &mut Vec<crate::db::DbAlbumArtist>,
    track_artists: &mut Vec<crate::db::DbTrackArtist>,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<(), String> {
    use crate::db::{DbAlbumArtist, DbArtist, DbTrackArtist};

    if edit.album_artist_names.is_empty() {
        return Err("Album must have at least one artist".to_string());
    }
    if edit.tracks.len() != db_tracks.len() {
        return Err(format!(
            "Track count mismatch: seed has {} tracks, edit supplies {}",
            db_tracks.len(),
            edit.tracks.len()
        ));
    }

    let now = clock.now();

    // Helper: resolve an artist name to an id, inserting a fresh
    // (source-id-free) `DbArtist` row when the name doesn't match an
    // existing one. Lookups are case-insensitive — the DB layer's
    // `find_or_create_artists` matches the same way.
    let ensure_artist = |artists: &mut Vec<DbArtist>, name: &str| -> String {
        if let Some(existing) = artists.iter().find(|a| a.name.eq_ignore_ascii_case(name)) {
            return existing.id.clone();
        }
        let new_artist = DbArtist {
            id: ids.new_id(),
            name: name.to_string(),
            sort_name: Some(name.to_string()),
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        let id = new_artist.id.clone();
        artists.push(new_artist);
        id
    };

    // Build the seed's album-artist name list (primary at [0], junction
    // rows by ascending position) so we can compare it to the edit's
    // list. Names come from the `artists` vec via the artist_ids on
    // `db_album.artist_id` / `album_artists`.
    let seeded_album_artist_names: Vec<String> = {
        let mut names = Vec::new();
        let primary = artists
            .iter()
            .find(|a| a.id == db_album.artist_id)
            .map(|a| a.name.clone())
            .ok_or_else(|| "primary album artist missing from artists vec".to_string())?;
        names.push(primary);
        let mut junction = album_artists.clone();
        junction.sort_by_key(|aa| aa.position);
        for aa in &junction {
            let name = artists
                .iter()
                .find(|a| a.id == aa.artist_id)
                .map(|a| a.name.clone())
                .ok_or_else(|| {
                    format!("album_artist references missing artist {}", aa.artist_id)
                })?;
            names.push(name);
        }
        names
    };

    db_album.title = edit.album_title.clone();
    db_album.artist_id = ensure_artist(artists, &edit.album_artist_names[0]);

    db_release.pressing = crate::db::Pressing {
        year: edit.pressing.year,
        format: edit.pressing.format.clone(),
        label: edit.pressing.label.clone(),
        catalog_number: edit.pressing.catalog_number.clone(),
        country: edit.pressing.country.clone(),
        barcode: edit.pressing.barcode.clone(),
    };

    for (track, t_edit) in db_tracks.iter_mut().zip(edit.tracks.iter()) {
        track.title = t_edit.title.clone();
        track.side = t_edit.side;
        track.track_number = t_edit.track_number;
    }

    // Album artists: only rebuild when the edit's names differ from the
    // seeded ones. Equality keeps the mapper's source-id-aware rows.
    if !names_equal(&seeded_album_artist_names, &edit.album_artist_names) {
        album_artists.clear();
        for (position, name) in edit.album_artist_names.iter().enumerate().skip(1) {
            let artist_id = ensure_artist(artists, name);
            album_artists.push(DbAlbumArtist::new(
                &db_album.id,
                &artist_id,
                position as i32,
                ids.new_id(),
                now,
            ));
        }
    }

    // Track artists: per-track, the editor's empty list means "share the
    // album artist." A seeded credit list that matches the album's list
    // round-trips through the editor as empty, so an empty edit
    // compares equal to such a seed. Anything else (different names,
    // different count) is a real change and rebuilds.
    for (track, t_edit) in db_tracks.iter().zip(edit.tracks.iter()) {
        let mut seeded: Vec<&DbTrackArtist> = track_artists
            .iter()
            .filter(|ta| ta.track_id == track.id)
            .collect();
        seeded.sort_by_key(|ta| ta.position);
        let seeded_names: Vec<String> = seeded
            .iter()
            .map(|ta| {
                artists
                    .iter()
                    .find(|a| a.id == ta.artist_id)
                    .map(|a| a.name.clone())
                    .ok_or_else(|| {
                        format!("track_artist references missing artist {}", ta.artist_id)
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let edit_names = &t_edit.artist_names;
        let unchanged = if edit_names.is_empty() {
            // Editor's "no override" form maps to either a literally
            // empty seed or a seed identical to the album's credits.
            seeded_names.is_empty() || names_equal(&seeded_names, &seeded_album_artist_names)
        } else {
            names_equal(&seeded_names, edit_names)
        };

        if !unchanged {
            track_artists.retain(|ta| ta.track_id != track.id);
            for (position, name) in edit_names.iter().enumerate() {
                let artist_id = ensure_artist(artists, name);
                track_artists.push(DbTrackArtist::new(
                    &track.id,
                    &artist_id,
                    position as i32,
                    ids.new_id(),
                    now,
                ));
            }
        }
    }

    Ok(())
}

/// Case-insensitive equality on lists of artist names. Matches the rule
/// `find_or_create_artists` uses for canonicalization, so two name lists
/// the DB would treat as identical compare equal here.
fn names_equal(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.eq_ignore_ascii_case(y))
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn common_ancestor<'a>(a: &'a Path, b: &Path) -> &'a Path {
    let mut longest = a;
    loop {
        if b.starts_with(longest) {
            return longest;
        }
        match longest.parent() {
            Some(parent) => longest = parent,
            None => return longest,
        }
    }
}

/// Fetch + parse + detail-build a release. Mirrors the handle's
/// Worker-side fetch + DB-shape mapping. Takes the bare `LibraryManager`
/// since the worker doesn't hold an `ImportServiceHandle`. Reads through
/// the session-wide MB/Discogs LRU caches; cache hit is the norm post-
/// prefetch. Used by the import worker (folder/CD) and by re-identify
/// to source the cross-linked identity vec for an existing release.
pub(crate) async fn prepare_release(
    library_manager: &LibraryManager,
    release_ref: &MetadataRef,
) -> Result<crate::import::folder_scanner::PreparedRelease, String> {
    match release_ref.source {
        MetadataSource::MusicBrainz => {
            let discogs_client = library_manager
                .discogs_client()
                .map_err(|e| format!("Failed to read Discogs key: {e}"))?;
            crate::import::search::commit_mb_release(
                library_manager,
                &release_ref.id,
                discogs_client.as_ref(),
            )
            .await
        }
        MetadataSource::Discogs => {
            let client = library_manager
                .discogs_client()
                .map_err(|e| format!("Failed to read Discogs key: {e}"))?
                .ok_or_else(|| "Discogs API key not configured".to_string())?;
            crate::import::search::commit_discogs_release(
                &client,
                &release_ref.id,
                library_manager.clock().as_ref(),
                library_manager.ids().as_ref(),
            )
            .await
            .map_err(|e| format!("Failed to fetch Discogs release: {e}"))
        }
    }
}

#[cfg(test)]
mod tests;
