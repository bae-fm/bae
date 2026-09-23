//! The row's unread marker: the question a result asks, while the person has
//! not seen it.

use super::*;
use crate::identify::NeedsYou;
use crate::import::MetadataAuthor;

/// A run that found several pressings for a folder whose tags already make a
/// valid draft leaves the row Ready — the tags answer it — and flags the
/// pressings it found until the person opens the row.
#[test]
fn a_ready_row_flags_the_pressings_a_run_found_until_it_is_read() {
    let tag_draft = CandidateStateListRow {
        metadata_provenance: Some(MetadataProvenance::FileMetadata),
        metadata_author: MetadataAuthor::Prefill,
        metadata_draft_valid: true,
        ..several_matches_state()
    };

    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), unread(tag_draft.clone()));
    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert_eq!(
        row.attention,
        Some(NeedsYou::SeveralMatches { count: 3 }),
        "unread, the row flags what the run found"
    );

    rows.states.insert("hash-Release".to_string(), tag_draft);
    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert_eq!(row.attention, None, "read, the flag is gone");
}

/// Every question a result asks flags the row while unread — the
/// disagreements of identification's own pick included — and a result that
/// asks nothing flags nothing.
#[test]
fn the_flag_is_the_question_the_unread_result_asks() {
    let cases = [
        (
            several_matches_state(),
            Some(NeedsYou::SeveralMatches { count: 3 }),
        ),
        (not_found_state(), Some(NeedsYou::NoMatch)),
        (
            with_verdict(ready_state("mb-1"), |verdict| {
                verdict.summary.track_count = Some(10);
            }),
            Some(NeedsYou::TrackCountDisagrees {
                local: 10,
                source: 11,
            }),
        ),
        (ready_state("mb-1"), None),
    ];
    for (state, expected) in cases {
        let mut rows = queue();
        rows.candidates = vec![candidate("Release")];
        rows.states.insert("hash-Release".to_string(), unread(state));

        let flat = flattened(&rows, &view(TriageTab::Pending));
        assert_eq!(row_for(&flat, "Release").attention, expected);
    }
}

/// A row past the point of being asked anything flags nothing, whatever its
/// unread result would ask: an imported folder's own release is in the
/// library now, and a skipped one was set aside.
#[test]
fn done_and_skipped_rows_flag_nothing() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states.insert(
        "hash-Release".to_string(),
        unread(several_matches_state()),
    );
    rows.skipped.insert((root(), "Release".to_string()));
    let flat = flattened(&rows, &view(TriageTab::Skipped));
    assert_eq!(row_for(&flat, "Release").attention, None);

    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states.insert(
        "hash-Release".to_string(),
        unread(several_matches_state()),
    );
    imported(&mut rows, "Release", "rel-1", 1);
    let flat = flattened(&rows, &view(TriageTab::Done));
    assert_eq!(row_for(&flat, "Release").attention, None);
}
