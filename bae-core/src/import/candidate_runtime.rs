//! Per-candidate state that lives only as long as the process: a run in
//! flight, its toolbar, extracted signals, an import's claim and progress.
//!
//! Nothing here has a row. It is accumulated from the import event bus and
//! published per key — one [`CandidateRuntimeChange`] for the one candidate
//! an event concerned — so a consumer holding the list never receives the
//! list again because one row's run advanced.

use super::candidates::{CandidateImportStatusSnapshot, CandidateRuntimeSnapshot, ImportedRelease};
use super::folder_scanner::{FolderCandidate, ReleaseFileScope};
use super::handle::{ImportEvent, ScanEvent};
use super::types::{ImportProgress, ImportStep, PrepareStep};
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
    /// The key has no runtime any more: its folder left the scan, or its
    /// files changed shape so what was recorded described a folder that no
    /// longer exists.
    Removed { key: String },
}

/// The file shape a candidate's runtime was recorded against. A scan that
/// reports the same key with a different shape invalidates the runtime: the
/// verdict, signals, and progress described the old files.
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
    /// A key without an entry has had no events; reads treat absence as the
    /// idle runtime. Also holds `reidentify:`-prefixed keys, which have no
    /// scanned folder.
    runtime: HashMap<String, CandidateRuntimeSnapshot>,
    /// The shape last reported for each scanned key, whether or not the key
    /// has runtime, so a reshape can be told from a repeat.
    shapes: HashMap<String, CandidateShape>,
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
    /// Every key's runtime right now, for a subscriber that joins after runs
    /// have started.
    pub fn all(&self) -> HashMap<String, CandidateRuntimeSnapshot> {
        self.inner.lock().unwrap().runtime.clone()
    }

    /// A key's runtime, or `None` for a key nothing has been recorded against.
    pub fn get(&self, key: &str) -> Option<CandidateRuntimeSnapshot> {
        self.inner.lock().unwrap().runtime.get(key).cloned()
    }

    /// A key's runtime, idle when nothing has been recorded against it — the
    /// designed initial state, not an error.
    pub fn runtime_for(&self, key: &str) -> CandidateRuntimeSnapshot {
        self.get(key).unwrap_or_else(CandidateRuntimeSnapshot::idle)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CandidateRuntimeChange> {
        self.changes.subscribe()
    }

    fn publish(&self, change: CandidateRuntimeChange) {
        // No receivers is the designed state before any subscriber exists;
        // a change nobody is listening for is not an error.
        let _ = self.changes.send(change);
    }

    fn update(&self, key: &str, mutate: impl FnOnce(&mut CandidateRuntimeSnapshot)) {
        let runtime = {
            let mut inner = self.inner.lock().unwrap();
            let runtime = inner
                .runtime
                .entry(key.to_string())
                .or_insert_with(CandidateRuntimeSnapshot::idle);
            mutate(runtime);
            runtime.clone()
        };
        self.publish(CandidateRuntimeChange::Updated {
            key: key.to_string(),
            runtime,
        });
    }

    fn remove(&self, key: &str) {
        let removed = self.inner.lock().unwrap().runtime.remove(key).is_some();
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
        self.update(candidate_key, |runtime| {
            runtime.import_status = Some(CandidateImportStatusSnapshot::Importing {
                progress_percent: 0,
                step: Some(ImportStep::Preparing(PrepareStep::Queued)),
            });
        });
    }

    /// Undo [`Self::claim_for_import`] for a command that never made it onto
    /// the worker's queue.
    pub(super) fn release_import_claim(&self, candidate_key: &str) {
        self.update(candidate_key, |runtime| runtime.import_status = None);
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
                let status = match progress {
                    ImportProgress::Preparing { step, .. } => {
                        CandidateImportStatusSnapshot::Importing {
                            progress_percent: 0,
                            step: Some(ImportStep::Preparing(*step)),
                        }
                    }
                    ImportProgress::Progress { percent, phase, .. } => {
                        CandidateImportStatusSnapshot::Importing {
                            progress_percent: *percent as u32,
                            step: Some(ImportStep::Running(*phase)),
                        }
                    }
                    ImportProgress::Complete { id, album_id, .. } => {
                        CandidateImportStatusSnapshot::Complete {
                            release: ImportedRelease {
                                release_id: id.clone(),
                                album_id: album_id.clone(),
                            },
                        }
                    }
                    ImportProgress::RemoteUploadQueued {
                        id,
                        album_id,
                        outbox_revision,
                        ..
                    } => CandidateImportStatusSnapshot::CloudUploadQueued {
                        release: ImportedRelease {
                            release_id: id.clone(),
                            album_id: album_id.clone(),
                        },
                        outbox_revision: *outbox_revision,
                    },
                    ImportProgress::Failed { error, .. } => CandidateImportStatusSnapshot::Error {
                        error: error.clone(),
                    },
                };
                self.update(candidate_key, |runtime| {
                    runtime.import_status = Some(status)
                });
            }
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                run: _,
                state,
                toolbar,
                priority: _,
            } => {
                // A terminal state followed by `Idle` is a driver being torn
                // down after settling — the sweep cancels its own drivers once
                // they settle, and cancellation broadcasts `Idle` on its way
                // out. The candidate's answer doesn't stop being its answer
                // because the machinery that produced it exited, so the
                // terminal state stays. A genuine mid-run cancel goes
                // `Triangulating` → `Idle` and resets as before.
                //
                // What the retained terminal state covers is bounded: the
                // interval before the verdict's durable write lands (cleared
                // by `CandidateVerdictStored` below), and terminal states
                // that never store — a settle shaped by a lookup that never
                // answered — which are session-only by design.
                let torn_down = matches!(state, crate::identify::IdentifyState::Idle)
                    && self
                        .get(candidate_key)
                        .is_some_and(|runtime| runtime.identify_state.is_terminal());
                if !torn_down {
                    self.update(candidate_key, |runtime| {
                        runtime.identify_state = state.clone();
                        runtime.toolbar = toolbar.clone();
                    });
                }
            }
            ImportEvent::SignalsUpdated {
                candidate_key,
                signals,
                priority: _,
            } => {
                self.update(candidate_key, |runtime| {
                    runtime.signals = Some(signals.clone())
                });
            }
            // The candidate's answer now lives in its stored verdict row, and
            // the candidate list serves it from there as the resumed state.
            // The recorded terminal state has done its job — carrying the
            // answer across the interval between settling and the durable
            // write — so it clears, leaving the runtime to hold only what has
            // no row: runs in flight, and extraction's signals, which are
            // facts about the files rather than about this run. Only a
            // terminal state clears: a newer run's in-flight state must not
            // be blanked by the previous run's write landing.
            ImportEvent::Scan(ScanEvent::CandidateVerdictStored { candidate_key }) => {
                let terminal = self
                    .get(candidate_key)
                    .is_some_and(|runtime| runtime.identify_state.is_terminal());
                if terminal {
                    self.update(candidate_key, |runtime| {
                        runtime.identify_state = crate::identify::IdentifyState::Idle;
                        runtime.toolbar = Vec::new();
                    });
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
            // Queue progress is a queue-wide number with no candidate to record
            // it against, loudness ticks go straight to their leaf view, and
            // the remaining scan events change rows, not runtime.
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
