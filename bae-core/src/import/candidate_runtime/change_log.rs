//! Which keys changed, in the order they last changed, for readers that each
//! read on from where they last looked.

use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ChangeLog {
    /// How many changes there have been; a reader that has read up to it has
    /// read them all.
    sequence: u64,
    /// Every key that has changed, once, at its latest change. A key that
    /// left stays, so a reader that has not looked since hears it left.
    by_sequence: BTreeMap<u64, String>,
    at: HashMap<String, u64>,
}

impl ChangeLog {
    /// `key` changed.
    pub(crate) fn record(&mut self, key: &str) {
        self.sequence += 1;
        if let Some(previous) = self.at.insert(key.to_string(), self.sequence) {
            self.by_sequence.remove(&previous);
        }
        self.by_sequence.insert(self.sequence, key.to_string());
    }

    /// Where a reader that has read every change so far has read up to.
    pub(crate) fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Each key that changed after `seen`, once, in the order of its latest
    /// change.
    pub(crate) fn since(&self, seen: u64) -> impl Iterator<Item = &str> {
        self.by_sequence
            .range(seen + 1..)
            .map(|(_, key)| key.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reader_hears_each_key_once_from_where_it_last_looked() {
        let mut log = ChangeLog::default();
        log.record("a");
        log.record("b");
        let seen = log.sequence();
        log.record("a");
        log.record("c");
        log.record("a");
        assert_eq!(log.since(0).collect::<Vec<_>>(), ["b", "c", "a"]);
        assert_eq!(log.since(seen).collect::<Vec<_>>(), ["c", "a"]);
        assert_eq!(log.since(log.sequence()).count(), 0);
    }
}
