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
        DbAlbum, DbAlbumArtist, DbFile, DbImport, DbRelease, DbReleaseMetadata, DbTrack,
        DbTrackArtist,
    },
    crate::import::folder_registry::ImportFolderRegistry,
    crate::import::folder_scanner::{
        scan_for_candidates_with_callback, InvalidCandidate, ScanItem,
    },
    crate::import::handle::{fetch_artist_images, remap_album_artists, remap_track_artists},
    crate::import::track_to_file_mapper::map_tracks_to_files,
    crate::import::types::{
        CoverSelection, CueAudioAnalysis, CueFlacAnalysis, DiscoveredFile, ImportPhase,
        PrepareStep, TrackFile,
    },
    crate::storage::local::ReleaseStorageImpl,
    crate::util::content_type::ContentType,
    crate::util::content_type_hint::ContentTypeHint,
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

/// Keyed by each file's absolute path — unique identity, no collisions even when
/// two files share a bare filename across subfolders (e.g. CD1/CDImage.ape and
/// CD2/CDImage.ape). Each entry lists (track_id, start_byte, end_byte) within
/// that file.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
type TrackProgressMap = HashMap<PathBuf, Vec<(String, i64, i64)>>;

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
    /// Per-source identity rows for the release. Empty for Unknown.
    /// Commit writes one `release_identities` row per element.
    identities: Vec<crate::import::types::ReleaseIdentity>,
    album_title: String,
    artist_name: String,
}

/// Resolve the probe-verified `ContentType` for a discovered file.
///
/// Audio files are probed (only the decoder knows which codec the container
/// holds — `.m4a` could be ALAC or AAC, and the extension alone can't tell).
/// Non-audio files get their `ContentType` derived from the extension hint:
/// that's an honest mapping for images (an extension on image bytes predicts
/// the codec), text, and PDF. Anything the hint can't classify becomes
/// `OctetStream`, which flows through the DB as-is.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn resolve_file_content_type(path: &Path) -> Result<ContentType, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| format!("File has no extension: {:?}", path))?;
    let hint = ContentTypeHint::from_extension(ext);

    if hint.is_audio() {
        let path_str = path
            .to_str()
            .ok_or_else(|| format!("Invalid path: {}", path.display()))?;
        let probe = crate::audio_codec::probe_audio_from_path(path_str)
            .ok_or_else(|| format!("Failed to probe audio file: {}", path.display()))?;
        return Ok(probe.content_type);
    }

    Ok(match hint {
        ContentTypeHint::Jpeg => ContentType::Jpeg,
        ContentTypeHint::Png => ContentType::Png,
        ContentTypeHint::Gif => ContentType::Gif,
        ContentTypeHint::Webp => ContentType::Webp,
        ContentTypeHint::Bmp => ContentType::Bmp,
        ContentTypeHint::Svg => ContentType::Svg,
        ContentTypeHint::PlainText => ContentType::PlainText,
        ContentTypeHint::Pdf => ContentType::Pdf,
        // Audio hints were handled above; anything else is unknown binary.
        _ => ContentType::OctetStream,
    })
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn storage_mode_label(mode: &StorageMode) -> &'static str {
    match mode {
        StorageMode::Managed { pin: true } => "managed (pin)",
        StorageMode::Managed { pin: false } => "managed (unpin)",
        StorageMode::Unmanaged => "unmanaged",
    }
}

/// Build an audio format for a track inside a CUE-backed file (shared FLAC or APE image).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn cue_backed_audio_format(
    db_track_id: &str,
    file_path: &Path,
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
    id: String,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<crate::db::DbAudioFormat, String> {
    use crate::db::DbAudioFormat;

    let cue_track = cue_pair.cue_sheet.tracks.get(cue_index).ok_or_else(|| {
        format!(
            "CUE track index {} out of bounds for {}",
            cue_index,
            file_path.display()
        )
    })?;

    let pregap_ms = cue_track
        .pregap_duration_ms()
        .filter(|&ms| ms > 0)
        .map(|ms| ms as i64);

    // Every CUE codec decodes its shared file natively and is trimmed to the
    // track's sample window -- one shape across FLAC, APE and ALAC.
    let fmt = cue_analysis_format(cue_pair);
    let start_sample = cue_track.audio_start_sample(fmt.sample_rate);
    let end_sample = cue_track.end_sample(fmt.sample_rate);
    Ok(DbAudioFormat::new(
        db_track_id,
        fmt.content_type,
        fmt.sample_rate as i64,
        fmt.bits_per_sample,
        fmt.channels as i64,
        start_sample as i64,
        end_sample.map(|s| s as i64),
        id,
        now,
    )
    .with_pregap(pregap_ms))
}

/// Build an audio format for a per-track file (FLAC, MP3, APE, etc.) via FFmpeg probe.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn standalone_probed_audio_format(
    db_track_id: &str,
    file_path: &Path,
    id: String,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<crate::db::DbAudioFormat, String> {
    use crate::db::DbAudioFormat;

    let path_str = file_path
        .to_str()
        .ok_or_else(|| format!("Invalid path: {}", file_path.display()))?;
    let probe = crate::audio_codec::probe_audio_from_path(path_str)
        .ok_or_else(|| format!("Failed to probe audio file: {}", file_path.display()))?;

    // A per-track file is its own whole-file window: (0, None) samples and the
    // default (0, None) byte span -- the whole file.
    Ok(DbAudioFormat::new(
        db_track_id,
        probe.content_type,
        probe.sample_rate as i64,
        probe.bits_per_sample.map(|b| b as i64),
        probe.channels as i64,
        0,
        None,
        id,
        now,
    ))
}

/// The audio format descriptor of a CUE-backed file, read once from its analysis
/// and shared by every caller that needs any of these fields, so the per-codec
/// extraction lives in one match instead of being repeated per field.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct CueAnalysisFormat {
    content_type: ContentType,
    sample_rate: u32,
    bits_per_sample: Option<i64>,
    channels: u32,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn cue_analysis_format(cue_pair: &CueFlacAnalysis) -> CueAnalysisFormat {
    match &cue_pair.analysis {
        CueAudioAnalysis::Flac { flac_info } => CueAnalysisFormat {
            content_type: ContentType::Flac,
            sample_rate: flac_info.sample_rate,
            bits_per_sample: Some(flac_info.bits_per_sample as i64),
            channels: flac_info.channels,
        },
        CueAudioAnalysis::Ape { ape_info } => CueAnalysisFormat {
            content_type: ContentType::Ape,
            sample_rate: ape_info.sample_rate,
            bits_per_sample: Some(ape_info.bits_per_sample as i64),
            channels: ape_info.channels as u32,
        },
        CueAudioAnalysis::Alac {
            sample_rate,
            channels,
            bits_per_sample,
            ..
        } => CueAnalysisFormat {
            content_type: ContentType::Alac,
            sample_rate: *sample_rate,
            bits_per_sample: bits_per_sample.map(|b| b as i64),
            channels: *channels,
        },
    }
}

