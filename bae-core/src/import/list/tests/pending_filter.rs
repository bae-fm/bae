//! Filtering Pending's rows, and importing a selection of them.

use super::*;
use crate::import::triage::{selection_offers, CandidateAction, CandidateLiveState, SelectionMember};

/// `state` with a draft to read: what identification or the tags wrote.
fn drafted(state: CandidateStateListRow) -> CandidateStateListRow {
    CandidateStateListRow {
        metadata_summary: Some(crate::import::TriageMetadataSummary {
            album_title: "Album".to_string(),
            album_artist_assignments: vec![crate::import::ArtistAssignment::named("Artist")],
        }),
        ..state
    }
}

/// Several pressings found, and one of them picked for the folder.
fn picked_among_several_state() -> CandidateStateListRow {
    drafted(CandidateStateListRow {
        metadata_provenance: Some(external_release_seed("mb-2")),
        metadata_author: crate::import::MetadataAuthor::Identification,
        metadata_draft_valid: true,
        ..several_matches_state()
    })
}

/// One release picked, whose track count differs from the folder's.
fn track_count_differs_state() -> CandidateStateListRow {
    drafted(with_verdict(auto_importable_state("mb-5"), |verdict| {
        verdict.track_count = Some(10);
    }))
}

fn every_kind() -> ImportQueueRows {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Identified"),
        candidate("Picked Among Several"),
        candidate("Track Count Differs"),
        candidate("Several"),
        candidate("Several Tagged"),
        candidate("Nothing Found"),
        candidate("Lookup Failed"),
        candidate("Failed Import"),
        candidate("Tagged"),
        candidate("Unidentified"),
        candidate("Imported"),
        candidate("Set Aside"),
    ];
    let states = [
        ("Identified", drafted(auto_importable_state("mb-1"))),
        ("Picked Among Several", picked_among_several_state()),
        ("Track Count Differs", track_count_differs_state()),
        ("Several", several_matches_state()),
        (
            "Several Tagged",
            drafted(CandidateStateListRow {
                metadata_provenance: Some(MetadataProvenance::FileMetadata),
                metadata_author: crate::import::MetadataAuthor::Prefill,
                metadata_draft_valid: true,
                ..several_matches_state()
            }),
        ),
        ("Nothing Found", not_found_state()),
        (
            "Lookup Failed",
            with_verdict(not_found_state(), |verdict| {
                verdict.kind = VerdictKind::Failed;
            }),
        ),
        ("Failed Import", drafted(auto_importable_state("mb-3"))),
        ("Tagged", drafted(prefilled_from_tags_state())),
    ];
    for (name, state) in states {
        rows.states.insert(format!("hash-{name}"), state);
    }
    rows.failures
        .insert("hash-Failed Import".to_string(), "Import failed".to_string());
    imported(&mut rows, "Imported", "release-1", 1);
    rows.skipped.insert((root(), "Set Aside".to_string()));
    rows
}

fn shown(rows: &ImportQueueRows, tab: TriageTab, filter: Option<PendingFilter>) -> Vec<String> {
    let flat = flattened(
        rows,
        &ImportListView {
            tab,
            pending_filter: filter,
            ..ImportListView::default()
        },
    );
    let mut shown = sequence(rows, &flat);
    shown.sort();
    shown
}

/// Each filter keeps exactly its own rows, wherever automatic import would or would not take them
/// them; a row can match more than one.
#[test]
fn each_filter_keeps_exactly_its_own_rows() {
    let rows = every_kind();
    let cases = [
        (
            None,
            vec![
                "candidate Failed Import",
                "candidate Identified",
                "candidate Lookup Failed",
                "candidate Nothing Found",
                "candidate Picked Among Several",
                "candidate Several",
                "candidate Several Tagged",
                "candidate Tagged",
                "candidate Track Count Differs",
                "candidate Unidentified",
            ],
        ),
        (
            Some(PendingFilter::Identified),
            vec![
                "candidate Failed Import",
                "candidate Identified",
                "candidate Picked Among Several",
                "candidate Track Count Differs",
            ],
        ),
        (
            Some(PendingFilter::NeedsYou),
            vec!["candidate Several", "candidate Several Tagged"],
        ),
        (
            Some(PendingFilter::LookupError),
            vec!["candidate Lookup Failed"],
        ),
        (
            Some(PendingFilter::ImportError),
            vec!["candidate Failed Import"],
        ),
    ];
    for (filter, expected) in cases {
        assert_eq!(shown(&rows, TriageTab::Pending, filter), expected, "{filter:?}");
    }
}

