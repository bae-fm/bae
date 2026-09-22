//! The identifications in flight right now, counted as one group.

use std::collections::HashSet;

/// Every identification happening right now, whoever started it, counted for
/// the one pair of numbers a surface draws: how many have ended, out of how
/// many there are.
///
/// A batch opens when the first identification is admitted and is over when
/// the last one ends. Being over resets it: the next identification anyone
/// starts is counted from zero rather than continuing a total that describes
/// work already finished.
///
/// The keys are held as sets rather than counted as they arrive, because the
/// same key can be admitted again while the batch is still open — given back
/// to the queue after a run it was on was replayed, or asked for by hand
/// after its first answer — and that is one candidate to whoever is watching
/// the count, not two.
#[derive(Default)]
pub(super) struct IdentificationBatch {
    /// Every key admitted since the batch opened.
    admitted: HashSet<String>,
    /// The admitted keys whose identification has ended.
    ended: HashSet<String>,
}

impl IdentificationBatch {
    /// `key`'s identification has begun. Opens a batch when none is open, and
    /// takes the key back out of the ended set when its identification has
    /// started again inside the batch that already counted it. Reports whether
    /// the counts changed.
    pub(super) fn admit(&mut self, key: &str) -> bool {
        let counted = self.admitted.insert(key.to_string());
        let started_again = self.ended.remove(key);
        counted || started_again
    }

    /// `key`'s identification has ended — a verdict stored, refused,
    /// abandoned, failed to save, or dropped from the queue before it ran.
    /// The batch is over once every key it admitted has ended, and resets
    /// itself there. Reports whether the counts changed.
    ///
    /// A key the batch never admitted ends nothing: it was not part of this
    /// batch's work, and counting it would put the ended count past the total.
    pub(super) fn end(&mut self, key: &str) -> bool {
        if !self.admitted.contains(key) || !self.ended.insert(key.to_string()) {
            return false;
        }
        if self.ended.len() == self.admitted.len() {
            self.admitted.clear();
            self.ended.clear();
        }
        true
    }

    /// How many of the batch's identifications have ended, out of how many it
    /// holds. `(0, 0)` is no batch at all, which is what a surface draws
    /// nothing for.
    pub(super) fn progress(&self) -> (u32, u32) {
        (self.ended.len() as u32, self.admitted.len() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_admitted_key_opens_a_batch() {
        let mut batch = IdentificationBatch::default();
        assert_eq!(batch.progress(), (0, 0));
        assert!(batch.admit("a"));
        assert_eq!(batch.progress(), (0, 1));
        assert!(batch.admit("b"));
        assert_eq!(batch.progress(), (0, 2));
    }

    #[test]
    fn a_key_admitted_twice_is_counted_once() {
        let mut batch = IdentificationBatch::default();
        assert!(batch.admit("a"));
        assert!(!batch.admit("a"), "the same work, admitted again");
        assert_eq!(batch.progress(), (0, 1));
    }

    #[test]
    fn ending_one_of_several_advances_the_count() {
        let mut batch = IdentificationBatch::default();
        batch.admit("a");
        batch.admit("b");
        assert!(batch.end("a"));
        assert_eq!(batch.progress(), (1, 2));
        assert!(!batch.end("a"), "it had already ended");
        assert_eq!(batch.progress(), (1, 2));
    }

    #[test]
    fn a_key_that_starts_again_inside_the_batch_is_waited_on_again() {
        let mut batch = IdentificationBatch::default();
        batch.admit("a");
        batch.admit("b");
        batch.end("a");
        assert_eq!(batch.progress(), (1, 2));
        assert!(batch.admit("a"));
        assert_eq!(
            batch.progress(),
            (0, 2),
            "its identification is in flight again, and the total still counts it once"
        );
    }

    #[test]
    fn the_batch_is_over_once_every_key_has_ended() {
        let mut batch = IdentificationBatch::default();
        batch.admit("a");
        batch.admit("b");
        batch.end("a");
        assert!(batch.end("b"));
        assert_eq!(batch.progress(), (0, 0));
    }

    #[test]
    fn a_second_batch_starts_from_zero() {
        let mut batch = IdentificationBatch::default();
        batch.admit("a");
        batch.admit("b");
        batch.end("a");
        batch.end("b");
        assert!(batch.admit("c"));
        assert_eq!(
            batch.progress(),
            (0, 1),
            "the batch that drained is over; this one counts only its own work"
        );
    }

    #[test]
    fn a_key_the_batch_never_admitted_ends_nothing() {
        let mut batch = IdentificationBatch::default();
        batch.admit("a");
        assert!(!batch.end("b"));
        assert_eq!(batch.progress(), (0, 1));
    }

    #[test]
    fn ending_a_key_with_no_batch_open_changes_nothing() {
        let mut batch = IdentificationBatch::default();
        assert!(!batch.end("a"));
        assert_eq!(batch.progress(), (0, 0));
    }
}
