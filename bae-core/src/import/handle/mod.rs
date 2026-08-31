use crate::import::folder_registry::{ImportFolderRegistry, WatchedFolder};
use crate::import::folder_scanner::{
    FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate,
};
use crate::import::types::{ImportCommand, ImportProgress, MetadataSource, StorageMode};
use crate::library::manager::discogs_validation_from_result as validation_from_validate_result;
use crate::library::LibraryManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

mod edits;
mod import;
mod scan;
mod search;
mod watch;

use super::candidate_runtime::CandidateRuntime;
use super::candidates::{
    CandidateRuntimeSnapshot, ImportCandidateSnapshot, WatchedFolderScanStatus,
};

#[cfg(test)]
mod tests;

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
    /// Identify pipeline transitioned to a new state. Emitted by the
    /// `identify` module; carries the full state, which the signals toolbar
    /// (the interactive badge row) is a projection of.
    IdentifyStateChanged {
        candidate_key: String,
        /// The run this state belongs to. A settled run's driver keeps
        /// broadcasting its state, so a consumer waiting on a later run of the
        /// same candidate matches on this, not on the key.
        run: crate::identify::IdentifyRunId,
        state: crate::identify::IdentifyState,
        /// The run's own priority, carried so a consumer can tell a candidate
        /// a person opened from one the background sweep picked up. The UI bus
        /// re-renders for the first and not the second.
        priority: crate::util::rate_limiter::CallPriority,
    },
    /// Full snapshot of a candidate's extracted signals (disc ID, barcodes,
    /// classified text), emitted on every transition — extraction start, each
    /// source/OCR completion, natural end, and cancellation. The reducer writes
    /// it wholesale, so it needs no partial-update logic.
    SignalsUpdated {
        candidate_key: String,
        signals: crate::signals::Signals,
        /// The extraction's own priority — same meaning as
        /// [`ImportEvent::IdentifyStateChanged`]'s.
        priority: crate::util::rate_limiter::CallPriority,
    },
    /// How much of the import queue the background sweep has answered. Both
    /// counts are the sweep's own: `total` is how many candidates it is
    /// responsible for, which is a fact about the queue rather than something a
    /// view can infer from the rows it happens to hold.
    QueueIdentifyProgress {
        identified: u32,
        total: u32,
    },
}

/// Search query — one of the three search modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSources {
    One(MetadataSource),
    Both,
}

