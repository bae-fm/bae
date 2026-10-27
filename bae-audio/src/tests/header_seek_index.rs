fn index(entries: &[u64], length: Option<u64>) -> HeaderSeekIndex {
    HeaderSeekIndex {
        entries: entries.to_vec(),
        length,
    }
}

#[test]
fn an_index_lands_within_its_widest_stretch() {
    let spaced = index(&[0, 100, 200], Some(300));
    assert!(spaced.lands_within(100));
    assert!(!spaced.lands_within(99));
}

#[test]
fn the_stretch_after_the_last_entry_counts() {
    assert!(!index(&[0, 100, 200], Some(301)).lands_within(100));
}

#[test]
fn the_stretch_before_the_first_entry_counts() {
    assert!(!index(&[101, 200], Some(300)).lands_within(100));
}

/// A seek with no entry past the first sample searches the file or decodes
/// from its first frame, however short the file.
#[test]
fn an_index_with_no_entry_past_the_first_sample_lands_nowhere() {
    assert!(!index(&[], Some(50)).lands_within(100));
    assert!(!index(&[0], Some(50)).lands_within(100));
}

#[test]
fn an_index_without_a_stated_length_leaves_its_end_unbounded() {
    assert!(!index(&[0, 100, 200], None).lands_within(100));
}
