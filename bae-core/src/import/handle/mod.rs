use crate::import::folder_scanner::{
    FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate,
};
use crate::import::types::{Catalog, ImportCommand, ImportProgress};
use crate::import::watched_folder::WatchedFolder;
use crate::library::manager::discogs_validation_from_result as validation_from_validate_result;
use crate::library::LibraryManager;
use crate::util::worker_thread::WorkerThread;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

mod candidate_facts_watch;
mod combinations;
mod edits;
mod identification;
mod import;
mod lookup_choices;
mod reset;
mod scan;
mod search;
mod session;
mod watch;

use super::candidate_runtime::CandidateRuntime;
use super::candidates::{
    CandidateRuntimeSnapshot, ImportCandidateSnapshot, WatchedFolderScanStatus,
};

#[cfg(test)]
mod tests;

/// The import event channel and the candidate runtime it records into.
///
/// Every event is recorded in the runtime before it is broadcast, so a
/// subscriber that hears an event and then asks the runtime finds the event's
/// effect already there.
#[derive(Clone)]
pub struct ImportEventBus {
    sender: broadcast::Sender<ImportEvent>,
    runtime: CandidateRuntime,
    /// Stops the thread sending one chosen progress event until a test lets
    /// it through, so the test can act partway through an import.
    #[cfg(test)]
    progress_hold: Arc<ProgressHold>,
}

/// Where [`ImportEventBus::hold_progress_at`] stops a sender.
#[cfg(test)]
#[derive(Default)]
struct ProgressHold {
    state: std::sync::Mutex<HoldState>,
    changed: std::sync::Condvar,
}

#[cfg(test)]
#[derive(Default, Clone, Copy, PartialEq)]
enum HoldState {
    #[default]
    Off,
    /// The next measured percent of this phase stops its sender.
    Armed(crate::import::ImportPhase),
    /// A sender is stopped, its event neither recorded nor broadcast.
    Held,
    /// The stopped sender was let go.
    Released,
}

