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
//! This runtime holds what is not a standing: the state each run last
//! published, which a pane draws while the table names that run; how far an
//! import has got; a person's typed search; extraction's
//! [`Signals`](crate::signals::Signals); and the whole of a library release's
//! re-identify run, which is no list row's and has no standing. The count of
//! identifications in flight is the table's too, moved by each change in its
//! own write. [`CandidateRuntimeSnapshot`] is the table's standing with these
//! beside it.
//!
//! A key has an entry only while something is held for it. Readers of the
//! snapshots and the count wake on the live read of the table and what is
//! held ([`RuntimeView`]); readers of the signals wake on [`Revisions`].

use super::candidate_search::CandidateSearch;
use super::candidates::{CandidateRuntimeSnapshot, ImportInFlight};
use super::folder_scanner::{FolderCandidate, ReleaseFileScope};
use super::handle::{ImportEvent, ScanEvent};
use super::search::{MetadataResult, SearchQuery};
use super::types::{Catalog, ImportProgress, ImportStep};
use crate::db::{Database, LibraryStatus};
use crate::identify::{IdentifyRunId, IdentifyState};
use crate::signals::{Failure, Signals};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::debug;

mod snapshots;
mod snapshots_watch;
mod standing_writes;
mod stored_standing;
mod values_watch;
mod work;
pub use snapshots_watch::RuntimeSnapshotsWatch;
pub(crate) use standing_writes::RunSubject;
pub(crate) use stored_standing::{
    IdentificationChange, IdentificationMove, IdentificationStage, Identifying,
    StoredIdentification, StoredImport, StoredStanding,
};
pub(crate) use snapshots::RuntimeView;
pub(crate) use values_watch::{RuntimeValue, RuntimeValuesWatch};
pub(crate) use work::CandidateWork;

#[cfg(test)]
pub(crate) mod tests;

/// One key's runtime after a change, or its removal.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateRuntimeChange {
    Updated {
        key: String,
        runtime: CandidateRuntimeSnapshot,
    },
    /// Nothing is running for the key any more.
    Removed { key: String },
    /// Every key's runtime, as a reader first reads it.
    Reset {
        runtimes: HashMap<String, CandidateRuntimeSnapshot>,
    },
}

/// How many times the signals the runtime holds have changed. A reader wakes
/// on it moving and reads them.
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

/// The state a run last published, and whether it is a folder candidate's,
/// whose standing the table holds, or a library release's.
#[derive(Clone, PartialEq)]
struct PublishedState {
    state: IdentifyState,
    of_candidate: bool,
}

/// How far the import that last reported for a key has got. The import id
/// tells it from an import that already ended.
#[derive(Clone, PartialEq)]
struct ImportReport {
    import_id: String,
    in_flight: ImportInFlight,
}

/// What this runtime holds for one key.
#[derive(Clone, Default, PartialEq)]
struct CandidateRuntimeState {
    /// The state each of the key's runs last published, by run, until the run
    /// ends.
    runs: BTreeMap<IdentifyRunId, PublishedState>,
    import: Option<ImportReport>,
    search: Option<RunningSearch>,
}

impl CandidateRuntimeState {
    /// Nothing is held for the key, so its entry is removed.
    fn is_idle(&self) -> bool {
        self.runs.is_empty() && self.import.is_none() && self.search.is_none()
    }
}

