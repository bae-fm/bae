use super::*;

/// The share done is the units done divided by the units in all.
#[test]
fn the_share_done_is_the_units_done_over_the_total() {
    assert_eq!(share_done(25, 100), 0.25);
    assert_eq!(share_done(100, 100), 1.0);
}

/// A whole of no units has a share done of 0.
#[test]
fn a_whole_of_no_units_has_none_done() {
    assert_eq!(share_done(0, 0), 0.0);
}

/// A step's bar fills to the share of its units done. A step of no units
/// has no share, so its bar moves without a count.
#[test]
fn a_step_of_no_units_has_no_share() {
    assert_eq!(step_share(1, 4), Some(0.25));
    assert_eq!(step_share(0, 0), None);
}
