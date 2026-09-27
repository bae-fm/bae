//! Filtering Pending by what each row's stored lookup result says — not by
//! where the Ready rule places it, which a row's commands and the Ready set
//! are read from.

use super::*;

/// One candidate per lookup result a Pending row can hold, a few placed
/// somewhere the result alone would not put them, and one on Done and one on
/// Skipped.
fn every_outcome() -> ImportQueueRows {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Ready"),
        candidate("Several"),
        candidate("Nothing Found"),
        candidate("Nothing to Look Up"),
        candidate("Wrong Medium"),
        candidate("Lookup Failed"),
        candidate("Failed Import"),
        candidate("Unidentified"),
        candidate("Tagged"),
        candidate("Imported"),
        candidate("Set Aside"),
    ];
    rows.states
        .insert("hash-Ready".to_string(), ready_state("mb-1"));
    rows.states
        .insert("hash-Several".to_string(), several_matches_state());
    rows.states
        .insert("hash-Nothing Found".to_string(), not_found_state());
    rows.states.insert(
        "hash-Nothing to Look Up".to_string(),
        with_verdict(not_found_state(), |verdict| {
            verdict.kind = VerdictKind::ManualOnly;
            verdict.track_count = Some(11);
        }),
    );
    rows.states
        .insert("hash-Wrong Medium".to_string(), wrong_medium_state());
    rows.states.insert(
        "hash-Lookup Failed".to_string(),
        with_verdict(ready_state("mb-4"), |verdict| {
            verdict.kind = VerdictKind::Failed;
        }),
    );
    rows.states
        .insert("hash-Failed Import".to_string(), ready_state("mb-2"));
    rows.failures
        .insert("hash-Failed Import".to_string(), "boom".to_string());
    rows.states
        .insert("hash-Tagged".to_string(), prefilled_from_tags_state());
    imported(&mut rows, "Imported", "release-1", 1);
    rows.skipped
        .insert((root(), "Set Aside".to_string()));
    rows
}

/// A verdict whose one release the folder's own files rule out: a 96 kHz rip
/// against a CD.
fn wrong_medium_state() -> CandidateStateListRow {
    with_verdict(ready_state("mb-cd"), |verdict| {
        verdict.medium_conflict = Some(crate::identify::MediumConflict::NotCdAudio);
    })
}

fn shown(
    rows: &ImportQueueRows,
    tab: TriageTab,
    identification: Option<IdentificationOutcome>,
) -> Vec<String> {
    let flat = flattened(
        rows,
        &ImportListView {
            tab,
            identification,
            ..ImportListView::default()
        },
    );
    let mut shown = sequence(rows, &flat);
    shown.sort();
    shown
}

/// Each outcome keeps exactly the rows whose lookup result reads as it,
/// wherever the Ready rule placed them: a row the folder's medium rules out
/// found one release, and a row answered by its tags was never identified.
#[test]
fn every_outcome_keeps_exactly_its_own_rows() {
    let rows = every_outcome();
    let cases = [
        (
            None,
            vec![
                "candidate Failed Import",
                "candidate Lookup Failed",
                "candidate Nothing Found",
                "candidate Nothing to Look Up",
                "candidate Ready",
                "candidate Several",
                "candidate Tagged",
                "candidate Unidentified",
                "candidate Wrong Medium",
            ],
        ),
        (
            Some(IdentificationOutcome::NotIdentified),
            vec!["candidate Tagged", "candidate Unidentified"],
        ),
        (
            Some(IdentificationOutcome::OneRelease),
            vec![
                "candidate Failed Import",
                "candidate Ready",
                "candidate Wrong Medium",
            ],
        ),
        (
            Some(IdentificationOutcome::SeveralReleases),
            vec!["candidate Several"],
        ),
        (
            Some(IdentificationOutcome::NoMatch),
            vec!["candidate Nothing Found", "candidate Nothing to Look Up"],
        ),
        (
            Some(IdentificationOutcome::LookupFailed),
            vec!["candidate Lookup Failed"],
        ),
    ];
    for (filter, expected) in cases {
        assert_eq!(shown(&rows, TriageTab::Pending, filter), expected, "{filter:?}");
    }
}

/// Done and Skipped rows are past identification, so a filter chosen on
/// Pending leaves them alone.
#[test]
fn the_identification_filter_leaves_done_and_skipped_alone() {
    let rows = every_outcome();
    for filter in IdentificationOutcome::ALL {
        assert_eq!(
            shown(&rows, TriageTab::Done, Some(filter)),
            vec!["candidate Imported"]
        );
        assert_eq!(
            shown(&rows, TriageTab::Skipped, Some(filter)),
            vec!["candidate Set Aside"]
        );
    }
}

/// The Ready set a bulk import and select-all act on is the filtered list's,
/// and the filter composes with the text filter: both have to keep a row.
#[test]
fn the_ready_set_and_the_text_filter_follow_the_identification_filter() {
    // No Done row: a text filter reads a Done row's library text, which this
    // queue has no album for.
    let mut rows = every_outcome();
    rows.candidates.retain(|row| row.display_path != "Imported");
    rows.imported.clear();
    rows.candidates.push(candidate("Also Ready"));
    rows.states
        .insert("hash-Also Ready".to_string(), ready_state("mb-3"));

    let ready_keys = |flat: &Flattened| {
        flat.summary
            .ready
            .iter()
            .map(|row| row.candidate_key.clone())
            .collect::<Vec<_>>()
    };

    let several = flattened(
        &rows,
        &ImportListView {
            identification: Some(IdentificationOutcome::SeveralReleases),
            ..ImportListView::default()
        },
    );
    assert!(
        several.summary.ready.is_empty(),
        "no Ready row is among the rows that found several releases"
    );

    let not_identified = flattened(
        &rows,
        &ImportListView {
            identification: Some(IdentificationOutcome::NotIdentified),
            ..ImportListView::default()
        },
    );
    assert_eq!(
        ready_keys(&not_identified),
        vec![key("Tagged")],
        "a draft the tags seeded is Ready, and was never identified"
    );

    let one_texted = flattened(
        &rows,
        &ImportListView {
            identification: Some(IdentificationOutcome::OneRelease),
            filter_text: "also".to_string(),
            ..ImportListView::default()
        },
    );
    assert_eq!(sequence(&rows, &one_texted), vec!["candidate Also Ready"]);
    assert_eq!(ready_keys(&one_texted), vec![key("Also Ready")]);
    assert_eq!(
        one_texted.summary.counts.pending, 10,
        "the tab counts are the whole queue's, whatever the list shows"
    );
}

/// Locating a candidate clears the filters, so a row the filter hides is still
/// found where it sits.
#[test]
fn locating_a_candidate_ignores_the_identification_filter() {
    let rows = every_outcome();
    let location = locate_candidate(
        &rows,
        &request(ImportListView {
            identification: Some(IdentificationOutcome::OneRelease),
            ..ImportListView::default()
        }),
        &key("Unidentified"),
    )
    .expect("the queue flattens")
    .expect("the candidate is found");
    assert_eq!(location.tab, TriageTab::Pending);
}
