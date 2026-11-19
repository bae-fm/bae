use super::*;

/// A candidate nobody identified opens on its draft, pre-filled from the
/// folder's tags or blank, since that is where its metadata is; so does one
/// whose answer someone acted on.
#[test]
fn a_pane_with_nothing_to_answer_opens_on_the_draft() {
    use MetadataPresentation::Draft;
    assert_eq!(MetadataPresentation::opening(false, false, false, false), Draft);
    assert_eq!(MetadataPresentation::opening(false, false, true, false), Draft);
    assert_eq!(MetadataPresentation::opening(false, true, true, false), Draft);
}

/// A run in flight, a verdict nobody acted on (several matches, none, a
/// failed run) and a picked release that failed a check against the folder
/// each open on Find online: the answer, and the way to act on it, are there.
#[test]
fn a_run_or_an_answer_waiting_on_the_person_opens_on_find_online() {
    use MetadataPresentation::FindOnline;
    assert_eq!(MetadataPresentation::opening(true, false, false, false), FindOnline);
    assert_eq!(MetadataPresentation::opening(true, true, true, false), FindOnline);
    assert_eq!(MetadataPresentation::opening(false, true, false, false), FindOnline);
    assert_eq!(MetadataPresentation::opening(false, true, true, true), FindOnline);
}
