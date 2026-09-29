//! What is happening right now for each candidate, one fact per field: the
//! queue it waits in, the run in flight, the answer being written, the write
//! that failed, the import running, the search a person typed.
//!
//! Each field has one writer and is never inferred from another. The queue
//! owns `queued`; the identify driver's reports own the run; the write of
//! its answer owns `save_failed`; the import worker owns `import`; the search
//! owns `search`. Where waiting hands over to running, the run's first
//! report clears `queued`, so a key never reads as idle in between.
//!
//! Every identification of an import candidate is counted here, in one batch
//! (see [`batch::IdentificationBatch`]): a key joins when it is admitted and
//! leaves when it is neither waiting nor holding a run.
//!
//! A key has an entry only while something is happening for it. Readers wake
//! on [`Revisions`] and read what they draw. The typed search and
//! extraction's [`Signals`](crate::signals::Signals) live here too, beside the
//! run they belong to, and are dropped with the rest of the key's entry.

use super::candidate_search::CandidateSearch;
use super::candidates::{Admission, CandidateRuntimeSnapshot, ImportInFlight, SaveFailure};
use super::folder_scanner::{FolderCandidate, ReleaseFileScope};
use super::handle::{ImportEvent, ScanEvent};
use super::search::{MetadataResult, SearchQuery};
use super::types::{ImportProgress, ImportStep, Catalog, PrepareStep};
use crate::db::LibraryStatus;
use crate::identify::{IdentifyRunId, IdentifyState};
use crate::signals::{Failure, Signals};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::debug;

mod batch;
mod facts_watch;
mod snapshots_watch;
mod values_watch;
mod work;
pub(crate) use facts_watch::RuntimeFactsWatch;
pub use snapshots_watch::RuntimeSnapshotsWatch;
pub(crate) use values_watch::{RuntimeValue, RuntimeValuesWatch};
pub(crate) use work::CandidateWork;

#[cfg(test)]
pub(crate) mod tests;

use batch::IdentificationBatch;

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

/// How many times each part of the runtime has changed. A reader wakes on
/// either moving and reads the part it draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Revisions {
    /// Every key's runtime and the two counts, which move only with it.
    pub(crate) runtime: u64,
    pub(crate) signals: u64,
}

/// The files a candidate's runtime was recorded against; a scan reporting
/// the key with other files drops the runtime.
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

/// A run and the state it is at; the snapshot carries only the state.
#[derive(Clone, PartialEq)]
struct RunState {
    run: IdentifyRunId,
    state: IdentifyState,
}

/// The import that owns a key and how far it has got. The import id tells its
/// reports from those of an import that already ended.
#[derive(Clone, PartialEq)]
struct ClaimedImport {
    import_id: String,
    in_flight: ImportInFlight,
}

/// The run whose durable write did not land, and what stopped it.
#[derive(Clone, PartialEq)]
struct FailedSave {
    run: IdentifyRunId,
    failure: SaveFailure,
}

/// One key's runtime; [`CandidateRuntimeSnapshot`] is derived from it.
#[derive(Clone, Default, PartialEq)]
struct CandidateRuntimeState {
    /// Set by the queue on admission; cleared by the run's first report, or
    /// by the queue when no run came of it.
    queued: Option<Admission>,
    /// From the driver's reports; never terminal and never `Idle`.
    running: Option<RunState>,
    /// The terminal answer a run reached, held until whoever asked for it
    /// disposes of it: the verdict write, or a re-identify sheet closing.
    answered: Option<RunState>,
    /// Set when an answer's write fails; cleared by the key's next run.
    save_failed: Option<FailedSave>,
    /// Set when an import is queued, advanced by its reports, cleared when it
    /// ends.
    import: Option<ClaimedImport>,
    search: Option<RunningSearch>,
}

/// Where a key stands in identification, as the count reads it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Identifying {
    /// Admitted and its run has not reported yet, which is what joins the
    /// batch; a re-identified library release is never marked, so the count
    /// is the import queue's work alone.
    queued: bool,
    /// Waiting, running, or holding an answer not yet disposed of; a failed
    /// write is over.
    in_flight: bool,
}

impl Identifying {
    fn of(state: &CandidateRuntimeState) -> Self {
        Self {
            queued: state.queued.is_some(),
            in_flight: state.queued.is_some()
                || state.running.is_some()
                || state.answered.is_some(),
        }
    }
}

impl CandidateRuntimeState {
    /// Nothing is happening for the key, so its entry is removed.
    fn is_idle(&self) -> bool {
        self.queued.is_none()
            && self.running.is_none()
            && self.answered.is_none()
            && self.save_failed.is_none()
            && self.import.is_none()
            && self.search.is_none()
    }