pub enum SearchQuery {
    General {
        artist: String,
        album: String,
        sources: SearchSources,
    },
    CatalogNumber {
        catalog_number: String,
        sources: SearchSources,
    },
    Barcode {
        barcode: String,
        sources: SearchSources,
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

/// Handle for sending import requests and subscribing to progress updates.
///
/// A thin orchestration layer that dispatches prefetches, builds
/// `ImportCommand`s carrying just `MetadataRef`s, and forwards them to the
/// worker. It holds no caches of its own — the network-layer caches in
/// `crate::musicbrainz`, `crate::discogs::client`, and the Cover Art Archive
/// client serve every caller transparently.
#[derive(Clone)]
pub struct ImportServiceHandle {
    requests_tx: mpsc::UnboundedSender<crate::import::service::ImportWorkerMessage>,
    /// The worker's OS thread, joined once at teardown (`stop_and_join`). The
    /// thread holds a `LibraryManager` clone, which pins coven's exclusive
    /// store-open lock — until it exits the same library can't reopen
    /// in-process, so teardown must not return before the join. Shared across
    /// handle clones; `take`n by whichever runs the join.
    worker_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    library_manager: LibraryManager,
    clock: coven::ClockRef,
    ids: coven::IdRef,
    /// Unified event channel — all import service events go here.
    event_tx: broadcast::Sender<ImportEvent>,
    folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    runtime: CandidateRuntime,
    folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
    watcher_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    runtime_handle: tokio::runtime::Handle,
}

#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// The watched-folder list changed (queried at load, or after add/remove).
    /// Carries the full ordered list; the reducer replaces its copy.
    WatchedFoldersChanged {
        folders: Vec<WatchedFolder>,
    },
    FolderCandidate {
        candidate: FolderCandidate,
        skipped: bool,
        is_added: bool,
    },
    CandidateDiscovered {
        candidate: FolderCandidate,
        skipped: bool,
        is_added: bool,
    },
    /// A leaf folder that looks like a release but failed validation
    /// (corrupt/zero-byte audio, corrupt image, CUE referencing missing audio).
    /// The reducer surfaces it under the Skipped tab with its reason. The key is
    /// the folder path, shared with `CandidateRemoved` for reconciliation.
    InvalidCandidate(InvalidCandidate),
    /// A candidate is gone: the watcher re-scanned its folder and the release
    /// no longer resolves on disk, or the folder it belonged to stopped being
    /// watched (one event per candidate the folder held). The reducer removes
    /// it by key (the key is the candidate's folder path); the extraction
    /// service cancels the key's in-flight extraction on this event.
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
    /// The user bound one of a candidate's track sheets to an audio file, or
    /// cleared the binding. Carries the re-derived candidate, like
    /// [`Self::FolderCandidate`] — a bound sheet is a different disc, with a
    /// different track count and source-audio summary, so every index holding those
    /// replaces its copy from this rather than keeping stale ones.
    ///
    /// It also says the candidate's stored identify verdict was cleared, which
    /// is what brings it back to the queue sweep.
    CandidateBindingChanged {
        candidate: FolderCandidate,
    },
    CandidateVerdictStored {
        candidate_key: String,
    },
    /// The candidate's editable metadata draft or its provenance changed. The
    /// triage projection re-reads so the row carries the persisted state.
    CandidateMetadataChanged {
        candidate_key: String,
    },
    FolderScanStatusChanged {
        status: WatchedFolderScanStatus,
    },
    Finished,
}

/// Commands to the folder-watch reconciliation task. The scan installs OS
/// watches from blocking work as directories are reached; synchronous callers
/// only persist intent and enqueue commands.
pub(crate) enum WatcherCommand {
    Rescan(std::path::PathBuf),
    Refresh {
        path: std::path::PathBuf,
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    SetFolderReleaseDecision {
        target: (FolderReleaseDecisionKey, FolderReleaseDecision),
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Remove {
        path: std::path::PathBuf,
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Shutdown {
        completion: std::sync::mpsc::Sender<()>,
    },
}

impl ImportServiceHandle {
    pub(super) fn new(
        requests_tx: mpsc::UnboundedSender<crate::import::service::ImportWorkerMessage>,
        worker_thread: std::thread::JoinHandle<()>,
        watcher_thread: std::thread::JoinHandle<()>,
        library_manager: LibraryManager,
        clock: coven::ClockRef,
        ids: coven::IdRef,
        runtime_handle: tokio::runtime::Handle,
        watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
        event_tx: broadcast::Sender<ImportEvent>,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        runtime: CandidateRuntime,
        folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    ) -> Self {
        let handle = Self {
            requests_tx,
            worker_thread: Arc::new(Mutex::new(Some(worker_thread))),
            library_manager,
            clock,
            ids,
            event_tx,
            folder_registry,
            runtime,
            folder_state_commit,
            watcher_tx,
            watcher_thread: Arc::new(Mutex::new(Some(watcher_thread))),
            runtime_handle,
        };
        handle.start_runtime_recorder();
        handle
    }

    /// Accumulate every candidate's runtime from the bus. Lock-free by
    /// design: the only runtime a durable write gates on is the import claim,
    /// which [`Self::claim_candidate_for_import`] records directly under the
    /// commit lock rather than through an event.
    fn start_runtime_recorder(&self) {
        let mut events = self.event_tx.subscribe();
        let runtime = self.runtime.clone();
        let library_manager = self.library_manager.clone();
        self.runtime_handle.spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => runtime.record_event(&event),
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!("candidate runtime recorder dropped {count} import events");
                        library_manager.record_telemetry(
                            crate::diagnostics::TelemetryEvent::Anomaly {
                                kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                            },
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    pub(crate) fn start_candidate_services(
        &self,
    ) -> (
        crate::identify::IdentifyServiceHandle,
        crate::signals::ExtractionServiceHandle,
    ) {
        let identify = crate::identify::IdentifyServiceHandle::new(
            self.library_manager.clone(),
            self.runtime_handle.clone(),
            self.event_tx.clone(),
        );
        let extraction = crate::signals::ExtractionService::start(
            self.runtime_handle.clone(),
            self.event_tx.clone(),
            self.library_manager.clone(),
        );
        (identify, extraction)
    }

    /// Stop and join the worker thread. Idempotent (the join handle is taken
    /// once); called from `AppServicesInner`'s drop so the worker's
    /// `LibraryManager` clone — and the store-open lock it pins — is released
    /// before teardown returns. An explicit `Shutdown` message rather than
    /// channel closure: `self` holds a live sender, so the channel can't close
    /// before the join.
    pub fn stop_and_join(&self) {
        if let Some(watcher_thread) = self.watcher_thread.lock().unwrap().take() {
            let (completion, receiver) = std::sync::mpsc::channel();
            if self
                .watcher_tx
                .send(WatcherCommand::Shutdown { completion })
                .is_ok()
                && receiver.recv().is_err()
            {
                tracing::warn!("folder scan coordinator ended without acknowledging shutdown");
            }
            if let Err(panic) = watcher_thread.join() {
                tracing::warn!("folder scan coordinator panicked before join: {panic:?}");
            }
        }
        let Some(join_handle) = self.worker_thread.lock().unwrap().take() else {
            return;
        };
        if self
            .requests_tx
            .send(crate::import::service::ImportWorkerMessage::Shutdown)
            .is_err()
        {
            // The worker already exited (its loop only ends on Shutdown or a
            // panic); the join below surfaces which.
            tracing::warn!("import command channel closed before shutdown");
        }
        // A panicked worker thread already reported itself; joining from Drop
        // must not repropagate, but the panic shouldn't vanish either.
        if let Err(panic) = join_handle.join() {
            tracing::warn!("import worker thread panicked before join: {panic:?}");
        }
    }

    pub(crate) fn announce_candidate_verdict_stored(&self, candidate_key: String) {
        send_event(
            &self.event_tx,
            ImportEvent::Scan(ScanEvent::CandidateVerdictStored { candidate_key }),
        );
    }

    pub(crate) fn announce_queue_identify_progress(&self, identified: u32, total: u32) {
        send_event(
            &self.event_tx,
            ImportEvent::QueueIdentifyProgress { identified, total },
        );
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn emit_event_for_test(&self, event: ImportEvent) {
        send_event(&self.event_tx, event);
    }

    /// Claim a candidate the way committing an import does, for a test with no
    /// worker behind it. The claim is the only runtime a durable write gates
    /// on, so a test about what a claim does has to make a real one.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn claim_candidate_for_import_for_test(&self, candidate_key: &str) {
        self.claim_candidate_for_import(candidate_key).await;
    }

    /// Every key with something in flight right now.
    pub fn candidate_runtimes(&self) -> HashMap<String, CandidateRuntimeSnapshot> {
        self.runtime.all()
    }

    /// What is in flight for one key — the read a view does once when it
    /// appears, after it has subscribed to the changes.
    pub fn candidate_runtime(&self, key: &str) -> Option<CandidateRuntimeSnapshot> {
        self.runtime.get(key)
    }

    pub(crate) fn replace_automatic_identification_queue(
        &self,
        queued_keys: impl IntoIterator<Item = String>,
    ) {
        self.runtime
            .replace_automatic_identification_queue(queued_keys);
    }

    pub(crate) fn queue_explicit_identification(&self, candidate_key: &str) {
        self.runtime.queue_explicit_identification(candidate_key);
    }

    pub(crate) fn requeue_automatic_identification(&self, candidate_key: &str) {
        self.runtime.requeue_automatic_identification(candidate_key);
    }

    pub(crate) fn clear_automatic_identification(&self, candidate_key: &str) {
        self.runtime.clear_automatic_identification(candidate_key);
    }

    /// The signals extraction has found for one key so far. `None` before the
    /// first snapshot, and for a key whose run settled in an earlier session —
    /// what that run stored is on the candidate's row instead.
    ///
    /// The read a form does once when it opens, after it has subscribed to
    /// the UI bus, so a form opened partway through a run has the pool the
    /// run has built rather than an empty one.
    pub fn candidate_signals(&self, key: &str) -> Option<crate::signals::Signals> {
        self.runtime.signals(key)
    }

    /// Every key with something in flight right now, and one change per key
    /// as runs advance. The subscription is taken before the read, so no
    /// change lands between the two.
    pub fn subscribe_candidate_runtime(
        &self,
    ) -> (
        HashMap<String, CandidateRuntimeSnapshot>,
        broadcast::Receiver<super::CandidateRuntimeChange>,
    ) {
        let changes = self.runtime.subscribe();
        (self.runtime.all(), changes)
    }

    /// One candidate's pane as it reads back from the tables, with this
    /// process's runtime for it folded in.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn candidate_pane(
        &self,
        key: &str,
    ) -> Result<Option<crate::import::ImportCandidateDetail>, crate::library::LibraryError> {
        let runtime = self.runtime.get(key);
        let facts = runtime
            .as_ref()
            .map(crate::import::triage::TriageRuntimeFacts::of)
            .unwrap_or_default();
        Ok(self
            .library_manager
            .load_import_candidate(key)
            .await?
            .map(|projection| projection.resolve(&facts)))
    }

    /// The first import list `accept` admits, waiting through the query's
    /// values until one does. The list lands after the commit it reflects, so
    /// a test that just observed a scan event waits here for the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_list(
        &self,
        view: crate::import::ImportListView,
        mut accept: impl FnMut(&crate::import::ImportListProjection) -> bool,
    ) -> crate::import::ImportListProjection {
        let (initial_runtime, changes) = self.subscribe_candidate_runtime();
        let request = crate::import::ImportListRequest {
            view,
            windows: std::iter::once(crate::library::LibraryPageWindow {
                offset: 0,
                limit: u64::MAX,
            })
            .collect(),
            runtime_facts: crate::import::list::facts_of(&initial_runtime),
            upload_standing: Default::default(),
        };
        let query = self.library_manager.subscribe_import_list(request.clone());
        let runtime = self.runtime.clone();
        let subscription = crate::import::ImportListSubscription::start(
            query,
            request,
            changes,
            move || runtime.all(),
            self.library_manager.subscribe_outbox_values(),
            &self.runtime_handle,
        );
        loop {
            let snapshot = subscription
                .next()
                .await
                .expect("the import list query stays open");
            let projection = crate::import::ImportListProjection {
                windows: snapshot.windows,
                total_count: snapshot.total_count,
                summary: snapshot.summary,
            };
            if accept(&projection) {
                return projection;
            }
        }
    }

    /// One candidate by key, read from the tables with its runtime joined:
    /// the scanned folder, whether it was skipped, whether its bytes are
    /// already in the library. A key with runtime but no scanned folder — a
    /// library release being re-identified — answers with its runtime alone.
    pub async fn get_candidate(
        &self,
        key: &str,
    ) -> Result<Option<ImportCandidateSnapshot>, crate::library::LibraryError> {
        let stored = self.library_manager.load_folder_scan_item(key).await?;
        let candidate = match stored {
            Some(crate::import::folder_scanner::ScanItem::Valid(candidate)) => {
                Some((candidate, true))
            }
            Some(crate::import::folder_scanner::ScanItem::Discovered(candidate)) => {
                Some((candidate, false))
            }
            Some(crate::import::folder_scanner::ScanItem::Invalid(candidate)) => {
                return Ok(Some(ImportCandidateSnapshot::Invalid(candidate)))
            }
            Some(crate::import::folder_scanner::ScanItem::Decided { .. }) | None => None,
        };
        if let Some((candidate, actionable)) = candidate {
            let skipped = self
                .folder_registry
                .lock()
                .unwrap()
                .is_skipped(&candidate.watched_folder_path, &candidate.path)
                .map_err(|error| crate::library::LibraryError::Internal(error.to_string()))?;
            let is_added = self
                .library_manager
                .is_content_hash_imported(&candidate.files.content_hash())
                .await?;
            return Ok(Some(ImportCandidateSnapshot::Folder {
                candidate,
                runtime: self.runtime.get(key),
                actionable,
                skipped,
                is_added,
            }));
        }
        Ok(self
            .runtime
            .get(key)
            .map(|runtime| ImportCandidateSnapshot::Runtime {
                key: key.to_string(),
                runtime,
            }))
    }

    /// The stored scan entry for an actionable folder candidate at `key`, read
    /// read under the commit lock rather than through the list's own query —
    /// for a write whose check has to be exact, under the commit lock.
    pub(super) async fn stored_actionable_candidate(
        &self,
        key: &str,
    ) -> Result<Option<FolderCandidate>, crate::library::LibraryError> {
        Ok(
            match self.library_manager.load_folder_scan_item(key).await? {
                Some(crate::import::folder_scanner::ScanItem::Valid(candidate)) => Some(candidate),
                Some(
                    crate::import::folder_scanner::ScanItem::Discovered(_)
                    | crate::import::folder_scanner::ScanItem::Invalid(_)
                    | crate::import::folder_scanner::ScanItem::Decided { .. },
                )
                | None => None,
            },
        )
    }

    /// Claim `candidate_key` for an import that is about to be queued.
    ///
    /// Takes the folder-state commit lock, which
    /// [`Self::save_candidate_verdict_if_current`] holds across *both* its
    /// check and its write. So by the time this returns, either that write has
    /// already landed or it has yet to read the candidate and will find it
    /// claimed — there is no interval in which a verdict is stored for a
    /// candidate whose import has been committed to.
    pub(crate) async fn claim_candidate_for_import(&self, candidate_key: &str) {
        let _commit = self.folder_state_commit.lock().await;
        self.runtime.claim_for_import(candidate_key);
    }

    async fn release_import_claim(&self, candidate_key: &str) {
        let _commit = self.folder_state_commit.lock().await;
        self.runtime.release_import_claim(candidate_key);
    }

    /// Store one candidate's verdict, unless the candidate has moved on from
    /// the shape the verdict describes — its files were re-decided, it was
    /// skipped, it is already in the library, or an import has claimed it.
    ///
    /// The commit lock spans the check and the write, and everything that can
    /// invalidate a verdict — a scan, a file re-decision, a skip, an import
    /// claim — is written under the same lock, so a `true` return means the row
    /// describes the candidate as it was at the moment it landed. The check
    /// reads the stored entry rather than waiting on the list, which is a
    /// query that lands after the commit it reflects, and this has to see the
    /// commit.
    pub(crate) async fn save_candidate_verdict_if_current(
        &self,
        candidate_key: &str,
        row: &crate::db::NewImportCandidateVerdict,
    ) -> Result<bool, crate::library::LibraryError> {
        let _commit = self.folder_state_commit.lock().await;
        let Some(candidate) = self.sweepable_candidate(candidate_key).await? else {
            return Ok(false);
        };
        if candidate.files.content_hash() != row.content_hash
            || candidate.file_edit_revision != row.expected_edit_revision
        {
            return Ok(false);
        }
        self.library_manager
            .save_import_candidate_verdict(row)
            .await
    }

    /// The candidate at `key` as the queue sweep is responsible for it: a
    /// stored, actionable folder that is not skipped, not already in the
    /// library, and not claimed by an import. Read from the tables and the
    /// runtime right now rather than through the list — the sweep asks this
    /// straight after the event that changed the answer, and the list's query
    /// lands after the commit it reflects.
    pub(crate) async fn sweepable_candidate(
        &self,
        key: &str,
    ) -> Result<Option<FolderCandidate>, crate::library::LibraryError> {
        let Some(candidate) = self.stored_actionable_candidate(key).await? else {
            return Ok(None);
        };
        let initial_source = self
            .library_manager
            .load_import_candidate(key)
            .await?
            .map(|projection| projection.initial_metadata_source);
        if initial_source != Some(crate::config::DefaultImportMetadataSource::FindOnline) {
            return Ok(None);
        }
        let skipped = self
            .folder_registry
            .lock()
            .unwrap()
            .is_skipped(&candidate.watched_folder_path, &candidate.path)
            .map_err(|error| crate::library::LibraryError::Internal(error.to_string()))?;
        if skipped
            || self
                .library_manager
                .is_content_hash_imported(&candidate.files.content_hash())
                .await?
            || self
                .runtime
                .get(key)
                .is_some_and(|runtime| runtime.import.is_some())
        {
            return Ok(None);
        }
        Ok(Some(candidate))
    }
}

/// Remap the parsed (temporary) IDs a link row points at to their actual DB IDs.
///
/// A `ParsedAlbum`'s link rows reference artist and work IDs minted during
/// parsing; reconcile may have resolved those to existing DB rows. `label` names
/// the endpoint being remapped in the unmapped-ID error.
pub(crate) fn remap_links<T: Clone>(
    links: &[T],
    id_map: &HashMap<String, String>,
    label: &str,
    target_id: impl Fn(&T) -> &str,
    assign_target_id: impl Fn(&mut T, String),
) -> Result<Vec<T>, crate::import::ImportError> {
    links
        .iter()
        .map(|link| {
            let parsed_id = target_id(link);
            let actual_id =
                id_map
                    .get(parsed_id)
                    .ok_or_else(|| crate::import::ImportError::Internal {
                        detail: format!("{label} ID {parsed_id} not found in the import's ID map"),
                    })?;
            let mut remapped = link.clone();
            assign_target_id(&mut remapped, actual_id.clone());
            Ok(remapped)
        })
        .collect()
}

/// Project a parsed album (mapper output) into the editor's `ReleaseUserEdit`
/// shape. The one way the edit-metadata form is seeded, from every path: a
/// source-backed import (the prefetch's `seed`), a File Tags import's local
/// evidence, and reset-to-source's cached source payload.
///
/// It projects the very `ParsedAlbum` the commit worker applies the editor's
/// overlay onto, which is what lets `apply_user_edit_to_seed` tell an untouched
/// field from an edited one. Seeding the editor from any other shape — the
/// picker's release detail, say — makes an untouched artist list read as a
/// deletion and drops the release's secondary album artists.
///
/// Track artist names are emitted positionally per existing `track_artists` row;
/// an empty per-track list means "share the album artist" in the editor's
/// convention.
pub fn parsed_album_to_user_edit(parsed: &super::ParsedAlbum) -> crate::import::ReleaseUserEdit {
    // A ParsedAlbum is self-consistent by construction (the mapper builds its
    // artists and junctions together), so a missing reference is a bug here, not
    // a user-facing error.
    let album_artist_assignments = crate::import::artist_assignments::album_artist_assignments(
        &parsed.artists,
        &parsed.album_artists,
        &parsed.album.artist_id,
    )
    .expect("ParsedAlbum album_artists reference its own artists");

    let tracks = parsed
        .tracks
        .iter()
        .map(|t| {
            let artist_assignments = crate::import::artist_assignments::track_artist_assignments(
                &parsed.artists,
                &parsed.track_artists,
                &t.id,
            )
            .expect("ParsedAlbum track_artists reference its own artists");
            crate::import::TrackUserEdit {
                title: t.title.clone(),
                side: t.side,
                track_number: t.track_number,
                artist_assignments,
                // A seed says what the release is, not which of the folder's
                // audio backs each track; the track slots settle that, and
                // stamp the binding onto the rows they hand to the editor.
                file: None,
            }
        })
        .collect();

    crate::import::ReleaseUserEdit {
        album_title: parsed.album.title.clone(),
        album_artist_assignments,
        album_year: parsed.album.year,
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

/// The files the import pipeline writes rows for and accounts bytes against, in
/// the release's own `relative_path` order.
///
/// Reads [`CategorizedFiles::release_files`], the same iterator
/// [`CategorizedFiles::content_hash`] covers — the payload and the fingerprint
/// that identifies it are one set by construction.
pub(crate) fn flatten_categorized_files(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Vec<crate::import::folder_scanner::ScannedFile> {
    categorized.release_files().cloned().collect()
}