/// Done and Skipped rows are past identification, so a filter chosen on
/// Pending leaves them alone.
#[test]
fn the_pending_filter_leaves_done_and_skipped_alone() {
    let rows = every_kind();
    for filter in PendingFilter::ALL {
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

/// The filter composes with the text filter: both have to keep a row.
#[test]
fn the_text_filter_composes_with_the_pending_filter() {
    let mut rows = every_kind();
    rows.candidates.retain(|row| row.display_path != "Imported");
    rows.imported.clear();
    let texted = |filter, text: &str| {
        flattened(
            &rows,
            &ImportListView {
                pending_filter: filter,
                filter_text: text.to_string(),
                ..ImportListView::default()
            },
        )
    };
    let needs_you = texted(Some(PendingFilter::NeedsYou), "album");
    assert_eq!(
        sequence(&rows, &needs_you),
        vec!["candidate Several Tagged"],
        "of the two Needs you rows only the drafted one reads Album"
    );
    assert_eq!(
        needs_you.summary.counts.pending, 10,
        "the tab counts are the whole queue's, whatever the list shows"
    );
    assert!(texted(Some(PendingFilter::Identified), "nothing")
        .items
        .is_empty());
}

/// Locating a candidate clears the filters, so a row the filter hides is still
/// found where it sits.
#[test]
fn locating_a_candidate_ignores_the_pending_filter() {
    let rows = every_kind();
    let location = locate_candidate(
        &rows,
        &request(ImportListView {
            pending_filter: Some(PendingFilter::Identified),
            ..ImportListView::default()
        }),
        &key("Unidentified"),
    )
    .expect("the queue flattens")
    .expect("the candidate is found");
    assert_eq!(location.tab, TriageTab::Pending);
}

/// Selecting every identified row and importing them takes each one — a pick
/// among several releases and a track count that differs as much as an auto-importable
/// row — and a tags-only row selected beside them imports too.
#[test]
fn importing_a_selection_takes_every_row_with_a_draft_to_import() {
    let rows = every_kind();
    let identified = flattened(
        &rows,
        &ImportListView {
            pending_filter: Some(PendingFilter::Identified),
            ..ImportListView::default()
        },
    );
    let mut members = select_all(&identified, &TriageRuntimeFacts::default());
    let everything = flattened(&rows, &view(TriageTab::Pending));
    members.extend(
        select_all(&everything, &TriageRuntimeFacts::default())
            .into_iter()
            .filter(|member| member.candidate_key == key("Tagged")),
    );

    let import = selection_offers(&members)
        .into_iter()
        .find(|offer| offer.action == CandidateAction::Import)
        .expect("the selection offers to import");
    let mut imported: Vec<String> = import.candidate_keys;
    imported.sort();
    let mut expected: Vec<String> = [
        "Failed Import",
        "Identified",
        "Picked Among Several",
        "Tagged",
        "Track Count Differs",
    ]
    .into_iter()
    .map(key)
    .collect();
    expected.sort();
    assert_eq!(imported, expected);
}

/// What `flat` shows, as the selection members Select All makes of it, each
/// with what `facts` leave its row offering.
fn select_all(flat: &Flattened, facts: &TriageRuntimeFacts) -> Vec<SelectionMember> {
    flat.items
        .iter()
        .filter_map(|item| match item {
            ItemRef::Candidate { index, .. } => {
                let row = &flat.rows[*index].row;
                Some(SelectionMember {
                    candidate_key: row.candidate_key.clone(),
                    actions: CandidateLiveState::of(&row.action_basis, facts.clone()).actions,
                })
            }
            ItemRef::Header(_) | ItemRef::Invalid { .. } => None,
        })
        .collect()
}

/// Selecting everything Identified shows and importing it takes every shown
/// row and no hidden one.
#[test]
fn importing_all_identified_rows_takes_exactly_the_shown_ones() {
    let rows = every_kind();
    let identified = flattened(
        &rows,
        &ImportListView {
            pending_filter: Some(PendingFilter::Identified),
            ..ImportListView::default()
        },
    );
    let members = select_all(&identified, &TriageRuntimeFacts::default());
    let shown: Vec<String> = members.iter().map(|m| m.candidate_key.clone()).collect();

    let import = selection_offers(&members)
        .into_iter()
        .find(|offer| offer.action == CandidateAction::Import)
        .expect("the selection offers to import");
    assert_eq!(import.candidate_keys, shown);
    assert!(!import.candidate_keys.contains(&key("Tagged")));
}

/// Rows being identified offer no import: selecting everything Identifying
/// shows offers to cancel the identification instead.
#[test]
fn selecting_all_identifying_rows_offers_their_cancel_and_no_import() {
    let rows = every_kind();
    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: ImportListView {
                pending_filter: Some(PendingFilter::Identifying),
                ..ImportListView::default()
            },
            live_matches: [key("Identified"), key("Tagged")].into_iter().collect(),
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");
    let identifying = TriageRuntimeFacts {
        identification: Some(crate::import::IdentificationStatus::Running),
        import: None,
    };
    let members = select_all(&flat, &identifying);
    assert_eq!(members.len(), 2, "the filter shows the rows being identified");

    let offers = selection_offers(&members);
    assert!(!offers.iter().any(|offer| offer.action == CandidateAction::Import));
    let cancel = offers
        .iter()
        .find(|offer| offer.action == CandidateAction::CancelIdentification)
        .expect("the selection offers to cancel identifying");
    assert_eq!(cancel.candidate_keys.len(), 2);
}

/// What each filter's rows offer at rest is their own: Needs you imports
/// whatever draft it has, a failed lookup imports a valid draft and offers
/// the retry, and a failed import imports again.
#[test]
fn each_filter_s_rows_offer_what_their_drafts_and_lookups_allow() {
    let rows = every_kind();
    let offers_of = |filter| {
        let flat = flattened(
            &rows,
            &ImportListView {
                pending_filter: Some(filter),
                ..ImportListView::default()
            },
        );
        selection_offers(&select_all(&flat, &TriageRuntimeFacts::default()))
    };
    let keys = |offers: &[crate::import::triage::SelectionOffer], action| {
        offers
            .iter()
            .find(|offer| offer.action == action)
            .map(|offer| offer.candidate_keys.clone())
            .unwrap_or_default()
    };

    let needs_you = offers_of(PendingFilter::NeedsYou);
    assert_eq!(
        keys(&needs_you, CandidateAction::Import),
        vec![key("Several Tagged")],
        "the tags' draft imports; no draft, no import"
    );

    let lookup_error = offers_of(PendingFilter::LookupError);
    assert!(keys(&lookup_error, CandidateAction::Import).is_empty());
    assert_eq!(
        keys(&lookup_error, CandidateAction::RetryIdentification),
        vec![key("Lookup Failed")]
    );

    let import_error = offers_of(PendingFilter::ImportError);
    assert_eq!(
        keys(&import_error, CandidateAction::Import),
        vec![key("Failed Import")]
    );
}
