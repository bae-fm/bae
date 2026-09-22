//! What is happening right now for each candidate, one fact per field: the
//! queue it is waiting in, the run in flight, the answer being written, the
//! write that failed, the import running, the search a person typed.
//!
//! **Each field has one writer and is never inferred from another.** The
//! identification queue owns `queued`, for both of its admissions; the
//! identify driver's broadcasts own the run and the state it reached; the
//! write of that run's answer owns the end of it and `save_failed`; the import
//! worker owns `import`; and a candidate's search owns `search`. Nothing here
//! reads one field to decide another, and nothing depends on the order two
//! producers happened to reach it in.
//!
//! The one place two producers meet is the handover from waiting to running:
//! a key stops waiting because its run started, and the run that started is
//! what says so, in its first broadcast. Whoever queued the key clears the
//! mark only when no run came of it. Told from the queue side instead, the
//! mark went before the first broadcast arrived and the key read as nothing
//! at all in between — which is a candidate that shows as idle mid-flight,
//! and an identification count that drops the key and picks it up again.
//!
//! **Every identification of an import candidate is counted here**, in one
//! batch that is whatever is being identified right now however it was
//! started — see [`batch::IdentificationBatch`]. The count belongs here
//! because this is what holds it: a key joins the batch when it is admitted to
//! identification, which is what `queued` says and what both admissions do
//! first; it leaves when it is neither waiting nor holding a run.
//!
//! One entry per key that has any of them, and no entry at all otherwise.
//! Everything an entry used to outlive itself carrying has a table now: the
//! verdict the run settled on, the signals a settled run stored, the release
//! an import wrote, the error one failed with. Whoever wants those reads the
//! rows.
//!
//! Changes are published per key — one [`CandidateRuntimeChange`] for the one
//! candidate an event concerned — so a consumer holding the list never
//! receives the list again because one row's run advanced.
//!
//! A candidate's typed search is held here as the value each source's landing
//! folds into, not as a copy of one held elsewhere: [`CandidateRuntime`]
//! starts, retries, lands and clears it, and every one of those publishes the
//! key in the same call. So the search a surface draws is the search a landing
//! reads back, and the run numbers that tell a current landing from a
//! superseded one are kept beside it under the same lock.
//!
//! Extraction's [`Signals`](crate::signals::Signals) are held here too, beside
//! the run they were extracted for, and deliberately *not* in the published
//! snapshot: they change at extraction's own cadence, one form reads them, and
//! that form is fed by its own UI-bus event. A settle takes the pair for its
//! own run rather than keeping a second copy of the snapshots as they arrive.
//! What they share with the rest of this map is a lifetime — they describe the
//! same key's current files and are dropped by the same events —
//! which is why they live here rather than in a second map somebody would
//! have to remember to clear.

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
use tracing::info;

mod batch;

#[cfg(test)]
mod tests;

use batch::IdentificationBatch;

/// One key's runtime after a change, or its removal.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateRuntimeChange {
    Updated {
        key: String,
        runtime: CandidateRuntimeSnapshot,
    },
    /// Nothing is running for the key any more: its import ended, its run
    /// settled and stored, its folder left the scan, or its files changed
    /// shape so what was recorded described a folder that no longer exists.
    Removed { key: String },
    /// The complete runtime after an atomic multi-key queue change.
    Reset {
        runtimes: HashMap<String, CandidateRuntimeSnapshot>,
    },
}

/// The file shape a candidate's runtime was recorded against. A scan that
/// reports the same key with a different shape invalidates the runtime: the
/// state and progress described the old files.
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

/// A candidate's search and the run it is on. A run number tells a landing
/// from a superseded search apart from one that is still current.
#[derive(Clone, PartialEq)]
struct RunningSearch {
    run: u64,
    search: CandidateSearch,
}

/// A run and the state it is at. The run id tells this run's states from a
/// superseded run's, and is bookkeeping rather than something a surface
/// draws, so [`CandidateRuntimeSnapshot`] carries only the state.
#[derive(Clone, PartialEq)]
struct RunState {
    run: IdentifyRunId,
    state: IdentifyState,
}

/// The run whose durable write did not land, and what stopped it.
#[derive(Clone, PartialEq)]
struct FailedSave {
    run: IdentifyRunId,
    error: String,
}

