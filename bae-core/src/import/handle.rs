use crate::discogs::DiscogsClient;
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::folder_registry::{ImportFolderRegistry, WatchedFolder};
use crate::import::folder_scanner::{FolderCandidate, InvalidCandidate};
use crate::import::progress::ImportProgressHandle;
use crate::import::types::{
    DiscoveredFile, ImportCommand, ImportProgress, MetadataSource, StorageMode,
};
use crate::library::LibraryManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

/// Send an import event on the broadcast bus, logging on send failure.
/// `broadcast::Sender::send` returns `Err` only when there are zero active
/// receivers — a warning is appropriate (something upstream lost interest)
/// but not fatal.
pub(super) fn send_event(sender: &broadcast::Sender<ImportEvent>, ev: ImportEvent) {
    if let Err(e) = sender.send(ev) {
        warn!("import event send failed: {}", e);
    }
}
/// All events emitted by the import service. One channel, one subscriber (the bus).
#[derive(Debug, Clone)]
pub enum ImportEvent {
    Scan(ScanEvent),
    ImportProgress {
        candidate_key: String,
        progress: ImportProgress,
    },
    /// Per-track loudness measurement progress for an importing candidate. A
    /// high-frequency tick (one per track as its decode+measure completes,
    /// plus a 0/N start and an N/N final), routed to a native leaf view rather
    /// than the candidate row's coarse step. Separate from `ImportProgress` so
    /// it bypasses the release/import progress subscribers — it carries the
    /// candidate key, not a release/import id.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    ImportLoudnessProgress {
        candidate_key: String,
        tracks_done: u32,
        tracks_total: u32,
    },
    /// Identify pipeline transitioned to a new state. Emitted by the
    /// `identify` module; carries the full state payload plus the pre-shaped
    /// signals toolbar (the interactive badge row) projected from the same
    /// transition, so the UI renders both from one event.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    IdentifyStateChanged {
        candidate_key: String,
        state: crate::identify::IdentifyState,
        toolbar: Vec<crate::identify::ToolbarSignal>,
    },
    /// Full snapshot of a candidate's extracted signals (disc ID, barcodes,
    /// classified text). Core emits this on every transition — extraction
    /// start, each source/OCR completion, natural end, and cancellation. The
    /// reducer writes it wholesale; no partial-update logic needed.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    SignalsUpdated {
        candidate_key: String,
        signals: crate::signals::Signals,
    },
}

/// Search query — one of the three search modes.
pub enum SearchQuery {
    General {
        artist: String,
        album: String,
        source: MetadataSource,
    },
    CatalogNumber {
        catalog_number: String,
        source: MetadataSource,
    },
    Barcode {
        barcode: String,
        source: MetadataSource,
    },
}

/// Search results grouped by release group, with the per-release library dupe
/// statuses the UI looks up by release id.
#[derive(Debug, Clone)]
pub struct GroupedSearchResults {
    pub groups: Vec<crate::import::release_group::ReleaseGroup>,
    pub statuses: Vec<crate::db::LibraryStatus>,
}

/// What `save_discogs_token` did with a submitted key, after validating against
/// Discogs first.
///
/// - `Valid` — Discogs accepted the key; it's stored and used.
/// - `Unvalidated` — Discogs was unreachable or rate-limited; the key is stored
///   optimistically and will be re-checked when possible.
/// - `Rejected` — Discogs returned 401; nothing is stored, so the UI keeps the
///   draft for the user to correct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscogsSaveOutcome {
    Valid,
    Unvalidated,
    Rejected,
}

/// Map a `DiscogsClient::validate_token` result to the validation state it
/// implies. Shared by the save and revalidate paths so both fold the same
/// outcomes the same way: success confirms the key, a 401 rejects it, and a
/// network/rate-limit failure leaves it unvalidated to retry. `validate_token`
/// only ever returns those variants; the remaining `DiscogsError`s (not
/// reachable from a search request) also fold to `Unvalidated` — "couldn't
/// confirm" — so this match stays total without a catch-all.
fn validation_from_validate_result(
    result: Result<(), crate::discogs::client::DiscogsError>,
) -> crate::config::DiscogsValidation {
    use crate::config::DiscogsValidation;
    use crate::discogs::client::DiscogsError;
    match result {
        Ok(()) => DiscogsValidation::Valid,
        Err(DiscogsError::InvalidApiKey) => DiscogsValidation::Rejected,
        Err(
            e @ (DiscogsError::RateLimit
            | DiscogsError::Request(_)
            | DiscogsError::NotFound
            | DiscogsError::Serialization(_)),
        ) => {
            debug!("Discogs validation couldn't confirm the key ({e}); leaving it unvalidated to retry later");
            DiscogsValidation::Unvalidated
        }
    }
}

/// Handle for sending import requests and subscribing to progress updates.
///
/// Holds no caches; the network-layer caches in
/// `crate::musicbrainz`, `crate::discogs::client`, and
/// the Cover Art Archive client carry session hits transparently for every
/// caller. The handle is a thin orchestration layer: it dispatches
/// fire-and-forget prefetches, builds `ImportCommand`s carrying just
/// `MetadataRef`s, and forwards them to the worker.
#[derive(Clone)]
pub struct ImportServiceHandle {
    pub requests_tx: mpsc::UnboundedSender<ImportCommand>,
    pub progress_handle: ImportProgressHandle,
    pub library_manager: LibraryManager,
    pub runtime_handle: tokio::runtime::Handle,
    pub watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
    /// Unified event channel — all import service events go here.
    pub event_tx: broadcast::Sender<ImportEvent>,
    /// The persistent watched-folder list. Mutated by `add_watched_folder` /
    /// `remove_watched_folder`, which persist it and broadcast the new list.
    pub folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    cover_art_archive: CoverArtArchiveClient,
}

#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// The watched-folder list changed (queried at load, or after add/remove).
    /// Carries the full ordered list; the reducer replaces its copy.
    WatchedFoldersChanged {
        folders: Vec<WatchedFolder>,
    },
    FolderCandidate(FolderCandidate),
    /// A leaf folder that looks like a release but failed validation
    /// (corrupt/zero-byte audio, corrupt image, CUE referencing missing audio).
    /// The reducer surfaces it under the Skipped tab with its reason. The key is
    /// the folder path, shared with `CandidateRemoved` for reconciliation.
    InvalidCandidate(InvalidCandidate),
    /// A previously-emitted candidate no longer resolves on disk — the watcher
    /// re-scanned its folder and the release is gone. The reducer removes it by
    /// key (the key is the candidate's folder path).
    CandidateRemoved {
        candidate_key: String,
    },
    /// The user manually skipped or unskipped a candidate. The reducer flips the
    /// candidate's `skipped` flag in place; the import view re-tabs it from New
    /// to Skipped (or back). The key is the candidate's folder path.
    CandidateSkipChanged {
        candidate_key: String,
        skipped: bool,
    },
    Finished,
}

/// Commands to the folder watcher. `Watch` starts watching a folder (idempotent)
/// and scans it; `Unwatch` stops watching it.
pub enum WatcherCommand {
    Watch(std::path::PathBuf),
    Unwatch(std::path::PathBuf),
}

impl ImportServiceHandle {
    /// Create a new ImportHandle with the given dependencies
    pub fn new(
        requests_tx: mpsc::UnboundedSender<ImportCommand>,
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
        event_tx: broadcast::Sender<ImportEvent>,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        cover_art_archive: CoverArtArchiveClient,
    ) -> Self {
        let progress_handle =
            ImportProgressHandle::new(event_tx.subscribe(), runtime_handle.clone());
        Self {
            requests_tx,
            progress_handle,
            library_manager,
            runtime_handle,
            watcher_tx,
            event_tx,
            folder_registry,
            cover_art_archive,
        }
    }