impl ImportEventBus {
    /// A bus whose subscribers may fall `capacity` events behind, recording
    /// into `runtime`, which announces its identification count on this bus.
    pub fn new(capacity: usize, runtime: CandidateRuntime) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        runtime.announce_on(sender.clone());
        Self {
            sender,
            runtime,
            #[cfg(test)]
            progress_hold: Arc::default(),
        }
    }

    /// Record `event` in the runtime, then broadcast it. The bus lives as long
    /// as the app, so having no subscriber is worth a warning.
    pub fn send(&self, event: ImportEvent) {
        #[cfg(test)]
        self.wait_if_held(&event);
        self.runtime.record_event(&event);
        if let Err(error) = self.sender.send(event) {
            warn!("import event broadcast had no subscribers: {error}");
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ImportEvent> {
        self.sender.subscribe()
    }

    /// Stop the first thread that sends a measured percent of `phase` before
    /// its event is recorded or broadcast, until [`Self::release_progress`].
    ///
    /// Only a percent above zero: those come from the threads doing the work,
    /// while a zero comes from the worker, which must stay free to hear a
    /// cancel.
    #[cfg(test)]
    pub(crate) fn hold_progress_at(&self, phase: crate::import::ImportPhase) {
        *self.progress_hold.state.lock().unwrap() = HoldState::Armed(phase);
    }

    /// Wait until a sender is stopped where [`Self::hold_progress_at`] said.
    #[cfg(test)]
    pub(crate) async fn progress_held(&self) {
        let hold = self.progress_hold.clone();
        tokio::task::spawn_blocking(move || {
            let state = hold.state.lock().unwrap();
            let _held = hold
                .changed
                .wait_while(state, |state| *state != HoldState::Held)
                .unwrap();
        })
        .await
        .unwrap();
    }

    /// Let the stopped sender go on.
    #[cfg(test)]
    pub(crate) fn release_progress(&self) {
        *self.progress_hold.state.lock().unwrap() = HoldState::Released;
        self.progress_hold.changed.notify_all();
    }

    #[cfg(test)]
    fn wait_if_held(&self, event: &ImportEvent) {
        let ImportEvent::ImportProgress {
            progress:
                ImportProgress::Progress {
                    phase,
                    percent: Some(percent),
                    ..
                },
            ..
        } = event
        else {
            return;
        };
        let mut state = self.progress_hold.state.lock().unwrap();
        if *state != HoldState::Armed(*phase) || *percent == 0 {
            return;
        }
        *state = HoldState::Held;
        self.progress_hold.changed.notify_all();
        let _released = self
            .progress_hold
            .changed
            .wait_while(state, |state| *state == HoldState::Held)
            .unwrap();
    }
}

/// Every event the import service emits, on one channel.
#[derive(Debug, Clone)]
pub enum ImportEvent {
    Scan(ScanEvent),
    ImportProgress {
        candidate_key: String,
        progress: ImportProgress,
    },
    /// An identify run's new state, in full; the signals toolbar shows a
    /// projection of it.
    IdentifyStateChanged {
        candidate_key: String,
        /// The run this state belongs to. A settled run keeps broadcasting, so
        /// a consumer waiting on a later run of the candidate matches on this,
        /// not the key.
        run: crate::identify::IdentifyRunId,
        state: crate::identify::IdentifyState,
        /// The run's priority, which tells a candidate a person opened from
        /// one the automatic admission picked up.
        priority: crate::util::rate_limiter::CallPriority,
    },
    /// Full snapshot of a candidate's extracted signals (disc ID, barcodes,
    /// classified text), sent on every change of the extraction, including an
    /// abort, which fails every signal.
    SignalsUpdated {
        candidate_key: String,
        /// The identify run this snapshot was extracted for. Runs of one
        /// candidate share its key, so a run's verdict is stored beside only
        /// the snapshots that name that run.
        run: crate::identify::IdentifyRunId,
        signals: crate::signals::Signals,
        /// Where the artwork pass feeding the snapshot has got to, shown as the
        /// run's step. Kept outside the snapshot because a stored snapshot has
        /// no pass.
        artwork: crate::signals::ArtworkScan,
        /// The extraction's priority, as in
        /// [`ImportEvent::IdentifyStateChanged`].
        priority: crate::util::rate_limiter::CallPriority,
    },
    /// How many of the running batch of identifications have ended, out of how
    /// many, whoever started them; `(0, 0)` is none running. Counted by the
    /// candidate runtime, since a view's rows may be filtered.
    IdentificationProgress {
        identified: u32,
        total: u32,
    },
    /// How many imports are waiting for the worker or running, announced by
    /// the candidate runtime whenever one starts or ends.
    ImportsInFlight {
        count: u32,
    },
}

/// Search results grouped by release group, with the per-release library dupe
/// statuses the UI looks up by release id.
#[derive(Debug, Clone)]
pub struct GroupedSearchResults {
    pub groups: Vec<crate::import::release_group::ReleaseGroup>,
    pub statuses: Vec<crate::db::LibraryStatus>,
}

/// What `save_discogs_token` did with a submitted key after checking it with
/// Discogs.
///
/// - `Valid` — Discogs accepted the key; it is stored.
/// - `Unvalidated` — Discogs was unreachable or rate-limited; the key is stored
///   and re-checked later.
/// - `Rejected` — Discogs returned 401; nothing is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscogsSaveOutcome {
    Valid,
    Unvalidated,
    Rejected,
}

