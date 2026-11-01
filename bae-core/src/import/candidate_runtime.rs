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
//! This runtime holds what is not stored: how far through its step an import
//! is, for the import percents' stream alone; a person's typed search; and
//! extraction's [`Signals`](crate::signals::Signals).
//! [`CandidateRuntimeSnapshot`] is the stored runtime with these beside it.
//!
//! A key has an entry only while something is held for it. Readers of the
//! snapshots wake on [`RuntimeView`], which a read of the table
//! and a change to what is held each move key by key, naming the keys they
//! moved; readers of the signals wake on [`Revisions`], which names the keys
//! whose signals moved.

use super::candidate_key::CandidateKey;
use super::candidate_search::CandidateSearch;
use super::candidates::CandidateRuntimeSnapshot;
use super::folder_scanner::{FolderCandidate, ReleaseFileScope};
use super::handle::{ImportEvent, ScanEvent};
use super::search::{MetadataResult, SearchQuery};
use super::types::ImportProgress;
use crate::db::{Database, DbError};
use crate::identify::{IdentifyRunId, IdentifyState, RunOutcome};
use crate::signals::{Failure, Signals};
use bae_catalogs::catalog::Catalog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::debug;

mod change_log;
mod snapshots;
mod snapshots_watch;
mod standing_writes;
mod stored_standing;
mod values_watch;
mod work;
pub(crate) use snapshots::RuntimeView;
pub use snapshots_watch::RuntimeSnapshotsWatch;
pub(crate) use standing_writes::RunSubject;
pub(crate) use stored_standing::{
    IdentificationChange, IdentificationMove, IdentificationStage, Identifying,
    StoredIdentification, StoredImport, StoredRuntime, StoredStanding,
};
pub(crate) use values_watch::{RuntimeValue, RuntimeValuesWatch};
pub(crate) use work::CandidateWork;

#[cfg(test)]
pub(crate) mod tests;

/// One key's runtime after a change, or its removal.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateRuntimeChange {
    Updated {
        key: CandidateKey,
        runtime: CandidateRuntimeSnapshot,
    },
    /// Nothing is running for the key any more.
    Removed { key: CandidateKey },
    /// Every key's runtime, as a reader first reads it.
    Reset {
        runtimes: HashMap<CandidateKey, CandidateRuntimeSnapshot>,
    },
}

/// How many times the signals the runtime holds have changed. A reader wakes
/// on it moving and reads the keys whose signals moved since it last looked
/// ([`CandidateRuntime::signals_since`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Revisions {
    pub(crate) signals: u64,
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

/// A candidate's search and the run it is on, which tells a current landing
/// from a superseded one.
#[derive(Clone, PartialEq)]
struct RunningSearch {
    run: u64,
    search: CandidateSearch,
}

/// How far through its step the import that last reported for a key is. The
/// import id tells it from an import that already ended.
#[derive(Clone, PartialEq)]
struct ImportReport {
    import_id: String,
    /// How far through its step it is, once the step counts any.
    percent: Option<u32>,
}

/// What this runtime holds for one key.
#[derive(Clone, Default, PartialEq)]
struct CandidateRuntimeState {
    search: Option<RunningSearch>,
}

impl CandidateRuntimeState {
    /// Nothing is held for the key, so its entry is removed.
    fn is_idle(&self) -> bool {
        self.search.is_none()
    }
}

#[derive(Default)]
struct Inner {
    /// Also holds `reidentify:` keys, which have no scanned folder.
    runtime: HashMap<CandidateKey, CandidateRuntimeState>,
    /// The files last reported for each scanned key, so a reshape can be told
    /// from a repeat.
    shapes: HashMap<CandidateKey, CandidateShape>,
    /// How far through its step each key's import is, as it last reported.
    imports: HashMap<CandidateKey, ImportReport>,
    /// The latest signals extraction reported for each key, and their run.
    signals: HashMap<CandidateKey, (IdentifyRunId, Signals)>,
    /// Which keys' signals moved, so a reader reads only those.
    signal_changes: change_log::ChangeLog,
    /// The next search run number, one counter across every key so no old
    /// run is mistaken for a current one.
    next_search_run: u64,
}

impl Inner {
    fn mint_search_run(&mut self) -> u64 {
        let run = self.next_search_run;
        self.next_search_run += 1;
        run
    }
}

