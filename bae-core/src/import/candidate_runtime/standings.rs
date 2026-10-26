//! Where each candidate stands, written to the device-local
//! `import_candidate_standing` table: what the import list and the pane read
//! beside the rows, so every read carries each row's standing composed.
//!
//! The runtime is the table's one writer. Every change to a key's standing —
//! the [`TriageRuntimeFacts`] its runtime reads as — marks the key, and the
//! runtime's writer writes the marked keys as they stand when it reads them,
//! in one transaction. A key it reads with nothing running loses its row.
//!
//! Two writes end what a row says with the result that ends it, in that
//! result's own transaction: a run's verdict, and an import's release. Each
//! holds [`StandingsHeld`] across its write and the runtime change that ends
//! the run or the import, so the writer, which takes the same hold, never
//! writes between them: no read sees the result without the end, or the end
//! without the result.

use super::{CandidateRuntime, CandidateRuntimeState, Inner};
use crate::import::triage::TriageRuntimeFacts;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};

/// The keys whose standing changed since the writer last read them.
#[derive(Default)]
struct Marked {
    keys: BTreeSet<String>,
}

/// What tells the writer a key was marked.
enum Wake {
    Writer(mpsc::UnboundedSender<()>),
    /// A runtime with no library behind it, for a test of the runtime alone.
    #[cfg(any(test, feature = "test-utils"))]
    NoTable,
}

#[derive(Clone)]
pub(super) struct Standings {
    marked: Arc<Mutex<Marked>>,
    /// Taken by the writer for each write, and by a write that ends a run or
    /// an import with its result.
    hold: Arc<tokio::sync::Mutex<()>>,
    /// Dropped with the last clone of the runtime, which ends the writer.
    wake: Arc<Wake>,
    /// Whether every marked key has been written.
    written: Arc<watch::Sender<bool>>,
}

/// The hold on the table a write that ends a run or an import keeps across
/// its write and the runtime change that ends it.
pub(crate) struct StandingsHeld {
    _hold: tokio::sync::OwnedMutexGuard<()>,
}

impl Standings {
    /// Standings written by a writer on `runtime_handle` into `library`'s
    /// table, reading the runtime `inner` holds.
    pub(super) fn writing(
        inner: Arc<Mutex<Inner>>,
        library: crate::library::LibraryManager,
        runtime_handle: &tokio::runtime::Handle,
    ) -> Self {
        let (wake, woken) = mpsc::unbounded_channel();
        let standings = Self {
            marked: Arc::default(),
            hold: Arc::default(),
            wake: Arc::new(Wake::Writer(wake)),
            written: Arc::new(watch::Sender::new(true)),
        };
        runtime_handle.spawn(write(standings.writer_half(), inner, library, woken));
        standings
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) fn without_table() -> Self {
        Self {
            marked: Arc::default(),
            hold: Arc::default(),
            wake: Arc::new(Wake::NoTable),
            written: Arc::new(watch::Sender::new(true)),
        }
    }

    /// What the writer holds: everything but the wake, so the last runtime
    /// dropping its wake ends the writer.
    fn writer_half(&self) -> WriterHalf {
        WriterHalf {
            marked: self.marked.clone(),
            hold: self.hold.clone(),
            written: self.written.clone(),
        }
    }

    /// `key`'s standing changed.
    pub(super) fn mark(&self, key: &str) {
        self.marked.lock().unwrap().keys.insert(key.to_string());
        self.written.send_replace(false);
        match &*self.wake {
            Wake::Writer(wake) => {
                // The writer ends with every runtime, or with the async
                // runtime it runs on as the app shuts down.
                if wake.send(()).is_err() {
                    tracing::warn!("candidate standings: {key} changed after the writer ended");
                }
            }
            #[cfg(any(test, feature = "test-utils"))]
            Wake::NoTable => {}
        }
    }

    pub(super) async fn hold(&self) -> StandingsHeld {
        StandingsHeld {
            _hold: self.hold.clone().lock_owned().await,
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(super) async fn written(&self) {
        self.written
            .subscribe()
            .wait_for(|written| *written)
            .await
            .expect("the runtime holds the sender it waits on");
    }
}

struct WriterHalf {
    marked: Arc<Mutex<Marked>>,
    hold: Arc<tokio::sync::Mutex<()>>,
    written: Arc<watch::Sender<bool>>,
}

/// Write each marked key as the runtime holds it now, until the runtime is
/// gone.
async fn write(
    writer: WriterHalf,
    inner: Arc<Mutex<Inner>>,
    library: crate::library::LibraryManager,
    mut woken: mpsc::UnboundedReceiver<()>,
) {
    while woken.recv().await.is_some() {
        while woken.try_recv().is_ok() {}
        let _hold = writer.hold.lock().await;
        let keys = std::mem::take(&mut writer.marked.lock().unwrap().keys);
        if !keys.is_empty() {
            let standings: Vec<(String, TriageRuntimeFacts)> = {
                let inner = inner.lock().unwrap();
                keys.into_iter()
                    .map(|key| {
                        let standing = standing_of(inner.runtime.get(&key));
                        (key, standing)
                    })
                    .collect()
            };
            if let Err(error) = library.write_candidate_standings(standings).await {
                tracing::error!("candidate standings: could not write the marked keys: {error}");
            }
        }
        if writer.marked.lock().unwrap().keys.is_empty() {
            writer.written.send_replace(true);
        }
    }
}

/// The standing a key's runtime reads as; nothing running with no entry.
fn standing_of(state: Option<&CandidateRuntimeState>) -> TriageRuntimeFacts {
    match state {
        Some(state) => TriageRuntimeFacts::of(&state.snapshot()),
        None => TriageRuntimeFacts::default(),
    }
}

impl CandidateRuntime {
    /// Hold the standings table for a write that ends a run or an import with
    /// its result: the writer writes nothing until the hold is dropped, after
    /// the runtime change that ends it.
    pub(crate) async fn hold_standings(&self) -> StandingsHeld {
        self.standings.hold().await
    }

    /// Where `key` stands once `end` is applied to its runtime, for the write
    /// that ends it to store in its own transaction, under the hold that write
    /// keeps until the runtime change is made.
    pub(super) fn standing_once(
        &self,
        key: &str,
        end: impl FnOnce(&mut CandidateRuntimeState),
    ) -> TriageRuntimeFacts {
        let mut state = self
            .inner
            .lock()
            .unwrap()
            .runtime
            .get(key)
            .cloned()
            .unwrap_or_default();
        end(&mut state);
        standing_of(Some(&state))
    }

    /// Once every standing changed so far is written, for a test that reads
    /// the table right after changing the runtime.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn standings_written(&self) {
        self.standings.written().await
    }
}
