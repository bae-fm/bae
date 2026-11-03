use super::*;

/// A candidate nobody has touched opens on its draft — pre-filled from the
/// folder's tags or blank, that is where its metadata is.
#[test]
fn a_fresh_pane_opens_on_the_draft() {
    assert_eq!(CandidateSession::initial(false, false).presentation, MetadataPresentation::Draft);
    assert_eq!(CandidateSession::initial(true, false).presentation, MetadataPresentation::Draft);
    assert_eq!(CandidateSession::initial(true, true).presentation, MetadataPresentation::Draft);
}

/// A verdict nobody has acted on — several matches, none, a failed run —
/// opens on Find online: the answer, and the way to act on it, are there.
#[test]
fn an_unanswered_verdict_opens_on_find_online() {
    assert_eq!(
        CandidateSession::initial(false, true).presentation,
        MetadataPresentation::FindOnline
    );
}

/// Each move puts the pane where it says; a move to the draft keeps the
/// section Find online opens on, and naming no section opens the last one.
#[test]
fn each_move_puts_the_pane_where_it_says() {
    let session = CandidateSession::initial(false, false);
    let searched = session.clone().moved(PaneMove::Search);
    assert_eq!(searched.presentation, MetadataPresentation::FindOnline);
    assert_eq!(searched.find_online_section, FindOnlineSection::Search);
    let back = searched.moved(PaneMove::Back);
    assert_eq!(back.presentation, MetadataPresentation::Draft);
    assert_eq!(back.find_online_section, FindOnlineSection::Search);
    let reopened = back.moved(PaneMove::FindOnline);
    assert_eq!(reopened.presentation, MetadataPresentation::FindOnline);
    assert_eq!(reopened.find_online_section, FindOnlineSection::Search);
    for pane_move in [PaneMove::Admitted, PaneMove::Automatic] {
        let moved = reopened.clone().moved(pane_move);
        assert_eq!(moved.presentation, MetadataPresentation::FindOnline);
        assert_eq!(moved.find_online_section, FindOnlineSection::Automatic);
    }
    for pane_move in [PaneMove::SettledOnPick, PaneMove::Picked, PaneMove::Back] {
        assert_eq!(reopened.clone().moved(pane_move).presentation, MetadataPresentation::Draft);
    }
}