/// Handle for sending import requests and subscribing to progress updates.
#[derive(Clone)]
pub struct ImportServiceHandle {
    /// The import worker. Its `LibraryManager` clone holds coven's store-open
    /// lock, so teardown must not return before it is joined.
    worker: WorkerThread<crate::import::service::ImportWorkerMessage>,
    library_manager: LibraryManager,
    /// The one writer of candidates' stored state.
    preparations: crate::import::CandidatePreparations,
    clock: coven::ClockRef,
    ids: coven::IdRef,
    /// Reads a folder's audio files for their embedded tags; the scan's
    /// pre-fill uses the same reader.
    file_tags: std::sync::Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    /// The import service's event channel.
    event_tx: ImportEventBus,
    /// What the bus's events have said about every candidate.
    runtime: CandidateRuntime,
    /// The identify driver and the extraction feeding it, held here because
    /// the commands that decide a candidate end its identification as part of
    /// their own write.
    identify: crate::identify::IdentifyServiceHandle,
    extraction: crate::signals::ExtractionServiceHandle,
    folder_state_commit: crate::import::FolderStateCommit,
    import_cancels: crate::import::import_cancel::ImportCancels,
    /// Where a release a combination makes goes to be identified on its own.
    automatic_admissions: crate::import::identification::AutomaticAdmissions,
    watcher: WorkerThread<WatcherCommand>,
    runtime_handle: tokio::runtime::Handle,
}

#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// The full ordered watched-folder list, sent at load and after each
    /// change.
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
    /// A folder that looks like a release but failed validation (corrupt or
    /// empty audio, corrupt image, a CUE sheet naming missing audio), keyed by
    /// its folder path.
    InvalidCandidate(InvalidCandidate),
    /// A candidate is gone: a rescan no longer finds it, or its watched folder
    /// was removed (one event per candidate). The extraction service cancels
    /// the key's running extraction on this event.
    CandidateRemoved {
        candidate_key: String,
    },
    /// A person skipped or unskipped a candidate.
    CandidateSkipChanged {
        candidate_key: String,
        skipped: bool,
    },
    /// A person bound one of a candidate's track sheets to an audio file, or
    /// cleared the binding. Carries the re-derived candidate, since a bound
    /// sheet is a different disc, and says its stored identify verdict was
    /// cleared.
    CandidateBindingChanged {
        candidate: FolderCandidate,
    },
    /// The candidate's metadata draft or its provenance changed.
    CandidateMetadataChanged {
        candidate_key: String,
    },
    FolderScanStatusChanged {
        status: WatchedFolderScanStatus,
    },
    Finished,
}

