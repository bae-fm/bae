//! The work in flight for each candidate: its extraction and its identify
//! run. Recording the candidate's removal or rebinding ends it.
//!
//! Each start for a key and kind gets a fresh generation and cancels the one
//! before it; a finishing task releases its own entry only while its
//! generation is still current, so a cancelled task on its way out cannot
//! evict its successor.

use super::CandidateRuntime;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

/// What kind of work a candidate has in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CandidateWork {
    Extraction,
    Identify,
}

/// Every key's work in flight and the counter that hands out generations,
/// under one lock so a generation and its entry advance together.
#[derive(Default)]
pub(super) struct WorkInFlight {
    running: HashMap<(CandidateWork, String), (u64, CancellationToken)>,
    next_generation: u64,
}

impl CandidateRuntime {
    /// Start `work` for `key` with a fresh generation, cancelling any before
    /// it, and hand its token and generation to the task `construct` builds.
    pub(crate) fn start_work<T>(
        &self,
        work: CandidateWork,
        key: String,
        construct: impl FnOnce(CancellationToken, u64) -> T,
    ) -> T {
        let token = CancellationToken::new();
        let mut state = self.work.lock().unwrap();
        let generation = state.next_generation;
        state.next_generation += 1;
        let prior = state
            .running
            .insert((work, key), (generation, token.clone()));
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
        key: &str,
        generation: u64,
        emit: impl FnOnce() -> R,
    ) -> Option<R> {
        let state = self.work.lock().unwrap();
        match state.running.get(&(work, key.to_string())) {
            Some((current, _)) if *current == generation => Some(emit()),
            _ => None,
        }
    }

    /// Cancel `key`'s `work`.
    pub(crate) fn cancel_work(&self, work: CandidateWork, key: &str) {
        let entry = self
            .work
            .lock()
            .unwrap()
            .running
            .remove(&(work, key.to_string()));
        if let Some((_, token)) = entry {
            token.cancel();
        }
    }

    /// Remove `key`'s `work` only while it is still `generation`.
    pub(crate) fn release_work(&self, work: CandidateWork, key: &str, generation: u64) {
        let mut state = self.work.lock().unwrap();
        let entry = (work, key.to_string());
        if state
            .running
            .get(&entry)
            .is_some_and(|(current, _)| *current == generation)
        {
            state.running.remove(&entry);
        }
    }

    pub(crate) fn is_working(&self, work: CandidateWork, key: &str) -> bool {
        self.work
            .lock()
            .unwrap()
            .running
            .contains_key(&(work, key.to_string()))
    }

    /// Every key with `work` in flight.
    pub(crate) fn working_keys(&self, work: CandidateWork) -> Vec<String> {
        self.work
            .lock()
            .unwrap()
            .running
            .keys()
            .filter(|(kind, _)| *kind == work)
            .map(|(_, key)| key.clone())
            .collect()
    }

    /// End every kind of work in flight for `key`.
    pub(super) fn end_work(&self, key: &str) {
        let ended: Vec<CancellationToken> = {
            let mut state = self.work.lock().unwrap();
            [CandidateWork::Extraction, CandidateWork::Identify]
                .into_iter()
                .filter_map(|work| state.running.remove(&(work, key.to_string())))
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
        key: &str,
    ) -> Option<impl std::future::Future<Output = ()> + Send + 'static> {
        self.work
            .lock()
            .unwrap()
            .running
            .get(&(work, key.to_string()))
            .map(|(_, token)| token.clone().cancelled_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: CandidateWork = CandidateWork::Extraction;

    #[test]
    fn starting_a_key_again_advances_the_generation_and_cancels_the_last() {
        let runtime = CandidateRuntime::default();
        let (first, first_generation) =
            runtime.start_work(WORK, "cand".to_string(), |token, generation| (token, generation));
        let (second, second_generation) =
            runtime.start_work(WORK, "cand".to_string(), |token, generation| (token, generation));

        assert!(second_generation > first_generation);
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
    }

    #[test]
    fn generations_are_unique_across_keys_and_kinds() {
        let runtime = CandidateRuntime::default();
        let a = runtime.start_work(WORK, "a".to_string(), |_, generation| generation);
        let b = runtime.start_work(WORK, "b".to_string(), |_, generation| generation);
        let c = runtime.start_work(CandidateWork::Identify, "a".to_string(), |_, generation| {
            generation
        });
        assert!(a != b && b != c && a != c);
    }

    #[test]
    fn only_the_current_generation_emits() {
        let runtime = CandidateRuntime::default();
        let stale = runtime.start_work(WORK, "cand".to_string(), |_, generation| generation);
        assert_eq!(runtime.while_work_current(WORK, "cand", stale, || "sent"), Some("sent"));

        let live = runtime.start_work(WORK, "cand".to_string(), |_, generation| generation);
        assert_eq!(runtime.while_work_current(WORK, "cand", stale, || "sent"), None);
        assert_eq!(runtime.while_work_current(WORK, "cand", live, || "sent"), Some("sent"));

        runtime.cancel_work(WORK, "cand");
        assert_eq!(runtime.while_work_current(WORK, "cand", live, || "sent"), None);
    }

    #[test]
    fn a_release_removes_only_the_current_generation() {
        let runtime = CandidateRuntime::default();
        let stale = runtime.start_work(WORK, "cand".to_string(), |_, generation| generation);
        let (live, live_generation) =
            runtime.start_work(WORK, "cand".to_string(), |token, generation| (token, generation));

        runtime.release_work(WORK, "cand", stale);
        assert!(runtime.is_working(WORK, "cand"));
        assert!(!live.is_cancelled());

        runtime.release_work(WORK, "cand", live_generation);
        assert!(!runtime.is_working(WORK, "cand"));
    }

    #[test]
    fn a_removal_or_rebind_ends_every_kind_of_work_for_its_key_alone() {
        let runtime = CandidateRuntime::default();
        let extraction =
            runtime.start_work(CandidateWork::Extraction, "gone".to_string(), |token, _| token);
        let identify =
            runtime.start_work(CandidateWork::Identify, "gone".to_string(), |token, _| token);
        let other = runtime.start_work(CandidateWork::Identify, "kept".to_string(), |token, _| token);

        runtime.record_event(&crate::import::ImportEvent::Scan(
            crate::import::ScanEvent::CandidateRemoved {
                candidate_key: "gone".to_string(),
            },
        ));

        assert!(extraction.is_cancelled() && identify.is_cancelled());
        assert!(!other.is_cancelled());
        assert_eq!(runtime.working_keys(CandidateWork::Identify), vec!["kept".to_string()]);
    }
}
