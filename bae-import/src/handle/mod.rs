use crate::folder_scanner::{
    FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate,
};
use crate::types::{ImportCommand, ImportProgress};
use crate::watched_folder::WatchedFolder;
use bae_util::worker_thread::WorkerThread;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

mod cancel;
mod combinations;
mod commit;
mod covers;
mod discogs_key;
mod documents;
mod draft_source;
mod edits;
mod events;
mod file_decisions;
mod file_tags;
mod identification;
mod import;
mod list;
mod lookup_choices;
mod picks;
mod runtime;
mod search;
mod session;
mod shared_album;
mod skip;
mod verdict_write;
mod watch;

pub use commit::CandidateStanding;
pub use covers::{CoverTarget, RemoteCoverGallery};
pub use import::ImportStart;
pub(crate) use picks::external_metadata_with_cover;
pub use runtime::ImportCandidateSnapshot;
pub use search::GroupedSearchResults;
pub(crate) use verdict_write::VerdictWrite;

use super::candidate_runtime::CandidateRuntime;
use crate::watching::WatcherCommand;
use crate::CandidateRuntimeSnapshot;
use crate::{ImportEvent, ImportEventBus, ImportEvents, ScanEvent};
#[cfg(any(test, feature = "test-utils"))]
use crate::{ImportProgressFeed, ScanEvents};

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
    file_tags: std::sync::Arc<dyn crate::file_tag_reader::FileTagReader>,
    /// The import service's event channel.
    event_tx: ImportEventBus,
    /// What the bus's events have said about every candidate.
    runtime: CandidateRuntime,
    /// The identify driver and the extraction feeding it, held here because
    /// the commands that decide a candidate end its identification as part of
    /// their own write.
    identify: crate::IdentifyService,
    extraction: crate::extraction::ExtractionService,
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
        let identify = crate::IdentifyService::new(
            database.clone(),
            catalogs.clone(),
            release_reads.clone(),
            runtime_handle.clone(),
            event_tx.clone(),
            runtime.clone(),
            diagnostics.clone(),
        );
        let extraction = crate::extraction::ExtractionService::new(
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
}