/// Commands to the folder scan coordinator.
pub(crate) enum WatcherCommand {
    /// Read every watched folder the store lists.
    RescanAll,
    Rescan(std::path::PathBuf),
    Refresh {
        path: std::path::PathBuf,
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    SetFolderReleaseDecision {
        target: (FolderReleaseDecisionKey, FolderReleaseDecision),
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// Stop watching `roots` and, given `parent` (the folder holding exactly
    /// them), watch it in their place. `completion` hears once the change is
    /// stored, or why it was not.
    Remove {
        roots: Vec<std::path::PathBuf>,
        parent: Option<std::path::PathBuf>,
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Shutdown {
        completion: std::sync::mpsc::Sender<()>,
    },
}

impl ImportServiceHandle {
    /// The release stored at `key`, when it may be worked on; a grouping's
    /// release that cannot be fails with why.
    pub async fn get_release_candidate(
        &self,
        key: &str,
    ) -> Result<Option<crate::import::folder_scanner::FolderCandidate>, crate::import::ImportError>
    {
        self.library_manager
            .load_release_candidate(key)
            .await?
            .map_err(|reason| crate::import::ImportError::GroupingBlocked { reason })
    }

    pub(super) fn new(
        worker: WorkerThread<crate::import::service::ImportWorkerMessage>,
        watcher: WorkerThread<WatcherCommand>,
        services: crate::import::ImportServices,
        runtime: CandidateRuntime,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        let crate::import::ImportServices {
            event_tx,
            library_manager,
            preparations,
            clock,
            ids,
            file_tags,
            directories: _,
            folder_state_commit,
            import_cancels,
            automatic_admissions,
        } = services;
        let identify = crate::identify::IdentifyServiceHandle::new(
            library_manager.clone(),
            runtime_handle.clone(),
            event_tx.clone(),
        );
        let extraction = crate::signals::ExtractionService::start(
            runtime_handle.clone(),
            event_tx.clone(),
            library_manager.clone(),
        );
        Self {
            worker,
            library_manager,
            preparations,
            clock,
            ids,
            file_tags,
            event_tx,
            runtime,
            identify,
            extraction,
            folder_state_commit,
            import_cancels,
            automatic_admissions,
            watcher,
            runtime_handle,
        }
    }

    /// The releases this service finds, for the one identification queue that
    /// takes them.
    pub(crate) fn take_automatic_admissions(
        &self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<String>> {
        self.automatic_admissions.take()
    }

    /// Stop and join both worker threads; later calls do nothing. Called from
    /// `AppServicesInner`'s drop so the store-open lock is released before
    /// teardown returns. Each is sent `Shutdown` because `self` holds a live
    /// sender, so neither channel closes on its own.
    pub fn stop_and_join(&self) {
        self.watcher.stop_and_join(|watcher_tx| {
            let (completion, receiver) = std::sync::mpsc::channel();
            if watcher_tx
                .send(WatcherCommand::Shutdown { completion })
                .is_ok()
                && receiver.recv().is_err()
            {
                tracing::warn!("folder scan coordinator ended without acknowledging shutdown");
            }
        });
        self.worker.stop_and_join(|requests_tx| {
            if requests_tx
                .send(crate::import::service::ImportWorkerMessage::Shutdown)
                .is_err()
            {
                // The worker already exited (its loop only ends on Shutdown or
                // a panic); the join surfaces which.
                tracing::warn!("import command channel closed before shutdown");
            }
        });
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn emit_event_for_test(&self, event: ImportEvent) {
        self.event_tx.send(event);
    }

    /// Claim a candidate the way committing an import does, for a test with no
    /// worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn claim_candidate_for_import_for_test(&self, candidate_key: &str, import_id: &str) {
        self.claim_candidate_for_import(candidate_key, import_id)
            .await;
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

    /// The query the pane subscribes to for one candidate, before this
    /// process's runtime is folded in, for a test that watches it change.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_candidate_pane(
        &self,
        key: &str,
    ) -> coven::ReconfigurableLiveQuery<
        Option<String>,
        Option<crate::import::ImportCandidateDetailProjection>,
    > {
        self.library_manager
            .subscribe_import_candidate(Some(key.to_string()))
    }

    /// The import list for `view` as one window over every item, for a test
    /// that watches it change.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_whole_list(
        &self,
        view: crate::import::ImportListView,
    ) -> crate::import::ImportListSubscription {
        let runtime_facts = self.watch_runtime_facts();
        let request = crate::import::ImportListRequest {
            live_matches: crate::import::PendingFilter::live_matches(
                view.pending_filter,
                runtime_facts.facts(),
            ),
            view,
            windows: std::iter::once(crate::library::LibraryPageWindow {
                offset: 0,
                limit: u64::MAX,
            })
            .collect(),
            upload_standing: Default::default(),
        };
        let query = self.library_manager.subscribe_import_list(request.clone());
        crate::import::ImportListSubscription::start(
            query,
            self.library_manager.subscribe_folder_scan_progress(),
            request,
            self.library_manager.subscribe_outbox_values(),
            runtime_facts,
            &self.runtime_handle,
        )
    }

    /// The first import list `accept` admits, waiting through the query's
    /// values until one does. The list lands after the commit it reflects, so
    /// a test that just observed a scan event waits here for the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_list(
        &self,
        view: crate::import::ImportListView,
        mut accept: impl FnMut(&crate::import::ImportListSnapshot) -> bool,
    ) -> crate::import::ImportListSnapshot {
        let subscription = self.subscribe_whole_list(view);
        loop {
            let snapshot = subscription
                .next()
                .await
                .expect("the import list query stays open");
            if accept(&snapshot) {
                return snapshot;
            }
        }
    }

    /// One candidate by key, read from the tables with its runtime joined. A
    /// key with runtime but no scanned folder (a library release being
    /// re-identified) answers with its runtime alone.
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
            Some(
                crate::import::folder_scanner::ScanItem::Decided { .. }
                | crate::import::folder_scanner::ScanItem::Sidecar(_),
            )
            | None => None,
        };
        if let Some((candidate, actionable)) = candidate {
            let standing = self.candidate_standing(key, &candidate).await?;
            return Ok(Some(ImportCandidateSnapshot::Folder {
                candidate,
                runtime: self.runtime.get(key),
                actionable,
                skipped: standing.skipped,
                is_added: standing.imported,
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

    /// The stored candidate whose preparation may still be changed. A caller
    /// about to write holds `folder_state_commit` across this and the write so
    /// an import claim cannot land between them; without the lock it only
    /// refuses early.
    pub(super) async fn editable_candidate_for_commit(
        &self,
        key: &str,
    ) -> Result<crate::import::folder_scanner::FolderCandidate, crate::import::ImportError> {
        let candidate = self.get_release_candidate(key).await?.ok_or_else(|| {
            crate::import::ImportError::Internal {
                detail: format!("{key} is not an actionable folder candidate"),
            }
        })?;
        self.candidate_standing(key, &candidate).await?.editable()?;
        Ok(candidate)
    }

    /// Whether the stored candidate at `key` is set aside, already in the
    /// library, or claimed by a running import. Read from the tables and the
    /// runtime because the list's query lands after the commit it reflects.
    pub(crate) async fn candidate_standing(
        &self,
        key: &str,
        candidate: &crate::import::folder_scanner::FolderCandidate,
    ) -> Result<crate::import::CandidateStanding, crate::library::LibraryError> {
        Ok(crate::import::CandidateStanding {
            skipped: self
                .library_manager
                .is_release_candidate_skipped(candidate)
                .await?,
            imported: self
                .library_manager
                .is_content_hash_imported(&candidate.files.content_hash())
                .await?,
            claimed: self
                .runtime
                .get(key)
                .is_some_and(|runtime| runtime.import.is_some()),
        })
    }

    /// Take `folder_state_commit` and recheck under it that the candidate is
    /// still at the revision the edit was prepared from. The caller holds the
    /// guard across its write; `operation` names it in the lock's log lines.
    pub(super) async fn commit_lock_for_revision(
        &self,
        operation: &'static str,
        key: &str,
        expected_content_hash: &str,
        expected_file_edit_revision: u64,
    ) -> Result<crate::import::FolderStateCommitGuard, crate::import::ImportError> {
        let commit = self.folder_state_commit.lock(operation).await;
        let candidate = self.editable_candidate_for_commit(key).await?;
        if candidate.files.content_hash() != expected_content_hash
            || candidate.file_edit_revision != expected_file_edit_revision
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!("{key} changed before its edit could be stored"),
            });
        }
        Ok(commit)
    }

    /// Claim `candidate_key` for the test import `import_id`, under the lock
    /// [`Self::save_candidate_verdict_if_current`] holds across its check and
    /// write, so no verdict is stored for a claimed candidate.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn claim_candidate_for_import(&self, candidate_key: &str, import_id: &str) {
        let _commit = self
            .folder_state_commit
            .lock("claim a candidate for a test")
            .await;
        self.runtime
            .claim_for_import(candidate_key, import_id)
            .expect("a test claims a candidate no import owns");
    }

    async fn release_import_claim(&self, candidate_key: &str, import_id: &str) {
        let _commit = self
            .folder_state_commit
            .lock("release an import claim")
            .await;
        self.runtime.release_import_claim(candidate_key, import_id);
    }

    /// Run a durable write to completion once it has been asked for.
    ///
    /// The write runs as a task on the import runtime and this only waits on
    /// it, so dropping the caller's future ends the wait and nothing else.
    /// Coven commits a write whether or not the future that asked survives, so
    /// the bookkeeping and lock release after it must run too. Fails through
    /// `From<JoinError>` only when the task panicked or the runtime is gone.
    pub(crate) async fn committed<V, E>(
        &self,
        write: impl std::future::Future<Output = Result<V, E>> + Send + 'static,
    ) -> Result<V, E>
    where
        V: Send + 'static,
        E: From<tokio::task::JoinError> + Send + 'static,
    {
        self.runtime_handle.spawn(write).await?
    }

    /// Store the verdict `run` reached, unless the candidate can no longer be
    /// answered or its files are not the ones the verdict answers. The check
    /// and the write share the commit lock, and the write ends `run`'s pending
    /// save itself, so a caller torn down mid-write leaves nothing pending.
    pub(crate) async fn save_candidate_verdict_if_current(
        &self,
        candidate_key: &str,
        run: crate::identify::IdentifyRunId,
        row: &crate::db::NewImportCandidateVerdict,
    ) -> Result<bool, crate::library::LibraryError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_string();
        let row = row.clone();
        self.committed(async move {
            let wrote = this
                .save_candidate_verdict_if_current_write(&candidate_key, &row)
                .await;
            match &wrote {
                Ok(_) => this.runtime.end_identification_answer(&candidate_key, run),
                Err(error) => {
                    this.runtime
                        .fail_identification(&candidate_key, run, error.to_string())
                }
            }
            wrote
        })
        .await
    }

