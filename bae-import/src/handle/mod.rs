use crate::folder_scanner::{
    FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate,
};
use crate::types::{ImportCommand, ImportProgress};
use crate::watched_folder::WatchedFolder;
use bae_util::worker_thread::WorkerThread;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

mod cancel;
mod combinations;
mod commit;
mod covers;
mod discogs_key;
mod draft_source;
mod edits;
mod event_bus;
mod file_decisions;
mod file_tags;
mod identification;
mod identity_choices;
mod import;
mod list;
mod lookup_choices;
mod picks;
mod search;
mod session;
mod shared_album;
mod skip;
mod verdict_write;
mod watch;

pub use commit::CandidateStanding;
pub use event_bus::{ImportEvent, ImportEventBus, ImportEvents, ScanEvent};
#[cfg(any(test, feature = "test-utils"))]
pub use event_bus::{ImportProgressFeed, ScanEvents};
pub use import::ImportStart;
pub(crate) use picks::external_metadata_with_cover;
pub use search::GroupedSearchResults;
pub(crate) use verdict_write::VerdictWrite;

use super::candidate_runtime::CandidateRuntime;
use crate::folder_scan_status::WatchedFolderScanStatus;
use crate::watching::WatcherCommand;
use crate::CandidateRuntimeSnapshot;

#[cfg(test)]
#[path = "mod_tests.rs"]
pub(crate) mod tests;

/// Handle for sending import requests and subscribing to progress updates.
#[derive(Clone)]
pub struct ImportServiceHandle {
    /// The import worker. Its clone of the store holds coven's store-open
    /// lock, so teardown must not return before it is joined.
    worker: WorkerThread<crate::service::ImportWorkerMessage>,
    /// The library's store, which the handle reads and writes directly.
    database: bae_db::Database,
    /// Import's own tables in that store.
    tables: crate::tables::ImportTables,
    /// The catalogs as this library may ask them.
    catalogs: crate::CatalogSessions,
    /// Where a catalog release is read from storage or fetched.
    release_reads: crate::ReleaseReads,
    /// The provider images covers are fetched through.
    remote_images: crate::RemoteImages,
    /// The one writer of candidates' stored state.
    preparations: crate::CandidatePreparations,
    /// The clock a refused import's failure is stamped with.
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
    /// Reads a folder's audio files for their embedded tags; the scan's
    /// pre-fill uses the same reader.
    file_tags: std::sync::Arc<dyn crate::file_tag_snapshot::FileTagReader>,
    /// The import service's event channel.
    event_tx: ImportEventBus,
    /// What the bus's events have said about every candidate.
    runtime: CandidateRuntime,
    /// The identify driver and the extraction feeding it, held here because
    /// the commands that decide a candidate end its identification as part of
    /// their own write.
    identify: crate::IdentifyServiceHandle,
    extraction: crate::extraction::ExtractionServiceHandle,
    folder_state_commit: crate::FolderStateCommit,
    import_cancels: crate::import_cancel::ImportCancels,
    watcher: WorkerThread<WatcherCommand>,
    runtime_handle: tokio::runtime::Handle,
    /// Where a panic in work the handle starts and nobody awaits is reported.
    diagnostics: bae_diagnostics::Diagnostics,
    /// Cancelled when the library closes, before its store does: every list
    /// the handle opens ends with it.
    closing: tokio_util::sync::CancellationToken,
}

impl ImportServiceHandle {
    /// The release stored at `key`, when it may be worked on; a grouping's
    /// release that cannot be fails with why.
    pub async fn get_release_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<crate::folder_scanner::FolderCandidate>, crate::ImportError> {
        self.tables
            .load_release_candidate(key)
            .await?
            .map_err(|reason| crate::ImportError::GroupingBlocked { reason })
    }

    /// [`Self::get_release_candidate`], refusing a key that names no stored
    /// release.
    pub(super) async fn require_release_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<crate::folder_scanner::FolderCandidate, crate::ImportError> {
        self.get_release_candidate(key).await?.ok_or_else(|| crate::ImportError::Internal {
            detail: format!("{key} is not a scanned candidate"),
        })
    }

    /// The handle over the started workers. Starts the live read of every
    /// candidate's runtime as one of the library's own tasks, which closing
    /// the library ends; refused once the library is closing.
    pub(super) fn new(
        worker: WorkerThread<crate::service::ImportWorkerMessage>,
        watcher: WorkerThread<WatcherCommand>,
        services: crate::ImportServices,
        runtime: CandidateRuntime,
        runtime_handle: tokio::runtime::Handle,
    ) -> Result<Self, crate::ImportError> {
        let crate::ImportServices {
            event_tx,
            database,
            tables,
            catalogs,
            remote_images,
            preparations,
            clock,
            ids,
            file_tags,
            directories: _,
            folder_state_commit,
            import_cancels,
            diagnostics,
            closing,
        } = services;
        let release_reads = crate::ReleaseReads::new(database.clone(), catalogs.clone());
        let identify = crate::IdentifyServiceHandle::new(
            database.clone(),
            catalogs.clone(),
            release_reads.clone(),
            runtime_handle.clone(),
            event_tx.clone(),
            runtime.clone(),
            diagnostics.clone(),
        );
        let extraction = crate::extraction::ExtractionService::start(
            runtime_handle.clone(),
            event_tx.clone(),
            runtime.clone(),
            database.clone(),
            diagnostics.clone(),
        );
        Ok(Self {
            worker,
            database,
            tables,
            catalogs,
            release_reads,
            remote_images,
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
            watcher,
            runtime_handle,
            diagnostics,
            closing,
        })
    }