#[derive(Clone)]
pub struct CandidateRuntime {
    inner: Arc<Mutex<Inner>>,
    /// Its own lock: work emits through the bus while holding it, and the
    /// bus records into `inner`.
    work: Arc<Mutex<work::WorkInFlight>>,
    revisions: Arc<watch::Sender<Revisions>>,
    /// The library the standings table is in.
    database: Database,
    /// Where a bug the runtime finds with no caller to fail is reported.
    diagnostics: crate::diagnostics::Diagnostics,
    /// Every key's snapshot, kept key by key from the live read of the table
    /// and what `inner` holds.
    view: Arc<watch::Sender<snapshots::RuntimeView>>,
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
            revisions: Arc::new(watch::Sender::new(Revisions::default())),
            database,
            diagnostics,
            // The library opens with the table empty and nothing held, so
            // nothing is in flight until the first change.
            view: Arc::new(watch::Sender::new(snapshots::RuntimeView::default())),
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
        self.database
            .load_candidate_standing(key)
            .await
            .expect("a test reads the standings table")
    }

    /// Another runtime over this one's library, holding nothing of what this
    /// one holds, for a test of what the table alone decides.
    #[cfg(test)]
    pub(crate) fn over_the_same_library_for_test(&self) -> Self {
        Self::new(self.database.clone(), self.diagnostics.clone())
    }

    /// The signals extraction has found for a key so far, or `None` before it
    /// has reported any.
    pub fn signals(&self, key: &CandidateKey) -> Option<Signals> {
        self.held()
            .signals
            .get(key)
            .map(|(_, signals)| signals.clone())
    }

    /// The signals `run` was judged against, or `None` once the key is on
    /// another run.
    pub(super) fn run_signals(&self, key: &CandidateKey, run: IdentifyRunId) -> Option<Signals> {
        self.held()
            .signals
            .get(key)
            .filter(|(extracted, _)| *extracted == run)
            .map(|(_, signals)| signals.clone())
    }

    /// Wakes whenever a part of the runtime changes; a reader takes it before
    /// its first read, so no change lands between the two.
    pub(crate) fn watch_revisions(&self) -> watch::Receiver<Revisions> {
        self.revisions.subscribe()
    }

    /// The signals of each key whose signals moved after `seen`, and where
    /// that leaves a reader who has read them. A key whose signals went with
    /// its files is left out.
    pub(crate) fn signals_since(&self, seen: u64) -> (u64, Vec<(CandidateKey, Signals)>) {
        let inner = self.held();
        let moved = inner
            .signal_changes
            .since(seen)
            .filter_map(|key| {
                inner
                    .signals
                    .get(key)
                    .map(|(_, signals)| (key.clone(), signals.clone()))
            })
            .collect();
        (inner.signal_changes.sequence(), moved)
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

    /// Apply `mutate` to the key's entry under the map's lock, creating one if
    /// needed, and compose the key's snapshot again if it changed. An entry
    /// left idle is removed. Returns what `mutate` returned.
    fn set<R>(
        &self,
        key: &CandidateKey,
        mutate: impl FnOnce(&mut Inner, &mut CandidateRuntimeState) -> R,
    ) -> R {
        let (result, changed) = {
            let mut inner = self.held();
            let previous = inner.runtime.get(key).cloned();
            let mut next = previous.clone().unwrap_or_default();
            let result = mutate(&mut inner, &mut next);
            let next = (!next.is_idle()).then_some(next);
            let changed = next != previous;
            match next {
                Some(next) => inner.runtime.insert(key.clone(), next),
                None => inner.runtime.remove(key),
            };
            (result, changed)
        };
        if changed {
            self.recompose(key);
        }
        result
    }

    /// Put `key` on a new search run carrying `search`, and return the run a
    /// landing must name to count.
    pub(super) fn start_search(&self, key: &CandidateKey, search: CandidateSearch) -> u64 {
        self.set(key, |inner, runtime| {
            let run = inner.mint_search_run();
            runtime.search = Some(RunningSearch { run, search });
            run
        })
    }

    /// Put every failed source of `key`'s search back to looking on a new run,
    /// returning the query, the sources to re-ask and the run; `None` when
    /// there is nothing to re-ask.
    pub(super) fn retry_search(
        &self,
        key: &CandidateKey,
    ) -> Option<(SearchQuery, Vec<Catalog>, u64)> {
        self.set(key, |inner, runtime| {
            let running = runtime.search.as_mut()?;
            let mut search = running.search.clone();
            search.restart_failed();
            let sources = search.searching_sources();
            if sources.is_empty() {
                return None;
            }
            let run = inner.mint_search_run();
            let query = search.query.clone();
            *running = RunningSearch { run, search };
            Some((query, sources, run))
        })
    }

    /// Drop `key`'s search, so nothing its run has out can land.
    pub(super) fn clear_search(&self, key: &CandidateKey) {
        self.set(key, |_, runtime| runtime.search = None);
    }

    /// Stop asking `source` on every running search. Each keeps its run, so
    /// the other source's lookup in flight still lands; the dropped source's
    /// answer is refused because that part is no longer looking.
    pub(super) fn switch_source_off(&self, source: Catalog) {
        let searching: Vec<CandidateKey> = self
            .held()
            .runtime
            .iter()
            .filter(|(_, state)| state.search.is_some())
            .map(|(key, _)| key.clone())
            .collect();
        for key in searching {
            self.set(&key, |_, runtime| {
                if let Some(running) = runtime.search.as_mut() {
                    running.search.switch_off(source);
                }
            });
        }
    }

    /// Whether `run` is still the run `key`'s search is on.
    #[cfg(test)]
    pub(super) fn search_run_is_current(&self, key: &CandidateKey, run: u64) -> bool {
        self.held()
            .runtime
            .get(key)
            .and_then(|state| state.search.as_ref())
            .is_some_and(|running| running.run == run)
    }

    /// Land one source's answer on `key`'s search if `run` is still its run;
    /// `false` when it was cleared or superseded. Landing and superseding
    /// share the map's lock, so an old run never writes over a newer one.
    pub(super) fn land_search(
        &self,
        key: &CandidateKey,
        run: u64,
        source: Catalog,
        outcome: Result<Vec<MetadataResult>, Failure>,
    ) -> bool {
        self.set(key, |_, runtime| {
            let Some(running) = runtime.search.as_mut() else {
                return false;
            };
            if running.run != run {
                return false;
            }
            running.search.record(source, outcome);
            true
        })
    }

    /// Land what `release`'s documents state its album is on whatever search
    /// `key` is on now — a statement about the release stays true whichever
    /// search holds it — returning what to keep; nothing when the key has no
    /// search, or its search does not hold the release.
    pub(super) fn land_picked(
        &self,
        key: &CandidateKey,
        release: &crate::import::MetadataRef,
        links: crate::import::album_links::AlbumLinks,
    ) -> Vec<(String, Vec<crate::import::MetadataRef>)> {
        self.set(key, |_, runtime| match runtime.search.as_mut() {
            Some(running) => running.search.record_picked(release, links),
            None => Vec::new(),
        })
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
            .publish_candidate_run(key, run, &outcome, signals)
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
                    .publish_release_run(key, run, &outcome, signals)
                    .await
            }
        }
    }

    /// The scan dropped or reshaped the key: its work in flight ends, and its
    /// search and signals go with the files they were of. What its runs
    /// published goes as each run ends; an import outlives it, and only that
    /// import's ending clears it.
    fn end_candidate(&self, key: &CandidateKey) {
        self.end_work(key);
        let signals_removed = {
            let mut inner = self.held();
            let removed = inner.signals.remove(key).is_some();
            if removed {
                inner.signal_changes.record(key);
            }
            removed
        };
        self.set(key, |_, runtime| runtime.search = None);
        if signals_removed {
            self.revisions
                .send_modify(|revisions| revisions.signals += 1);
        }
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
        self.held()
            .shapes
            .insert(key.clone(), CandidateShape::of(candidate));
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
            let own = inner
                .imports
                .get(key)
                .is_some_and(|report| report.import_id == import_id);
            match progress {
                ImportProgress::Preparing { .. } => {
                    inner.imports.insert(
                        key.clone(),
                        ImportReport {
                            import_id: import_id.to_string(),
                            percent: None,
                        },
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

    /// Hold what `event` says that is not a standing: how far an import has
    /// got, which files a candidate has, the signals extraction found. Every
    /// standing an event announces is written before it is sent.
    pub(super) fn record_event(&self, event: &ImportEvent) {
        match event {
            ImportEvent::ImportProgress {
                candidate_key,
                progress,
            } => self.record_import_progress(candidate_key, progress),
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
            ImportEvent::SignalsUpdated {
                candidate_key,
                run,
                signals,
                artwork: _,
                priority: _,
            } => {
                let moved = {
                    let mut inner = self.held();
                    let previous = inner
                        .signals
                        .insert(candidate_key.clone(), (*run, signals.clone()));
                    let moved = previous.is_none_or(|(_, previous)| previous != *signals);
                    if moved {
                        inner.signal_changes.record(candidate_key);
                    }
                    moved
                };
                if moved {
                    self.revisions
                        .send_modify(|revisions| revisions.signals += 1);
                }
            }
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