    async fn save_candidate_verdict_if_current_write(
        &self,
        candidate_key: &str,
        row: &crate::db::NewImportCandidateVerdict,
    ) -> Result<bool, crate::library::LibraryError> {
        let _commit = self
            .folder_state_commit
            .lock("store an identification verdict")
            .await;
        let Some(candidate) = self.answerable_candidate(candidate_key).await? else {
            return Ok(false);
        };
        // A verdict on other files than the candidate has now would describe
        // a shape it no longer has.
        if candidate.files.content_hash() != row.content_hash
            || candidate.file_edit_revision != row.file_edit_revision
        {
            return Ok(false);
        }
        self.preparations.store_verdict(row).await
    }

    /// The candidate at `key` as an identification can still answer it: a
    /// stored, actionable candidate whose standing is answerable.
    pub(crate) async fn answerable_candidate(
        &self,
        key: &str,
    ) -> Result<Option<crate::import::folder_scanner::FolderCandidate>, crate::library::LibraryError>
    {
        let Some(candidate) = self.get_release_candidate(key).await? else {
            return Ok(None);
        };
        if !self.candidate_standing(key, &candidate).await?.answerable() {
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

/// Project a parsed album into the editor's `ReleaseUserEdit`; every path
/// seeds the edit-metadata form through this.
///
/// It projects the same `ParsedAlbum` the commit worker applies the edit onto,
/// which is what lets `apply_user_edit_to_seed` tell an untouched field from an
/// edited one.
///
/// An empty per-track artist list means the track shares the album artist.
pub fn parsed_album_to_user_edit(parsed: &super::ParsedAlbum) -> crate::import::ReleaseUserEdit {
    // The mapper builds a ParsedAlbum's artists and links together, so a
    // missing reference is a bug.
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
                // A seed does not say which audio file backs each track; the
                // track slots settle that.
                file: None,
            }
        })
        .collect();

    crate::import::ReleaseUserEdit {
        album_title: parsed.album.title.clone(),
        album_artist_assignments,
        album_year: parsed.album.year,
        pressing: parsed.release.pressing.clone(),
        tracks,
    }
}

/// The files the import writes rows for and counts bytes against, in
/// `relative_path` order: the same set [`CategorizedFiles::content_hash`]
/// covers.
pub(crate) fn flatten_categorized_files(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Vec<crate::import::folder_scanner::ScannedFile> {
    categorized.release_files().cloned().collect()
}
