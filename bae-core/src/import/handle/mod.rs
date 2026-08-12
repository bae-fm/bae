use crate::import::folder_registry::{ImportFolderRegistry, WatchedFolder};
use crate::import::folder_scanner::{
    FolderCandidate, FolderReleaseBoundary, FolderReleaseDecision, FolderReleaseDecisionKey,
    InvalidCandidate,
};
use crate::import::types::{ImportCommand, ImportProgress, MetadataSource, StorageMode};
use crate::library::manager::discogs_validation_from_result as validation_from_validate_result;
use crate::library::LibraryManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

mod import;
mod scan;
mod search;
mod watch;

use super::candidate_store::{
    CandidateStore, ImportCandidateSnapshot, ImportCandidatesSnapshot, WatchedFolderScanStatus,
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
    /// Per-track loudness measurement progress for an importing candidate: a
    /// high-frequency tick routed to a native leaf view rather than the candidate
    /// row's coarse step. Separate from `ImportProgress` so it bypasses the
    /// release/import progress subscribers — it carries the candidate key, not a
    /// release or import id.
    ImportLoudnessProgress {
        candidate_key: String,
        tracks_done: u32,
        tracks_total: u32,
        /// Overall scan progress 0..1 for the determinate bar; advances within a
        /// track as it's measured, not just at track boundaries.
        fraction: f32,
    },
    /// Identify pipeline transitioned to a new state. Emitted by the
    /// `identify` module; carries the full state payload plus the pre-shaped
    /// signals toolbar (the interactive badge row) projected from the same
    /// transition, so the UI renders both from one event.
    IdentifyStateChanged {
        candidate_key: String,
        state: crate::identify::IdentifyState,
        toolbar: Vec<crate::identify::ToolbarSignal>,
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

impl ImportServiceHandle {
    pub(crate) fn record_candidate_event(&self, event: &ImportEvent) {
        self.candidate_store.record_event(event);
    }
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
    candidate_store: CandidateStore,
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
    FolderReleaseBoundary(FolderReleaseBoundary),
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
    /// different track count and format label, so every index holding those
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
    /// The user chose an identity for the candidate — a pressing, or its own
    /// tags — and the choice was persisted. The triage projection re-reads so
    /// the row carries it.
    CandidateIdentityPicked {
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
        candidate_store: CandidateStore,
        folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    ) -> Self {
        Self {
            requests_tx,
            worker_thread: Arc::new(Mutex::new(Some(worker_thread))),
            library_manager,
            clock,
            ids,
            event_tx,
            folder_registry,
            candidate_store,
            folder_state_commit,
            watcher_tx,
            watcher_thread: Arc::new(Mutex::new(Some(watcher_thread))),
            runtime_handle,
        }
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

    /// Broadcast a resumed identify state — a stored verdict standing back up
    /// as the state opening its candidate shows, with no run behind it. Rides
    /// the same event every live driver's transitions do, so the runtime
    /// recorder and both UIs consume it identically. The toolbar is empty
    /// because a resumed state has no live signals to badge (see
    /// [`crate::identify::TerminalVerdict::resume_state`]); `Interactive`
    /// because only a selection resumes one.
    pub(crate) fn broadcast_resumed_identify_state(
        &self,
        candidate_key: String,
        state: crate::identify::IdentifyState,
    ) {
        send_event(
            &self.event_tx,
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                state,
                toolbar: Vec::new(),
                priority: crate::util::rate_limiter::CallPriority::Interactive,
            },
        );
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

    pub fn get_import_candidates(&self) -> ImportCandidatesSnapshot {
        let watched_folders = self.watched_folders();
        self.candidate_store.snapshot(watched_folders)
    }

    pub fn get_candidate(&self, key: &str) -> Option<ImportCandidateSnapshot> {
        self.candidate_store.get(key)
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
        self.candidate_store.claim_for_import(candidate_key);
    }

    async fn release_import_claim(&self, candidate_key: &str) {
        let _commit = self.folder_state_commit.lock().await;
        self.candidate_store.release_import_claim(candidate_key);
    }

    /// Store one candidate's verdict, unless the candidate has moved on from
    /// the shape the verdict describes — its files were re-decided, it was
    /// skipped, it is already in the library, or an import has claimed it.
    ///
    /// The commit lock spans the check and the write, and everything that can
    /// invalidate a verdict — a scan, a file re-decision, a skip, an import
    /// claim — is written under the same lock, so a `true` return means the row
    /// describes the candidate as it was at the moment it landed. (The UI event
    /// bus also records import progress into this state without the lock, but
    /// only ever onto a candidate an import already claimed.)
    pub(crate) async fn save_candidate_verdict_if_current(
        &self,
        candidate_key: &str,
        row: &crate::db::NewImportCandidateVerdict,
    ) -> Result<bool, crate::library::LibraryError> {
        let _commit = self.folder_state_commit.lock().await;
        let current = matches!(
            self.candidate_store.get(candidate_key),
            Some(ImportCandidateSnapshot::Folder {
                candidate,
                runtime,
                actionable: true,
                skipped: false,
                is_added: false,
            }) if candidate.files.content_hash() == row.content_hash
                && candidate.file_edit_revision == row.expected_edit_revision
                && runtime.import_status.is_none()
        );
        if !current {
            return Ok(false);
        }
        self.library_manager
            .save_import_candidate_verdict(row)
            .await
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
/// source-backed import (the prefetch's `seed`), an Unknown import's local
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
    let album_artist_names = crate::import::artist_names::album_artist_names(
        &parsed.artists,
        &parsed.album_artists,
        &parsed.album.artist_id,
    )
    .expect("ParsedAlbum album_artists reference its own artists");

    let tracks = parsed
        .tracks
        .iter()
        .map(|t| {
            let artist_names = crate::import::artist_names::track_artist_names(
                &parsed.artists,
                &parsed.track_artists,
                &t.id,
            )
            .expect("ParsedAlbum track_artists reference its own artists");
            crate::import::TrackUserEdit {
                title: t.title.clone(),
                side: t.side,
                track_number: t.track_number,
                artist_names,
                // A seed says what the release is, not which of the folder's
                // audio backs each track; the track slots settle that, and
                // stamp the binding onto the rows they hand to the editor.
                file: None,
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

/// Shape the prefetched editor seed for the user's identity claim:
///
/// - **Exact**: `pressing` stays as the picked release has it.
/// - **Approximate** / **Unknown**: `pressing` is `PressingEdit::blank()` — the
///   user didn't claim a specific pressing, so showing them the source's
///   pressing data would imply a claim they never made. They can still fill in
///   fields they know, and the overlay carries those edits to commit.
///
/// Everything else in the seed comes from the release itself (see
/// [`crate::import::search::ImportReleasePrefetch`]) and is the same under every
/// choice, so flipping the claim re-runs this over the kept seed instead of
/// re-fetching.
///
/// The UI calls this rather than branching on `IdentityChoice` itself — the
/// bridge stays thin.
pub fn shape_user_edit_for_choice(
    seed: &super::ReleaseUserEdit,
    choice: &super::IdentityChoice,
) -> super::ReleaseUserEdit {
    let mut edit = seed.clone();
    match choice {
        super::IdentityChoice::Exact { .. } => {}
        super::IdentityChoice::Approximate { .. } | super::IdentityChoice::Unknown => {
            edit.pressing = super::PressingEdit::blank();
        }
    }
    edit
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
