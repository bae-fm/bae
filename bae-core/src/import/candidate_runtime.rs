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
//! One exception, and it is deliberate: a run that settles without an answer
//! worth storing — a lookup that never responded — has nothing to write, so
//! its terminal state is held here for the rest of the session. It is the one
//! thing in the map that is not strictly in flight, and it is here because
//! [`IdentifyPhase::NoAnswer`](crate::import::IdentifyPhase) has no row to be
//! read from.
//!
//! Changes are published per key — one [`CandidateRuntimeChange`] for the one
//! candidate an event concerned — so a consumer holding the list never
//! receives the list again because one row's run advanced.
//!
//! Extraction's [`Signals`](crate::signals::Signals) are held here too, and
//! deliberately *not* in the published snapshot: they change at extraction's
//! own cadence, one form reads them, and that form is fed by its own UI-bus
//! event. What they share with the rest of this map is a lifetime — they
//! describe the same key's current files and are dropped by the same events —
//! which is why they live here rather than in a second map somebody would
//! have to remember to clear.

use super::candidates::{CandidateRuntimeSnapshot, ImportInFlight};
use super::folder_scanner::{FolderCandidate, ReleaseFileScope};
use super::handle::{ImportEvent, ScanEvent};
use super::types::{ImportProgress, ImportStep, PrepareStep};
use crate::signals::Signals;
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

#[derive(Default)]
struct Inner {
    /// A key without an entry has nothing running. Also holds
    /// `reidentify:`-prefixed keys, which have no scanned folder.
    runtime: HashMap<String, CandidateRuntimeSnapshot>,
    /// The shape last reported for each scanned key, whether or not the key
    /// has runtime, so a reshape can be told from a repeat.
    shapes: HashMap<String, CandidateShape>,
    /// The latest signals extraction reported for each key. Read by a form
    /// that opens partway through a run; every later value reaches it on the
    /// UI bus.
    signals: HashMap<String, Signals>,
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
        self.inner.lock().unwrap().runtime.clone()
    }

    /// What is in flight for a key, or `None` when nothing is.
    pub fn get(&self, key: &str) -> Option<CandidateRuntimeSnapshot> {
        self.inner.lock().unwrap().runtime.get(key).cloned()
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
    /// publish what it left behind. An entry `mutate` empties is removed
    /// rather than kept as a value meaning "nothing is running" — absence
    /// already means that, and two spellings of it would need reconciling
    /// everywhere the map is read. A mutation that changes nothing publishes
    /// nothing.
    fn set(&self, key: &str, mutate: impl FnOnce(&mut CandidateRuntimeSnapshot)) {
        let change = {
            let mut inner = self.inner.lock().unwrap();
            let previous = inner.runtime.get(key).cloned();
            let mut next = previous.clone().unwrap_or(CandidateRuntimeSnapshot {
                identify: None,
                import: None,
            });
            mutate(&mut next);
            if Some(&next) == previous.as_ref() {
                None
            } else if next.identify.is_none() && next.import.is_none() {
                inner.runtime.remove(key);
                Some(CandidateRuntimeChange::Removed {
                    key: key.to_string(),
                })
            } else {
                inner.runtime.insert(key.to_string(), next.clone());
                Some(CandidateRuntimeChange::Updated {
                    key: key.to_string(),
                    runtime: next,
                })
            }
        };
        if let Some(change) = change {
            self.publish(change);
        }
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
        self.set(candidate_key, |runtime| {
            runtime.import = Some(ImportInFlight {
                progress_percent: 0,
                step: Some(ImportStep::Preparing(PrepareStep::Queued)),
            });
        });
    }

    /// Undo [`Self::claim_for_import`] for a command that never made it onto
    /// the worker's queue.
    pub(super) fn release_import_claim(&self, candidate_key: &str) {
        self.set(candidate_key, |runtime| runtime.import = None);
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
                        progress_percent: 0,
                        step: Some(ImportStep::Preparing(*step)),
                    }),
                    ImportProgress::Progress { percent, phase, .. } => Some(ImportInFlight {
                        progress_percent: *percent as u32,
                        step: Some(ImportStep::Running(*phase)),
                    }),
                    ImportProgress::Complete { .. }
                    | ImportProgress::RemoteUploadQueued { .. }
                    | ImportProgress::Failed { .. } => None,
                };
                self.set(candidate_key, |runtime| runtime.import = in_flight);
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
                // What the retained terminal state covers is bounded: the
                // interval before the verdict's durable write lands (cleared
                // by `CandidateVerdictStored` below), and terminal states
                // that never store — a settle shaped by a lookup that never
                // answered — which are session-only by design.
                let torn_down = matches!(state, crate::identify::IdentifyState::Idle)
                    && self.get(candidate_key).is_some_and(|runtime| {
                        runtime
                            .identify
                            .is_some_and(|identify| identify.is_terminal())
                    });
                if !torn_down {
                    let identify = match state {
                        crate::identify::IdentifyState::Idle => None,
                        live => Some(live.clone()),
                    };
                    self.set(candidate_key, |runtime| runtime.identify = identify);
                }
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
                    self.set(candidate_key, |runtime| runtime.identify = None);
                }
            }
            ImportEvent::Scan(
                ScanEvent::FolderCandidate { candidate, .. }
                | ScanEvent::CandidateDiscovered { candidate, .. }
                | ScanEvent::CandidateBindingChanged { candidate },
            ) => self.observe_shape(candidate),
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
                priority: _,
            } => {
                self.inner
                    .lock()
                    .unwrap()
                    .signals
                    .insert(candidate_key.clone(), signals.clone());
            }
            // Queue progress is a queue-wide number with no candidate to
            // record it against, loudness ticks go straight to their leaf
            // view, and the remaining scan events change rows, not runtime.
            ImportEvent::Scan(
                ScanEvent::WatchedFoldersChanged { .. }
                | ScanEvent::FolderReleaseBoundary(_)
                | ScanEvent::CandidateSkipChanged { .. }
                | ScanEvent::CandidateIdentityPicked { .. }
                | ScanEvent::FolderScanStatusChanged { .. }
                | ScanEvent::Finished,
            )
            | ImportEvent::ImportLoudnessProgress { .. }
            | ImportEvent::QueueIdentifyProgress { .. } => {}
        }
    }
}
