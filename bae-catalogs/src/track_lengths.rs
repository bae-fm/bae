//! When a length a source states for a track disagrees with the length of
//! its audio. Every device compiles this, since the library's SQL calls it.

/// How far a row's two lengths may differ before the row says so.
///
/// A source that rounds its track lengths to whole seconds is out by up to one;
/// lossy encoder delay and padding add a fraction more; a pregap counted on one
/// side and not the other is up to two on a Red Book disc. Three seconds
/// absorbs all of that.
///
/// What it deliberately does not absorb is a wrong pairing. Two different
/// tracks off one album differ by tens of seconds far more often than by three,
/// and the row shows both numbers regardless — this only decides whether to
/// point at them, with no consequence beyond a mark: nothing here disables the
/// commit.
pub const LENGTH_DISAGREEMENT_MS: u64 = 3_000;

/// Whether two lengths, in milliseconds, are further apart than
/// [`LENGTH_DISAGREEMENT_MS`].
pub fn lengths_disagree(a_ms: u64, b_ms: u64) -> bool {
    a_ms.abs_diff(b_ms) > LENGTH_DISAGREEMENT_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_disagree_only_past_the_threshold_in_either_direction() {
        assert!(!lengths_disagree(180_000, 180_000));
        assert!(!lengths_disagree(180_000, 180_000 + LENGTH_DISAGREEMENT_MS));
        assert!(!lengths_disagree(180_000 + LENGTH_DISAGREEMENT_MS, 180_000));
        assert!(lengths_disagree(180_000, 180_001 + LENGTH_DISAGREEMENT_MS));
        assert!(lengths_disagree(180_001 + LENGTH_DISAGREEMENT_MS, 180_000));
    }
}
