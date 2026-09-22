//! What is happening right now for each candidate, one fact per field: the
//! queue it is waiting in, the run in flight, the answer being written, the
//! write that failed, the import running, the search a person typed.
//!
//! **Each field has one writer and is never inferred from another.** The
//! queue sweep owns `queued` — both its passes and the Lookup entry point it
//! exposes; the identify driver's broadcasts own `running`; the verdict write
//! owns `saving` and `save_failed`; the import worker owns `import`; and a
//! candidate's search owns `search`. Nothing here reads one field to decide
//! another, and nothing depends on the order two producers happened to reach
//! it in.
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
//! because this is what holds it: the queue sweep knows only its own passes,
//! and a person starting a Lookup by hand is not one of them. A key joins the
//! batch when it is admitted to identification, which is what `queued` says
//! and what both of those do first; it leaves when it is neither waiting,
//! running, nor having its answer written.
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
//! Extraction's [`Signals`](crate::signals::Signals) are held here too, and
//! deliberately *not* in the published snapshot: they change at extraction's
//! own cadence, one form reads them, and that form is fed by its own UI-bus
//! event. What they share with the rest of this map is a lifetime — they
//! describe the same key's current files and are dropped by the same events —
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
    /// Written by the sweep when it plans a run — by its passes and by the
    /// Lookup a person starts. Cleared by the first broadcast of the run that
    /// was waited for, or by whoever queued it when no run came of it.
    queued: Option<Admission>,
    /// Written from the driver's broadcasts. Never terminal and never `Idle`:
    /// both of those end the run rather than being a state it sits at.
    running: Option<RunState>,
    /// Written when a run broadcasts its terminal state, cleared when that
    /// run's write lands, is refused, or is abandoned. Always terminal.
    saving: Option<RunState>,
    /// Written when a write fails, cleared by the next run of this key.
    save_failed: Option<FailedSave>,
    import: Option<ImportInFlight>,
    search: Option<RunningSearch>,
}

/// Where a key stands in identification, as the count reads it.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Identifying {
    /// It has been admitted to identification and its run has not reported
    /// yet. This is what joins a key to the batch: both ways an import
    /// candidate's identification begins — a sweep pass and a person's Lookup
    /// — mark it here first, and a library release being re-identified in its
    /// own sheet is marked by neither, so the import pane's count is the
    /// import queue's work and nothing else.
    queued: bool,
    /// Something is still to come for the key: it is waiting, running, or
    /// having its answer written. A failed write is not one — that run is over
    /// and the row says how it went.
    in_flight: bool,
}

