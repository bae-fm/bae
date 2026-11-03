use super::*;
use crate::test_releases::{mb_release_values, mb_vinyl_release};

/// A reading numbers each track on its own side (A1, A2 -> 1, 2; B1 -> 1),
/// not by a release-wide 1..N index.
#[test]
fn a_reading_numbers_tracks_per_side() {
    assert_eq!(
        value_labels(&mb_release_values(&mb_vinyl_release("vinyl-release"))),
        ["A1", "A2", "B1"]
    );
}
