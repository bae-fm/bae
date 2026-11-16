use super::*;

/// A bar's fraction is its bytes done of its bytes in all.
#[test]
fn a_bar_is_filled_by_its_bytes_done() {
    assert_eq!(UploadBar { bytes_done: 25, bytes_total: 100 }.fraction(), 0.25);
}

/// A bar of no bytes has done none of them.
#[test]
fn a_bar_of_no_bytes_is_empty() {
    assert_eq!(UploadBar { bytes_done: 0, bytes_total: 0 }.fraction(), 0.0);
}
