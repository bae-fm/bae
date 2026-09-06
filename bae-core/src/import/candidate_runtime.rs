//! What is happening right now for each candidate: the live identify driver's
//! state, and a running import's progress.
//!
//! One entry per key that has either, and no entry at all otherwise — a
//! finished import leaves, and so does a settled run once its verdict is
//! stored. Everything an entry used to outlive itself carrying has a table
//! now: the verdict the run settled on, the signals a settled run stored, the
//! release an import wrote, the error one failed with. Whoever wants those
//! reads the rows.
//!
//! A terminal identify state stays here across the interval between the reducer
//! producing it and the verdict transaction committing it. The stored event
//! then removes it. A failed commit becomes an explicit runtime failure rather
//! than a terminal result that still looks in flight.
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
use super::candidates::{CandidateIdentifyRuntime, CandidateRuntimeSnapshot, ImportInFlight};
use super::folder_scanner::{FolderCandidate, ReleaseFileScope};
use super::handle::{ImportEvent, ScanEvent};
use super::search::{MetadataResult, SearchQuery};
use super::types::{ImportProgress, ImportStep, MetadataSource, PrepareStep};
use crate::db::LibraryStatus;
use crate::signals::{LookupFailure, Signals};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[cfg(test)]
mod tests;

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

/// One key's runtime as this map holds it. [`CandidateRuntimeSnapshot`] is
/// derived from it rather than kept beside it: the run a search is on is
/// bookkeeping for the landings, and no surface draws it.
#[derive(Clone, Default, PartialEq)]
struct CandidateRuntimeState {
    identify: Option<CandidateIdentifyRuntime>,
    import: Option<ImportInFlight>,
    search: Option<RunningSearch>,
}

impl CandidateRuntimeState {
    /// Nothing is happening for the key. Such an entry is removed rather than
    /// kept as a value meaning "nothing is running" — absence already means
    /// that, and two spellings of it would need reconciling everywhere the
    /// map is read.
    fn is_idle(&self) -> bool {
        self.identify.is_none() && self.import.is_none() && self.search.is_none()
    }