/// One key's runtime as this map holds it. [`CandidateRuntimeSnapshot`] is
/// derived from it rather than kept beside it: the run numbers are
/// bookkeeping for the producers that write these fields, and no surface
/// draws them.
#[derive(Clone, Default, PartialEq)]
struct CandidateRuntimeState {
    /// Written by the identification queue when it admits a candidate, on
    /// either admission. Cleared by the first broadcast of the run that was
    /// waited for, or by the queue when no run came of it.
    queued: Option<Admission>,
    /// Written from the driver's broadcasts. Never terminal and never `Idle`:
    /// both of those end the run rather than being a state it sits at.
    running: Option<RunState>,
    /// The answer a run reached, held until whoever asked for it says what
    /// became of it — the verdict write for a candidate, and for a library
    /// release being re-identified, the sheet closing. Always terminal.
    ///
    /// Separate from a write being under way: a re-identify sheet's run writes
    /// no verdict at all, and an answer that is nobody's to write would
    /// otherwise sit here for the life of the process.
    answered: Option<RunState>,
    /// Written when a write of an answer fails, cleared by the next run of this
    /// key.
    save_failed: Option<FailedSave>,
    import: Option<ImportInFlight>,
    search: Option<RunningSearch>,
}

/// Where a key stands in identification, as the count reads it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Identifying {
    /// It has been admitted to identification and its run has not reported
    /// yet. This is what joins a key to the batch: both admissions mark it
    /// here first, and a library release being re-identified in its own sheet
    /// is marked by neither, so the import pane's count is the import queue's
    /// work and nothing else.
    queued: bool,
    /// Something is still to come for the key: it is waiting, running, or
    /// holding an answer nobody has disposed of yet. A failed write is not one
    /// — that run is over and the row says how it went.
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
    /// Nothing is happening for the key. Such an entry is removed rather than
    /// kept as a value meaning "nothing is running" — absence already means
    /// that, and two spellings of it would need reconciling everywhere the
    /// map is read.
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
            import: self.import.clone(),
            search: self.search.as_ref().map(|running| running.search.clone()),
        }
    }
}

/// Whether the answer the key holds is `run`'s. A newer run's answer is not an
/// older disposal's to take.
fn answered_on(runtime: &CandidateRuntimeState, run: IdentifyRunId) -> bool {
    runtime
        .answered
        .as_ref()
        .is_some_and(|answered| answered.run == run)
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
    /// A key without an entry has nothing running. Also holds
    /// `reidentify:`-prefixed keys, which have no scanned folder.
    runtime: HashMap<String, CandidateRuntimeState>,
    /// The shape last reported for each scanned key, whether or not the key
    /// has runtime, so a reshape can be told from a repeat.
    shapes: HashMap<String, CandidateShape>,
    /// The latest signals extraction reported for each key, and the run it
    /// reported them for. Read by a form that opens partway through a run —
    /// every later value reaches it on the UI bus — and by the settle of that
    /// run's answer, which takes only its own run's snapshot.
    signals: HashMap<String, (IdentifyRunId, Signals)>,
    /// The number the next search run takes. One counter across every key, so
    /// a run a key has moved off — superseded, or cleared and started again —
    /// can never be mistaken for the run it is on now.
    next_search_run: u64,
    /// The identifications in flight, counted for the surfaces that draw how
    /// far along they are.
    batch: IdentificationBatch,
}

impl Inner {
    fn mint_search_run(&mut self) -> u64 {
        let run = self.next_search_run;
        self.next_search_run += 1;
        run
    }

    /// Carry what a key's change did to its identification into the batch, and
    /// report whether that moved the counts.
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
    /// Where the identification count is announced: the import bus this
    /// runtime records into. Handed over once, by the bus that owns it, so a
    /// runtime constructed on its own — a unit test's — has nobody to tell
    /// and says nothing.
    events: Arc<OnceLock<broadcast::Sender<ImportEvent>>>,
}

