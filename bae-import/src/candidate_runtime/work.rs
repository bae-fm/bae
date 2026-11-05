//! The work in flight for each candidate: its extraction and its identify
//! run. The scan dropping or reshaping the candidate ends it.
//!
//! Each start for a key and kind gets a fresh generation and cancels the one
//! before it; a finishing task releases its own entry only while its
//! generation is still current, so a cancelled task on its way out cannot
//! evict its successor.

use super::CandidateRuntime;
use crate::CandidateKey;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

/// What kind of work a candidate has in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CandidateWork {
    Extraction,
    Identify,
}

/// The work lock is poisoned: a panic left the work in flight partway
/// through a change.
#[derive(Debug)]
pub(crate) struct WorkLockPoisoned;

impl std::fmt::Display for WorkLockPoisoned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the candidate work lock is poisoned")
    }
}

/// Every key's work in flight and the counter that hands out generations,
/// under one lock so a generation and its entry advance together.
#[derive(Default)]
pub(super) struct WorkInFlight {
    running: HashMap<(CandidateWork, CandidateKey), (u64, CancellationToken)>,
    next_generation: u64,
}

impl CandidateRuntime {
    /// Start `work` for `key` with a fresh generation, cancelling any before
    /// it, and hand its token and generation to the task `construct` builds.
    pub(crate) fn start_work<T>(
        &self,
        work: CandidateWork,
        key: CandidateKey,
        construct: impl FnOnce(CancellationToken, u64) -> T,
    ) -> T {
        let token = CancellationToken::new();
        let mut state = self.work.lock().expect("candidate work lock poisoned");
        let generation = state.next_generation;
        state.next_generation += 1;
        let prior = state.running.insert((work, key), (generation, token.clone()));
        drop(state);
        if let Some((_, prior)) = prior {
            prior.cancel();
        }
        construct(token, generation)
    }

    /// Run `emit` only while `generation` is still `key`'s current `work`,
    /// under the lock a replacement or cancel takes, so work that was
    /// replaced or ended says nothing after that.
    pub(crate) fn while_work_current<R>(
        &self,
        work: CandidateWork,
        key: &CandidateKey,
        generation: u64,
        emit: impl FnOnce() -> R,
    ) -> Option<R> {
        let state = self.work.lock().expect("candidate work lock poisoned");
        match state.running.get(&(work, key.clone())) {
            Some((current, _)) if *current == generation => Some(emit()),
            _ => None,
        }
    }

    /// Cancel `key`'s `work`.
    pub(crate) fn cancel_work(&self, work: CandidateWork, key: &CandidateKey) {
        let entry = self
            .work
            .lock()
            .expect("candidate work lock poisoned")
            .running
            .remove(&(work, key.clone()));
        if let Some((_, token)) = entry {
            token.cancel();
        }
    }

    /// Remove `key`'s `work` only while it is still `generation`. An error
    /// when a panic poisoned the work lock: a caller that may run during an
    /// unwind reports it instead of panicking again.
    pub(crate) fn release_work(
        &self,
        work: CandidateWork,
        key: &CandidateKey,
        generation: u64,
    ) -> Result<(), WorkLockPoisoned> {
        let mut state = match self.work.lock() {
            Ok(state) => state,
            Err(_) => return Err(WorkLockPoisoned),
        };
        let entry = (work, key.clone());
        if state.running.get(&entry).is_some_and(|(current, _)| *current == generation) {
            state.running.remove(&entry);
        }
        Ok(())
    }

    /// Poison the work lock, as a panic in work emitted under it would.
    #[cfg(test)]
    pub(super) fn poison_work_lock(&self) {
        let work = self.work.clone();
        let poisoner = std::thread::spawn(move || {
            let _held = work.lock().expect("the work lock is not yet poisoned");
            panic!("poison the work lock");
        });
        assert!(poisoner.join().is_err());
    }

    pub(crate) fn is_working(&self, work: CandidateWork, key: &CandidateKey) -> bool {
        self.work
            .lock()
            .expect("candidate work lock poisoned")
            .running
            .contains_key(&(work, key.clone()))
    }

    /// Every key with `work` in flight.
    pub(crate) fn working_keys(&self, work: CandidateWork) -> Vec<CandidateKey> {
        self.work
            .lock()
            .expect("candidate work lock poisoned")
            .running
            .keys()
            .filter(|(kind, _)| *kind == work)
            .map(|(_, key)| key.clone())
            .collect()
    }

    /// End every kind of work in flight for `key`.
    pub(super) fn end_work(&self, key: &CandidateKey) {
        let ended: Vec<CancellationToken> = {
            let mut state = self.work.lock().expect("candidate work lock poisoned");
            [CandidateWork::Extraction, CandidateWork::Identify]
                .into_iter()
                .filter_map(|work| state.running.remove(&(work, key.clone())))
                .map(|(_, token)| token)
                .collect()
        };
        for token in ended {
            token.cancel();
        }
    }

    /// What ends once `key`'s `work` in flight is cancelled.
    #[cfg(test)]
    pub(crate) fn work_cancelled_for_test(
        &self,
        work: CandidateWork,
        key: &CandidateKey,
    ) -> Option<impl std::future::Future<Output = ()> + Send + 'static> {
        self.work
            .lock()
            .expect("candidate work lock poisoned")
            .running
            .get(&(work, key.clone()))
            .map(|(_, token)| token.clone().cancelled_owned())
    }
}

#[cfg(test)]
#[path = "work_tests.rs"]
mod tests;
