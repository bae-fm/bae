//! Which candidates a person has open in a pane right now.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// The candidate keys whose pane is open, each counted once per surface
/// holding it open.
///
/// Identification reads this in the same transaction that stores a result: a
/// result landing on an open candidate is one the person is looking at, so it
/// is stored read. Opening a candidate registers it here before its stored
/// result is marked read, and both writes go through the one database writer,
/// so a result either lands after the registration and is read, or lands
/// first and is cleared by the mark that follows it.
#[derive(Debug, Clone, Default)]
pub struct OpenCandidates {
    keys: Arc<Mutex<BTreeMap<String, usize>>>,
}

impl OpenCandidates {
    /// Every key some surface holds open.
    pub(crate) fn keys(&self) -> Vec<String> {
        self.keys
            .lock()
            .expect("the open-candidate lock is never poisoned: nothing panics holding it")
            .keys()
            .cloned()
            .collect()
    }

    /// Hold `key` open until the returned guard drops.
    pub(crate) fn open(&self, key: &str) -> OpenCandidate {
        *self
            .keys
            .lock()
            .expect("the open-candidate lock is never poisoned: nothing panics holding it")
            .entry(key.to_string())
            .or_default() += 1;
        OpenCandidate {
            candidates: self.clone(),
            key: key.to_string(),
        }
    }
}

/// One surface holding one candidate open. Dropping it closes the candidate
/// for that surface; the candidate stays open while any other holds it.
pub struct OpenCandidate {
    candidates: OpenCandidates,
    key: String,
}

impl Drop for OpenCandidate {
    fn drop(&mut self) {
        let mut keys = self
            .candidates
            .keys
            .lock()
            .expect("the open-candidate lock is never poisoned: nothing panics holding it");
        let holders = keys
            .get_mut(&self.key)
            .expect("an open candidate is counted until its last guard drops");
        *holders -= 1;
        if *holders == 0 {
            keys.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two surfaces holding one candidate open keep it open until both let
    /// go, and closing one candidate leaves the others open.
    #[test]
    fn a_candidate_stays_open_while_any_surface_holds_it() {
        let open = OpenCandidates::default();
        let first = open.open("a");
        let second = open.open("a");
        let other = open.open("b");
        assert_eq!(open.keys(), vec!["a".to_string(), "b".to_string()]);

        drop(first);
        assert_eq!(open.keys(), vec!["a".to_string(), "b".to_string()]);
        drop(second);
        assert_eq!(open.keys(), vec!["b".to_string()]);
        drop(other);
        assert!(open.keys().is_empty());
    }
}
