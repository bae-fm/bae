//! What is happening right now for each candidate, one fact per field: the
//! queue it waits in, the run in flight, the answer being written, the write
//! that failed, the import running, the search a person typed.
//!
//! Each field has one writer and is never inferred from another. The queue
//! owns `queued`; the identify driver's broadcasts own the run; the write of
//! its answer owns `save_failed`; the import worker owns `import`; the search
//! owns `search`. Where waiting hands over to running, the run's first
//! broadcast clears `queued`, so a key never reads as idle in between.
//!
//! Every identification of an import candidate is counted here, in one batch
//! (see [`batch::IdentificationBatch`]): a key joins when it is admitted and
//! leaves when it is neither waiting nor holding a run.
//!
//! A key has an entry only while something is happening for it. Changes are
//! published per key, so a consumer holding the list is not sent it again.
//! The typed search and extraction's [`Signals`](crate::signals::Signals)
//! live here too, beside the run they belong to, and are dropped with the
//! rest of the key's entry; the signals are not in the published snapshot.

use super::candidate_search::CandidateSearch;
use super::candidates::{Admission, CandidateRuntimeSnapshot, ImportInFlight};
use super::folder_scanner::{FolderCandidate, ReleaseFileScope};
use super::handle::{ImportEvent, ScanEvent};
use super::search::{MetadataResult, SearchQuery};
use super::types::{ImportProgress, ImportStep, Catalog, PrepareStep};
use crate::db::LibraryStatus;
use crate::identify::{IdentifyRunId, IdentifyState};
use crate::signals::{LookupFailure, Signals};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::broadcast;
use tracing::{debug, info};

mod batch;
mod facts_watch;
pub(crate) use facts_watch::RuntimeFactsWatch;

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
    /// The complete runtime after an atomic multi-key queue change.
    Reset {
        runtimes: HashMap<String, CandidateRuntimeSnapshot>,
    },
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
    error: String,
}

/// One key's runtime; [`CandidateRuntimeSnapshot`] is derived from it.
#[derive(Clone, Default, PartialEq)]
struct CandidateRuntimeState {
    /// Set by the queue on admission; cleared by the run's first broadcast, or
    /// by the queue when no run came of it.
    queued: Option<Admission>,
    /// From the driver's broadcasts; never terminal and never `Idle`.
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
            save_failed: self.save_failed.as_ref().map(|failed| failed.error.clone()),
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
    /// How many keys an import owns right now.
    importing: u32,
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
    changes: broadcast::Sender<CandidateRuntimeChange>,
    /// Test readers that hear every change, however far behind they read.
    #[cfg(test)]
    every_change:
        Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<CandidateRuntimeChange>>>>,
    /// Where the identification count is announced: the delivery of the bus
    /// this runtime records into, set once by that bus. A runtime with no bus
    /// announces nothing.
    events: Arc<OnceLock<crate::import::handle::EventDelivery>>,
}

