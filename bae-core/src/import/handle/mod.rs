use crate::import::folder_scanner::{
    FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate,
};
use crate::import::types::{Catalog, ImportCommand, ImportProgress};
use crate::import::watched_folder::WatchedFolder;
use crate::library::manager::DiscogsCheck;
use crate::library::LibraryManager;
use crate::util::worker_thread::WorkerThread;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

mod combinations;
mod edits;
mod identification;
mod import;
mod lookup_choices;
mod scan;
mod search;
mod session;
mod shared_album;
mod verdict_write;
mod watch;

pub use import::ImportStart;
pub(crate) use verdict_write::VerdictWrite;
pub(crate) use scan::external_metadata_with_cover;

use super::candidate_runtime::CandidateRuntime;
use super::candidates::{
    CandidateRuntimeSnapshot, ImportCandidateSnapshot, WatchedFolderScanStatus,
};

#[cfg(test)]
mod tests;

/// The import event channel and the candidate runtime it records into.
///
/// Every event is recorded in the runtime before any reader hears it, so a
/// reader that hears an event and then asks the runtime finds its effect
/// already there.
#[derive(Clone)]
pub struct ImportEventBus {
    delivery: EventDelivery,
    runtime: CandidateRuntime,
    /// Stops the thread sending one chosen event until a test lets it
    /// through, so the test can act partway through the work that sends it.
    #[cfg(test)]
    send_hold: Arc<SendHold>,
}

/// Where [`ImportEventBus::hold_send_where`] stops a sender.
#[cfg(test)]
struct SendHold {
    state: std::sync::Mutex<HoldState>,
    changed: std::sync::Condvar,
    /// Set once a sender is stopped, for a test to await.
    held: tokio::sync::watch::Sender<bool>,
}

