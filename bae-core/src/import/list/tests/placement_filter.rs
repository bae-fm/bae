//! Filtering Pending by where the tables place each row: the same placement
//! a row's section, its commands and the Ready set are read from.

use super::*;
use crate::identify::NeedsYouKind;

/// One candidate per Pending placement, plus one on Done and one on Skipped.
fn every_placement() -> ImportQueueRows {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Ready"),
        candidate("Several"),
        candidate("Nothing Found"),
        candidate("Failed Import"),
        candidate("Unanswered"),
        candidate("Imported"),
        candidate("Set Aside"),
    ];
    rows.states
        .insert("hash-Ready".to_string(), ready_state("mb-1"));
    rows.states
        .insert("hash-Several".to_string(), several_matches_state());
    rows.states
        .insert("hash-Nothing Found".to_string(), not_found_state());
    rows.states
        .insert("hash-Failed Import".to_string(), ready_state("mb-2"));
    rows.failures
        .insert("hash-Failed Import".to_string(), "boom".to_string());
    imported(&mut rows, "Imported", "release-1", 1);
    rows.skipped
        .insert((root(), "Set Aside".to_string()));
    rows
}

fn shown(rows: &ImportQueueRows, tab: TriageTab, placement: PlacementFilter) -> Vec<String> {
    let flat = flattened(
        rows,
        &ImportListView {
            tab,
            placement,
            ..ImportListView::default()
        },
    );
    sequence(rows, &flat)
}

#[test]
fn every_placement_filter_keeps_exactly_its_own_rows() {
    let rows = every_placement();
    let cases = [
        (
            PlacementFilter::Any,
            vec![
                "candidate Failed Import",
                "candidate Nothing Found",
                "candidate Ready",
                "candidate Several",
                "candidate Unanswered",
            ],
        ),
        (PlacementFilter::Ready, vec!["candidate Ready"]),
        (
            PlacementFilter::NeedsYou(None),
            vec!["candidate Nothing Found", "candidate Several"],
        ),
        (
            PlacementFilter::NeedsYou(Some(NeedsYouKind::SeveralMatches)),
            vec!["candidate Several"],
        ),
        (
            PlacementFilter::NeedsYou(Some(NeedsYouKind::NoMatch)),
            vec!["candidate Nothing Found"],
        ),
        (
            PlacementFilter::NeedsYou(Some(NeedsYouKind::LookupFailed)),
            vec![],
        ),
        (PlacementFilter::Failed, vec!["candidate Failed Import"]),
        (PlacementFilter::Unanswered, vec!["candidate Unanswered"]),
    ];
    for (filter, expected) in cases {
        let mut shown = shown(&rows, TriageTab::Pending, filter);
        shown.sort();
        assert_eq!(shown, expected, "{filter:?}");
    }
}

/// Done and Skipped hold no placements to choose between, so a filter chosen
/// on Pending leaves their rows alone.
#[test]
fn a_placement_filter_leaves_done_and_skipped_alone() {
    let rows = every_placement();
    for filter in [PlacementFilter::Ready, PlacementFilter::Unanswered] {
        assert_eq!(
            shown(&rows, TriageTab::Done, filter),
            vec!["candidate Imported"]
        );
        assert_eq!(
            shown(&rows, TriageTab::Skipped, filter),
            vec!["candidate Set Aside"]
        );
    }
}

/// The Ready set a bulk import and select-all act on is the filtered list's,
/// and the filter composes with the text filter: both have to keep a row.
#[test]
fn the_ready_set_and_the_text_filter_follow_the_placement_filter() {
    // No Done row: a text filter reads a Done row's library text, which this
    // queue has no album for.
    let mut rows = every_placement();
    rows.candidates.retain(|row| row.display_path != "Imported");
    rows.imported.clear();
    rows.candidates.push(candidate("Also Ready"));
    rows.states
        .insert("hash-Also Ready".to_string(), ready_state("mb-3"));

    let needs_you = flattened(
        &rows,
        &ImportListView {
            placement: PlacementFilter::NeedsYou(None),
            ..ImportListView::default()
        },
    );
    assert!(
        needs_you.summary.ready.is_empty(),
        "no Ready row is in a Needs-you list"
    );

    let ready_texted = flattened(
        &rows,
        &ImportListView {
            placement: PlacementFilter::Ready,
            filter_text: "also".to_string(),
            ..ImportListView::default()
        },
    );
    assert_eq!(sequence(&rows, &ready_texted), vec!["candidate Also Ready"]);
    assert_eq!(
        ready_texted
            .summary
            .ready
            .iter()
            .map(|row| row.candidate_key.as_str())
            .collect::<Vec<_>>(),
        vec![key("Also Ready").as_str()]
    );
    assert_eq!(
        ready_texted.summary.counts.pending, 6,
        "the tab counts are the whole queue's, whatever the list shows"
    );
}

/// Locating a candidate clears the filters, so a row the filter hides is still
/// found where it sits.
#[test]
fn locating_a_candidate_ignores_the_placement_filter() {
    let rows = every_placement();
    let location = locate_candidate(
        &rows,
        &request(ImportListView {
            placement: PlacementFilter::Ready,
            ..ImportListView::default()
        }),
        &key("Unanswered"),
    )
    .expect("the queue flattens")
    .expect("the candidate is found");
    assert_eq!(location.tab, TriageTab::Pending);
}