impl Default for CandidateRuntime {
    fn default() -> Self {
        let (changes, _) = broadcast::channel(1024);
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            changes,
            #[cfg(test)]
            every_change: Arc::default(),
            events: Arc::new(OnceLock::new()),
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

    pub fn subscribe(&self) -> broadcast::Receiver<CandidateRuntimeChange> {
        self.changes.subscribe()
    }

    /// Every change from now on, none dropped, since [`Self::subscribe`]
    /// drops what a slow reader falls behind on.
    #[cfg(test)]
    pub(crate) fn every_change(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<CandidateRuntimeChange> {
        let (reader, changes) = tokio::sync::mpsc::unbounded_channel();
        self.every_change.lock().unwrap().push(reader);
        changes
    }

    /// Take the delivery of the one bus this runtime records into, which
    /// calls this as it is built.
    pub(super) fn announce_on(&self, events: crate::import::handle::EventDelivery) {
        assert!(
            self.events.set(events).is_ok(),
            "a candidate runtime records into one import bus"
        );
    }

    fn publish(&self, change: CandidateRuntimeChange) {
        #[cfg(test)]
        self.every_change
            .lock()
            .unwrap()
            .retain(|reader| reader.send(change.clone()).is_ok());
        // Nobody listening yet is not an error.
        let _ = self.changes.send(change);
    }

    /// Say how far the identifications in flight have got. Delivered rather
    /// than sent through the bus, since this runs inside the bus's recording.
    fn announce(&self, (identified, total): (u32, u32)) {
        let Some(events) = self.events.get() else {
            return;
        };
        info!("identification progress at {identified}/{total}");
        events.deliver(ImportEvent::IdentificationProgress { identified, total });
    }

    /// Say how many imports are in flight, the same way as [`Self::announce`].
    fn announce_imports(&self, count: u32) {
        let Some(events) = self.events.get() else {
            return;
        };
        events.deliver(ImportEvent::ImportsInFlight { count });
    }

    /// Apply `mutate` to the key's entry under the map's lock, creating one if
    /// needed, and publish the snapshot it left if that changed. An entry left
    /// idle is removed. Returns what `mutate` returned.
    fn set<R>(
        &self,
        key: &str,
        mutate: impl FnOnce(&mut Inner, &mut CandidateRuntimeState) -> R,
    ) -> R {
        let (result, change, progress, importing) = {
            let mut inner = self.inner.lock().unwrap();
            let entry = inner.runtime.get(key);
            let previous = entry.map(CandidateRuntimeState::snapshot);
            let was_identifying = entry.map(Identifying::of).unwrap_or_default();
            let was_importing = entry.is_some_and(|entry| entry.import.is_some());
            let mut next = entry.cloned().unwrap_or_default();
            let result = mutate(&mut inner, &mut next);
            let is_identifying = Identifying::of(&next);
            let is_importing = next.import.is_some();
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
            let progress = inner
                .count_identification(key, was_identifying, is_identifying)
                .then(|| inner.batch.progress());
            let importing = match (was_importing, is_importing) {
                (false, true) => {
                    inner.importing += 1;
                    Some(inner.importing)
                }
                (true, false) => {
                    inner.importing -= 1;
                    Some(inner.importing)
                }
                _ => None,
            };
            (result, change, progress, importing)
        };
        if let Some(change) = change {
            self.publish(change);
        }
        if let Some(progress) = progress {
            self.announce(progress);
        }
        if let Some(count) = importing {
            self.announce_imports(count);
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
        outcome: Result<Vec<(MetadataResult, LibraryStatus)>, LookupFailure>,
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

    /// Drop everything held for a key, its signals too.
    fn remove(&self, key: &str) {
        let (removed, progress) = {
            let mut inner = self.inner.lock().unwrap();
            inner.signals.remove(key);
            let was_identifying = inner
                .runtime
                .get(key)
                .map(Identifying::of)
                .unwrap_or_default();
            let removed = inner.runtime.remove(key).is_some();
            let progress = inner
                .count_identification(key, was_identifying, Identifying::default())
                .then(|| inner.batch.progress());
            (removed, progress)
        };
        if removed {
            self.publish(CandidateRuntimeChange::Removed {
                key: key.to_string(),
            });
        }
        if let Some(progress) = progress {
            self.announce(progress);
        }
    }

    /// Mark every one of `keys` as waiting on `admission`, published as one
    /// change so the count opens at its total.
    pub(super) fn admit(&self, keys: Vec<String>, admission: Admission) {
        if keys.is_empty() {
            return;
        }
        let (reset, progress) = {
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
            let progress = counted.then(|| inner.batch.progress());
            ((next != previous).then_some(next), progress)
        };
        if let Some(runtimes) = reset {
            self.publish(CandidateRuntimeChange::Reset { runtimes });
        }
        if let Some(progress) = progress {
            self.announce(progress);
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

    /// `run`'s answer could not be written; the entry says why.
    pub(super) fn fail_identification(
        &self,
        candidate_key: &str,
        run: IdentifyRunId,
        error: String,
    ) {
        self.set(candidate_key, |_, runtime| {
            if answered_on(runtime, run) {
                runtime.answered = None;
            }
            runtime.save_failed = Some(FailedSave { run, error });
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
                    step: Some(ImportStep::Preparing(PrepareStep::Queued)),
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
    /// shape changes nothing; a different shape drops the key's runtime.
    fn observe_shape(&self, candidate: &FolderCandidate) {
        let key = candidate.path.to_string_lossy().into_owned();
        let shape = CandidateShape::of(candidate);
        let reshaped = {
            let mut inner = self.inner.lock().unwrap();
            let previous = inner.shapes.insert(key.clone(), shape.clone());
            previous.is_some_and(|previous| previous != shape)
        };
        if reshaped {
            self.remove(&key);
        }
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
                        step: Some(ImportStep::Preparing(*step)),
                    }),
                    ImportProgress::Progress { percent, phase, .. } => Some(ImportInFlight {
                        progress_percent: percent.map(u32::from),
                        step: Some(ImportStep::Running(*phase)),
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
            // A rebound sheet is a different disc, so its search goes.
            ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }) => {
                self.clear_search(&candidate.path.to_string_lossy());
                self.observe_shape(candidate);
            }
            ImportEvent::Scan(ScanEvent::InvalidCandidate(candidate)) => {
                let key = candidate.path.to_string_lossy().into_owned();
                self.inner.lock().unwrap().shapes.remove(&key);
                self.remove(&key);
            }
            ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }) => {
                self.inner.lock().unwrap().shapes.remove(candidate_key);
                self.remove(candidate_key);
            }
            // Kept but not published: no runtime consumer draws them.
            ImportEvent::SignalsUpdated {
                candidate_key,
                run,
                signals,
                artwork: _,
                priority: _,
            } => {
                self.inner
                    .lock()
                    .unwrap()
                    .signals
                    .insert(candidate_key.clone(), (*run, signals.clone()));
            }
            // The counts are this map's own announcements, and these scan
            // events change rows, not runtime.
            ImportEvent::Scan(
                ScanEvent::WatchedFoldersChanged { .. }
                | ScanEvent::CandidateSkipChanged { .. }
                | ScanEvent::CandidateMetadataChanged { .. }
                | ScanEvent::FolderScanStatusChanged { .. }
                | ScanEvent::Finished,
            )
            | ImportEvent::IdentificationProgress { .. }
            | ImportEvent::ImportsInFlight { .. } => {}
        }
    }
}