    /// Every release a pick claims, read whole: `primary`, and its
    /// `partners` on other catalogs, from storage or fetched now.
    pub(crate) async fn read_pick(
        &self,
        primary: &bae_catalogs::records::LookupRef,
        partners: &[bae_catalogs::records::LookupRef],
        priority: bae_catalogs::rate_limiter::CallPriority,
    ) -> Result<
        (
            bae_catalogs::source_release::PreparedRelease,
            Vec<bae_catalogs::source_release::PreparedRelease>,
        ),
        crate::ImportError,
    > {
        let release = self
            .release_reads
            .release(primary, bae_catalogs::payloads::ReadDepth::Complete, priority)
            .await?;
        let partners = self
            .release_reads
            .partners(primary, partners, priority, &crate::CatalogApplySteps::unread())
            .await?;
        Ok((release, partners))
    }

    /// Stop the first thread that reports a measured percent of `phase` of
    /// an import before the report is recorded or delivered, until
    /// [`Self::release_progress`], for a test that acts while an import is
    /// partway through.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn hold_progress_at(&self, phase: crate::ImportPhase) {
        self.event_tx.hold_progress_at(phase);
    }

    /// Wait until a thread is stopped where [`Self::hold_progress_at`] said.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn progress_held(&self) {
        self.event_tx.send_held().await;
    }

    /// Let the stopped thread go on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn release_progress(&self) {
        self.event_tx.release_send();
    }

    /// Stop and join both worker threads; later calls do nothing. Called from
    /// `AppServicesInner`'s drop so the store-open lock is released before
    /// teardown returns. Each is sent `Shutdown` because `self` holds a live
    /// sender, so neither channel closes on its own.
    pub fn stop_and_join(&self) {
        self.watcher.stop_and_join(|watcher_tx| {
            let (completion, receiver) = std::sync::mpsc::channel();
            if watcher_tx.send(WatcherCommand::Shutdown { completion }).is_ok()
                && receiver.recv().is_err()
            {
                tracing::warn!("folder scan coordinator ended without acknowledging shutdown");
            }
        });
        self.worker.stop_and_join(|requests_tx| {
            if requests_tx.send(crate::service::ImportWorkerMessage::Shutdown).is_err() {
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
    pub async fn claim_candidate_for_import_for_test(
        &self,
        candidate_key: &crate::CandidateKey,
        import_id: &str,
    ) {
        self.claim_candidate_for_import(candidate_key, import_id).await;
    }

    /// End an import's claim on a candidate the way the import's own ending
    /// does, for a test with no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn end_candidate_import_for_test(
        &self,
        candidate_key: &crate::CandidateKey,
        import_id: &str,
    ) {
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
        candidate_key: &crate::CandidateKey,
        import_id: &str,
        step: crate::ImportStep,
    ) {
        self.runtime
            .move_import(candidate_key, import_id, Some(step))
            .await
            .expect("a test moves the claim it made");
    }

    /// Every key with something in flight right now, read from the standings
    /// table and the runtime.
    pub async fn candidate_runtimes(
        &self,
    ) -> Result<HashMap<crate::CandidateKey, CandidateRuntimeSnapshot>, crate::ImportError> {
        self.runtime.snapshots().await
    }

    /// What is in flight for one key right now, read from the standings table
    /// and the runtime.
    pub async fn candidate_runtime(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<CandidateRuntimeSnapshot>, crate::ImportError> {
        self.runtime.snapshot(key).await
    }

    /// Where one key stands, as its row reads it from the standings table, for
    /// a test.
    #[cfg(test)]
    pub(crate) async fn standing_facts_for_test(
        &self,
        key: &crate::CandidateKey,
    ) -> crate::TriageRuntimeFacts {
        self.runtime.standing_facts(key).await.expect("a test reads the standings table")
    }

    /// Put `key` where a run's answer being written puts it — queued, `run`
    /// started and then answered with `answer` — as the queue and the
    /// identify driver would, for a test of what disposes of that answer.
    #[cfg(test)]
    pub(crate) async fn hold_answer_for_test(
        &self,
        key: &crate::CandidateKey,
        run: crate::IdentifyRunId,
        answer: &bae_identify::IdentifyState,
    ) {
        let candidate =
            self.get_release_candidate(key).await.unwrap().expect("the candidate is stored");
        let files = crate::CandidateFiles::of(&candidate);
        self.runtime
            .admit(vec![(key.clone(), files.clone())], crate::Admission::Requested)
            .await
            .unwrap();
        assert!(self.runtime.run_started(key, &files, run, answer).await.unwrap());
        assert!(self.runtime.run_answered(key, run, answer.picks_unattended()).await.unwrap());
    }

    /// Each running import's percent, by candidate key, as its import last
    /// reported.
    pub fn import_percents(&self) -> HashMap<crate::CandidateKey, u32> {
        self.runtime.percents()
    }

    /// Each running import's percent, by candidate key, now and as it moves.
    pub fn watch_import_percents(&self) -> crate::ImportPercentsWatch {
        self.runtime.watch_import_percents()
    }

    /// Every key's runtime as each write of its standing leaves it, in the
    /// order they land, from now on, for a test that checks that order.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_runtime_change_for_test(&self) -> crate::CandidateRuntimeChanges {
        self.runtime.every_change()
    }

    /// Every move of the identification count from now on, for a test.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_identification_count_for_test(&self) -> crate::IdentificationCounts {
        self.runtime.every_count()
    }

    /// The import list for `view` as one window over every item, for a test
    /// that watches it change.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn subscribe_whole_list(&self, view: crate::ImportListView) -> crate::LiveImportList {
        self.subscribe_list_window(view, bae_db::LibraryPageWindow { offset: 0, limit: u64::MAX })
            .await
    }

    /// The import list for `view` through one window, for a test that
    /// watches what changes off the page.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn subscribe_list_window(
        &self,
        view: crate::ImportListView,
        window: bae_db::LibraryPageWindow,
    ) -> crate::LiveImportList {
        self.open_list(view, std::iter::once(window).collect())
            .await
            .expect("the import list opens")
    }

    /// The first folder scans `accept` admits, waiting through the query's
    /// values until one does.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_folder_scans(
        &self,
        mut accept: impl FnMut(&crate::FolderScanProgress) -> bool,
    ) -> crate::FolderScanProgress {
        let mut scans = self.tables.subscribe_folder_scan_progress();
        loop {
            let progress = scans
                .next()
                .await
                .expect("the folder scan query stays open")
                .expect("the folder scan query reads");
            if accept(&progress) {
                return progress;
            }
        }
    }

    /// The first import list `accept` admits, waiting through the query's
    /// values until one does. The list lands after the commit it reflects, so
    /// a test that just observed a scan event waits here for the read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn wait_for_list(
        &self,
        view: crate::ImportListView,
        mut accept: impl FnMut(&crate::ImportListSnapshot) -> bool,
    ) -> crate::ImportListSnapshot {
        let subscription = self.subscribe_whole_list(view).await;
        loop {
            let snapshot = subscription.next().await.expect("the import list query stays open");
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
        key: &crate::CandidateKey,
    ) -> Result<Option<ImportCandidateSnapshot>, crate::ImportError> {
        let stored = self.tables.load_folder_scan_item(key).await?;
        let candidate = match stored {
            Some(crate::folder_scanner::ScanItem::Valid(candidate)) => Some((candidate, true)),
            Some(crate::folder_scanner::ScanItem::Discovered(candidate)) => {
                Some((candidate, false))
            }
            Some(crate::folder_scanner::ScanItem::Invalid(candidate)) => {
                return Ok(Some(ImportCandidateSnapshot::Invalid(candidate)))
            }
            None => None,
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
        Ok(runtime
            .map(|runtime| ImportCandidateSnapshot::Runtime { key: key.to_string(), runtime }))
    }

    /// Claim `candidate_key` for the test import `import_id`, under the lock
    /// [`Self::save_candidate_verdict_if_current`] holds across its check and
    /// write, so no verdict is stored for a claimed candidate.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn claim_candidate_for_import(
        &self,
        candidate_key: &crate::CandidateKey,
        import_id: &str,
    ) {
        let _commit = self.folder_state_commit.lock("claim a candidate for a test").await;
        self.runtime
            .claim_for_import(candidate_key, import_id, false)
            .await
            .expect("a test claims a candidate no import owns");
    }
}

/// One candidate by key, with its runtime joined: the read behind every
/// "what is this key right now" question.
#[derive(Debug, Clone)]
pub enum ImportCandidateSnapshot {
    Folder {
        candidate: FolderCandidate,
        /// `None` when nothing is running for this key.
        runtime: Option<CandidateRuntimeSnapshot>,
        actionable: bool,
        skipped: bool,
        is_added: bool,
    },
    Invalid(InvalidCandidate),
    /// A key with runtime but no scanned folder — a library release being
    /// re-identified.
    Runtime {
        key: String,
        runtime: CandidateRuntimeSnapshot,
    },
}