#[cfg(test)]
impl Default for SendHold {
    fn default() -> Self {
        Self {
            state: Default::default(),
            changed: Default::default(),
            held: tokio::sync::watch::Sender::new(false),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
enum HoldState {
    #[default]
    Off,
    /// The next event this matches stops its sender.
    Armed(Box<dyn Fn(&ImportEvent) -> bool + Send>),
    /// A sender is stopped, its event neither recorded nor delivered.
    Held,
    /// The stopped sender was let go.
    Released,
}

/// The one way an event reaches the bus's readers.
#[derive(Clone)]
struct EventDelivery {
    every_event: Arc<std::sync::Mutex<EveryEvent>>,
    /// The feed's end, holding every event since the bus started, until the
    /// one reader in the app that acts on every event takes it.
    feed: Arc<std::sync::Mutex<Option<mpsc::UnboundedReceiver<ImportEvent>>>>,
}

/// The readers that hear every event, and — for a test reader from the start
/// — every event delivered so far, under one lock so a reader misses nothing
/// and hears nothing twice.
struct EveryEvent {
    readers: Vec<mpsc::UnboundedSender<ImportEvent>>,
    #[cfg(any(test, feature = "test-utils"))]
    delivered: Vec<ImportEvent>,
}

impl EventDelivery {
    fn new() -> Self {
        let (feed_reader, feed) = mpsc::unbounded_channel();
        Self {
            every_event: Arc::new(std::sync::Mutex::new(EveryEvent {
                readers: vec![feed_reader],
                #[cfg(any(test, feature = "test-utils"))]
                delivered: Vec::new(),
            })),
            feed: Arc::new(std::sync::Mutex::new(Some(feed))),
        }
    }

    /// Hand `event` to every reader.
    fn deliver(&self, event: ImportEvent) {
        let mut every_event = self.every_event.lock().unwrap();
        #[cfg(any(test, feature = "test-utils"))]
        every_event.delivered.push(event.clone());
        every_event
            .readers
            .retain(|reader| reader.send(event.clone()).is_ok());
    }
}

impl ImportEventBus {
    /// A bus recording into `runtime`.
    pub fn new(runtime: CandidateRuntime) -> Self {
        Self {
            delivery: EventDelivery::new(),
            runtime,
            #[cfg(test)]
            send_hold: Arc::default(),
        }
    }

    /// Record `event` in the runtime, then hand it to every reader.
    pub fn send(&self, event: ImportEvent) {
        #[cfg(test)]
        self.wait_if_held(&event);
        self.runtime.record_event(&event);
        self.delivery.deliver(event);
    }

    /// Record `event` in the runtime, await `written` — the standing it
    /// announces — and hand the event to every reader only once that write
    /// landed: a reader that hears it finds the standing in the table, and
    /// what the event says beside it already recorded. A write that fails is
    /// returned, and the event is not sent.
    pub(crate) async fn send_once_written<E>(
        &self,
        event: ImportEvent,
        written: impl std::future::Future<Output = Result<(), E>>,
    ) -> Result<(), E> {
        #[cfg(test)]
        self.wait_if_held(&event);
        self.runtime.record_event(&event);
        written.await?;
        self.delivery.deliver(event);
        Ok(())
    }

    /// Every event since the bus started, none dropped, each on the reader
    /// before its send returns, for the one reader in the app that acts on
    /// every event, whenever it starts. A reader that also takes commands
    /// hears every event sent before a command before the command itself.
    /// `None` once taken.
    pub(crate) fn take_feed(&self) -> Option<mpsc::UnboundedReceiver<ImportEvent>> {
        self.delivery.feed.lock().unwrap().take()
    }

    /// Every event sent from now on, none dropped, for a test to wait on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_event(&self) -> mpsc::UnboundedReceiver<ImportEvent> {
        let (reader, events) = mpsc::unbounded_channel();
        self.delivery.every_event.lock().unwrap().readers.push(reader);
        events
    }

    /// Every event this bus has delivered and will deliver, none dropped —
    /// for a test that starts reading after the work it waits on began.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_event_from_start(&self) -> mpsc::UnboundedReceiver<ImportEvent> {
        let (reader, events) = mpsc::unbounded_channel();
        let mut every_event = self.delivery.every_event.lock().unwrap();
        for event in &every_event.delivered {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the receiver is returned below, so the send cannot fail"
            )]
            let _ = reader.send(event.clone());
        }
        every_event.readers.push(reader);
        events
    }

    /// Stop the first thread that sends an event `matches` takes, before the
    /// event is recorded or delivered, until [`Self::release_send`].
    #[cfg(test)]
    pub(crate) fn hold_send_where(
        &self,
        matches: impl Fn(&ImportEvent) -> bool + Send + 'static,
    ) {
        *self.send_hold.state.lock().unwrap() = HoldState::Armed(Box::new(matches));
    }

    /// [`Self::hold_send_where`] a measured percent of `phase` above zero:
    /// those come from the threads doing the work, while a zero comes from
    /// the worker, which must stay free to hear a cancel.
    #[cfg(test)]
    pub(crate) fn hold_progress_at(&self, phase: crate::import::ImportPhase) {
        self.hold_send_where(move |event| {
            matches!(
                event,
                ImportEvent::ImportProgress {
                    progress: ImportProgress::Progress {
                        phase: sent,
                        percent: Some(percent),
                        ..
                    },
                    ..
                } if *sent == phase && *percent > 0
            )
        });
    }

    /// Wait until a sender is stopped where [`Self::hold_send_where`] said.
    #[cfg(test)]
    pub(crate) async fn send_held(&self) {
        self.send_hold
            .held
            .subscribe()
            .wait_for(|held| *held)
            .await
            .expect("the hold outlives the wait for it");
    }

    /// Let the stopped sender go on.
    #[cfg(test)]
    pub(crate) fn release_send(&self) {
        *self.send_hold.state.lock().unwrap() = HoldState::Released;
        self.send_hold.changed.notify_all();
    }

    #[cfg(test)]
    fn wait_if_held(&self, event: &ImportEvent) {
        let mut state = self.send_hold.state.lock().unwrap();
        if !matches!(&*state, HoldState::Armed(matches) if matches(event)) {
            return;
        }
        *state = HoldState::Held;
        self.send_hold.held.send_replace(true);
        let _released = self
            .send_hold
            .changed
            .wait_while(state, |state| matches!(state, HoldState::Held))
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
        /// The run this state belongs to. A settled run keeps reporting, so
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
/// - `Unvalidated` — Discogs could not be asked (unreachable, rate-limited, an
///   answer bae could not read); the key is stored and re-checked later, and
///   the failure says why.
/// - `Rejected` — Discogs returned 401; nothing is stored.
#[derive(Debug)]
pub enum DiscogsSaveOutcome {
    Valid,
    Unvalidated(crate::discogs::client::DiscogsError),
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
    ids: coven::IdSourceRef,
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
        /// The write this announces listed the release for the first time —
        /// new to the library, or listed anew by a person's decision about
        /// how its folder reads — while identification runs on its own.
        found_while_automatic: bool,
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
    /// was removed (one event per candidate).
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
    /// The candidate's metadata draft, its provenance, or its release link
    /// changed.
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
    /// Read one folder as a person decided. `completion` hears the keys of
    /// the releases the folder now reads as once they are stored, or why they
    /// were not.
    SetFolderReleaseDecision {
        target: (FolderReleaseDecisionKey, FolderReleaseDecision),
        completion: tokio::sync::oneshot::Sender<Result<Vec<String>, String>>,
    },
    /// Stop watching `roots` and, given `parent` (the folder holding exactly
    /// them), watch it in their place. `completion` hears once the change is
    /// stored, or why it was not.
    Remove {
        roots: Vec<std::path::PathBuf>,
        parent: Option<std::path::PathBuf>,
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// Take `report` as if the filesystem watch had sent it; `taken` hears
    /// once the reads it calls for are asked for.
    #[cfg(test)]
    WatchReport {
        report: crate::import::service::WatchReport,
        taken: tokio::sync::oneshot::Sender<()>,
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

    /// The handle over the started workers. Starts the live read of every
    /// candidate's runtime as one of the library's own tasks, which closing
    /// the library ends; refused once the library is closing.
    pub(super) fn new(
        worker: WorkerThread<crate::import::service::ImportWorkerMessage>,
        watcher: WorkerThread<WatcherCommand>,
        services: crate::import::ImportServices,
        runtime: CandidateRuntime,
        runtime_handle: tokio::runtime::Handle,
    ) -> Result<Self, crate::library::LibraryError> {
        let crate::import::ImportServices {
            event_tx,
            library_manager,
            preparations,
            clock: _,
            ids,
            file_tags,
            directories: _,
            folder_state_commit,
            import_cancels,
        } = services;
        library_manager.spawn_supervised_task(
            "candidate runtime snapshots",
            runtime.clone().keep_snapshots_current(),
        )?;
        let identify = crate::identify::IdentifyServiceHandle::new(
            library_manager.clone(),
            runtime_handle.clone(),
            event_tx.clone(),
            runtime.clone(),
        );
        let extraction = crate::signals::ExtractionService::start(
            runtime_handle.clone(),
            event_tx.clone(),
            runtime.clone(),
            library_manager.clone(),
        );
        Ok(Self {
            worker,
            library_manager,
            preparations,
            ids,
            file_tags,
            event_tx,
            runtime,
            identify,
            extraction,
            folder_state_commit,
            import_cancels,
            watcher,
            runtime_handle,
        })
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

    /// End an import's claim on a candidate the way the import's own ending
    /// does, for a test with no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn end_candidate_import_for_test(&self, candidate_key: &str, import_id: &str) {
        self.runtime
            .move_import(candidate_key, import_id, None)
            .await
            .expect("a test ends the claim it made");
    }

    /// Move an import's claim the way the worker's steps do, for a test with
    /// no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn move_candidate_import_for_test(
        &self,
        candidate_key: &str,
        import_id: &str,
        standing: crate::import::ImportStanding,
    ) {
        self.runtime
            .move_import(candidate_key, import_id, Some(standing))
            .await
            .expect("a test moves the claim it made");
    }

    /// Every key with something in flight right now, read from the standings
    /// table and the runtime.
    pub async fn candidate_runtimes(
        &self,
    ) -> Result<HashMap<String, CandidateRuntimeSnapshot>, crate::library::LibraryError> {
        self.runtime.snapshots().await
    }

    /// What is in flight for one key right now, read from the standings table
    /// and the runtime.
    pub async fn candidate_runtime(
        &self,
        key: &str,
    ) -> Result<Option<CandidateRuntimeSnapshot>, crate::library::LibraryError> {
        self.runtime.snapshot(key).await
    }

    /// Where one key stands, as its row reads it from the standings table, for
    /// a test.
    #[cfg(test)]
    pub(crate) async fn standing_facts_for_test(&self, key: &str) -> crate::import::TriageRuntimeFacts {
        self.runtime
            .standing_facts(key)
            .await
            .expect("a test reads the standings table")
    }

    /// Put `key` where a run's answer being written puts it — queued, `run`
    /// started and then answered with `answer` — as the queue and the
    /// identify driver would, for a test of what disposes of that answer.
    #[cfg(test)]
    pub(crate) async fn hold_answer_for_test(
        &self,
        key: &str,
        run: crate::identify::IdentifyRunId,
        answer: &crate::identify::IdentifyState,
    ) {
        let candidate = self
            .get_release_candidate(key)
            .await
            .unwrap()
            .expect("the candidate is stored");
        let files = crate::import::CandidateFiles::of(&candidate);
        self.runtime
            .admit(
                vec![(key.to_string(), files.clone())],
                crate::import::Admission::Requested,
            )
            .await
            .unwrap();
        self.runtime.record_run_state(key, run, true, answer);
        assert!(self.runtime.run_started(key, &files, run).await.unwrap());
        assert!(self
            .runtime
            .run_answered(key, run, answer.picks_unattended())
            .await
            .unwrap());
    }

    /// What is in flight for one key as the runtime stream last read it — the
    /// read a view does once when it appears, after it has subscribed to the
    /// stream, so the two agree.
    pub fn latest_candidate_runtime(&self, key: &str) -> Option<CandidateRuntimeSnapshot> {
        self.runtime.latest_snapshots().get(key).cloned()
    }

    /// Every key with something in flight, and each key that changes after.
    pub fn watch_candidate_runtimes(&self) -> super::RuntimeSnapshotsWatch {
        super::RuntimeSnapshotsWatch::of(&self.runtime)
    }

    /// Every key's runtime as each write of its standing leaves it, in the
    /// order they land, from now on, for a test that checks that order.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_runtime_change_for_test(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<super::CandidateRuntimeChange> {
        self.runtime.every_change()
    }

    /// Every move of the identification count from now on, for a test.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_identification_count_for_test(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<(u32, u32)> {
        self.runtime.every_count()
    }

    /// Each candidate's signals and the two counts, as they change.
    pub(crate) fn watch_runtime_values(
        &self,
    ) -> super::candidate_runtime::RuntimeValuesWatch {
        super::candidate_runtime::RuntimeValuesWatch::of(&self.runtime)
    }

    /// One candidate's pane as it reads back from the tables, its standing
    /// among them.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn candidate_pane(
        &self,
        key: &str,
    ) -> Result<Option<crate::import::ImportCandidateDetail>, crate::library::LibraryError> {
        Ok(self
            .library_manager
            .load_import_candidate(key)
            .await?
            .map(crate::import::ImportCandidateDetailProjection::resolve))
    }

    /// The query the pane subscribes to for one candidate, for a test that
    /// watches it change.
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
        self.subscribe_list_window(
            view,
            crate::library::LibraryPageWindow {
                offset: 0,
                limit: u64::MAX,
            },
        )
    }

    /// The import list for `view` through one window, for a test that
    /// watches what changes off the page.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_list_window(
        &self,
        view: crate::import::ImportListView,
        window: crate::library::LibraryPageWindow,
    ) -> crate::import::ImportListSubscription {
        crate::import::ImportListSubscription::start(
            |request| self.library_manager.subscribe_import_list(request),
            self.library_manager.subscribe_folder_scan_progress(),
            view,
            std::iter::once(window).collect(),
            self.library_manager.subscribe_outbox_values(),
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
        let runtime = self.runtime.snapshot(key).await?;
        if let Some((candidate, actionable)) = candidate {
            let standing = self.candidate_standing(key, &candidate).await?;
            return Ok(Some(ImportCandidateSnapshot::Folder {
                candidate,
                runtime,
                actionable,
                skipped: standing.skipped,
                is_added: standing.imported,
            }));
        }
        Ok(runtime.map(|runtime| ImportCandidateSnapshot::Runtime {
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
    /// library, or claimed by a running import — the claim read from its
    /// standing, which a caller about to write reads under the commit lock.
    /// Read from the tables because the list's query lands after the commit
    /// it reflects.
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
            claimed: self.runtime.import_claimed(key).await?,
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
            .claim_for_import(candidate_key, import_id, false)
            .await
            .expect("a test claims a candidate no import owns");
    }

    async fn release_import_claim(
        &self,
        candidate_key: &str,
        import_id: &str,
    ) -> Result<(), crate::library::LibraryError> {
        let _commit = self
            .folder_state_commit
            .lock("release an import claim")
            .await;
        Ok(self
            .runtime
            .move_import(candidate_key, import_id, None)
            .await?)
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
}

/// Remap the parsed (temporary) IDs a link row points at to their actual DB IDs.
///
/// A `ReleaseRows`'s link rows reference artist and work IDs minted when the
/// rows were built; reconcile may have resolved those to existing DB rows. `label` names
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