    /// Fetch raw cover-art bytes for `url`. The UI calls this when it
    /// needs to render a remote cover — the bytes are not threaded
    /// back through the import command, but a session-wide LRU cache
    /// in `cover_art` keeps the URL's bytes warm so the commit
    /// worker's later download is a cache hit, not a re-fetch.
    pub async fn fetch_cover_bytes(&self, url: String) -> Result<Vec<u8>, String> {
        let (bytes, _content_type) = super::cover_art::download_cover_art_bytes(&url).await?;
        Ok(bytes)
    }

    fn discogs_client(&self) -> Result<DiscogsClient, String> {
        self.library_manager
            .discogs_client()?
            .ok_or_else(|| "Discogs API key not configured".to_string())
    }

    /// Run a Discogs search and map its error to a display string for the
    /// caller. The search's outcome folds into the stored key's validation
    /// inside `DiscogsClient` itself (it self-reports each call to the
    /// validation observer); this wrapper doesn't record anything.
    async fn run_discogs_search(
        &self,
        search: impl std::future::Future<
            Output = Result<
                Vec<super::search::MetadataResult>,
                crate::discogs::client::DiscogsError,
            >,
        >,
    ) -> Result<Vec<super::search::MetadataResult>, String> {
        search
            .await
            .map_err(|e| format!("Discogs search failed: {e}"))
    }

    /// Search for releases, check library status, and bundle the results into
    /// release-group cards in one call.
    pub async fn search_with_status(
        &self,
        query: SearchQuery,
    ) -> Result<GroupedSearchResults, String> {
        use crate::db::LibraryCheck;

        let results = match query {
            SearchQuery::General {
                artist,
                album,
                source,
            } => match source {
                MetadataSource::MusicBrainz => {
                    self.search_musicbrainz(artist, album, None, None).await?
                }
                MetadataSource::Discogs => {
                    let client = self.discogs_client()?;
                    self.run_discogs_search(super::search::search_discogs(
                        &client, artist, album, None, None,
                    ))
                    .await?
                }
            },
            SearchQuery::CatalogNumber {
                catalog_number,
                source,
            } => match source {
                MetadataSource::MusicBrainz => {
                    super::search::search_mb_by_catalog_number(
                        &self.cover_art_archive,
                        catalog_number,
                    )
                    .await?
                }
                MetadataSource::Discogs => {
                    let client = self.discogs_client()?;
                    self.run_discogs_search(super::search::search_discogs_by_catalog_number(
                        &client,
                        catalog_number,
                    ))
                    .await?
                }
            },
            SearchQuery::Barcode { barcode, source } => match source {
                MetadataSource::MusicBrainz => {
                    super::search::search_mb_by_barcode(&self.cover_art_archive, barcode).await?
                }
                MetadataSource::Discogs => {
                    let client = self.discogs_client()?;
                    self.run_discogs_search(super::search::search_discogs_by_barcode(
                        &client, barcode,
                    ))
                    .await?
                }
            },
        };

        let checks: Vec<LibraryCheck> = results.iter().map(LibraryCheck::from).collect();

        let statuses = self
            .library_manager
            .check_releases_in_library(&checks)
            .await
            .map_err(|e| format!("Failed to check library status: {e}"))?;

        let status_map: HashMap<String, crate::db::LibraryStatus> = statuses
            .into_iter()
            .map(|s| (s.release_id.clone(), s))
            .collect();

        // `check_releases_in_library` returns exactly one status per input
        // check, keyed by `release_id`, so a miss is a broken invariant —
        // surface it rather than fabricating a "not in library" default that
        // would silently misclassify a real failure.
        let mut statuses = Vec::with_capacity(results.len());
        for r in &results {
            let status = status_map
                .get(&r.release_id)
                .cloned()
                .ok_or_else(|| format!("library status missing for release {}", r.release_id))?;
            statuses.push(status);
        }

        // Grouping is the UI's shape, so it happens here in core: the search
        // surface renders one card per release group with the pressings beneath.
        let groups = crate::import::release_group::group_results(results);

        Ok(GroupedSearchResults { groups, statuses })
    }

    pub async fn search_discogs(
        &self,
        artist: String,
        album: String,
        year: Option<String>,
        label: Option<String>,
    ) -> Result<Vec<super::search::MetadataResult>, String> {
        let client = self.discogs_client()?;
        self.run_discogs_search(super::search::search_discogs(
            &client, artist, album, year, label,
        ))
        .await
    }

    pub async fn search_musicbrainz(
        &self,
        artist: String,
        album: String,
        year: Option<String>,
        label: Option<String>,
    ) -> Result<Vec<super::search::MetadataResult>, String> {
        super::search::search_musicbrainz(&self.cover_art_archive, artist, album, year, label).await
    }

    /// Fetch available remote cover art options for a release.
    /// Reads `release_identities` for the release and queries each
    /// source's cover endpoint:
    ///
    /// - **MusicBrainz** — `(source, source_release_id)` pulls the
    ///   per-pressing CAA cover; `(source, source_group_id)` pulls the
    ///   release-group (album-level) CAA cover.
    /// - **Discogs** — `(source, source_release_id)` pulls the per-pressing
    ///   cover via the Discogs API.
    ///
    /// Returns covers in the order they were resolved; the picker uses
    /// this list as-is. Unknown imports (no identity rows) return an
    /// empty list — no source to query.
    pub async fn fetch_remote_covers(
        &self,
        release_id: &str,
    ) -> Result<Vec<super::cover_art::RemoteCover>, String> {
        let identities = self
            .library_manager
            .get_release_identities(release_id)
            .await
            .map_err(|e| format!("{e}"))?;

        let mut covers = Vec::new();

        for identity in &identities {
            match identity.source {
                MetadataSource::MusicBrainz => {
                    for cover in self
                        .cover_art_archive
                        .fetch_candidates(
                            identity.source_release_id.as_deref(),
                            Some(identity.source_group_id.as_str()),
                        )
                        .await
                    {
                        super::cover_art::push_unique_cover(&mut covers, cover);
                    }
                }
                MetadataSource::Discogs => {
                    // Discogs only exposes per-release covers via the API;
                    // no master-level cover endpoint to mirror MB's CAA.
                    let Some(rid) = &identity.source_release_id else {
                        debug!(
                            release_id,
                            source_group_id = %identity.source_group_id,
                            "skipping Discogs cover fetch: Approximate identity (no source_release_id)"
                        );
                        continue;
                    };
                    let client = match self.library_manager.discogs_client() {
                        Ok(Some(c)) => c,
                        Ok(None) => {
                            debug!(
                                release_id,
                                source_release_id = %rid,
                                "skipping Discogs cover fetch: Discogs client not configured"
                            );
                            continue;
                        }
                        Err(e) => {
                            warn!(
                                release_id,
                                source_release_id = %rid,
                                "skipping Discogs cover fetch: {e}"
                            );
                            continue;
                        }
                    };
                    match client.get_release(rid).await {
                        Ok((discogs_release, _raw_json)) => {
                            if let Some(cover) = discogs_release.remote_cover() {
                                covers.push(cover);
                            }
                        }
                        Err(ref e) => {
                            warn!(
                                release_id,
                                source_release_id = %rid,
                                err = %e,
                                "Discogs cover fetch failed; skipping this source"
                            );
                        }
                    }
                }
            }
        }

        Ok(covers)
    }

    /// Prefetch for the confirmation pane. Fetches the release and builds
    /// the picker/confirm detail — no DB-shape mapping. The fetch goes
    /// through the network LRU caches, so the worker's later commit-time
    /// fetch hits cache for the same response.
    pub async fn prefetch_release(
        &self,
        release_id: &str,
        source: MetadataSource,
    ) -> Result<super::search::ImportSearchReleaseDetail, String> {
        match source {
            MetadataSource::MusicBrainz => {
                super::search::prefetch_mb_release(&self.cover_art_archive, release_id).await
            }
            MetadataSource::Discogs => {
                let client = self.discogs_client()?;
                super::search::prefetch_discogs_release(&client, release_id).await
            }
        }
    }