    fn snapshot(&self) -> CandidateRuntimeSnapshot {
        CandidateRuntimeSnapshot {
            queued: self.queued,
            running: self.running.as_ref().map(|run| run.state.clone()),
            saving: self.answered.as_ref().map(|run| run.state.clone()),
            save_failed: self.save_failed.as_ref().map(|failed| failed.failure.clone()),
            import: self.import.as_ref().map(|claimed| claimed.in_flight.clone()),
            search: self.search.as_ref().map(|running| running.search.clone()),
        }
    }
}

/// Whether the answer the key holds is `run`'s.
fn answered_on(runtime: &CandidateRuntimeState, run: IdentifyRunId) -> bool {
    runtime
        .answered
        .as_ref()
        .is_some_and(|answered| answered.run == run)
}

/// Whether `import_id` is the import that holds the key.
fn claimed_by(runtime: &CandidateRuntimeState, import_id: &str) -> bool {
    runtime
        .import
        .as_ref()
        .is_some_and(|claimed| claimed.import_id == import_id)
}

fn snapshots(
    runtime: &HashMap<String, CandidateRuntimeState>,
) -> HashMap<String, CandidateRuntimeSnapshot> {
    runtime
        .iter()
        .map(|(key, state)| (key.clone(), state.snapshot()))
        .collect()
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
    /// The identifications in flight.
    batch: IdentificationBatch,
}

impl Inner {
    fn mint_search_run(&mut self) -> u64 {
        let run = self.next_search_run;
        self.next_search_run += 1;
        run
    }

    /// Apply a key's change to the batch; `true` when the counts moved.
    fn count_identification(&mut self, key: &str, was: Identifying, is: Identifying) -> bool {
        if !was.queued && is.queued {
            return self.batch.admit(key);
        }
        if was.in_flight && !is.in_flight {
            return self.batch.end(key);
        }
        false
    }

    /// What every key's identification is doing right now.
    fn identifying(&self) -> HashMap<String, Identifying> {
        self.runtime
            .iter()
            .map(|(key, state)| (key.clone(), Identifying::of(state)))
            .collect()
    }
}

#[derive(Clone)]
pub struct CandidateRuntime {
    inner: Arc<Mutex<Inner>>,
    /// Its own lock: work emits through the bus while holding it, and the
    /// bus records into `inner`.
    work: Arc<Mutex<work::WorkInFlight>>,
    revisions: Arc<watch::Sender<Revisions>>,
    /// Test readers that hear every change to a key's runtime, and every
    /// move of the identification count.
    #[cfg(any(test, feature = "test-utils"))]
    every_change: TestReaders<CandidateRuntimeChange>,
    #[cfg(any(test, feature = "test-utils"))]
    every_count: TestReaders<(u32, u32)>,
}

#[cfg(any(test, feature = "test-utils"))]
type TestReaders<T> = Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<T>>>>;

impl Default for CandidateRuntime {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            work: Arc::default(),
            revisions: Arc::new(watch::Sender::new(Revisions::default())),
            #[cfg(any(test, feature = "test-utils"))]
            every_change: Arc::default(),
            #[cfg(any(test, feature = "test-utils"))]
            every_count: Arc::default(),
        }
    }
}

impl CandidateRuntime {
    /// Every key with something in flight right now, for a subscriber that
    /// joins after runs have started.
    pub fn all(&self) -> HashMap<String, CandidateRuntimeSnapshot> {
        snapshots(&self.inner.lock().unwrap().runtime)
    }

    /// What is in flight for a key, or `None` when nothing is.
    pub fn get(&self, key: &str) -> Option<CandidateRuntimeSnapshot> {
        self.inner
            .lock()
            .unwrap()
            .runtime
            .get(key)
            .map(CandidateRuntimeState::snapshot)
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

    /// How far the identifications in flight have got: ended of admitted.
    pub(crate) fn identification_progress(&self) -> (u32, u32) {
        self.inner.lock().unwrap().batch.progress()
    }

    /// How many keys an import owns right now.
    pub(crate) fn imports_in_flight(&self) -> u32 {
        self.inner
            .lock()
            .unwrap()
            .runtime
            .values()
            .filter(|state| state.import.is_some())
            .count() as u32
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

    /// Every change to a key's runtime from now on, for a test.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn every_change(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<CandidateRuntimeChange> {
        let (reader, changes) = tokio::sync::mpsc::unbounded_channel();
        self.every_change.lock().unwrap().push(reader);
        changes
    }

    /// Every move of the identification count from now on, for a test.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn every_count(&self) -> tokio::sync::mpsc::UnboundedReceiver<(u32, u32)> {
        let (reader, counts) = tokio::sync::mpsc::unbounded_channel();
        self.every_count.lock().unwrap().push(reader);
        counts
    }