#[derive(Default)]
struct Inner {
    /// Also holds `reidentify:` keys, which have no scanned folder.
    runtime: HashMap<String, CandidateRuntimeState>,
    /// The files last reported for each scanned key, so a reshape can be told
    /// from a repeat.
    shapes: HashMap<String, CandidateShape>,
    /// The latest signals extraction reported for each key, and their run.
    signals: HashMap<String, (IdentifyRunId, Signals)>,
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
    /// Bumped with every change to what `inner` holds for a key, which the
    /// live runtime snapshots read beside the standings table.
    held_changed: Arc<watch::Sender<()>>,
    /// The library the standings table is in.
    database: Database,
    /// Where a bug the runtime finds with no caller to fail is reported.
    diagnostics: crate::diagnostics::Diagnostics,
    /// Every key's snapshot and the identification count as the live read
    /// last read them.
    view: Arc<watch::Sender<Arc<snapshots::RuntimeView>>>,
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
    /// A runtime over the standings table of `database`, which the library
    /// emptied as it opened, reporting its bugs to `diagnostics`.
    pub(crate) fn new(database: Database, diagnostics: crate::diagnostics::Diagnostics) -> Self {
        Self {
            inner: Arc::default(),
            work: Arc::default(),
            revisions: Arc::new(watch::Sender::new(Revisions::default())),
            held_changed: Arc::new(watch::Sender::new(())),
            database,
            diagnostics,
            // The library opens with the table empty and nothing held, so
            // nothing is in flight until the first change.
            view: Arc::new(watch::Sender::new(Arc::default())),
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
    pub(crate) async fn stored_standing_for_test(&self, key: &str) -> StoredStanding {
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
    pub fn signals(&self, key: &str) -> Option<Signals> {
        self.inner
            .lock()
            .unwrap()
            .signals
            .get(key)
            .map(|(_, signals)| signals.clone())
    }

    /// The signals `run` was judged against, or `None` once the key is on
    /// another run.
    pub(super) fn run_signals(&self, key: &str, run: IdentifyRunId) -> Option<Signals> {
        self.inner
            .lock()
            .unwrap()
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

    /// The latest signals extraction reported for every key.
    pub(crate) fn all_signals(&self) -> HashMap<String, Signals> {
        self.inner
            .lock()
            .unwrap()
            .signals
            .iter()
            .map(|(key, (_, signals))| (key.clone(), signals.clone()))
            .collect()
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

    /// Say what this runtime holds for a key changed.
    fn held_changed(&self) {
        self.held_changed.send_replace(());
    }

    /// Apply `mutate` to the key's entry under the map's lock, creating one if
    /// needed, and say so if it changed. An entry left idle is removed.
    /// Returns what `mutate` returned.
    fn set<R>(
        &self,
        key: &str,
        mutate: impl FnOnce(&mut Inner, &mut CandidateRuntimeState) -> R,
    ) -> R {
        let (result, changed) = {
            let mut inner = self.inner.lock().unwrap();
            let previous = inner.runtime.get(key).cloned();
            let mut next = previous.clone().unwrap_or_default();
            let result = mutate(&mut inner, &mut next);
            let next = (!next.is_idle()).then_some(next);
            let changed = next != previous;
            match next {
                Some(next) => inner.runtime.insert(key.to_string(), next),
                None => inner.runtime.remove(key),
            };
            (result, changed)
        };
        if changed {
            self.held_changed();
        }
        result
    }

    /// Put `key` on a new search run carrying `search`, and return the run a
    /// landing must name to count.
    pub(super) fn start_search(&self, key: &str, search: CandidateSearch) -> u64 {
        self.set(key, |inner, runtime| {
            let run = inner.mint_search_run();
            runtime.search = Some(RunningSearch { run, search });
            run
        })
    }

    /// Put every failed source of `key`'s search back to looking on a new run,
    /// returning the query, the sources to re-ask and the run; `None` when
    /// there is nothing to re-ask.
    pub(super) fn retry_search(&self, key: &str) -> Option<(SearchQuery, Vec<Catalog>, u64)> {
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
    pub(super) fn clear_search(&self, key: &str) {
        self.set(key, |_, runtime| runtime.search = None);
    }

    /// Stop asking `source` on every running search. Each keeps its run, so
    /// the other source's lookup in flight still lands; the dropped source's
    /// answer is refused because that part is no longer looking.
    pub(super) fn switch_source_off(&self, source: Catalog) {
        let searching: Vec<String> = self
            .inner
            .lock()
            .unwrap()
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
    pub(super) fn search_run_is_current(&self, key: &str, run: u64) -> bool {
        self.inner
            .lock()
            .unwrap()
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
        key: &str,
        run: u64,
        source: Catalog,
        outcome: Result<Vec<(MetadataResult, LibraryStatus)>, Failure>,
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
        key: &str,
        release: &crate::import::MetadataRef,
        links: crate::import::album_links::AlbumLinks,
    ) -> Vec<(String, Vec<crate::import::album_links::AlbumLink>)> {
        self.set(key, |_, runtime| match runtime.search.as_mut() {
            Some(running) => running.search.record_picked(release, links),
            None => Vec::new(),
        })
    }

    /// The state `run` of `key` published; a folder candidate's is drawn
    /// while the standings table names the run.
    pub(crate) fn record_run_state(
        &self,
        key: &str,
        run: IdentifyRunId,
        of_candidate: bool,
        state: &IdentifyState,
    ) {
        self.set(key, |_, runtime| {
            runtime.runs.insert(
                run,
                PublishedState {
                    state: state.clone(),
                    of_candidate,
                },
            );
        });
    }

    /// `run` of `key` is over: what it published is not drawn any more.
    pub(crate) fn forget_run(&self, key: &str, run: IdentifyRunId) {
        self.set(key, |_, runtime| {
            runtime.runs.remove(&run);
        });
    }

    /// Every run of `key` is over.
    fn forget_runs(&self, key: &str) {
        self.set(key, |_, runtime| runtime.runs.clear());
    }

    /// The scan dropped or reshaped the key: its work in flight ends, and its
    /// search and signals go with the files they were of. What its runs
    /// published goes as each run ends; an import outlives it, and only that
    /// import's ending clears it.
    fn end_candidate(&self, key: &str) {
        self.end_work(key);
        let signals_removed = self.inner.lock().unwrap().signals.remove(key).is_some();
        self.set(key, |_, runtime| runtime.search = None);
        if signals_removed {
            self.revisions.send_modify(|revisions| revisions.signals += 1);
        }
    }

    /// A scan reported `candidate`. A first report or a repeat of the recorded
    /// shape changes nothing; a different shape ends the key.
    fn observe_shape(&self, candidate: &FolderCandidate) {
        let key = candidate.key();
        let shape = CandidateShape::of(candidate);
        let reshaped = {
            let mut inner = self.inner.lock().unwrap();
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
        self.inner
            .lock()
            .unwrap()
            .shapes
            .insert(key.clone(), CandidateShape::of(candidate));
        self.end_candidate(&key);
    }

    /// The scan no longer lists the key, so it ends.
    fn forget(&self, key: &str) {
        self.inner.lock().unwrap().shapes.remove(key);
        self.end_candidate(key);
    }

    /// Hold what `event` says that is not a standing: how far an import has
    /// got, which files a candidate has, the signals extraction found. Every
    /// standing an event announces is written before it is sent.
    pub(super) fn record_event(&self, event: &ImportEvent) {
        match event {
            ImportEvent::ImportProgress {
                candidate_key,
                progress,
            } => {
                let in_flight = match progress {
                    ImportProgress::Preparing { step, .. } => Some(ImportInFlight {
                        progress_percent: None,
                        step: ImportStep::Preparing(*step),
                    }),
                    ImportProgress::Progress { percent, phase, .. } => Some(ImportInFlight {
                        progress_percent: percent.map(u32::from),
                        step: ImportStep::Running(*phase),
                    }),
                    ImportProgress::Complete { .. }
                    | ImportProgress::RemoteUploadQueued { .. }
                    | ImportProgress::Failed { .. }
                    | ImportProgress::Cancelled { .. } => None,
                };
                let import_id = progress.import_id();
                self.set(candidate_key, |_, runtime| match in_flight {
                    Some(in_flight) => {
                        runtime.import = Some(ImportReport {
                            import_id: import_id.to_string(),
                            in_flight,
                        });
                    }
                    // An ending clears only its own import's progress: a late
                    // ending of an import that was replaced leaves the
                    // replacement's.
                    None => {
                        if runtime
                            .import
                            .as_ref()
                            .is_some_and(|report| report.import_id == import_id)
                        {
                            runtime.import = None;
                        } else {
                            debug!("{candidate_key}: import {import_id} ended after another reported");
                        }
                    }
                });
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
            ImportEvent::SignalsUpdated {
                candidate_key,
                run,
                signals,
                artwork: _,
                priority: _,
            } => {
                let previous = self
                    .inner
                    .lock()
                    .unwrap()
                    .signals
                    .insert(candidate_key.clone(), (*run, signals.clone()));
                if previous.is_none_or(|(_, previous)| previous != *signals) {
                    self.revisions.send_modify(|revisions| revisions.signals += 1);
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