/// Byte offset of each CUE track's end within the shared file, found by seeking
/// (no decode) -- computed once per file. `ends[i]` is track `i`'s end byte; the
/// last track has no entry and runs to EOF. The boundary is each track's
/// `end_sample` -- the same sample `cue_backed_audio_format` trims to -- so the
/// read-ahead ceiling and the decode window can't drift. `None` when the offsets
/// can't be read (non-UTF-8 path, a non-last track missing its end sample, or a
/// failed seek), distinct from a real empty result; the caller then spans the
/// whole file.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn cue_track_byte_ends(file_path: &Path, cue_pair: &CueFlacAnalysis) -> Option<Vec<u64>> {
    let sample_rate = cue_analysis_format(cue_pair).sample_rate;
    let tracks = &cue_pair.cue_sheet.tracks;
    // Seek to each non-last track's end sample (the last track runs to EOF, so it
    // has no end sample); every other track must have one.
    let end_samples = match tracks
        .iter()
        .take(tracks.len().saturating_sub(1))
        .map(|t| t.end_sample(sample_rate))
        .collect::<Option<Vec<u64>>>()
    {
        Some(samples) => samples,
        None => {
            warn!("cue_track_byte_ends: a non-last track has no end sample in {file_path:?}");
            return None;
        }
    };
    let Some(path) = file_path.to_str() else {
        warn!("cue_track_byte_ends: non-UTF-8 path, cannot seek byte offsets: {file_path:?}");
        return None;
    };
    crate::audio_codec::frame_byte_offsets(path, &end_samples)
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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

/// Send an import event on the broadcast bus, logging on send failure.
use crate::import::handle::send_event;

