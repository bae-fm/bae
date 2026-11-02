//! What is happening right now for each candidate.
//!
//! Where each folder candidate stands — its identification queued, running,
//! having its answer written or failed to write it, its import queued,
//! running or writing — is held in one place, the standings table (see
//! [`StoredStanding`]). Every change to it is a write its owner awaits, made
//! here or inside the owner's own write: the queue admits and withdraws, the
//! identify driver records its run starting, answering and ending, the
//! verdict write ends the answer, the import worker moves its import, and a
//! claim refuses while another import stands. A write that fails fails the
//! operation that asked for it.
//!
//! What each run last published is stored beside the standings
//! (`import_identify_run`, see [`StoredRuntime`]): a folder candidate's run
//! while the table names it, deleted in the write that moves the standing off
//! it, and a library release's re-identify run, which has no standing, until
//! it ends. The count of identifications in flight is the table's too, moved
//! by each change in its own write.
//!
//! A person's typed search is stored beside them too
//! (`import_candidate_search`, see [`CandidateSearch`]): each catalog's
//! answer lands on it in a write of its own.
//!
//! The signals extraction read for a run are stored with what the run
//! published, under the run, so a run's pane and its settle read the same
//! rows.
//!
//! This runtime holds what is not stored: how far through its step an import
//! is, for the import percents' stream alone.
//! [`CandidateRuntimeSnapshot`](crate::import::CandidateRuntimeSnapshot) is
//! the stored runtime as one read of the tables composes it, and a screen
//! follows one key's as a live query of those tables.

use super::candidate_key::CandidateKey;
use super::candidate_search::{CandidateSearch, SearchRun};
use super::folder_scanner::{FolderCandidate, ReleaseFileScope};
use super::handle::{ImportEvent, ScanEvent};
use super::types::ImportProgress;
use crate::catalog_model::search::{MetadataResult, SearchQuery};
use crate::catalog_model::Failure;
use crate::db::{Database, DbError};
use crate::identify::signals::Signals;
use crate::identify::{IdentifyState, RunOutcome};
use crate::import::IdentifyRunId;
use bae_catalogs::catalog::LookupCatalog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::debug;

mod snapshots;
mod standing_writes;
mod stored_standing;
mod work;
pub(crate) use snapshots::compose;
pub(crate) use standing_writes::RunSubject;
pub(crate) use stored_standing::{
    IdentificationChange, IdentificationMove, IdentificationStage, Identifying,
    StoredIdentification, StoredImport, StoredRuntime, StoredStanding,
};
pub(crate) use work::CandidateWork;

#[cfg(test)]
pub(crate) mod tests;

/// One key's runtime after a write of its standing, or its removal, as the
/// test readers of every change hear it.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateRuntimeChange {
    Updated {
        key: CandidateKey,
        runtime: super::candidates::CandidateRuntimeSnapshot,
    },
    /// Nothing is running for the key any more.
    Removed {
        key: CandidateKey,
    },
}

/// The files a candidate's runtime was recorded against; a scan reporting
/// the key with other files ends its work.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CandidateShape {
    content_hash: String,
    file_edit_revision: u64,
    scope: ReleaseFileScope,
    file_root: PathBuf,
}

impl CandidateShape {
    fn of(candidate: &FolderCandidate) -> Self {
        Self {
            content_hash: candidate.files.content_hash(),
            file_edit_revision: candidate.file_edit_revision,
            scope: candidate.scope,
            file_root: candidate.file_root.clone(),
        }
    }
}

/// How far through its step the import that last reported for a key is. The
/// import id tells it from an import that already ended.
#[derive(Clone, PartialEq)]
struct ImportReport {
    import_id: String,
    /// How far through its step it is, once the step counts any.
    percent: Option<u32>,
}

#[derive(Default)]
struct Inner {
    /// The files last reported for each scanned key, so a reshape can be told
    /// from a repeat.
    shapes: HashMap<CandidateKey, CandidateShape>,
    /// How far through its step each key's import is, as it last reported.
    imports: HashMap<CandidateKey, ImportReport>,
}