impl Default for CandidateRuntime {
    fn default() -> Self {
        let (changes, _) = broadcast::channel(1024);
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            changes,
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

    /// The settled snapshot `run` was judged against, or `None` once the key
    /// has moved on to another run — whose snapshot answers a different
    /// question and is not this run's to store.
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

    /// Take the bus this runtime announces its identification count on. The
    /// bus calls this as it is built, and one runtime records into one bus.
    pub(super) fn announce_on(&self, events: broadcast::Sender<ImportEvent>) {
        assert!(
            self.events.set(events).is_ok(),
            "a candidate runtime records into one import bus"
        );
    }

    fn publish(&self, change: CandidateRuntimeChange) {
        // No receivers is the designed state before any subscriber exists;
        // a change nobody is listening for is not an error.
        let _ = self.changes.send(change);
    }

    /// Say how far the identifications in flight have got.
    ///
    /// Sent straight onto the bus rather than back through it: the bus
    /// records every event here before broadcasting it, so a change recorded
    /// here announces from inside that recording, and going round again would
    /// record the announcement as well.
    fn announce(&self, (identified, total): (u32, u32)) {
        let Some(events) = self.events.get() else {
            return;
        };
        info!("identification progress at {identified}/{total}");
        // No receivers is the designed state before any subscriber exists.
        let _ = events.send(ImportEvent::IdentificationProgress { identified, total });
    }

    /// Apply `mutate` to the key's entry, creating one if it has none, and
    /// publish the snapshot it left behind. The map's own bookkeeping comes
    /// with it, so a mutation that needs a fresh search run mints one under
    /// the same lock that stores it. An entry `mutate` leaves idle is
    /// removed. A mutation that leaves the published snapshot where it was
    /// publishes nothing — the run a search moved onto is not a change any
    /// consumer draws. Whatever `mutate` computed comes back to the caller.
    fn set<R>(
        &self,
        key: &str,
        mutate: impl FnOnce(&mut Inner, &mut CandidateRuntimeState) -> R,
    ) -> R {
        let (result, change, progress) = {
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
            let progress = inner
                .count_identification(key, was_identifying, is_identifying)
                .then(|| inner.batch.progress());
            (result, change, progress)
        };
        if let Some(change) = change {
            self.publish(change);
        }
        if let Some(progress) = progress {
            self.announce(progress);
        }
        result
    }

    /// Put `key` on a new search run carrying `search`, superseding whatever
    /// it was on, and publish it. The number returned is what a landing proves
    /// it is still current by.
    pub(super) fn start_search(&self, key: &str, search: CandidateSearch) -> u64 {
        self.set(key, |inner, runtime| {
            let run = inner.mint_search_run();
            runtime.search = Some(RunningSearch { run, search });
            run
        })
    }

    /// Put every failed source of `key`'s search back to looking, on a new
    /// run, and publish it. The query and the sources to re-ask come back.
    /// `None` when the key has no search or nothing to re-ask, and then
    /// nothing changed and nothing was published.
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

    /// Take `key` off whatever search run it is on, so nothing that run has
    /// out can land, and publish the key without a search.
    pub(super) fn clear_search(&self, key: &str) {
        self.set(key, |_, runtime| runtime.search = None);
    }

    /// Stop asking `source` on every search running right now, and publish each
    /// key whose search changed. What the person switched off is switched off
    /// everywhere they can see it, not only in the pane they were looking at.
    ///
    /// The run each search is on is untouched: a lookup already out for the
    /// dropped source still lands on its current run and is dropped there,
    /// because a part that is not looking takes no answer. Superseding the run
    /// instead would take the other source's in-flight lookup down with it.
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

    /// Whether `run` is still the run `key`'s search is on — asked before a
    /// landing pays for work only a current run will use.
    pub(super) fn search_run_is_current(&self, key: &str, run: u64) -> bool {
        self.inner
            .lock()
            .unwrap()
            .runtime
            .get(key)
            .and_then(|state| state.search.as_ref())
            .is_some_and(|running| running.run == run)
    }

    /// Land one source's answer on `key`'s search and publish the search it
    /// leaves behind, if `run` is still its run. `false` means the run was
    /// cleared or superseded and the answer goes nowhere.
    ///
    /// The landing folds into the value this map holds, under its lock, and
    /// superseding or clearing a run happens under the same lock — so a run
    /// this one has replaced cannot write over it, and the other source's
    /// landing, which folded into the same value, is still there.
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

    /// Drop everything held for a key: what is in flight and the signals that
    /// described its files. Both are answers about a candidate that is gone.
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

    /// Mark every one of `keys` as waiting on `admission`, in one change.
    ///
    /// An admission that opens a batch is one act — a surface draws the whole
    /// of it, and the identification count opens at its total — so it publishes
    /// once rather than a key at a time. Keys it does not name are untouched:
    /// the queue says what leaves it.
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

    /// `run`'s answer is over: its row landed, was refused as stale, failed to
    /// be written, or no write was ever asked for it. Left in place, it would
    /// read as a commit still pending, for good.
    ///
    /// By run id, so a disposal that lands after a newer run has already
    /// answered takes only its own answer with it. Said by whoever was
    /// responsible for the answer: the verdict write for the runs it ran, and
    /// the settle that never reached one for the rest.
    pub(super) fn end_identification_answer(&self, candidate_key: &str, run: IdentifyRunId) {
        self.set(candidate_key, |_, runtime| {
            if answered_on(runtime, run) {
                runtime.answered = None;
            }
        });
    }

    /// `run` reached a terminal result that could not be committed. The row
    /// says why, and nothing is left waiting on a write that is not coming.
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

    /// Identification of this key is over, whatever it had reached: the run is
    /// cancelled and nothing is going to write what it found.
    ///
    /// The counterpart of a cancellation, and the only ending a library release
    /// re-identified in its own sheet ever gets — its run stores no verdict, so
    /// no write would end it.
    pub(super) fn end_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            runtime.running = None;
            runtime.answered = None;
        });
    }

    /// Record what `run` published for `candidate_key`.
    ///
    /// Three states, three facts. A non-terminal state is the run in flight. A
    /// terminal state is its answer, which ends the run and stands until
    /// whoever asked for it disposes of it. `Idle` is a cancellation, and ends
    /// only the run that broadcast it — a superseded run announces its ending
    /// after the run that replaced it has already reported, and a run's own
    /// terminal state has already left `running` for the answer.
    ///
    /// A state from a run this key was not already on is a fresh attempt, so
    /// it clears whatever the previous attempt's write failed with.
    ///
    /// A run reporting is also the end of the wait that preceded it: the key
    /// is not queued any more, because the thing it was queued for is
    /// happening. Said here rather than at the call that started the run, so
    /// there is no instant in which the key is neither waiting nor running.
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

    /// Record that an import owns this candidate.
    ///
    /// Written when the import command is queued, not when the worker's first
    /// `ImportProgress` comes back through [`Self::record_event`]. That event
    /// records the same fact, but far too late to gate anything on: it is
    /// emitted after the worker has dequeued the command and re-walked the
    /// folder — behind however many imports are already queued ahead of it.
    /// The automatic admission reads this field to decide whether a candidate still
    /// wants a verdict, and "the user has committed to importing it" has to be
    /// true here from the moment they commit.
    pub(super) fn claim_for_import(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            runtime.import = Some(ImportInFlight {
                progress_percent: None,
                step: Some(ImportStep::Preparing(PrepareStep::Queued)),
            });
        });
    }

    /// Undo [`Self::claim_for_import`] for a command that never made it onto
    /// the worker's queue.
    pub(super) fn release_import_claim(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| runtime.import = None);
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
                // Every way an import ends leaves the map, because every one
                // of them has already written its row: the worker commits the
                // release before `Complete` and `RemoteUploadQueued`, and the
                // failure row before `Failed`. What the row says is what the
                // candidate is once nothing is running.
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
                    | ImportProgress::Failed { .. } => None,
                };
                self.set(candidate_key, |_, runtime| runtime.import = in_flight);
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
            // A rebound sheet is a different disc, so a query typed against
            // the old one asked about something else: the search goes, and
            // with it the run its lookups would otherwise land on.
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
            // Retained but not published: the form that reads these is fed by
            // the UI bus, and republishing the key here would wake every
            // runtime consumer for something none of them draws.
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
            // The identification count is this map's own announcement about
            // every key at once, with no candidate to record it against, and
            // the remaining scan events change rows, not runtime.
            ImportEvent::Scan(
                ScanEvent::WatchedFoldersChanged { .. }
                | ScanEvent::CandidateSkipChanged { .. }
                | ScanEvent::CandidateMetadataChanged { .. }
                | ScanEvent::FolderScanStatusChanged { .. }
                | ScanEvent::Finished,
            )
            | ImportEvent::IdentificationProgress { .. } => {}
        }
    }
}