    /// Say a key's runtime or the counts changed; `count` is the
    /// identification count when it moved.
    fn runtime_changed(&self, change: Option<CandidateRuntimeChange>, count: Option<(u32, u32)>) {
        #[cfg(any(test, feature = "test-utils"))]
        {
            if let Some(change) = &change {
                self.every_change
                    .lock()
                    .unwrap()
                    .retain(|reader| reader.send(change.clone()).is_ok());
            }
            if let Some(count) = count {
                self.every_count
                    .lock()
                    .unwrap()
                    .retain(|reader| reader.send(count).is_ok());
            }
        }
        #[cfg(not(any(test, feature = "test-utils")))]
        let _ = (change, count);
        self.revisions.send_modify(|revisions| revisions.runtime += 1);
    }

    /// Apply `mutate` to the key's entry under the map's lock, creating one if
    /// needed, and say so if its snapshot or the counts changed. An entry left
    /// idle is removed. Returns what `mutate` returned.
    fn set<R>(
        &self,
        key: &str,
        mutate: impl FnOnce(&mut Inner, &mut CandidateRuntimeState) -> R,
    ) -> R {
        let (result, change, count) = {
            let mut inner = self.inner.lock().unwrap();
            let entry = inner.runtime.get(key);
            let previous = entry.map(CandidateRuntimeState::snapshot);
            let was_identifying = entry.map(Identifying::of).unwrap_or_default();
            let mut next = entry.cloned().unwrap_or_default();
            let result = mutate(&mut inner, &mut next);
            let is_identifying = Identifying::of(&next);
            let change = if next.is_idle() {
                inner.runtime.remove(key);
                previous.is_some().then(|| CandidateRuntimeChange::Removed {
                    key: key.to_string(),
                })
            } else {
                let snapshot = next.snapshot();
                inner.runtime.insert(key.to_string(), next);
                (previous.as_ref() != Some(&snapshot)).then(|| CandidateRuntimeChange::Updated {
                    key: key.to_string(),
                    runtime: snapshot,
                })
            };
            let count = inner
                .count_identification(key, was_identifying, is_identifying)
                .then(|| inner.batch.progress());
            (result, change, count)
        };
        if change.is_some() || count.is_some() {
            self.runtime_changed(change, count);
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
    pub(super) fn retry_search(
        &self,
        key: &str,
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

    /// Mark the MusicBrainz groups `key`'s search needs album links for as being
    /// read, and return what to read — nothing when `run` is not its run.
    pub(super) fn start_reading_album_links(
        &self,
        key: &str,
        run: u64,
    ) -> crate::import::album_links::ToRead {
        self.set(key, |_, runtime| match runtime.search.as_mut() {
            Some(running) if running.run == run => running.search.start_reading_album_links(),
            Some(_) | None => crate::import::album_links::ToRead::default(),
        })
    }

    /// Land the album links read for `key`'s search if `run` is still its run,
    /// returning the links to keep; `None` when the run moved on.
    pub(super) fn land_album_links(
        &self,
        key: &str,
        run: u64,
        read: Vec<crate::import::album_links::GroupReading>,
    ) -> Option<Vec<(String, Vec<crate::import::album_links::AlbumLink>)>> {
        self.set(key, |_, runtime| match runtime.search.as_mut() {
            Some(running) if running.run == run => Some(running.search.record_album_links(read)),
            Some(_) | None => None,
        })
    }

    /// The scan dropped or reshaped the key: its work in flight ends and
    /// everything held for it goes, its signals too, except an import's
    /// claim, which only that import's ending clears.
    fn end_candidate(&self, key: &str) {
        self.end_work(key);
        let signals_removed = self.inner.lock().unwrap().signals.remove(key).is_some();
        self.set(key, |_, runtime| {
            *runtime = CandidateRuntimeState {
                import: runtime.import.take(),
                ..CandidateRuntimeState::default()
            };
        });
        if signals_removed {
            self.revisions.send_modify(|revisions| revisions.signals += 1);
        }
    }

    /// Mark every one of `keys` as waiting on `admission` as one change, so the
    /// count opens at its total.
    pub(super) fn admit(&self, keys: Vec<String>, admission: Admission) {
        if keys.is_empty() {
            return;
        }
        let (reset, counted) = {
            let mut inner = self.inner.lock().unwrap();
            let previous = snapshots(&inner.runtime);
            let was_identifying = inner.identifying();
            for key in &keys {
                inner.runtime.entry(key.clone()).or_default().queued = Some(admission);
            }
            let next = snapshots(&inner.runtime);
            let mut counted = false;
            for key in &keys {
                if !was_identifying.get(key).copied().unwrap_or_default().queued {
                    counted |= inner.batch.admit(key);
                }
            }
            ((next != previous).then_some(next), counted.then(|| inner.batch.progress()))
        };
        if reset.is_some() || counted.is_some() {
            self.runtime_changed(
                reset.map(|runtimes| CandidateRuntimeChange::Reset { runtimes }),
                counted,
            );
        }
    }

    /// This key is not waiting any more: its run started, or nothing came of
    /// the admission that put it here.
    pub(super) fn withdraw(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| runtime.queued = None);
    }

    /// `run`'s answer is disposed of, whatever became of it. Only that run's
    /// answer goes, not a newer run's.
    pub(super) fn end_identification_answer(&self, candidate_key: &str, run: IdentifyRunId) {
        self.set(candidate_key, |_, runtime| {
            if answered_on(runtime, run) {
                runtime.answered = None;
            }
        });
    }

    /// `run`'s answer could not be written; the entry says why. bae's own
    /// failure, so it is logged as one.
    pub(super) fn fail_identification(
        &self,
        candidate_key: &str,
        run: IdentifyRunId,
        failure: SaveFailure,
    ) {
        tracing::error!(
            "identification: {candidate_key}'s answer did not land: {}",
            failure.error()
        );
        self.set(candidate_key, |_, runtime| {
            if answered_on(runtime, run) {
                runtime.answered = None;
            }
            runtime.save_failed = Some(FailedSave { run, failure });
        });
    }

    /// Identification of this key is over and nothing will write what it
    /// found: a cancel, or a re-identified library release's sheet closing.
    pub(super) fn end_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            runtime.running = None;
            runtime.answered = None;
        });
    }

    /// Record what `run` published: a non-terminal state is the run in flight,
    /// a terminal one its answer, and `Idle` a cancel that ends only that run.
    /// A new run clears the last run's failed write, and any state but `Idle`
    /// ends the wait, so the key is never neither waiting nor running.
    fn record_identify_state(&self, candidate_key: &str, run: IdentifyRunId, state: &IdentifyState) {
        self.set(candidate_key, |_, runtime| {
            if matches!(state, IdentifyState::Idle) {
                if runtime
                    .running
                    .as_ref()
                    .is_some_and(|running| running.run == run)
                {
                    runtime.running = None;
                }
                return;
            }
            runtime.queued = None;
            if !runtime
                .running
                .as_ref()
                .is_some_and(|running| running.run == run)
            {
                runtime.save_failed = None;
            }
            if state.is_terminal() {
                runtime.running = None;
                runtime.answered = Some(RunState {
                    run,
                    state: state.clone(),
                });
            } else {
                runtime.running = Some(RunState {
                    run,
                    state: state.clone(),
                });
            }
        });
    }

    /// Record that `import_id` owns this candidate, from the moment its command
    /// is queued rather than when the worker first reports, since whether
    /// identification may still answer the candidate reads this. Refused when
    /// an import already owns it.
    pub(super) fn claim_for_import(
        &self,
        candidate_key: &str,
        import_id: &str,
    ) -> Result<(), crate::import::ImportError> {
        self.set(candidate_key, |_, runtime| {
            if runtime.import.is_some() {
                return Err(crate::import::ImportError::CandidateImportInProgress);
            }
            runtime.import = Some(ClaimedImport {
                import_id: import_id.to_string(),
                in_flight: ImportInFlight {
                    progress_percent: None,
                    step: ImportStep::Preparing(PrepareStep::Queued),
                },
            });
            Ok(())
        })
    }

    /// Undo [`Self::claim_for_import`] for a command that never made it onto
    /// the worker's queue.
    pub(super) fn release_import_claim(&self, candidate_key: &str, import_id: &str) {
        self.set(candidate_key, |_, runtime| {
            if claimed_by(runtime, import_id) {
                runtime.import = None;
            }
        });
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

    pub(super) fn record_event(&self, event: &ImportEvent) {
        match event {
            ImportEvent::ImportProgress {
                candidate_key,
                progress,
            } => {
                // Every ending leaves the map; its row is already written.
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
                // Only the import holding the claim moves it; a late report
                // from an ended import is dropped.
                self.set(candidate_key, |_, runtime| {
                    if !claimed_by(runtime, import_id) {
                        debug!("{candidate_key}: dropped a report of import {import_id}, which does not hold it");
                        return;
                    }
                    runtime.import = in_flight.map(|in_flight| ClaimedImport {
                        import_id: import_id.to_string(),
                        in_flight,
                    });
                });
            }
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                run,
                state,
                priority: _,
            } => self.record_identify_state(candidate_key, *run, state),
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
