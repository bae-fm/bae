//! Which keys changed, in the order they last changed, for readers that each
//! read on from where they last looked.

use crate::import::CandidateKey;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ChangeLog {
    /// How many changes there have been; a reader that has read up to it has
    /// read them all.
    sequence: u64,
    /// Every key that has changed, once, at its latest change. A key that
    /// left stays, so a reader that has not looked since hears it left.
    by_sequence: BTreeMap<u64, CandidateKey>,
    at: HashMap<CandidateKey, u64>,
}

impl ChangeLog {
    /// `key` changed.
    pub(crate) fn record(&mut self, key: &CandidateKey) {
        self.sequence += 1;
        if let Some(previous) = self.at.insert(key.clone(), self.sequence) {
            self.by_sequence.remove(&previous);
        }
        self.by_sequence.insert(self.sequence, key.clone());
    }

    /// Where a reader that has read every change so far has read up to.
    pub(crate) fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Each key that changed after `seen`, once, in the order of its latest
    /// change.
    pub(crate) fn since(&self, seen: u64) -> impl Iterator<Item = &CandidateKey> {
        self.by_sequence.range(seen + 1..).map(|(_, key)| key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reader_hears_each_key_once_from_where_it_last_looked() {
        let mut log = ChangeLog::default();
        log.record(&crate::import::CandidateKey::test("a"));
        log.record(&crate::import::CandidateKey::test("b"));
        let seen = log.sequence();
        log.record(&crate::import::CandidateKey::test("a"));
        log.record(&crate::import::CandidateKey::test("c"));
        log.record(&crate::import::CandidateKey::test("a"));
        let keys = |names: &[&str]| {
            names
                .iter()
                .map(|name| crate::import::CandidateKey::test(name))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            log.since(0).cloned().collect::<Vec<_>>(),
            keys(&["b", "c", "a"])
        );
        assert_eq!(
            log.since(seen).cloned().collect::<Vec<_>>(),
            keys(&["c", "a"])
        );
        assert_eq!(log.since(log.sequence()).count(), 0);
    }
}