#[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
pub struct ImportService {
    commands_rx: mpsc::UnboundedReceiver<ImportCommand>,
    event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
    library_manager: LibraryManager,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
impl ImportService {
    pub fn start(
        _runtime_handle: tokio::runtime::Handle,
        library_manager: LibraryManager,
    ) -> ImportServiceHandle {
        let (commands_tx, _commands_rx) = mpsc::unbounded_channel();
        let (scan_tx, _scan_rx) = mpsc::unbounded_channel();
        let (event_tx, _) = broadcast::channel(1024);

        ImportServiceHandle::new(
            commands_tx,
            library_manager,
            _runtime_handle,
            scan_tx,
            event_tx,
        )
    }
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
        )
    }

    async fn do_import(&self, command: ImportCommand) {
        let ImportCommand::Folder {
            import_id,
            candidate_key,
            folder,
            selected_cover,
            storage_mode,
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

    /// Reconcile the prepared release against existing library state,
    /// record the import, and remap parsed artist IDs to their actual
    /// DB IDs.
    ///
    /// Input is the already-mapped `ParsedAlbum` plus the raw metadata
    /// pairs (empty for Unknown). The caller chooses the mapper based
    /// on identity choice — `prepare_release` for Exact / Approximate,
    /// `map_file_tags_to_db` for Unknown. This function does pure DB
    /// work and string remapping, no network.
    ///
    /// Applies the user's `identity_choice` post-process on top of the
    /// mapper output:
    ///
    /// - **Exact** — mapper output as-is. Identity rows keep their
    ///   `source_release_id`; pressing-level metadata (year, format,
    ///   label, catalog_number, country) seeds from the picked
    ///   release.
    /// - **Approximate** — identity rows get `source_release_id = None`
    ///   (cross-source rows from url-rels mirror the user's choice and
    ///   also become group-only); pressing-level metadata is cleared
    ///   so the release row reflects "user didn't claim a specific
    ///   pressing." Album-group-stable fields (title, artist, tracks)
    ///   stay populated from the picked release.
    /// - **Unknown** — mapper output (empty identity vec, file-tag
    ///   release fields) passes through. `metadata_source` is
    ///   `'file_tags'`; `metadata_source_release_id` stays NULL.
    ///   The album lookup is skipped (empty identities), so the
    ///   release lands on a fresh album.
    ///
    /// For Exact / Approximate, `metadata_source` and
    /// `metadata_source_release_id` are kept pointing at the picked
    /// release — the release records which source release seeded it
    /// regardless of identity claim. For Unknown those columns
    /// already arrive set by `map_file_tags_to_db`.
    ///
    /// `user_edit` is an optional overlay from the confirmation-page
    /// editor. Applied after the choice transformation so the user's
    /// edits win over the seeded values.
    ///
    /// Shared between folder and CD imports; callers handle the parts
    /// that differ (cover art, file discovery, ripping).
    async fn reconcile_prepared_release(
        &self,
        parsed: crate::import::ParsedAlbum,
        resolved_metadata: Vec<(String, String)>,
        import_id: &str,
        source_path: &str,
        identity_choice: &crate::import::IdentityChoice,
        user_edit: Option<crate::import::ReleaseUserEdit>,
    ) -> Result<PreparedMetadata, String> {
        let library_manager = &self.library_manager;

        let crate::import::ParsedAlbum {
            album: mut db_album,
            release: mut db_release,
            tracks: mut db_tracks,
            mut artists,
            mut album_artists,
            mut track_artists,
            identities: parsed_identities,
        } = parsed;

        // Apply the identity choice on top of the mapper's output.
        // Exact / Unknown preserve the mapper's per-source release IDs
        // (Unknown's vec is empty anyway); Approximate NULLs them and
        // clears the pressing-level cluster so the seeded record
        // reflects "user claimed an album, not a pressing."
        let identities = apply_identity_choice(&parsed_identities, identity_choice);
        if matches!(
            identity_choice,
            crate::import::IdentityChoice::Approximate { .. }
        ) {
            // `disc_id` is a signal-domain field, not part of the
            // pressing cluster — the import pipeline supplies it from
            // the user's actual LOG/CUE artifacts separately, so wiping
            // the source's value keeps the seeded record's signals
            // reflecting only the user's physical media.
            db_release.pressing = crate::db::Pressing::blank();
            db_release.disc_id = None;
        }

        // User-edit overlay: apply the user's edits on top of the seed.
        // Done here so reseeded fields (Approximate cleared) can still
        // be user-overridden via the editor before commit. Returns
        // updated db_album / db_release / db_tracks / album_artists /
        // track_artists / artists with new artist rows merged in.
        if let Some(edit) = user_edit {
            apply_user_edit_to_seed(
                &edit,
                &mut db_album,
                &mut db_release,
                &mut db_tracks,
                &mut artists,
                &mut album_artists,
                &mut track_artists,
                library_manager.clock().as_ref(),
                library_manager.ids().as_ref(),
            )?;
        }

        let album_title = db_album.title.clone();
        let artist_name = artists
            .iter()
            .find(|a| a.id == db_album.artist_id)
            .expect("primary artist must be in artists vec")
            .name
            .clone();

        let existing_album_id = library_manager
            .find_existing_album_for_import(&identities)
            .await?;
        if let Some(album_id) = &existing_album_id {
            db_release.album_id = album_id.clone();
        }

        let db_import = DbImport::new(
            import_id,
            &album_title,
            &artist_name,
            source_path,
            library_manager.clock().now(),
        );
        library_manager
            .insert_import(&db_import)
            .await
            .map_err(|e| format!("Failed to create import record: {}", e))?;

        let resolved = library_manager
            .find_or_create_artists(&artists)
            .await
            .map_err(|e| format!("Failed to resolve artists: {e}"))?;

        let artist_id_map: HashMap<String, String> = artists
            .iter()
            .zip(resolved.iter())
            .map(|(a, id)| (a.id.clone(), id.clone()))
            .collect();

        let remapped_primary_artist_id = artist_id_map
            .get(&db_album.artist_id)
            .ok_or_else(|| {
                format!(
                    "Primary artist ID {} not found in artist map",
                    db_album.artist_id
                )
            })?
            .clone();
        db_album.artist_id = remapped_primary_artist_id;

        let remapped_track_artists = remap_track_artists(&track_artists, &artist_id_map)?;
        let remapped_album_artists = if existing_album_id.is_none() {
            remap_album_artists(&album_artists, &artist_id_map)?
        } else {
            vec![]
        };

        let discogs_client = library_manager
            .discogs_client()
            .map_err(|e| format!("Failed to read Discogs key: {e}"))?;
        if let Some(ref discogs_client) = discogs_client {
            fetch_artist_images(library_manager, discogs_client, &artists, &artist_id_map).await;
        }

        Ok(PreparedMetadata {
            db_album,
            db_release,
            db_tracks,
            resolved_metadata,
            existing_album_id,
            remapped_track_artists,
            remapped_album_artists,
            identities,
            album_title,
            artist_name,
        })
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
                let audio_paths = crate::import::handle::categorized_audio_paths(&categorized);
                let folder_name = folder
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());
                let clock = library_manager.clock().clone();
                let ids = library_manager.ids().clone();
                let parsed = tokio::task::spawn_blocking(move || {
                    crate::import::file_tag_mapper::map_file_tags_to_db(
                        &audio_paths,
                        folder_name.as_deref(),
                        clock.as_ref(),
                        ids.as_ref(),
                    )
                })
                .await
                .map_err(|e| format!("file-tag mapping task failed: {e}"))??;
                (parsed, Vec::new())
            }
        };

        // Overwrite a prior import of the same folder tree: re-importing
        // replaces rather than duplicates. Remove the existing release(s)
        // carrying this content hash (and their managed files, via the library
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

        // Write remote cover to disk (filesystem only, no DB yet)
        let remote_cover_image = if let Some((bytes, content_type, url, source)) = remote_cover_data
        {
            let image_path = library_manager.image_path(&db_release.id);
            if let Some(parent) = image_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create images directory: {}", e))?;
            }
            std::fs::write(&image_path, bytes.as_slice())
                .map_err(|e| format!("Failed to write cover: {}", e))?;

            info!("Wrote remote cover art to {}", image_path.display());
            let now = library_manager.clock().now();
            Some(crate::db::DbLibraryImage {
                id: db_release.id.clone(),
                image_type: crate::db::LibraryImageType::Cover,
                content_type,
                file_size: bytes.len() as i64,
                width: None,
                height: None,
                source: source.as_str().to_string(),
                source_url: Some(url),
                // The import's finalize transaction computes the readable
                // cloud_path under a browsable home (it needs the album/artist
                // rows it inserts); it stays NULL on an opaque one.
                cloud_path: None,
                created_at: now,
            })
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

    /// Create a storage implementation for managed local storage.
    fn create_storage(&self) -> ReleaseStorageImpl {
        self.library_manager.create_release_storage()
    }

    /// Approximate the byte span of a CUE track within a single-blob file by
    /// linear interpolation: `position / total * file_size`. Import progress only
    /// -- it decides when a track's slice of the file copy reaches 100%, so an
    /// interpolation is good enough. `start`, `end`, and `total` share one unit
    /// (samples for FLAC/APE, milliseconds for ALAC); a `None` end runs to EOF.
    fn proportional_byte_span(
        start: f64,
        end: Option<f64>,
        total: f64,
        file_size: u64,
    ) -> (i64, i64) {
        if total <= 0.0 {
            return (0, file_size as i64);
        }
        let scale = |p: f64| (p / total * file_size as f64) as i64;
        let end = match end {
            Some(e) => scale(e),
            None => file_size as i64,
        };
        (scale(start), end)
    }

    /// Build a map from filename to track progress info for progress reporting.
    ///
    /// CUE-backed tracks already know their `cue_pair` + `cue_index`, so byte
    /// ranges come straight from the attached analysis. Standalone tracks span
    /// the whole file (0 → file_size).
    ///
    /// Keyed by absolute file path so disc subfolder siblings (e.g. CD1/CDImage.ape
    /// and CD2/CDImage.ape) don't collapse into one entry.
    fn build_track_progress_map(
        tracks_to_files: &[TrackFile],
        discovered_files: &[DiscoveredFile],
    ) -> Result<TrackProgressMap, String> {
        let mut result: TrackProgressMap = HashMap::new();
        let file_sizes: HashMap<&Path, usize> = discovered_files
            .iter()
            .map(|f| (f.path.as_path(), f.size as usize))
            .collect();

        for track_file in tracks_to_files {
            match track_file {
                TrackFile::CueBacked {
                    db_track,
                    file_path,
                    cue_pair,
                    cue_index,
                } => {
                    let cue_track = cue_pair.cue_sheet.tracks.get(*cue_index).ok_or_else(|| {
                        format!(
                            "CUE index {} out of bounds for {}",
                            cue_index,
                            file_path.display()
                        )
                    })?;
                    let (start_byte, end_byte) = match &cue_pair.analysis {
                        // Progress only: approximate each track's byte span from
                        // its sample/time window -- the file is one blob, so this
                        // just decides when a track's progress hits 100%.
                        CueAudioAnalysis::Flac { flac_info } => {
                            let file_size =
                                *file_sizes.get(file_path.as_path()).ok_or_else(|| {
                                    format!("file_sizes missing entry for {}", file_path.display())
                                })?;
                            Self::proportional_byte_span(
                                cue_track.audio_start_sample(flac_info.sample_rate) as f64,
                                cue_track
                                    .end_sample(flac_info.sample_rate)
                                    .map(|s| s as f64),
                                flac_info.total_samples as f64,
                                file_size as u64,
                            )
                        }
                        CueAudioAnalysis::Ape { ape_info } => Self::proportional_byte_span(
                            cue_track.audio_start_sample(ape_info.sample_rate) as f64,
                            cue_track.end_sample(ape_info.sample_rate).map(|s| s as f64),
                            ape_info.total_samples as f64,
                            ape_info.file_size,
                        ),
                        CueAudioAnalysis::Alac { duration_ms, .. } => {
                            // MP4 has no sample window here; interpolate from the
                            // CUE times against the container duration instead.
                            let file_size =
                                *file_sizes.get(file_path.as_path()).ok_or_else(|| {
                                    format!("file_sizes missing entry for {}", file_path.display())
                                })?;
                            Self::proportional_byte_span(
                                cue_track.start_time_ms() as f64,
                                cue_track.end_time_ms().map(|e| e as f64),
                                *duration_ms as f64,
                                file_size as u64,
                            )
                        }
                    };
                    result.entry(file_path.clone()).or_default().push((
                        db_track.id.clone(),
                        start_byte,
                        end_byte,
                    ));
                }
                TrackFile::Standalone {
                    db_track,
                    file_path,
                } => {
                    let file_size = *file_sizes.get(file_path.as_path()).ok_or_else(|| {
                        format!("file_sizes missing entry for {}", file_path.display())
                    })? as i64;
                    result.entry(file_path.clone()).or_default().push((
                        db_track.id.clone(),
                        0,
                        file_size,
                    ));
                }
            }
        }

        Ok(result)
    }

    /// Run an import, regardless of storage mode.
    ///
    /// The pipeline is the same for managed and unmanaged imports: read metadata,
    /// build DbFile + audio-format records, finalize atomically, emit events.
    /// Only byte placement differs per `StorageMode`:
    ///   - `Managed { pin: true }`: stream each source file into local storage via
    ///     `store_from_path`, report per-track Store progress from the write callback,
    ///     queue cloud upload.
    ///   - `Managed { pin: false }`: skip local storage, queue cloud upload with `source`.
    ///   - `Unmanaged`: record `unmanaged_path` and reference files in place; no cloud
    ///     upload since we don't own the files.
    ///
    /// All DB writes happen in one atomic transaction at the end. DbTracks —
    /// including their populated `duration_ms` — live inside `tracks_to_files`.
    #[allow(clippy::too_many_arguments)]
    async fn run_import(
        &self,
        storage_mode: &StorageMode,
        db_release: &mut DbRelease,
        discovered_files: &[DiscoveredFile],
        tracks_to_files: &[TrackFile],
        cover_image_path: Option<&Path>,
        import_id: &str,
        candidate_key: &str,
        remote_cover_set: bool,
        remote_cover_image: Option<crate::db::DbLibraryImage>,
        embedded_cover: Option<(Vec<u8>, crate::util::content_type::ContentType)>,
        db_metadata: &[DbReleaseMetadata],
        remapped_track_artists: &[crate::db::DbTrackArtist],
        remapped_album_artists: &[crate::db::DbAlbumArtist],
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

        // Strategy-specific byte placement and Store-phase progress reporting.
        // Each arm records this device's local copy (a managed pin or an in-place
        // unmanaged path; a managed cloud-only import keeps no local copy). A
        // managed import lands `managed = false` and only reaches `managed = true`
        // once its uploads finish (via `ReleaseUploadObserver`), so another device
        // never sees the release before its audio is in the cloud. An unmanaged import
        // stays `managed = false` for good.
        let mut local_copy: Option<crate::db::DbReleaseLocalCopy> = None;
        match storage_mode {
            StorageMode::Managed { pin: true } => {
                let storage = self.create_storage();
                let file_to_tracks =
                    Self::build_track_progress_map(tracks_to_files, discovered_files)?;
                let release_total_bytes: usize =
                    discovered_files.iter().map(|f| f.size as usize).sum();
                let mut release_bytes_written = 0usize;
                let import_id_owned = import_id.to_string();
                let candidate_key_owned = candidate_key.to_string();

                for (idx, file) in discovered_files.iter().enumerate() {
                    let file_id = &file_ids[&file.path];
                    let track_infos = file_to_tracks.get(&file.path).cloned().unwrap_or_default();
                    let event_tx = self.event_tx.clone();
                    let release_id = db_release.id.clone();
                    let import_id_for_closure = import_id_owned.clone();
                    let candidate_key_for_closure = candidate_key_owned.clone();
                    let base_bytes = release_bytes_written;
                    let file_size = file.size as usize;

                    storage
                        .store_from_path(
                            file_id,
                            &file.path,
                            Box::new(move |file_bytes_written, _file_total| {
                                let bytes_written = file_bytes_written as i64;
                                for (track_id, start_byte, end_byte) in &track_infos {
                                    if bytes_written > *start_byte {
                                        let percent = calculate_track_percent(
                                            file_bytes_written,
                                            *start_byte,
                                            *end_byte,
                                        );
                                        send_event(
                                            &event_tx,
                                            crate::import::handle::ImportEvent::ImportProgress {
                                                candidate_key: candidate_key_for_closure.clone(),
                                                progress: ImportProgress::Progress {
                                                    id: track_id.clone(),
                                                    percent,
                                                    phase: Some(ImportPhase::Store),
                                                    import_id: Some(import_id_for_closure.clone()),
                                                },
                                            },
                                        );
                                    }
                                }
                                let total_written = base_bytes + file_bytes_written;
                                let release_percent =
                                    (total_written * 100 / release_total_bytes.max(1)) as u8;
                                send_event(
                                    &event_tx,
                                    crate::import::handle::ImportEvent::ImportProgress {
                                        candidate_key: candidate_key_for_closure.clone(),
                                        progress: ImportProgress::Progress {
                                            id: release_id.clone(),
                                            percent: release_percent,
                                            phase: Some(ImportPhase::Store),
                                            import_id: Some(import_id_for_closure.clone()),
                                        },
                                    },
                                );
                            }),
                        )
                        .await
                        .map_err(|e| {
                            format!("Failed to store file {}: {}", file.relative_path, e)
                        })?;

                    release_bytes_written += file_size;
                    info!(
                        "Stored file {}/{}: {} ({} bytes)",
                        idx + 1,
                        total_files,
                        file.relative_path,
                        file_size,
                    );
                }
                // The pinned local copy; `managed` flips true once uploads finish.
                local_copy = Some(crate::db::DbReleaseLocalCopy {
                    release_id: db_release.id.clone(),
                    unmanaged_path: None,
                    pinned_locally: true,
                });
            }
            StorageMode::Managed { pin: false } => {
                // No local storage write; emit release-level progress per file so the
                // UI can advance while cloud uploads are queued later.
                for (idx, file) in discovered_files.iter().enumerate() {
                    let release_percent = ((idx + 1) * 100 / total_files.max(1)) as u8;
                    self.emit_store_progress(
                        candidate_key,
                        &db_release.id,
                        release_percent,
                        import_id,
                    );
                    info!(
                        "Recorded file {}/{}: {}",
                        idx + 1,
                        total_files,
                        file.relative_path,
                    );
                }
                // Managed cloud-only: no local copy on this device; `managed`
                // flips true once uploads finish.
            }
            StorageMode::Unmanaged => {
                // Record unmanaged_path from the files' common parent directory.
                let unmanaged_root = {
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
                    ancestor.ok_or_else(|| "No files to determine unmanaged path".to_string())?
                };
                let unmanaged_path = unmanaged_root
                    .to_str()
                    .ok_or_else(|| format!("Cannot convert path to string: {:?}", unmanaged_root))?
                    .to_string();
                local_copy = Some(crate::db::DbReleaseLocalCopy {
                    release_id: db_release.id.clone(),
                    unmanaged_path: Some(unmanaged_path),
                    pinned_locally: false,
                });

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
                            self.emit_store_progress(candidate_key, track_id, 100, import_id);
                        }
                    }
                    let release_percent = ((idx + 1) * 100 / total_files.max(1)) as u8;
                    self.emit_store_progress(
                        candidate_key,
                        &db_release.id,
                        release_percent,
                        import_id,
                    );
                    info!(
                        "Recorded file {}/{}: {}",
                        idx + 1,
                        total_files,
                        file.relative_path,
                    );
                }
            }
        }

        // Audio formats, cover image, and finalize are identical across strategies.
        let audio_formats = Self::build_audio_formats(
            tracks_to_files,
            &file_ids,
            self.library_manager.clock().as_ref(),
            self.library_manager.ids().as_ref(),
        )?;

        let local_cover_image = if !remote_cover_set {
            self.build_cover_image_record(&db_release.id, discovered_files, cover_image_path)?
        } else {
            None
        };

        // Embedded cover art is the last resort: write it only when no
        // remote selection and no folder image produced a cover. This
        // keeps the priority Remote/Local > folder image > embedded — a
        // tagged rip with embedded art but no sidecar image still gets a
        // cover, but a folder image always wins when present.
        let embedded_cover_image = match embedded_cover {
            Some((bytes, content_type)) if !remote_cover_set && local_cover_image.is_none() => {
                let image_path = library_manager.image_path(&db_release.id);
                if let Some(parent) = image_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create images directory: {e}"))?;
                }
                std::fs::write(&image_path, bytes.as_slice())
                    .map_err(|e| format!("Failed to write embedded cover: {e}"))?;
                info!("Wrote embedded cover art to {}", image_path.display());
                let now = library_manager.clock().now();
                Some(crate::db::DbLibraryImage {
                    id: db_release.id.clone(),
                    image_type: crate::db::LibraryImageType::Cover,
                    content_type,
                    file_size: bytes.len() as i64,
                    width: None,
                    height: None,
                    source: "embedded".to_string(),
                    source_url: None,
                    // Computed in the finalize transaction (see above).
                    cloud_path: None,
                    created_at: now,
                })
            }
            _ => None,
        };

        let library_image = remote_cover_image
            .as_ref()
            .or(local_cover_image.as_ref())
            .or(embedded_cover_image.as_ref());
        let cover_rel_id = Some((album_id, db_release.id.as_str()));

        // Managed imports queue one cloud upload per file, committed inside
        // the finalize transaction: a pin reads its staged `storage/` copy,
        // a cloud-only import reads the user's original file. Unmanaged
        // files are referenced in place and never upload.
        let cloud_uploads: Vec<crate::db::DbCloudUpload> = match storage_mode {
            StorageMode::Managed { pin } => discovered_files
                .iter()
                .map(|file| crate::db::DbCloudUpload {
                    file_id: file_ids[&file.path].clone(),
                    source_path: if *pin {
                        None
                    } else {
                        Some(file.path.to_string_lossy().to_string())
                    },
                })
                .collect(),
            StorageMode::Unmanaged => Vec::new(),
        };

        library_manager
            .finalize_import_atomic(
                new_album,
                db_release,
                tracks_to_files,
                db_metadata,
                remapped_track_artists,
                remapped_album_artists,
                &db_files,
                &audio_formats,
                library_image,
                cover_rel_id,
                import_id,
                identities,
                local_copy.as_ref(),
                &cloud_uploads,
            )
            .await
            .map_err(|e| format!("Failed to finalize import: {}", e))?;

        if new_album.is_some() {
            library_manager.emit_album_added(album_id).await;
        } else {
            library_manager
                .emit_release_added(album_id, &db_release.id)
                .await;
        }

        // The upload rows were committed with the import; surface them to the
        // outbox UI and kick the sync loop to start draining.
        if !cloud_uploads.is_empty() {
            library_manager.emit_outbox_changed().await;
            library_manager.trigger_sync();
        }

        // Emit Complete after the album/release events: the uploads were
        // committed atomically with the import, so Complete always implies
        // the cloud upload is queued.
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.to_string(),
                progress: ImportProgress::Complete {
                    id: db_release.id.to_string(),
                    import_id: import_id.to_string(),
                    album_id: album_id.to_string(),
                },
            },
        );

        info!("Import complete for release {}", db_release.id);
        Ok(())
    }

    fn emit_started(&self, candidate_key: &str, release_id: &str, import_id: &str) {
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.to_string(),
                progress: ImportProgress::Started {
                    id: release_id.to_string(),
                    import_id: Some(import_id.to_string()),
                },
            },
        );
    }

    fn emit_store_progress(&self, candidate_key: &str, id: &str, percent: u8, import_id: &str) {
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.to_string(),
                progress: ImportProgress::Progress {
                    id: id.to_string(),
                    percent,
                    phase: Some(ImportPhase::Store),
                    import_id: Some(import_id.to_string()),
                },
            },
        );
    }

    /// Build a cover image record from local files without writing to DB.
    /// Copies the cover file to the images cache directory and returns the
    /// DbLibraryImage record and the (album_id, release_id) pair for primary_release_id.
    #[allow(clippy::type_complexity)]
    fn build_cover_image_record(
        &self,
        release_id: &str,
        discovered_files: &[DiscoveredFile],
        cover_image_path: Option<&Path>,
    ) -> Result<Option<crate::db::DbLibraryImage>, String> {
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

        // Determine which file is the cover: match by absolute path if provided
        let cover_index = if let Some(selected_path) = cover_image_path {
            let found = image_files
                .iter()
                .position(|(f, _)| f.path.as_path() == selected_path);
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

        let cover_index = cover_index.unwrap_or_else(|| {
            image_files.sort_by(|(_, a), (_, b)| {
                let a_priority = Self::image_cover_priority(a);
                let b_priority = Self::image_cover_priority(b);
                a_priority.cmp(&b_priority)
            });
            0
        });

        let (cover_file, relative_path) = &image_files[cover_index];
        let content_type = resolve_file_content_type(&cover_file.path)?;
        let source_url = format!("release://{}", relative_path);

        // Copy cover to images cache
        let cache_path = self.library_manager.image_path(release_id);
        if let Some(parent) = cache_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                error!("Failed to create images directory: {}", e);
                return Ok(None);
            }
        }
        if let Err(e) = std::fs::copy(&cover_file.path, &cache_path) {
            error!("Failed to cache cover art: {}", e);
            return Ok(None);
        }

        let file_size = std::fs::metadata(&cache_path)
            .map(|m| m.len() as i64)
            .map_err(|e| format!("Failed to read cover art metadata: {e}"))?;

        info!("Cached cover art to {}", cache_path.display());

        let now = self.library_manager.clock().now();
        let db_image = DbLibraryImage {
            id: release_id.to_string(),
            image_type: LibraryImageType::Cover,
            content_type,
            file_size,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: Some(source_url),
            // Computed in the finalize transaction under a browsable home.
            cloud_path: None,
            created_at: now,
        };

        Ok(Some(db_image))
    }

    fn image_cover_priority(filename: &str) -> u8 {
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

    /// Build audio format records for all tracks. CUE-backed tracks already hold
    /// their shared analysis + index; standalone tracks are probed here.
    fn build_audio_formats(
        tracks_to_files: &[TrackFile],
        file_ids: &HashMap<PathBuf, String>,
        clock: &dyn crate::clock::Clock,
        ids: &dyn crate::id_provider::IdProvider,
    ) -> Result<Vec<crate::db::DbAudioFormat>, String> {
        let now = clock.now();
        let mut audio_formats = Vec::with_capacity(tracks_to_files.len());
        // Track end byte offsets per shared CUE file, computed once and reused
        // across that file's tracks (one ffmpeg open per file, not per track).
        // `None` means the offsets couldn't be read for that file.
        let mut cue_ends_by_file: HashMap<PathBuf, Option<Vec<u64>>> = HashMap::new();

        for track_file in tracks_to_files {
            // Each track carries the absolute path to its source file; that path
            // is the `file_ids` key — no bare-filename lookup that could collide
            // across disc subfolders.
            let file_id = file_ids.get(track_file.file_path()).ok_or_else(|| {
                format!(
                    "No DbFile registered for track source {:?}",
                    track_file.file_path()
                )
            })?;

            let format = match track_file {
                TrackFile::CueBacked {
                    db_track,
                    file_path,
                    cue_pair,
                    cue_index,
                } => {
                    let af = cue_backed_audio_format(
                        &db_track.id,
                        file_path,
                        cue_pair,
                        *cue_index,
                        ids.new_id(),
                        now,
                    )?;
                    // The last track (no entry) runs to EOF, as does any track
                    // whose offsets couldn't be read -- both keep the default
                    // whole-file span.
                    let ends = cue_ends_by_file.entry(file_path.clone()).or_insert_with(|| {
                        let computed = cue_track_byte_ends(file_path, cue_pair);
                        if computed.is_none() {
                            warn!(
                                "track byte offsets unavailable for {:?}; its tracks keep the whole-file read-ahead span",
                                file_path
                            );
                        }
                        computed
                    });
                    let end_byte = ends
                        .as_ref()
                        .and_then(|e| e.get(*cue_index))
                        .map(|&b| b as i64);
                    af.with_end_byte(end_byte)
                }
                TrackFile::Standalone {
                    db_track,
                    file_path,
                } => standalone_probed_audio_format(&db_track.id, file_path, ids.new_id(), now)?,
            };

            audio_formats.push(format.with_file_id(file_id));
        }

        Ok(audio_formats)
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
    clock: &dyn crate::clock::Clock,
    ids: &dyn crate::id_provider::IdProvider,
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
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use crate::id_provider::SequentialIdProvider;

    #[test]
    fn affected_roots_maps_changed_paths_to_their_watched_roots() {
        let root_a = PathBuf::from("/music/new rips");
        let root_b = PathBuf::from("/downloads/bandcamp");
        let roots = vec![root_a.clone(), root_b.clone()];

        // A change inside one root flags only that root.
        let changed = [Path::new("/music/new rips/Album/01.flac")];
        assert_eq!(affected_roots(&changed, &roots), vec![root_a.clone()]);

        // Changes under both roots flag both, in roots order, deduped.
        let changed = [
            Path::new("/downloads/bandcamp/X/cover.jpg"),
            Path::new("/music/new rips/Y"),
            Path::new("/music/new rips/Z"),
        ];
        assert_eq!(affected_roots(&changed, &roots), vec![root_a, root_b]);

        // A change outside every watched root flags nothing.
        let changed = [Path::new("/elsewhere/file")];
        assert!(affected_roots(&changed, &roots).is_empty());
    }

    /// `common_ancestor` derives the unmanaged-path root by folding over the
    /// files' parent dirs. It must compare path components, not string
    /// prefixes, so `/m/Album` and `/m/Album2` collapse to `/m` (a string
    /// prefix would wrongly keep `/m/Album`), and an ancestor argument returns
    /// itself rather than descending.
    #[test]
    fn common_ancestor_cases() {
        use std::path::Path;
        // Sibling files share their parent.
        assert_eq!(
            common_ancestor(Path::new("/m/Album/01.flac"), Path::new("/m/Album/02.flac")),
            Path::new("/m/Album")
        );
        // `a` is already an ancestor of `b`: keep `a`.
        assert_eq!(
            common_ancestor(Path::new("/m/Album"), Path::new("/m/Album/Disc1/01.flac")),
            Path::new("/m/Album")
        );
        // Component-wise, not string-prefix: Album vs Album2 don't share /m/Album.
        assert_eq!(
            common_ancestor(Path::new("/m/Album/x"), Path::new("/m/Album2/y")),
            Path::new("/m")
        );
        // Disjoint trees collapse to the root.
        assert_eq!(
            common_ancestor(Path::new("/a/b"), Path::new("/c/d")),
            Path::new("/")
        );
    }

    /// image_cover_priority decides which folder image wins as the cover when
    /// the user makes no explicit pick: a name containing "cover" or "front"
    /// (case-insensitive, anywhere in the name) ranks first, everything else
    /// second. The fallback sort relies on this ordering.
    #[test]
    fn image_cover_priority_ranks_front_and_cover_first() {
        assert_eq!(ImportService::image_cover_priority("Cover.jpg"), 0);
        assert_eq!(ImportService::image_cover_priority("front.png"), 0);
        assert_eq!(ImportService::image_cover_priority("FRONT.JPG"), 0);
        assert_eq!(
            ImportService::image_cover_priority("album-front-scan.jpg"),
            0
        );
        assert_eq!(ImportService::image_cover_priority("Back.jpg"), 1);
        assert_eq!(ImportService::image_cover_priority("inlay.png"), 1);
        assert_eq!(ImportService::image_cover_priority("disc1.jpg"), 1);
    }

    /// Deterministic clock for the `apply_user_edit_to_seed` tests — the
    /// exact instant is immaterial to what they assert (artist-row
    /// preservation / rebuild), only that the same one feeds every row.
    fn test_clock() -> FixedClock {
        FixedClock(
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
    }

    // ── apply_identity_choice ──────────────────────────────────────────

    fn mb_id_exact(group: &str, release: &str) -> crate::import::ReleaseIdentity {
        crate::import::ReleaseIdentity {
            source: crate::import::MetadataSource::MusicBrainz,
            source_group_id: group.to_string(),
            source_release_id: Some(release.to_string()),
        }
    }

    fn discogs_id_exact(group: &str, release: &str) -> crate::import::ReleaseIdentity {
        crate::import::ReleaseIdentity {
            source: crate::import::MetadataSource::Discogs,
            source_group_id: group.to_string(),
            source_release_id: Some(release.to_string()),
        }
    }

    fn mb_release_ref() -> crate::import::MetadataRef {
        crate::import::MetadataRef::new("rel-mb", crate::import::MetadataSource::MusicBrainz)
    }

    #[test]
    fn exact_choice_passes_mapper_output_through() {
        let mapper_output = vec![
            mb_id_exact("rg-mb", "rel-mb"),
            discogs_id_exact("master-d", "rel-d"),
        ];
        let result = apply_identity_choice(
            &mapper_output,
            &crate::import::IdentityChoice::Exact {
                release_ref: mb_release_ref(),
            },
        );
        assert_eq!(result, mapper_output);
    }

    #[test]
    fn approximate_choice_nulls_release_ids_on_every_row() {
        // Both the primary identity row AND any cross-source row from
        // url-rels mirror the user's choice — Approximate means a
        // group-level claim across the board.
        let mapper_output = vec![
            mb_id_exact("rg-mb", "rel-mb"),
            discogs_id_exact("master-d", "rel-d"),
        ];
        let result = apply_identity_choice(
            &mapper_output,
            &crate::import::IdentityChoice::Approximate {
                release_ref: mb_release_ref(),
            },
        );
        assert_eq!(result.len(), 2);
        for id in &result {
            assert!(
                id.source_release_id.is_none(),
                "Approximate must NULL source_release_id, got {id:?}"
            );
        }
        // Group IDs survive — the claim is at the group level.
        assert_eq!(result[0].source_group_id, "rg-mb");
        assert_eq!(result[1].source_group_id, "master-d");
    }

    #[test]
    fn unknown_choice_passes_empty_mapper_output_through() {
        // The file-tag mapper emits an empty identity vec; the choice
        // post-process is a no-op. Confirms Unknown writes zero
        // `release_identities` rows even when paired with a mapper
        // that somehow surfaces rows (defensive — file_tag_mapper
        // never does, but the projection is the algebraic identity).
        let result = apply_identity_choice(&[], &crate::import::IdentityChoice::Unknown);
        assert!(result.is_empty());
    }

    // ── apply_user_edit_to_seed ────────────────────────────────────────

    fn make_seed_album_release_track() -> (
        crate::db::DbAlbum,
        crate::db::DbRelease,
        crate::db::DbTrack,
        crate::db::DbArtist,
    ) {
        let now = chrono::Utc::now();
        let artist = crate::db::DbArtist {
            id: "artist-orig".to_string(),
            name: "Artist Name".to_string(),
            sort_name: Some("Artist Name".to_string()),
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        let album = crate::db::DbAlbum {
            id: "album-1".to_string(),
            title: "Album Title".to_string(),
            artist_id: artist.id.clone(),
            year: Some(2020),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = crate::db::DbRelease {
            id: "release-1".to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: crate::db::Pressing {
                year: Some(2020),
                format: Some("CD".to_string()),
                label: Some("Label Name".to_string()),
                catalog_number: Some("CAT-001".to_string()),
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
            metadata_source_release_id: Some("rel-mb".to_string()),
            managed: true,
            source_folder_name: None,
            content_hash: None,
            created_at: now,
        };
        let track = crate::db::DbTrack {
            id: "track-1".to_string(),
            release_id: release.id.clone(),
            title: "Original Title".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: Some(180000),
            discogs_position: None,
            created_at: now,
        };
        (album, release, track, artist)
    }

    #[test]
    fn user_edit_overrides_seeded_pressing_fields() {
        let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
        let mut tracks = vec![track];
        let mut artists = vec![seed_artist];
        let mut album_artists = Vec::new();
        let mut track_artists = Vec::new();

        let edit = crate::import::ReleaseUserEdit {
            album_title: "Edited Title".to_string(),
            album_artist_names: vec!["Edited Artist".to_string()],
            pressing: crate::import::PressingEdit {
                year: Some(1995),
                format: Some("Vinyl".to_string()),
                label: Some("Edited Label".to_string()),
                catalog_number: Some("EDIT-1".to_string()),
                country: Some("JP".to_string()),
                barcode: Some("4943674000000".to_string()),
            },
            tracks: vec![crate::import::TrackUserEdit {
                title: "Edited Track".to_string(),
                side: 1,
                track_number: Some(1),
                artist_names: vec![],
            }],
        };

        apply_user_edit_to_seed(
            &edit,
            &mut album,
            &mut release,
            &mut tracks,
            &mut artists,
            &mut album_artists,
            &mut track_artists,
            &test_clock(),
            &SequentialIdProvider::new("seed"),
        )
        .unwrap();

        assert_eq!(album.title, "Edited Title");
        assert_eq!(release.pressing.year, Some(1995));
        assert_eq!(release.pressing.format.as_deref(), Some("Vinyl"));
        assert_eq!(release.pressing.label.as_deref(), Some("Edited Label"));
        assert_eq!(release.pressing.catalog_number.as_deref(), Some("EDIT-1"));
        assert_eq!(release.pressing.country.as_deref(), Some("JP"));
        assert_eq!(release.pressing.barcode.as_deref(), Some("4943674000000"));
        assert_eq!(tracks[0].title, "Edited Track");

        // The new album artist gets a placeholder DbArtist row so the
        // import pipeline can canonicalize via find_or_create_artists.
        assert!(artists.iter().any(|a| a.name == "Edited Artist"));
        assert_eq!(
            album.artist_id,
            artists
                .iter()
                .find(|a| a.name == "Edited Artist")
                .unwrap()
                .id
        );
    }

    #[test]
    fn user_edit_can_fill_country_for_approximate_seed() {
        // Approximate seed clears pressing fields; the user can supply
        // them via the editor and the overlay applies the value.
        let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
        // Simulate the Approximate-cleared release row.
        release.pressing = crate::db::Pressing::blank();
        let mut tracks = vec![track];
        let mut artists = vec![seed_artist];
        let mut album_artists = Vec::new();
        let mut track_artists = Vec::new();

        let edit = crate::import::ReleaseUserEdit {
            album_title: album.title.clone(),
            album_artist_names: vec![artists[0].name.clone()],
            pressing: crate::import::PressingEdit {
                country: Some("JP".to_string()),
                ..crate::import::PressingEdit::blank()
            },
            tracks: vec![crate::import::TrackUserEdit {
                title: tracks[0].title.clone(),
                side: tracks[0].side,
                track_number: tracks[0].track_number,
                artist_names: vec![],
            }],
        };

        apply_user_edit_to_seed(
            &edit,
            &mut album,
            &mut release,
            &mut tracks,
            &mut artists,
            &mut album_artists,
            &mut track_artists,
            &test_clock(),
            &SequentialIdProvider::new("seed"),
        )
        .unwrap();

        assert_eq!(release.pressing.country.as_deref(), Some("JP"));
        assert!(release.pressing.year.is_none());
        assert!(release.pressing.format.is_none());
    }

    #[test]
    fn user_edit_track_count_mismatch_is_an_error() {
        let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
        let mut tracks = vec![track];
        let mut artists = vec![seed_artist];
        let mut album_artists = Vec::new();
        let mut track_artists = Vec::new();

        let edit = crate::import::ReleaseUserEdit {
            album_title: "T".to_string(),
            album_artist_names: vec!["A".to_string()],
            pressing: crate::import::PressingEdit::blank(),
            // Two edits but seed has one track.
            tracks: vec![
                crate::import::TrackUserEdit {
                    title: "X".to_string(),
                    side: 1,
                    track_number: Some(1),
                    artist_names: vec![],
                },
                crate::import::TrackUserEdit {
                    title: "Y".to_string(),
                    side: 1,
                    track_number: Some(2),
                    artist_names: vec![],
                },
            ],
        };

        let err = apply_user_edit_to_seed(
            &edit,
            &mut album,
            &mut release,
            &mut tracks,
            &mut artists,
            &mut album_artists,
            &mut track_artists,
            &test_clock(),
            &SequentialIdProvider::new("seed"),
        )
        .unwrap_err();
        assert!(err.contains("Track count mismatch"), "got: {err}");
    }

    /// Source-id linkage on artist rows (e.g. `musicbrainz_artist_id`)
    /// must survive a user edit that doesn't touch artist names. The
    /// editor round-trips an unchanged artist field as the same string
    /// it was seeded with, so the apply step must compare and short-
    /// circuit rather than rebuild rows from name-only placeholders.
    #[test]
    fn user_edit_preserves_source_id_artist_rows_when_names_unchanged() {
        let now = chrono::Utc::now();
        // Seeded artist row carrying the MB id the mapper attached.
        let seed_artist = crate::db::DbArtist {
            id: "artist-mb".to_string(),
            name: "Artist Name".to_string(),
            sort_name: Some("Artist Name".to_string()),
            discogs_artist_id: None,
            musicbrainz_artist_id: Some("mb-artist-1".to_string()),
            created_at: now,
        };
        let album = crate::db::DbAlbum {
            id: "album-1".to_string(),
            title: "Album Title".to_string(),
            artist_id: seed_artist.id.clone(),
            year: Some(2020),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        };
        let release = crate::db::DbRelease {
            id: "release-1".to_string(),
            album_id: album.id.clone(),
            release_name: None,
            pressing: crate::db::Pressing {
                year: Some(2020),
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
            metadata_source_release_id: Some("rel-mb".to_string()),
            managed: true,
            source_folder_name: None,
            content_hash: None,
            created_at: now,
        };
        let track = crate::db::DbTrack {
            id: "track-1".to_string(),
            release_id: release.id.clone(),
            title: "Track Title".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms: None,
            discogs_position: None,
            created_at: now,
        };
        // Seeded track credit pointing at the MB-id-bearing artist.
        let seed_track_artist = crate::db::DbTrackArtist::new(
            &track.id,
            &seed_artist.id,
            0,
            "track-artist-1".to_string(),
            now,
        );

        let mut album = album;
        let mut release = release;
        let mut tracks = vec![track];
        let mut artists = vec![seed_artist.clone()];
        let mut album_artists = Vec::<crate::db::DbAlbumArtist>::new();
        let mut track_artists = vec![seed_track_artist.clone()];

        // The user changes pressing fields but leaves artist names
        // alone. The track's edit ships `artist_names = []` because
        // the editor's "no override" form maps to empty when the
        // track's credit equals the album's.
        let edit = crate::import::ReleaseUserEdit {
            album_title: album.title.clone(),
            album_artist_names: vec![seed_artist.name.clone()],
            pressing: crate::import::PressingEdit {
                year: Some(1995),
                ..crate::import::PressingEdit::blank()
            },
            tracks: vec![crate::import::TrackUserEdit {
                title: tracks[0].title.clone(),
                side: tracks[0].side,
                track_number: tracks[0].track_number,
                artist_names: vec![],
            }],
        };

        apply_user_edit_to_seed(
            &edit,
            &mut album,
            &mut release,
            &mut tracks,
            &mut artists,
            &mut album_artists,
            &mut track_artists,
            &test_clock(),
            &SequentialIdProvider::new("seed"),
        )
        .unwrap();

        // The MB-id-bearing artist row must still exist with its
        // source binding intact — no fresh placeholder created.
        assert_eq!(artists.len(), 1, "no extra placeholder rows expected");
        assert_eq!(
            artists[0].musicbrainz_artist_id.as_deref(),
            Some("mb-artist-1"),
            "MB artist id must survive the edit",
        );
        assert_eq!(
            album.artist_id, seed_artist.id,
            "album.artist_id should still reference the seeded row",
        );

        // Track credit must still reference the seeded artist row.
        assert_eq!(track_artists.len(), 1);
        assert_eq!(track_artists[0].artist_id, seed_artist.id);
    }

    /// User-renaming an artist must rebuild the credit rows. The new
    /// name has no source binding, so the inserted `DbArtist` row
    /// carries `None` for both source ids.
    #[test]
    fn user_edit_renaming_album_artist_rebuilds_credits() {
        let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
        let mut tracks = vec![track];
        let mut artists = vec![seed_artist.clone()];
        let mut album_artists = Vec::new();
        let mut track_artists = Vec::new();

        let edit = crate::import::ReleaseUserEdit {
            album_title: album.title.clone(),
            album_artist_names: vec!["Different Artist".to_string()],
            pressing: crate::import::PressingEdit::blank(),
            tracks: vec![crate::import::TrackUserEdit {
                title: tracks[0].title.clone(),
                side: tracks[0].side,
                track_number: tracks[0].track_number,
                artist_names: vec![],
            }],
        };

        apply_user_edit_to_seed(
            &edit,
            &mut album,
            &mut release,
            &mut tracks,
            &mut artists,
            &mut album_artists,
            &mut track_artists,
            &test_clock(),
            &SequentialIdProvider::new("seed"),
        )
        .unwrap();

        let new_artist = artists
            .iter()
            .find(|a| a.name == "Different Artist")
            .expect("new placeholder should be inserted");
        assert!(new_artist.musicbrainz_artist_id.is_none());
        assert!(new_artist.discogs_artist_id.is_none());
        assert_eq!(album.artist_id, new_artist.id);
    }
}