impl Identifying {
    fn of(state: &CandidateRuntimeState) -> Self {
        Self {
            queued: state.queued.is_some(),
            in_flight: state.queued.is_some()
                || state.running.is_some()
                || state.saving.is_some(),
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
            && self.saving.is_none()
            && self.save_failed.is_none()
            && self.import.is_none()
            && self.search.is_none()
    }

    fn snapshot(&self) -> CandidateRuntimeSnapshot {
        CandidateRuntimeSnapshot {
            queued: self.queued,
            running: self.running.as_ref().map(|run| run.state.clone()),
            saving: self.saving.as_ref().map(|run| run.state.clone()),
            save_failed: self.save_failed.as_ref().map(|failed| failed.error.clone()),
            import: self.import.clone(),
            search: self.search.as_ref().map(|running| running.search.clone()),
        }
    }
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
    /// The latest signals extraction reported for each key. Read by a form
    /// that opens partway through a run; every later value reaches it on the
    /// UI bus.
    signals: HashMap<String, Signals>,
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
        self.inner.lock().unwrap().signals.get(key).cloned()
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

    /// Replace the automatic sweep's queued keys in one atomic change.
    /// Explicit Lookup queues and every other field belong to their own
    /// producers and are preserved.
    pub(super) fn replace_automatic_identification_queue(
        &self,
        queued_keys: impl IntoIterator<Item = String>,
    ) {
        let queued_keys: std::collections::HashSet<String> = queued_keys.into_iter().collect();
        let (reset, progress) = {
            let mut inner = self.inner.lock().unwrap();
            let previous = snapshots(&inner.runtime);
            let was_identifying = inner.identifying();
            for runtime in inner.runtime.values_mut() {
                if runtime.queued == Some(Admission::Automatic) {
                    runtime.queued = None;
                }
            }
            inner.runtime.retain(|_, runtime| !runtime.is_idle());
            for key in queued_keys {
                let runtime = inner.runtime.entry(key).or_default();
                if runtime.queued.is_none() {
                    runtime.queued = Some(Admission::Automatic);
                }
            }
            let next = snapshots(&inner.runtime);
            let is_identifying = inner.identifying();
            // The keys this change took out are counted before the ones it
            // brought in: a queue replaced wholesale ends the batch it
            // emptied, and what it queues instead is a batch of its own
            // rather than a total carrying finished work forward.
            let touched: std::collections::HashSet<&String> = was_identifying
                .keys()
                .chain(is_identifying.keys())
                .collect();
            let mut ended = Vec::new();
            let mut admitted = Vec::new();
            for key in touched {
                let was = was_identifying.get(key).copied().unwrap_or_default();
                let is = is_identifying.get(key).copied().unwrap_or_default();
                if was.in_flight && !is.in_flight {
                    ended.push(key.clone());
                } else if !was.queued && is.queued {
                    admitted.push(key.clone());
                }
            }
            let mut counted = false;
            for key in ended {
                counted |= inner.batch.end(&key);
            }
            for key in admitted {
                counted |= inner.batch.admit(&key);
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

    /// This key has been admitted to an explicit Lookup and its run has not
    /// started yet.
    pub(super) fn queue_explicit_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            runtime.queued = Some(Admission::Requested);
        });
    }

    /// The explicit Lookup that queued this key has started its run, or given
    /// up before starting one. Either way it is not waiting any more.
    pub(super) fn clear_explicit_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            if runtime.queued == Some(Admission::Requested) {
                runtime.queued = None;
            }
        });
    }

    /// A sweep-owned job is waiting for a slot.
    pub(super) fn requeue_automatic_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            runtime.queued = Some(Admission::Automatic);
        });
    }

    /// Remove this key only when it is waiting in the automatic sweep.
    pub(super) fn clear_automatic_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            if runtime.queued == Some(Admission::Automatic) {
                runtime.queued = None;
            }
        });
    }

    /// `run`'s save is over: its row landed, was refused as stale, or was
    /// abandoned because the candidate moved on. Left in place, it would read
    /// as a commit still pending, for good.
    ///
    /// By run id, so a write that lands after a newer run has already answered
    /// takes only its own save with it.
    pub(super) fn finish_identification_save(&self, candidate_key: &str, run: IdentifyRunId) {
        self.set(candidate_key, |_, runtime| {
            if runtime.saving.as_ref().is_some_and(|saving| saving.run == run) {
                runtime.saving = None;
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
            if runtime.saving.as_ref().is_some_and(|saving| saving.run == run) {
                runtime.saving = None;
            }
            runtime.save_failed = Some(FailedSave { run, error });
        });
    }

    /// Whether a terminal answer for this key is waiting on its durable write
    /// — the interval in which the run has ended but no row states its result
    /// yet, and nothing else may take the candidate over.
    pub(super) fn is_saving_identification(&self, candidate_key: &str) -> bool {
        self.inner
            .lock()
            .unwrap()
            .runtime
            .get(candidate_key)
            .is_some_and(|state| state.saving.is_some())
    }

    /// Record what `run` published for `candidate_key`.
    ///
    /// Three states, three facts. A non-terminal state is the run in flight.
    /// A terminal state is its answer, which ends the run and starts the save
    /// the write step owns. `Idle` is a cancellation, and ends only the run
    /// that broadcast it — a superseded run announces its ending after the run
    /// that replaced it has already reported.
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
                runtime.saving = Some(RunState {
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
    /// The queue sweep reads this field to decide whether a candidate still
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
                run: _,
                signals,
                artwork: _,
                priority: _,
            } => {
                self.inner
                    .lock()
                    .unwrap()
                    .signals
                    .insert(candidate_key.clone(), signals.clone());
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