#[derive(Clone)]
pub struct CandidateRuntime {
    inner: Arc<Mutex<Inner>>,
    /// Its own lock: work emits through the bus while holding it, and the
    /// bus records into `inner`.
    work: Arc<Mutex<work::WorkInFlight>>,
    /// The library the standings table is in.
    database: Database,
    /// Where a bug the runtime finds with no caller to fail is reported.
    diagnostics: crate::diagnostics::Diagnostics,
    /// The percent of each key's running import, apart from its snapshot: it
    /// moves many times a second, and the step only a few times an import.
    percents: Arc<watch::Sender<HashMap<CandidateKey, u32>>>,
    /// Test readers that hear every move of the identification count, and
    /// every key's runtime as each write of its standing leaves it.
    #[cfg(any(test, feature = "test-utils"))]
    every_count: TestReaders<(u32, u32)>,
    /// The count the test readers of every count were last told.
    #[cfg(any(test, feature = "test-utils"))]
    told_count: Arc<Mutex<(u32, u32)>>,
    #[cfg(any(test, feature = "test-utils"))]
    every_change: TestReaders<CandidateRuntimeChange>,
    /// The directory a test runtime's database is in, kept as long as the
    /// runtime is.
    #[cfg(any(test, feature = "test-utils"))]
    _test_directory: Option<Arc<tempfile::TempDir>>,
}

#[cfg(any(test, feature = "test-utils"))]
type TestReaders<T> = Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<T>>>>;

