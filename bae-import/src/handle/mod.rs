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
use crate::{ImportEvent, ImportEvents, ScanEvent};
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
    /// The import's shared dependencies, the same the folder scans hold.
    services: crate::ImportServices,
    /// Where a catalog release is read from storage or fetched.
    release_reads: crate::ReleaseReads,
    /// What the bus's events have said about every candidate.
    runtime: CandidateRuntime,
    /// The identify driver and the extraction feeding it, held here because
    /// the commands that decide a candidate end its identification as part of
    /// their own write.
    identify: crate::IdentifyService,
    extraction: crate::extraction::ExtractionService,
    watcher: WorkerThread<WatcherCommand>,
    /// What the folder scan coordinator holds of each root's scan that no
    /// table does, which the scan progress puts beside the stored scans.
    scans_in_memory: tokio::sync::watch::Receiver<crate::folder_scan_status::FolderScansInMemory>,
    runtime_handle: tokio::runtime::Handle,
}

impl ImportServiceHandle {
    /// The release stored at `key`, when it may be worked on; a grouping's
    /// release that cannot be fails with why.
    pub async fn get_release_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<crate::folder_scanner::FolderCandidate>, crate::ImportError> {
        self.services
            .tables
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

    /// The handle over the started workers and the services they share.
    pub(super) fn new(
        worker: WorkerThread<crate::service::ImportWorkerMessage>,
        watcher: WorkerThread<WatcherCommand>,
        scans_in_memory: tokio::sync::watch::Receiver<
            crate::folder_scan_status::FolderScansInMemory,
        >,
        services: crate::ImportServices,
        runtime: CandidateRuntime,
        release_reads: crate::ReleaseReads,
        identify: crate::IdentifyService,
        extraction: crate::extraction::ExtractionService,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            worker,
            services,
            release_reads,
            runtime,
            identify,
            extraction,
            watcher,
            scans_in_memory,
            runtime_handle,
        }
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