    fn snapshot(&self) -> CandidateRuntimeSnapshot {
        CandidateRuntimeSnapshot {
            identify: self.identify.clone(),
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
    changes: broadcast::Sender<CandidateRuntimeChange>,
}

impl Default for CandidateRuntime {
    fn default() -> Self {
        let (changes, _) = broadcast::channel(1024);
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            changes,
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

    fn publish(&self, change: CandidateRuntimeChange) {
        // No receivers is the designed state before any subscriber exists;
        // a change nobody is listening for is not an error.
        let _ = self.changes.send(change);
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
        let (result, change) = {
            let mut inner = self.inner.lock().unwrap();
            let entry = inner.runtime.get(key);
            let previous = entry.map(CandidateRuntimeState::snapshot);
            let mut next = entry.cloned().unwrap_or_default();
            let result = mutate(&mut inner, &mut next);
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
            (result, change)
        };
        if let Some(change) = change {
            self.publish(change);
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
    ) -> Option<(SearchQuery, Vec<MetadataSource>, u64)> {
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
        source: MetadataSource,
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
        let removed = {
            let mut inner = self.inner.lock().unwrap();
            inner.signals.remove(key);
            inner.runtime.remove(key).is_some()
        };
        if removed {
            self.publish(CandidateRuntimeChange::Removed {
                key: key.to_string(),
            });
        }
    }

    /// Replace the automatic sweep's queued keys in one atomic change.
    /// Explicit Lookup queues and driver-reported states belong to their own
    /// producers and are preserved.
    pub(super) fn replace_automatic_identification_queue(
        &self,
        queued_keys: impl IntoIterator<Item = String>,
    ) {
        let queued_keys: std::collections::HashSet<String> = queued_keys.into_iter().collect();
        let reset = {
            let mut inner = self.inner.lock().unwrap();
            let previous = snapshots(&inner.runtime);
            for runtime in inner.runtime.values_mut() {
                if runtime
                    .identify
                    .as_ref()
                    .is_some_and(CandidateIdentifyRuntime::is_automatic_queue)
                {
                    runtime.identify = None;
                }
            }
            inner.runtime.retain(|_, runtime| !runtime.is_idle());
            for key in queued_keys {
                let runtime = inner.runtime.entry(key).or_default();
                if runtime.identify.is_none() {
                    runtime.identify = Some(CandidateIdentifyRuntime::automatic_queue());
                }
            }
            let next = snapshots(&inner.runtime);
            (next != previous).then_some(next)
        };
        if let Some(runtimes) = reset {
            self.publish(CandidateRuntimeChange::Reset { runtimes });
        }
    }

    /// This key has been admitted to an explicit Lookup run but its driver has
    /// not reported a state yet.
    pub(super) fn queue_explicit_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            runtime.identify = Some(CandidateIdentifyRuntime::explicit_queue());
        });
    }

    /// A sweep-owned job is waiting for another attempt after a prior attempt
    /// produced no storable answer.
    pub(super) fn requeue_automatic_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            runtime.identify = Some(CandidateIdentifyRuntime::automatic_queue());
        });
    }

    /// Remove this key only when it is waiting in the automatic sweep.
    pub(super) fn clear_automatic_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| {
            if runtime
                .identify
                .as_ref()
                .is_some_and(CandidateIdentifyRuntime::is_automatic_queue)
            {
                runtime.identify = None;
            }
        });
    }

    /// The answer a run reached will never be stored — the candidate changed
    /// shape, was skipped, imported, or removed while it ran — so the key has
    /// nothing in flight. Left in place, the terminal state would read as a
    /// commit still pending, for good.
    pub(super) fn discard_identification(&self, candidate_key: &str) {
        self.set(candidate_key, |_, runtime| runtime.identify = None);
    }

    /// Identification reached a terminal result but could not commit it.
    /// Preserve the result for the pane and attach the failure that stopped the
    /// row, replacing either the representative's terminal state or a grouped
    /// candidate's queue marker.
    pub(super) fn fail_identification(&self, candidate_key: &str, error: String) {
        self.set(candidate_key, |_, runtime| {
            runtime.identify = Some(match runtime.identify.take() {
                Some(identify) => identify.into_finalization_failed(error),
                None => CandidateIdentifyRuntime::finalization_failed(error),
            });
        });
    }

    /// Report the shared identify job's state for one of its candidate keys.
    /// Duplicate-content candidates have one driver but every row represents
    /// the same live job, so the sweep applies each driver state to every key.
    pub(super) fn report_identification(
        &self,
        candidate_key: &str,
        state: &crate::identify::IdentifyState,
    ) {
        let current = self.get(candidate_key).and_then(|runtime| runtime.identify);
        let preserves_current = current.as_ref().is_some_and(|identify| {
            (matches!(state, crate::identify::IdentifyState::Idle) && identify.is_terminal())
                || (state.is_terminal() && identify.is_finalization_failed())
        });
        if !preserves_current {
            let identify = CandidateIdentifyRuntime::from_state(state.clone());
            self.set(candidate_key, |_, runtime| runtime.identify = identify);
        }
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

    /// Feed one event to the recorder directly, for a test that has no bus.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn record_event_for_test(&self, event: &ImportEvent) {
        self.record_event(event);
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
                run: _,
                state,
                priority: _,
            } => {
                // A terminal state followed by `Idle` is a driver being torn
                // down after settling — the sweep cancels its own drivers once
                // they settle, and cancellation broadcasts `Idle` on its way
                // out. The candidate's answer doesn't stop being its answer
                // because the machinery that produced it exited, so the
                // terminal state stays. A genuine mid-run cancel goes
                // `Triangulating` → `Idle` and clears as before.
                //
                // The retained terminal state covers the interval before the
                // verdict's durable write lands. `CandidateVerdictStored`
                // below clears it after the transaction commits.
                self.report_identification(candidate_key, state);
            }
            // The candidate's answer now lives in its stored verdict row, and
            // the candidate list serves it from there as the resumed state.
            // The recorded terminal state has done its job — carrying the
            // answer across the interval between settling and the durable
            // write — so it clears, leaving the runtime holding only what is
            // still happening. Only a terminal state clears: a newer run's
            // in-flight state must not be blanked by the previous run's write
            // landing.
            ImportEvent::Scan(ScanEvent::CandidateVerdictStored { candidate_key }) => {
                let terminal = self.get(candidate_key).is_some_and(|runtime| {
                    runtime
                        .identify
                        .is_some_and(|identify| identify.is_terminal())
                });
                if terminal {
                    self.set(candidate_key, |_, runtime| runtime.identify = None);
                }
            }
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
            // Queue progress is a queue-wide number with no candidate to
            // record it against, and the remaining scan events change rows,
            // not runtime.
            ImportEvent::Scan(
                ScanEvent::WatchedFoldersChanged { .. }
                | ScanEvent::CandidateSkipChanged { .. }
                | ScanEvent::CandidateMetadataChanged { .. }
                | ScanEvent::FolderScanStatusChanged { .. }
                | ScanEvent::Finished,
            )
            | ImportEvent::QueueIdentifyProgress { .. } => {}
        }
    }
}