impl CandidateRuntime {
    /// What the runtime holds, under its lock. The runtime is changed in
    /// several steps under it, so a panic while it is held leaves it partway
    /// through a change: taking it then reports that and ends the process.
    fn held(&self) -> std::sync::MutexGuard<'_, Inner> {
        crate::diagnostics::lock_or_crash(&self.diagnostics, "the candidate runtime", &self.inner)
    }

    /// A runtime over the standings table of `database`, which the library
    /// emptied as it opened, reporting its bugs to `diagnostics`.
    pub(crate) fn new(database: Database, diagnostics: crate::diagnostics::Diagnostics) -> Self {
        Self {
            inner: Arc::default(),
            work: Arc::default(),
            database,
            diagnostics,
            percents: Arc::new(watch::Sender::new(HashMap::new())),
            #[cfg(any(test, feature = "test-utils"))]
            every_count: Arc::default(),
            #[cfg(any(test, feature = "test-utils"))]
            told_count: Arc::default(),
            #[cfg(any(test, feature = "test-utils"))]
            every_change: Arc::default(),
            #[cfg(any(test, feature = "test-utils"))]
            _test_directory: None,
        }
    }

    /// A runtime over a library database of its own, in a directory it
    /// keeps, for a test of what the runtime holds and the standings it
    /// writes.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn for_test() -> Self {
        let directory = tempfile::TempDir::new().expect("a test runtime has a directory");
        let path = directory.path().join("library.db");
        let database = Database::new_test(
            path.to_str().expect("a temporary path is UTF-8"),
            Arc::new(coven::SystemClock),
        )
        .await
        .expect("a test runtime's database opens");
        Self {
            _test_directory: Some(Arc::new(directory)),
            ..Self::new(database, crate::diagnostics::Diagnostics::noop())
        }
    }

    /// Run `statements` against this runtime's library, for a test that
    /// stores the rows a scan would.
    #[cfg(test)]
    pub(crate) async fn execute_local_sql_for_test(&self, statements: &str) {
        self.database
            .execute_local_sql_for_test(statements)
            .await
            .expect("a test's statements run");
    }

    /// `key`'s row as the standings table reads it, for a test.
    #[cfg(test)]
    pub(crate) async fn stored_standing_for_test(&self, key: &CandidateKey) -> StoredStanding {
        self.database.load_candidate_standing(key).await.expect("a test reads the standings table")
    }

    /// Another runtime over this one's library, holding nothing of what this
    /// one holds, for a test of what the table alone decides.
    #[cfg(test)]
    pub(crate) fn over_the_same_library_for_test(&self) -> Self {
        Self::new(self.database.clone(), self.diagnostics.clone())
    }

    /// The signals `run` of `key` last read, as the run's rows hold them:
    /// what the settle of its answer stores beside its verdict. `None` once
    /// the run's rows are gone.
    pub(super) async fn run_signals(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
    ) -> Result<Option<Signals>, DbError> {
        self.database.load_candidate_run_signals(key, run).await
    }

    /// Every move of the identification count from now on, for a test.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn every_count(&self) -> tokio::sync::mpsc::UnboundedReceiver<(u32, u32)> {
        let (reader, counts) = tokio::sync::mpsc::unbounded_channel();
        self.every_count.lock().unwrap().push(reader);
        counts
    }

    /// Every key's runtime as each write of its standing leaves it, in the
    /// order the writes land, from now on, for a test.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn every_change(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<CandidateRuntimeChange> {
        let (reader, changes) = tokio::sync::mpsc::unbounded_channel();
        self.every_change.lock().unwrap().push(reader);
        changes
    }

    /// Store `search`, just started, as `key`'s in place of any search it
    /// had, and return the run each catalog's answer names to land on it.
    pub(super) async fn start_search(
        &self,
        key: &CandidateKey,
        search: &CandidateSearch,
    ) -> Result<SearchRun, DbError> {
        self.database.start_candidate_search(key, search).await
    }

    /// Put every failed part of `key`'s search back to searching, returning
    /// the run it is on, the query and the catalogs to ask again; `None` when
    /// there is nothing to ask again.
    pub(super) async fn retry_search(
        &self,
        key: &CandidateKey,
    ) -> Result<Option<(SearchRun, SearchQuery, Vec<LookupCatalog>)>, DbError> {
        self.database.retry_candidate_search(key).await
    }

    /// Stop asking `source` on every search. Each keeps its run, so the
    /// other source's answer still lands; the dropped source's is refused
    /// because that part is no longer searching.
    pub(super) async fn switch_source_off(&self, source: LookupCatalog) -> Result<(), DbError> {
        self.database.switch_search_source_off(source).await
    }

    /// Whether `run` is still the run `key`'s search is on.
    #[cfg(test)]
    pub(super) async fn search_run_is_current(&self, key: &CandidateKey, run: SearchRun) -> bool {
        self.database.candidate_search_run(key).await.expect("a test reads the search it started")
            == Some(run)
    }

    /// Land `source`'s answer on its part of the search `run`, if that part
    /// is still searching; `false` when the search was replaced or ended, or
    /// the part settled or was switched off. The landing decides in its own
    /// write, so an answer of an older search never writes over a newer one.
    pub(super) async fn land_search(
        &self,
        run: SearchRun,
        source: LookupCatalog,
        outcome: Result<Vec<MetadataResult>, Failure>,
    ) -> Result<bool, DbError> {
        self.database.land_candidate_search(run, source, outcome).await
    }

    /// Land what `release`'s documents state its album is on whatever search
    /// `key` is on now — a statement about the release stays true whichever
    /// search holds it — returning what to keep; nothing when the key has no
    /// search, or its search does not hold the release.
    pub(super) async fn land_picked(
        &self,
        key: &CandidateKey,
        release: &crate::catalog_model::records::LookupRef,
        links: crate::catalog_model::album_links::AlbumLinks,
    ) -> Result<Vec<(String, Vec<crate::catalog_model::records::MetadataRef>)>, DbError> {
        self.database.land_candidate_search_pick(key, release, links).await
    }

    /// The state `run` of the folder candidate `key` published, and the
    /// signals it last read where it read new ones, stored if the key's
    /// standing still names the run; whether they were.
    pub(crate) async fn publish_candidate_run(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
        state: &IdentifyState,
        signals: Option<&Signals>,
    ) -> Result<bool, DbError> {
        let outcome = published_outcome(key, run, state)?;
        self.database
            .publish_candidate_run(key, run, &outcome, state.read_releases(), signals)
            .await
    }

    /// The state `run` of the library release whose re-identify key is `key`
    /// published, with the signals it last read where it read new ones, or
    /// its end with `Idle`.
    pub(crate) async fn publish_release_run(
        &self,
        key: &CandidateKey,
        run: IdentifyRunId,
        state: &IdentifyState,
        signals: Option<&Signals>,
    ) -> Result<(), DbError> {
        match RunOutcome::of(state) {
            None => self.database.end_release_run(key, run).await,
            Some(outcome) => {
                self.database
                    .publish_release_run(key, run, &outcome, state.read_releases(), signals)
                    .await
            }
        }
    }

    /// The scan dropped or reshaped the key: its work in flight ends. Its
    /// search went in the scan's own write, and what its runs published, the
    /// signals they read among it, goes as each run ends; an import outlives
    /// it, and only that import's ending clears it.
    fn end_candidate(&self, key: &CandidateKey) {
        self.end_work(key);
    }

    /// A scan reported `candidate`. A first report or a repeat of the recorded
    /// shape changes nothing; a different shape ends the key.
    fn observe_shape(&self, candidate: &FolderCandidate) {
        let key = candidate.key();
        let shape = CandidateShape::of(candidate);
        let reshaped = {
            let mut inner = self.held();
            let previous = inner.shapes.insert(key.clone(), shape.clone());
            previous.is_some_and(|previous| previous != shape)
        };
        if reshaped {
            self.end_candidate(&key);
        }
    }

    /// `candidate` is read with other files whatever its recorded shape says,
    /// so the key ends.
    fn reshape(&self, candidate: &FolderCandidate) {
        let key = candidate.key();
        self.held().shapes.insert(key.clone(), CandidateShape::of(candidate));
        self.end_candidate(&key);
    }

    /// The scan no longer lists the key, so it ends.
    fn forget(&self, key: &CandidateKey) {
        self.held().shapes.remove(key);
        self.end_candidate(key);
    }

    /// Hold how far through its step `progress`'s import is, for the import
    /// percents' stream. An import's first report, of its preparation, opens
    /// its entry; a later report moves only its own import's entry, and an
    /// ending clears only its own, so a late report of an import that ended
    /// or was replaced moves nothing.
    fn record_import_progress(&self, key: &CandidateKey, progress: &ImportProgress) {
        let import_id = progress.import_id();
        let shown = {
            let mut inner = self.held();
            let own = inner.imports.get(key).is_some_and(|report| report.import_id == import_id);
            match progress {
                ImportProgress::Preparing { .. } => {
                    inner.imports.insert(
                        key.clone(),
                        ImportReport { import_id: import_id.to_string(), percent: None },
                    );
                }
                ImportProgress::Progress { percent, .. } if own => {
                    inner.imports.insert(
                        key.clone(),
                        ImportReport {
                            import_id: import_id.to_string(),
                            percent: percent.map(u32::from),
                        },
                    );
                }
                ImportProgress::Complete { .. }
                | ImportProgress::RemoteUploadQueued { .. }
                | ImportProgress::Failed { .. }
                | ImportProgress::Cancelled { .. }
                    if own =>
                {
                    inner.imports.remove(key);
                }
                _ => debug!("{key}: import {import_id} reported after it ended"),
            }
            inner.imports.get(key).and_then(|report| report.percent)
        };
        self.percents.send_if_modified(|percents| {
            let previous = match shown {
                Some(percent) => percents.insert(key.clone(), percent),
                None => percents.remove(key),
            };
            previous != shown
        });
    }

    /// Hold what `event` says that is not stored: how far an import has got,
    /// which files a candidate has. Every standing an event announces is
    /// written before it is sent.
    pub(super) fn record_event(&self, event: &ImportEvent) {
        match event {
            ImportEvent::ImportProgress { candidate_key, progress } => {
                self.record_import_progress(candidate_key, progress)
            }
            // The driver records its own run's state, beside the standing it
            // writes, before it sends it.
            ImportEvent::IdentifyStateChanged { .. } => {}
            ImportEvent::Scan(
                ScanEvent::FolderCandidate { candidate, .. }
                | ScanEvent::CandidateDiscovered { candidate, .. },
            ) => self.observe_shape(candidate),
            // A rebound sheet is a different disc.
            ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }) => {
                self.reshape(candidate)
            }
            ImportEvent::Scan(ScanEvent::InvalidCandidate(candidate)) => {
                self.forget(&candidate.key())
            }
            ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }) => {
                self.forget(candidate_key)
            }
            // The driver stores the signals its run reads with the state it
            // reaches from them.
            ImportEvent::SignalsUpdated { .. } => {}
            // These change rows, not runtime.
            ImportEvent::Scan(
                ScanEvent::WatchedFoldersChanged { .. }
                | ScanEvent::CandidateSkipChanged { .. }
                | ScanEvent::CandidateMetadataChanged { .. }
                | ScanEvent::FolderScanStatusChanged { .. }
                | ScanEvent::Finished,
            ) => {}
        }
    }
}

/// How `state`, which `run` of `key` published, stands; a run publishes no
/// `Idle` while it runs.
pub(crate) fn published_outcome(
    key: &CandidateKey,
    run: IdentifyRunId,
    state: &IdentifyState,
) -> Result<RunOutcome, DbError> {
    RunOutcome::of(state)
        .ok_or_else(|| DbError::Message(format!("{key}'s run {run:?} published no state to store")))
}