    /// Project the embedded tags of a folder's audio files into a
    /// `ReleaseUserEdit` shape so the edit-metadata form can seed itself
    /// from what's on disk. Used by the "Add as Unknown" affordance:
    /// the user clicks the link, the UI calls this to preview, then
    /// shows the editor for verification before commit.
    ///
    /// The commit-side worker re-reads tags from the same files at
    /// commit time, so the user's edits — applied via the
    /// `user_edit` overlay on the import command — are the source of
    /// truth for fields they touched. This preview is the seed only.
    pub async fn preview_file_tags_for_folder(
        &self,
        folder: std::path::PathBuf,
    ) -> Result<crate::import::ReleaseUserEdit, String> {
        // Captured before `folder` moves into the scan — the album-title
        // fallback when no file carries an ALBUM tag.
        let folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());

        let categorized = tokio::task::spawn_blocking(move || {
            crate::import::folder_scanner::collect_release_candidate_files(&folder)
        })
        .await
        .map_err(|e| format!("folder scan task failed: {e}"))??;

        let audio_paths = categorized_audio_paths(&categorized);
        if audio_paths.is_empty() {
            return Err("folder contains no audio files".to_string());
        }

        let clock = self.library_manager.clock().clone();
        let ids = self.library_manager.ids().clone();
        tokio::task::spawn_blocking(move || {
            let parsed = super::file_tag_mapper::map_file_tags_to_db(
                &audio_paths,
                folder_name.as_deref(),
                clock.as_ref(),
                ids.as_ref(),
            )?;
            Ok(parsed_album_to_user_edit(&parsed))
        })
        .await
        .map_err(|e| format!("file-tag projection task failed: {e}"))?
    }

    /// Build an import command and enqueue it. For Exact / Approximate
    /// the worker calls `prepare_release` itself to fetch and map the
    /// release — reading from the same LRU caches the UI's prefetch
    /// warmed up. For Unknown the worker reads embedded tags from the
    /// candidate's audio files instead. Remote cover bytes are not
    /// threaded through the command; `download_cover_art_bytes`
    /// consults the URL cache when the worker writes the cover.
    ///
    /// `identity_choice` carries both the user's claim shape and the
    /// release reference (when applicable): Exact preserves the
    /// mapper's `source_release_id`, Approximate NULLs it, Unknown
    /// writes zero `release_identities` rows.
    ///
    /// `user_edit` is an optional overlay from the confirmation page;
    /// when present, fields override the seeded metadata.
    pub fn start_import(
        &self,
        candidate_key: &str,
        folder: std::path::PathBuf,
        selected_cover: Option<super::types::CoverSelection>,
        storage_mode: StorageMode,
        pin: bool,
        identity_choice: super::types::IdentityChoice,
        user_edit: Option<super::types::ReleaseUserEdit>,
    ) -> Result<String, String> {
        let import_id = self.library_manager.ids().new_id();
        let command = ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: candidate_key.to_string(),
            folder,
            selected_cover,
            storage_mode,
            pin,
            identity_choice,
            user_edit,
        };

        self.send_command(command)?;
        Ok(import_id)
    }

    /// Validate a submitted Discogs key against Discogs, then persist it only if
    /// it isn't outright rejected. Validating first means a typo (401) never
    /// stores a bad key, while an offline/rate-limited save still stores the key
    /// optimistically so the user isn't blocked. See `DiscogsSaveOutcome`.
    pub async fn save_discogs_token(&self, token: &str) -> Result<DiscogsSaveOutcome, String> {
        use crate::config::DiscogsValidation;

        let client = DiscogsClient::new(token.to_string());
        match validation_from_validate_result(client.validate_token().await) {
            DiscogsValidation::Valid => {
                self.persist_discogs_key(token, DiscogsValidation::Valid)?;
                Ok(DiscogsSaveOutcome::Valid)
            }
            DiscogsValidation::Unvalidated => {
                self.persist_discogs_key(token, DiscogsValidation::Unvalidated)?;
                Ok(DiscogsSaveOutcome::Unvalidated)
            }
            DiscogsValidation::Rejected => Ok(DiscogsSaveOutcome::Rejected),
        }
    }

    /// Write the key to the keyring and record its validation in config. The
    /// shared persist path for the two outcomes that keep the key.
    fn persist_discogs_key(
        &self,
        token: &str,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), String> {
        self.library_manager
            .save_discogs_key(token)
            .map_err(|e| e.to_string())?;
        self.library_manager
            .set_discogs_key_stored(validation)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Re-check a stored `Unvalidated` key when possible (app launch,
    /// settings-tab open). No-op when no key is stored or the key is already
    /// settled `Valid`/`Rejected`. A 401 marks it `Rejected`; success confirms
    /// it `Valid`; network/rate-limit leaves it `Unvalidated` to retry later.
    pub async fn revalidate_discogs_token(&self) -> Result<(), String> {
        use crate::config::DiscogsValidation;

        if self.library_manager.discogs_validation() != Some(DiscogsValidation::Unvalidated) {
            return Ok(());
        }
        let Some(client) = self.library_manager.discogs_client()? else {
            // A stored `Unvalidated` key (the guard above) with no client means
            // the keyring entry and the config disagree — surface it rather than
            // silently leaving the key stuck unvalidated.
            warn!(
                "revalidate skipped: config says a Discogs key is stored but the keyring has none"
            );
            return Ok(());
        };
        match validation_from_validate_result(client.validate_token().await) {
            settled @ (DiscogsValidation::Valid | DiscogsValidation::Rejected) => self
                .library_manager
                .set_discogs_validation(settled)
                .map_err(|e| e.to_string()),
            DiscogsValidation::Unvalidated => Ok(()),
        }
    }

    /// Remove the Discogs API token from the OS keyring and clear the
    /// stored-key hint.
    pub fn remove_discogs_token(&self) -> Result<(), String> {
        self.library_manager
            .delete_discogs_key()
            .map_err(|e| e.to_string())?;
        self.library_manager
            .clear_discogs_key_stored()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The current watched-folder list. The UI fetches this when the import
    /// view appears to render the group headers, sidestepping the broadcast
    /// race (the list is durable; events only fire on later changes).
    pub fn watched_folders(&self) -> Vec<WatchedFolder> {
        self.folder_registry.lock().unwrap().watched_folders()
    }

    /// Add a folder to watch for imports: persist it, broadcast the new list,
    /// and start watching + scanning it so its releases appear as candidates and
    /// later on-disk changes propagate. A folder already watched is left as-is.
    pub fn add_watched_folder(&self, path: String) -> Result<(), String> {
        let library_dir = self.library_manager.library_dir();
        let mut registry = self.folder_registry.lock().unwrap();
        let added = registry.add(library_dir, path.clone())?;
        let folders = registry.watched_folders();
        drop(registry);
        if !added {
            return Ok(());
        }
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }),
        );
        self.watcher_tx
            .send(WatcherCommand::Watch(std::path::PathBuf::from(path)))
            .map_err(|_| "Failed to start watching folder".to_string())
    }

    /// Stop watching `path`: persist the removal, broadcast the new list, and
    /// stop the filesystem watcher for it. The reducer drops the folder's
    /// candidates by reconciling against the list, so no per-candidate removal
    /// events are needed here.
    pub fn remove_watched_folder(&self, path: String) -> Result<(), String> {
        let library_dir = self.library_manager.library_dir();
        let mut registry = self.folder_registry.lock().unwrap();
        let removed = registry.remove(library_dir, &path)?;
        let folders = registry.watched_folders();
        drop(registry);
        if removed {
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }),
            );
            self.watcher_tx
                .send(WatcherCommand::Unwatch(std::path::PathBuf::from(path)))
                .map_err(|_| "Failed to stop watching folder".to_string())?;
        }
        Ok(())
    }

    /// Mark the candidate at `path` skipped or unskipped, persisting the change
    /// and broadcasting it so the import view re-tabs the row (New ↔ Skipped).
    /// A no-op request (already in the requested state) persists nothing and
    /// emits no event.
    pub fn set_candidate_skipped(&self, path: String, skipped: bool) -> Result<(), String> {
        let library_dir = self.library_manager.library_dir();
        let mut registry = self.folder_registry.lock().unwrap();
        let changed = registry.set_skipped(library_dir, path.clone(), skipped)?;
        drop(registry);
        if changed {
            send_event(
                &self.event_tx,
                ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
                    candidate_key: path,
                    skipped,
                }),
            );
        }
        Ok(())
    }

    /// Start watching + scanning every watched folder, emitting one
    /// `FolderCandidate` per release found and `CandidateRemoved` for any that
    /// have since vanished. The UI calls this when the import view appears.
    pub fn scan_watched_folders(&self) -> Result<(), String> {
        let folders = self.folder_registry.lock().unwrap().watched_folders();
        for folder in folders {
            self.watcher_tx
                .send(WatcherCommand::Watch(std::path::PathBuf::from(folder.path)))
                .map_err(|_| "Failed to start watching folder".to_string())?;
        }
        Ok(())
    }

    /// Subscribe to scan events, filtered from the unified event channel.
    /// Returns an mpsc receiver that yields only ScanEvent variants.
    pub fn subscribe_folder_scan_events(&self) -> mpsc::UnboundedReceiver<ScanEvent> {
        let mut rx = self.event_tx.subscribe();
        let (tx, out_rx) = mpsc::unbounded_channel();
        self.runtime_handle.spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ImportEvent::Scan(event)) => {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Scan event subscriber lagged by {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        out_rx
    }

    /// Subscribe to the unified event channel.
    pub fn subscribe_events(&self) -> broadcast::Receiver<ImportEvent> {
        self.event_tx.subscribe()
    }
    /// Queue an import command and return the import_id for progress tracking.
    ///
    /// All heavy work (metadata resolution, file discovery, track mapping,
    /// DB insertion) happens in the service worker. This returns immediately.
    pub fn send_command(&self, command: ImportCommand) -> Result<String, String> {
        let ImportCommand::Folder { import_id, .. } = &command;
        let import_id = import_id.clone();
        self.requests_tx
            .send(command)
            .map_err(|_| "Failed to queue import command".to_string())?;
        Ok(import_id)
    }
    /// Subscribe to progress updates for a specific release
    /// Returns a filtered receiver that yields only updates for the specified release
    pub fn subscribe_release(
        &self,
        release_id: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<ImportProgress> {
        self.progress_handle.subscribe_release(release_id)
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
/// Remap track artist IDs from the parsed (temporary) artist IDs to the actual DB IDs.
///
/// The ParsedAlbum's track_artists reference artist IDs generated during parsing, but
/// find_or_create_artists may have resolved them to existing DB artists. This function
/// applies the same ID mapping used for album_artists.
pub fn remap_track_artists(
    track_artists: &[crate::db::DbTrackArtist],
    artist_id_map: &HashMap<String, String>,
) -> Result<Vec<crate::db::DbTrackArtist>, String> {
    track_artists
        .iter()
        .map(|ta| {
            let actual_id = artist_id_map.get(&ta.artist_id).ok_or_else(|| {
                format!("Track artist ID {} not found in artist map", ta.artist_id)
            })?;
            let mut remapped = ta.clone();
            remapped.artist_id = actual_id.clone();
            Ok(remapped)
        })
        .collect()
}

/// Remap album artist IDs from the parsed (temporary) artist IDs to the actual DB IDs.
pub fn remap_album_artists(
    album_artists: &[crate::db::DbAlbumArtist],
    artist_id_map: &HashMap<String, String>,
) -> Result<Vec<crate::db::DbAlbumArtist>, String> {
    album_artists
        .iter()
        .map(|aa| {
            let actual_id = artist_id_map.get(&aa.artist_id).ok_or_else(|| {
                format!("Album artist ID {} not found in artist map", aa.artist_id)
            })?;
            let mut remapped = aa.clone();
            remapped.artist_id = actual_id.clone();
            Ok(remapped)
        })
        .collect()
}

/// Fetch artist images for artists that have a Discogs ID but no image yet.
/// Best-effort: never fails the import.
pub(crate) async fn fetch_artist_images(
    library_manager: &LibraryManager,
    discogs_client: &DiscogsClient,
    parsed_artists: &[crate::db::DbArtist],
    artist_id_map: &HashMap<String, String>,
) {
    for parsed_artist in parsed_artists {
        let actual_id = match artist_id_map.get(&parsed_artist.id) {
            Some(id) => id,
            None => continue,
        };

        // Only fetch if the artist has a Discogs ID
        let discogs_artist_id = match &parsed_artist.discogs_artist_id {
            Some(id) => id.clone(),
            None => continue,
        };

        // Check if artist already has an image in DB
        if let Ok(Some(_)) = library_manager
            .get_library_image(actual_id, &crate::db::LibraryImageType::Artist)
            .await
        {
            continue;
        }

        crate::import::artist_image::fetch_and_save_artist_image(
            actual_id,
            &discogs_artist_id,
            discogs_client,
            library_manager,
        )
        .await;
    }
}

/// Project a parsed album (mapper output) into the editor's
/// `ReleaseUserEdit` shape. Used by the Unknown preview path so the
/// edit-metadata form can seed itself from the file-tag projection
/// without going through a wire-shape `ImportSearchReleaseDetail` that
/// would require a synthetic release id. Also used by the reset-to-source
/// path to project cached source data back into the editor.
///
/// Track artist names are emitted positionally per existing
/// `track_artists` rows; an empty per-track list rolls up to "share
/// the album artist" in the editor's convention.
pub fn parsed_album_to_user_edit(parsed: &super::ParsedAlbum) -> crate::import::ReleaseUserEdit {
    let primary_artist_name = parsed
        .artists
        .iter()
        .find(|a| a.id == parsed.album.artist_id)
        .map(|a| a.name.clone())
        .expect("primary artist id not found in ParsedAlbum.artists");

    let mut album_artist_names = vec![primary_artist_name];
    let mut junctions = parsed.album_artists.clone();
    junctions.sort_by_key(|aa| aa.position);
    for aa in &junctions {
        if let Some(artist) = parsed.artists.iter().find(|a| a.id == aa.artist_id) {
            album_artist_names.push(artist.name.clone());
        }
    }

    let tracks = parsed
        .tracks
        .iter()
        .map(|t| {
            let mut credits: Vec<&crate::db::DbTrackArtist> = parsed
                .track_artists
                .iter()
                .filter(|ta| ta.track_id == t.id)
                .collect();
            credits.sort_by_key(|ta| ta.position);
            let artist_names = credits
                .iter()
                .filter_map(|ta| {
                    parsed
                        .artists
                        .iter()
                        .find(|a| a.id == ta.artist_id)
                        .map(|a| a.name.clone())
                })
                .collect();
            crate::import::TrackUserEdit {
                title: t.title.clone(),
                side: t.side,
                track_number: t.track_number,
                artist_names,
            }
        })
        .collect();

    crate::import::ReleaseUserEdit {
        album_title: parsed.album.title.clone(),
        album_artist_names,
        pressing: crate::import::PressingEdit {
            year: parsed.release.pressing.year,
            format: parsed.release.pressing.format.clone(),
            label: parsed.release.pressing.label.clone(),
            catalog_number: parsed.release.pressing.catalog_number.clone(),
            country: parsed.release.pressing.country.clone(),
            barcode: parsed.release.pressing.barcode.clone(),
        },
        tracks,
    }
}

/// Project an `ImportSearchReleaseDetail` (the prefetch result for the
/// import confirmation pane) into the editor's `ReleaseUserEdit` shape,
/// honoring the user's identity choice:
///
/// - **Exact**: `pressing` comes from the picked release.
/// - **Approximate** / **Unknown**: `pressing` is `PressingEdit::blank()`.
///   The user explicitly didn't claim a specific pressing, so showing
///   them the source's pressing data would imply a claim they didn't
///   make. They can fill in fields they know (e.g., "JP" in country)
///   and the overlay carries those edits to commit.
///
/// Per-track artist override: when a track's source artist matches the
/// album-level artist or is missing, the per-track artist override is
/// empty (the track rolls up to "share the album artist" in the
/// editor's convention). When it differs, the override is the track's
/// artist verbatim.
///
/// Used by the Exact / Approximate import path so Swift can construct
/// the editor's seed values from a pre-shaped bridge payload — Swift
/// must not branch on `IdentityChoice` itself per the bridge-thinness
/// rule.
pub fn shape_user_edit_from_search_detail(
    detail: &super::search::ImportSearchReleaseDetail,
    choice: &super::IdentityChoice,
) -> super::ReleaseUserEdit {
    let primary_album_artist = match &detail.artist {
        Some(name) => name.clone(),
        None => {
            warn!(
                "shape_user_edit_from_search_detail: detail.artist is None for release {}; defaulting to empty (editor save will be disabled until user fills it in)",
                detail.release_id
            );
            String::new()
        }
    };

    // Number tracks per side, matching the per-side numbering the commit-side
    // mappers assign (musicbrainz_mapper / discogs_mapper reset their count at
    // each side, so A1,A2,B1,B2 -> 1,2,1,2). `detail.tracks` carries each
    // track's already-resolved `side` in the same order the seed was built, so
    // a counter keyed on side reproduces those numbers exactly. A release-global
    // `i + 1` index would diverge and, since `apply_user_edit_to_seed` writes
    // `track_number` verbatim onto the seed, corrupt the per-side numbers of any
    // multi-side vinyl/cassette/multi-disc release.
    let mut per_side_count: std::collections::HashMap<u32, i32> = std::collections::HashMap::new();
    let tracks = detail
        .tracks
        .iter()
        .map(|t| {
            let artist_names = match t.artist.as_deref() {
                Some(a) if !a.is_empty() && a != primary_album_artist => vec![a.to_string()],
                _ => Vec::new(),
            };
            let count = per_side_count.entry(t.side).or_insert(0);
            *count += 1;
            super::TrackUserEdit {
                title: t.title.clone(),
                side: t.side as i32,
                track_number: Some(*count),
                artist_names,
            }
        })
        .collect();

    let pressing = match choice {
        super::IdentityChoice::Exact { .. } => super::PressingEdit {
            year: detail.year,
            format: detail.format.clone(),
            label: detail.label.clone(),
            catalog_number: detail.catalog_number.clone(),
            country: detail.country.clone(),
            barcode: detail.barcode.clone(),
        },
        super::IdentityChoice::Approximate { .. } | super::IdentityChoice::Unknown => {
            super::PressingEdit::blank()
        }
    };

    super::ReleaseUserEdit {
        album_title: detail.title.clone(),
        album_artist_names: vec![primary_album_artist],
        pressing,
        tracks,
    }
}

/// Audio file paths from a `CategorizedFiles`, in the same order the
/// flattened pipeline produces. CUE-backed releases yield only the
/// audio file from each pair (the CUE itself carries no embedded
/// tags); per-track releases yield each track in scan order.
///
/// Used by the Unknown import path to feed `map_file_tags_to_db`.
pub fn categorized_audio_paths(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Vec<std::path::PathBuf> {
    use crate::import::folder_scanner::AudioContent;
    let mut paths = Vec::new();
    match &categorized.audio {
        AudioContent::CueFlacPairs { pairs, .. } => {
            for pair in pairs {
                paths.push(pair.audio_file.path.clone());
            }
        }
        AudioContent::TrackFiles { tracks, .. } => {
            for f in tracks {
                paths.push(f.path.clone());
            }
        }
    }
    paths
}

/// Flatten a `CategorizedFiles` into the `DiscoveredFile` list the downstream
/// import pipeline consumes for progress tracking and byte accounting.
/// Ordering mirrors the scan's structured output: audio first (pairs before
/// per-track, in natural sort within each), then artwork, then documents.
pub(crate) fn categorized_to_discovered_files(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Vec<DiscoveredFile> {
    use crate::import::folder_scanner::AudioContent;
    let mut files: Vec<DiscoveredFile> = Vec::new();
    let push = |files: &mut Vec<DiscoveredFile>, f: &crate::import::folder_scanner::ScannedFile| {
        files.push(DiscoveredFile {
            path: f.path.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size,
        });
    };
    match &categorized.audio {
        AudioContent::CueFlacPairs { pairs, .. } => {
            for pair in pairs {
                push(&mut files, &pair.cue_file);
                push(&mut files, &pair.audio_file);
            }
        }
        AudioContent::TrackFiles { tracks, .. } => {
            for f in tracks {
                push(&mut files, f);
            }
        }
    }
    for f in &categorized.artwork {
        push(&mut files, f);
    }
    for f in &categorized.documents {
        push(&mut files, f);
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, DbArtist};
    use chrono::Utc;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn test_config_and_keys(
        library_dir: &crate::library_dir::LibraryDir,
    ) -> (
        std::sync::Arc<crate::config::ConfigHandle>,
        crate::keys::KeyService,
    ) {
        // Unique id per test so keyring entries don't collide in the shared
        // process-global mock store (see `install_test_keyring`).
        let library_id = format!("test-{}", uuid::Uuid::new_v4());
        let config = crate::config::Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            library_dir.clone(),
            "Test Library".to_string(),
        );
        crate::config::install_test_keyring();
        (
            std::sync::Arc::new(crate::config::ConfigHandle::new(config)),
            crate::keys::KeyService::new(library_id),
        )
    }

    async fn setup_test_manager() -> (LibraryManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(crate::clock::SystemClock),
        )
        .await
        .unwrap();
        let library_dir = crate::library_dir::LibraryDir::new(temp_dir.path());
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let manager = LibraryManager::new(
            database,
            library_dir,
            config_handle,
            key_service,
            std::sync::Arc::new(crate::clock::SystemClock),
            std::sync::Arc::new(crate::id_provider::UuidProvider),
            tokio::runtime::Handle::current(),
        );
        (manager, temp_dir)
    }

    fn make_artist(name: &str, discogs_id: Option<&str>, mb_id: Option<&str>) -> DbArtist {
        let now = Utc::now();
        DbArtist {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            sort_name: None,
            discogs_artist_id: discogs_id.map(|s| s.to_string()),
            musicbrainz_artist_id: mb_id.map(|s| s.to_string()),
            created_at: now,
        }
    }

    #[tokio::test]
    async fn test_same_discogs_id_reuses_existing() {
        let (manager, _tmp) = setup_test_manager().await;
        let existing = make_artist("Artist One", Some("d123"), None);
        manager.insert_artist(&existing).await.unwrap();

        let incoming = make_artist("Artist One", Some("d123"), None);
        let resolved = manager
            .find_or_create_artists(std::slice::from_ref(&incoming))
            .await
            .unwrap();

        assert_eq!(resolved[0], existing.id);
    }

    #[tokio::test]
    async fn test_same_mb_id_reuses_existing() {
        let (manager, _tmp) = setup_test_manager().await;
        let existing = make_artist("Artist One", None, Some("mb-abc"));
        manager.insert_artist(&existing).await.unwrap();

        let incoming = make_artist("Artist One", None, Some("mb-abc"));
        let resolved = manager
            .find_or_create_artists(std::slice::from_ref(&incoming))
            .await
            .unwrap();

        assert_eq!(resolved[0], existing.id);
    }

    #[tokio::test]
    async fn test_same_name_no_ids_reuses_existing() {
        let (manager, _tmp) = setup_test_manager().await;
        let existing = make_artist("Artist One", None, None);
        manager.insert_artist(&existing).await.unwrap();

        let incoming = make_artist("Artist One", None, None);
        let resolved = manager
            .find_or_create_artists(std::slice::from_ref(&incoming))
            .await
            .unwrap();

        assert_eq!(resolved[0], existing.id);
    }

    #[tokio::test]
    async fn test_same_name_same_mb_id_reuses() {
        let (manager, _tmp) = setup_test_manager().await;
        let existing = make_artist("Artist Two", None, Some("mb-artist-two"));
        manager.insert_artist(&existing).await.unwrap();

        let incoming = make_artist("Artist Two", None, Some("mb-artist-two"));
        let resolved = manager
            .find_or_create_artists(std::slice::from_ref(&incoming))
            .await
            .unwrap();

        assert_eq!(resolved[0], existing.id);
    }

    #[tokio::test]
    async fn test_same_name_different_mb_id_creates_new() {
        let (manager, _tmp) = setup_test_manager().await;
        let existing = make_artist("Artist Two", None, Some("mb-artist-two-uk"));
        manager.insert_artist(&existing).await.unwrap();

        let incoming = make_artist("Artist Two", None, Some("mb-artist-two-ca"));
        let resolved = manager
            .find_or_create_artists(std::slice::from_ref(&incoming))
            .await
            .unwrap();

        // Should create a new artist, not reuse existing
        assert_eq!(resolved[0], incoming.id);
        assert_ne!(resolved[0], existing.id);
    }

    #[tokio::test]
    async fn test_same_name_different_discogs_id_creates_new() {
        let (manager, _tmp) = setup_test_manager().await;
        let existing = make_artist("Artist Two", Some("d100"), None);
        manager.insert_artist(&existing).await.unwrap();

        let incoming = make_artist("Artist Two", Some("d200"), None);
        let resolved = manager
            .find_or_create_artists(std::slice::from_ref(&incoming))
            .await
            .unwrap();

        assert_eq!(resolved[0], incoming.id);
        assert_ne!(resolved[0], existing.id);
    }

    #[tokio::test]
    async fn test_name_match_accumulates_ids() {
        let (manager, _tmp) = setup_test_manager().await;
        // Existing has discogs ID only
        let existing = make_artist("Artist One", Some("d456"), None);
        manager.insert_artist(&existing).await.unwrap();

        // Incoming has MB ID only — no conflict, should merge
        let incoming = make_artist("Artist One", None, Some("mb-xyz"));
        let resolved = manager
            .find_or_create_artists(std::slice::from_ref(&incoming))
            .await
            .unwrap();

        assert_eq!(resolved[0], existing.id);

        // Verify the existing artist now has both IDs
        let updated = manager
            .get_artist_by_id(&existing.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.discogs_artist_id.as_deref(), Some("d456"));
        assert_eq!(updated.musicbrainz_artist_id.as_deref(), Some("mb-xyz"));
    }

    #[tokio::test]
    async fn test_new_artist_inserts() {
        let (manager, _tmp) = setup_test_manager().await;

        let incoming = make_artist("New Band", Some("d999"), Some("mb-999"));
        let resolved = manager
            .find_or_create_artists(std::slice::from_ref(&incoming))
            .await
            .unwrap();

        assert_eq!(resolved[0], incoming.id);

        // Verify it's in the DB
        let saved = manager
            .get_artist_by_id(&incoming.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.name, "New Band");
        assert_eq!(saved.discogs_artist_id.as_deref(), Some("d999"));
        assert_eq!(saved.musicbrainz_artist_id.as_deref(), Some("mb-999"));
    }

    // ── find_existing_album_for_import (identity-based dedup) ────────

    use crate::db::{DbAlbum, DbRelease, DbTrack};
    use crate::import::ReleaseIdentity;

    /// Set up a manager with a single test artist that the helpers below
    /// reference for inserted albums.
    async fn setup_test_db_with_artist() -> (LibraryManager, TempDir) {
        let (manager, tmp) = setup_test_manager().await;
        let artist = DbArtist {
            id: "test-artist-id".to_string(),
            name: "Artist Name".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        manager.insert_artist(&artist).await.unwrap();
        (manager, tmp)
    }

    fn make_album(title: &str) -> DbAlbum {
        let now = Utc::now();
        DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: title.to_string(),
            artist_id: "test-artist-id".to_string(),
            year: Some(2024),
            primary_release_id: None,
            is_compilation: false,
            created_at: now,
        }
    }

    fn make_release(album_id: &str) -> DbRelease {
        let now = Utc::now();
        DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            pressing: crate::db::Pressing {
                year: Some(2024),
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            disc_id: None,
            metadata_source: crate::db::ReleaseMetadataSource::FileTags,
            metadata_source_release_id: None,
            remote: true,
            source_folder_name: None,
            content_hash: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        }
    }

    fn make_track(release_id: &str, number: i32) -> DbTrack {
        let now = Utc::now();
        DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            title: format!("Track {}", number),
            side: 1,
            track_number: Some(number),
            duration_ms: Some(180000),
            discogs_position: None,
            created_at: now,
        }
    }

    fn mb_identity(group: &str, release: &str) -> ReleaseIdentity {
        ReleaseIdentity {
            source: MetadataSource::MusicBrainz,
            source_group_id: group.to_string(),
            source_release_id: Some(release.to_string()),
        }
    }

    fn discogs_identity(master: &str, release: &str) -> ReleaseIdentity {
        ReleaseIdentity {
            source: MetadataSource::Discogs,
            source_group_id: master.to_string(),
            source_release_id: Some(release.to_string()),
        }
    }

    /// Insert an album + release with the supplied identity rows. Mirrors
    /// what the import commit path does, minus tracks/files; tests only
    /// need the identity rows reachable from the album.
    async fn insert_with_identities(
        manager: &LibraryManager,
        album: &DbAlbum,
        release: &DbRelease,
        identities: &[ReleaseIdentity],
    ) {
        let track = make_track(&release.id, 1);
        manager
            .insert_album_with_release_and_tracks(album, release, &[track], &[], &[])
            .await
            .unwrap();
        manager
            .insert_release_identities(&release.id, identities)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_exact_release_duplicate_rejected() {
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Album Title");
        let release = make_release(&album.id);
        let identities = vec![mb_identity("mb-rg-456", "mb-rel-123")];
        insert_with_identities(&manager, &album, &release, &identities).await;

        // Same MB release ID → rejected as duplicate.
        let incoming = vec![mb_identity("mb-rg-789", "mb-rel-123")];
        let result = manager.find_existing_album_for_import(&incoming).await;

        let err = result.expect_err("duplicate import should be rejected");
        assert!(
            err.contains("already in your library"),
            "Expected duplicate error, got: {err}",
        );
        assert!(
            err.contains("Album Title"),
            "Expected album title in error, got: {err}",
        );

        // Discogs side mirrors the same logic.
        let album2 = make_album("Other Album");
        let release2 = make_release(&album2.id);
        let identities2 = vec![discogs_identity("d-master-456", "d-rel-123")];
        insert_with_identities(&manager, &album2, &release2, &identities2).await;

        let incoming2 = vec![discogs_identity("d-master-456", "d-rel-123")];
        let result2 = manager.find_existing_album_for_import(&incoming2).await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn test_same_release_group_finds_existing_album() {
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Album Title");
        let release = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release,
            &[mb_identity("mb-rg-456", "mb-rel-123")],
        )
        .await;

        // Different release within the same group → merge into existing album.
        let incoming = vec![mb_identity("mb-rg-456", "mb-rel-999")];
        let merged = manager
            .find_existing_album_for_import(&incoming)
            .await
            .unwrap();
        assert_eq!(merged, Some(album.id));
    }

    #[tokio::test]
    async fn test_same_discogs_master_finds_existing_album() {
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Album Title");
        let release = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release,
            &[discogs_identity("d-master-456", "d-rel-123")],
        )
        .await;

        let incoming = vec![discogs_identity("d-master-456", "d-rel-999")];
        let merged = manager
            .find_existing_album_for_import(&incoming)
            .await
            .unwrap();
        assert_eq!(merged, Some(album.id));
    }

    #[tokio::test]
    async fn test_no_match_returns_none() {
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Existing Album");
        let release = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release,
            &[mb_identity("mb-rg-456", "mb-rel-123")],
        )
        .await;

        // Different group + different release → no match.
        let incoming = vec![mb_identity("mb-rg-999", "mb-rel-999")];
        let result = manager
            .find_existing_album_for_import(&incoming)
            .await
            .unwrap();
        assert_eq!(result, None);

        // Empty identity vec (Unknown) → skip lookup.
        let result_unknown = manager.find_existing_album_for_import(&[]).await.unwrap();
        assert_eq!(result_unknown, None);
    }

    #[tokio::test]
    async fn test_cross_source_no_false_merge() {
        let (manager, _tmp) = setup_test_db_with_artist().await;
        // Existing album holds only a Discogs identity row.
        let album = make_album("Album Title");
        let release = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release,
            &[discogs_identity("d-master-200", "d-rel-100")],
        )
        .await;

        // Unrelated MB import → should not merge.
        let incoming = vec![mb_identity("mb-rg-600", "mb-rel-500")];
        let result = manager
            .find_existing_album_for_import(&incoming)
            .await
            .unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_cross_source_merge_via_path_2() {
        // An MB-rooted release that carries both an MB and a Discogs row
        // (because MB url-rels resolved to a Discogs release at commit
        // time) is reachable from a later Discogs-only import of the same
        // master.
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Album Title");
        let release = make_release(&album.id);
        let identities = vec![
            mb_identity("mb-rg-100", "mb-rel-50"),
            discogs_identity("d-master-200", "d-rel-75"),
        ];
        insert_with_identities(&manager, &album, &release, &identities).await;

        // Later Discogs import of the same master, different pressing.
        let incoming = vec![discogs_identity("d-master-200", "d-rel-300")];
        let merged = manager
            .find_existing_album_for_import(&incoming)
            .await
            .unwrap();
        assert_eq!(merged, Some(album.id));
    }

    #[tokio::test]
    async fn test_cross_source_merge_via_path_2_inverse() {
        // Inverse of `test_cross_source_merge_via_path_2`: existing album
        // holds only a Discogs row. A later MB-rooted import resolves a
        // cross-link Discogs row pointing to the same master and thus
        // attaches via the Discogs match — even though the existing album
        // has no MB row to compare against.
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Album Title");
        let release = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release,
            &[discogs_identity("d-master-200", "d-rel-75")],
        )
        .await;

        // MB-rooted import: an MB row plus a Discogs cross-link to the
        // same Discogs master the existing album sits on.
        let incoming = vec![
            mb_identity("mb-rg-100", "mb-rel-50"),
            discogs_identity("d-master-200", "d-rel-300"),
        ];
        let merged = manager
            .find_existing_album_for_import(&incoming)
            .await
            .unwrap();
        assert_eq!(merged, Some(album.id));
    }

    #[tokio::test]
    async fn test_unknown_import_skips_lookup() {
        // Unknown imports never deduplicate against existing releases —
        // they always create a fresh album.
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Existing Album");
        let release = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release,
            &[mb_identity("mb-rg-1", "mb-rel-1")],
        )
        .await;

        // Empty identity vec — no match should be returned.
        let result = manager.find_existing_album_for_import(&[]).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_merge_release_into_existing_album() {
        // A second release with the same MB group attaches to the
        // existing album via `find_existing_album_for_import` returning
        // `Some(album_id)`. The caller redirects the new release's
        // album_id and inserts it as a sibling.
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Album Title");
        let release1 = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release1,
            &[mb_identity("mb-rg-500", "mb-rel-100")],
        )
        .await;

        // Lookup returns the existing album for a release in the same
        // MB group.
        let incoming = vec![mb_identity("mb-rg-500", "mb-rel-200")];
        let existing_album_id = manager
            .find_existing_album_for_import(&incoming)
            .await
            .unwrap();
        assert_eq!(existing_album_id, Some(album.id.clone()));

        // Insert a sibling release pointing at the existing album.
        let mut release2 = make_release(&album.id);
        release2.album_id = existing_album_id.unwrap();
        let track2 = make_track(&release2.id, 1);
        manager
            .insert_release_with_tracks(&release2, &[track2], &[], &[])
            .await
            .unwrap();
        manager
            .insert_release_identities(&release2.id, &incoming)
            .await
            .unwrap();

        let releases = manager.get_releases_for_album(&album.id).await.unwrap();
        assert_eq!(releases.len(), 2);
        let release_ids: Vec<&str> = releases.iter().map(|r| r.id.as_str()).collect();
        assert!(release_ids.contains(&release1.id.as_str()));
        assert!(release_ids.contains(&release2.id.as_str()));
    }

    // ── check_releases_in_library (identity-based status badges) ─────

    #[tokio::test]
    async fn test_check_release_in_library_exact_match() {
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Album Title");
        let release = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release,
            &[mb_identity("mb-rg-1", "mb-rel-1")],
        )
        .await;

        let checks = vec![crate::db::LibraryCheck {
            release_id: "mb-rel-1".to_string(),
            source: MetadataSource::MusicBrainz,
            source_group_id: Some("mb-rg-1".to_string()),
        }];
        let statuses = manager.check_releases_in_library(&checks).await.unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(statuses[0].release_in_library);
        assert!(statuses[0].album_in_library);
        assert_eq!(statuses[0].album_id.as_deref(), Some(album.id.as_str()));
        assert_eq!(statuses[0].album_title.as_deref(), Some("Album Title"));
    }

    #[tokio::test]
    async fn test_check_album_in_library_group_only() {
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Album Title");
        let release = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release,
            &[mb_identity("mb-rg-1", "mb-rel-1")],
        )
        .await;

        // Different release ID, same group → album_in_library only.
        let checks = vec![crate::db::LibraryCheck {
            release_id: "mb-rel-OTHER".to_string(),
            source: MetadataSource::MusicBrainz,
            source_group_id: Some("mb-rg-1".to_string()),
        }];
        let statuses = manager.check_releases_in_library(&checks).await.unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(!statuses[0].release_in_library);
        assert!(statuses[0].album_in_library);
        assert_eq!(statuses[0].album_id.as_deref(), Some(album.id.as_str()));
    }

    #[tokio::test]
    async fn test_check_release_not_in_library() {
        let (manager, _tmp) = setup_test_db_with_artist().await;

        let checks = vec![crate::db::LibraryCheck {
            release_id: "mb-rel-NONE".to_string(),
            source: MetadataSource::MusicBrainz,
            source_group_id: Some("mb-rg-NONE".to_string()),
        }];
        let statuses = manager.check_releases_in_library(&checks).await.unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(!statuses[0].release_in_library);
        assert!(!statuses[0].album_in_library);
        assert!(statuses[0].album_id.is_none());
    }

    #[tokio::test]
    async fn test_check_cross_source_doesnt_leak() {
        // An MB candidate against a Discogs-only library entry should
        // not match — different sources.
        let (manager, _tmp) = setup_test_db_with_artist().await;
        let album = make_album("Album Title");
        let release = make_release(&album.id);
        insert_with_identities(
            &manager,
            &album,
            &release,
            &[discogs_identity("d-master-1", "d-rel-1")],
        )
        .await;

        let checks = vec![crate::db::LibraryCheck {
            release_id: "mb-rel-1".to_string(),
            source: MetadataSource::MusicBrainz,
            source_group_id: Some("mb-rg-1".to_string()),
        }];
        let statuses = manager.check_releases_in_library(&checks).await.unwrap();
        assert!(!statuses[0].release_in_library);
        assert!(!statuses[0].album_in_library);
    }

    // -- Pure helpers: validation folding and artist-id remapping --

    #[test]
    fn validation_folds_validate_token_outcomes() {
        use crate::config::DiscogsValidation;
        use crate::discogs::client::DiscogsError;

        assert_eq!(
            validation_from_validate_result(Ok(())),
            DiscogsValidation::Valid
        );
        // A 401 is the one outcome that rejects the stored key.
        assert_eq!(
            validation_from_validate_result(Err(DiscogsError::InvalidApiKey)),
            DiscogsValidation::Rejected
        );
        // Anything that merely fails to confirm the key leaves it unvalidated
        // to retry — never rejected.
        for couldnt_confirm in [
            DiscogsError::RateLimit,
            DiscogsError::NotFound,
            DiscogsError::Serialization(serde_json::from_str::<i32>("nope").unwrap_err()),
        ] {
            assert_eq!(
                validation_from_validate_result(Err(couldnt_confirm)),
                DiscogsValidation::Unvalidated
            );
        }
    }

    fn track_artist(artist_id: &str) -> crate::db::DbTrackArtist {
        crate::db::DbTrackArtist {
            id: Uuid::new_v4().to_string(),
            track_id: "track-1".to_string(),
            artist_id: artist_id.to_string(),
            position: 3,
            created_at: Utc::now(),
        }
    }

    fn album_artist(artist_id: &str) -> crate::db::DbAlbumArtist {
        crate::db::DbAlbumArtist {
            id: Uuid::new_v4().to_string(),
            album_id: "album-1".to_string(),
            artist_id: artist_id.to_string(),
            position: 2,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn remap_track_artists_rewrites_id_and_preserves_the_rest() {
        let ta = track_artist("parsed-1");
        let map = std::collections::HashMap::from([("parsed-1".to_string(), "db-1".to_string())]);

        let remapped = remap_track_artists(std::slice::from_ref(&ta), &map).unwrap();
        assert_eq!(remapped.len(), 1);
        assert_eq!(remapped[0].artist_id, "db-1");
        // Everything other than the remapped artist id carries through.
        assert_eq!(remapped[0].id, ta.id);
        assert_eq!(remapped[0].track_id, "track-1");
        assert_eq!(remapped[0].position, 3);
    }

    #[test]
    fn remap_track_artists_errors_on_unmapped_id() {
        let ta = track_artist("orphan-track-artist");
        let err = remap_track_artists(std::slice::from_ref(&ta), &std::collections::HashMap::new())
            .unwrap_err();
        assert!(
            err.contains("orphan-track-artist"),
            "error should name the unmapped id: {err}"
        );
    }

    #[test]
    fn remap_album_artists_rewrites_id_and_preserves_the_rest() {
        let aa = album_artist("parsed-2");
        let map = std::collections::HashMap::from([("parsed-2".to_string(), "db-2".to_string())]);

        let remapped = remap_album_artists(std::slice::from_ref(&aa), &map).unwrap();
        assert_eq!(remapped.len(), 1);
        assert_eq!(remapped[0].artist_id, "db-2");
        assert_eq!(remapped[0].id, aa.id);
        assert_eq!(remapped[0].album_id, "album-1");
        assert_eq!(remapped[0].position, 2);
    }

    #[test]
    fn remap_album_artists_errors_on_unmapped_id() {
        let aa = album_artist("orphan-album-artist");
        let err = remap_album_artists(std::slice::from_ref(&aa), &std::collections::HashMap::new())
            .unwrap_err();
        assert!(
            err.contains("orphan-album-artist"),
            "error should name the unmapped id: {err}"
        );
    }

    fn detail_track(
        title: &str,
        side: u32,
        position: &str,
        artist: Option<&str>,
    ) -> crate::import::search::ReleaseTrack {
        crate::import::search::ReleaseTrack {
            title: title.to_string(),
            artist: artist.map(str::to_string),
            duration_ms: None,
            position: position.to_string(),
            side,
        }
    }

    /// A 2-side vinyl detail: A1, A2 on side 1; B1 on side 2. Album artist is
    /// "Artist Name".
    fn vinyl_detail() -> crate::import::search::ImportSearchReleaseDetail {
        crate::import::search::ImportSearchReleaseDetail {
            release_id: "rel-1".to_string(),
            source: MetadataSource::MusicBrainz,
            source_group_id: None,
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1969),
            format: Some("Vinyl".to_string()),
            label: Some("Label".to_string()),
            catalog_number: Some("CAT-1".to_string()),
            country: Some("US".to_string()),
            barcode: None,
            track_count: 3,
            tracks: vec![
                detail_track("A1 title", 1, "A1", None),
                detail_track("A2 title", 1, "A2", None),
                detail_track("B1 title", 2, "B1", None),
            ],
            cover_art: vec![],
        }
    }

    fn exact_choice() -> crate::import::IdentityChoice {
        crate::import::IdentityChoice::Exact {
            release_ref: crate::import::MetadataRef {
                id: "rel-1".to_string(),
                source: MetadataSource::MusicBrainz,
            },
        }
    }

    /// The editor seed must carry the same per-side track numbering the
    /// commit-side mappers assign (A1,A2 -> 1,2 ; B1 -> 1), not a release-global
    /// 1..N index. `apply_user_edit_to_seed` writes `track_number` verbatim onto
    /// the seed, so a flat index would overwrite the mapper's correct per-side
    /// numbers — corrupting any multi-side vinyl/cassette/multi-disc release
    /// edited via the Exact/Approximate confirmation pane.
    #[test]
    fn shape_user_edit_numbers_tracks_per_side() {
        let edit = shape_user_edit_from_search_detail(&vinyl_detail(), &exact_choice());
        let numbers: Vec<Option<i32>> = edit.tracks.iter().map(|t| t.track_number).collect();
        assert_eq!(numbers, vec![Some(1), Some(2), Some(1)]);
        let sides: Vec<i32> = edit.tracks.iter().map(|t| t.side).collect();
        assert_eq!(sides, vec![1, 1, 2]);
    }

    /// Exact seeds the pressing fields from the picked release; Approximate and
    /// Unknown blank them — the user didn't claim a specific pressing.
    #[test]
    fn shape_user_edit_pressing_follows_identity_choice() {
        let exact = shape_user_edit_from_search_detail(&vinyl_detail(), &exact_choice());
        assert_eq!(exact.pressing.year, Some(1969));
        assert_eq!(exact.pressing.label.as_deref(), Some("Label"));
        assert_eq!(exact.pressing.country.as_deref(), Some("US"));

        let blank = crate::import::PressingEdit::blank();
        let approx = shape_user_edit_from_search_detail(
            &vinyl_detail(),
            &crate::import::IdentityChoice::Approximate {
                release_ref: crate::import::MetadataRef {
                    id: "rel-1".to_string(),
                    source: MetadataSource::MusicBrainz,
                },
            },
        );
        assert_eq!(approx.pressing.year, blank.year);
        assert_eq!(approx.pressing.label, blank.label);
        assert_eq!(approx.pressing.country, blank.country);

        let unknown = shape_user_edit_from_search_detail(
            &vinyl_detail(),
            &crate::import::IdentityChoice::Unknown,
        );
        assert_eq!(unknown.pressing.year, blank.year);
        assert_eq!(unknown.pressing.label, blank.label);
    }

    /// A track whose source artist matches the album artist (or is missing)
    /// seeds an empty per-track override (the editor's "share the album artist"
    /// convention); a differing artist seeds that name verbatim.
    #[test]
    fn shape_user_edit_per_track_artist_override() {
        let mut detail = vinyl_detail();
        detail.tracks = vec![
            detail_track("same", 1, "A1", Some("Artist Name")),
            detail_track("none", 1, "A2", None),
            detail_track("diff", 2, "B1", Some("Guest Artist")),
        ];
        let edit = shape_user_edit_from_search_detail(&detail, &exact_choice());
        assert_eq!(edit.tracks[0].artist_names, Vec::<String>::new());
        assert_eq!(edit.tracks[1].artist_names, Vec::<String>::new());
        assert_eq!(
            edit.tracks[2].artist_names,
            vec!["Guest Artist".to_string()]
        );
    }
}
