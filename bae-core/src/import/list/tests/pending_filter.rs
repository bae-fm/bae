//! Filtering Pending's rows, and importing a selection of them.

use super::*;
use crate::import::triage::{
    keys_for, selection_offers, CandidateAction, CandidateLiveState, SelectionMember,
};

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
    rows.failures.insert(
        "hash-Failed Import".to_string(),
        "Import failed".to_string(),
    );
    imported(&mut rows, "Imported", "release-1", 1);
    rows.skipped.insert((root(), "Set Aside".to_string()));
    rows
}

/// The filters with each of `checked` checked, as the person checks them.
fn checked(checked: &[PendingFilter]) -> PendingFilters {
    checked
        .iter()
        .fold(PendingFilters::default(), |filters, &filter| {
            filters.with_checked(filter, true)
        })
}

fn shown(rows: &ImportQueueRows, tab: TriageTab, filters: PendingFilters) -> Vec<String> {
    let flat = flattened(
        rows,
        &ImportListView {
            tab,
            pending_filters: filters,
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
            &[][..],
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
            &[PendingFilter::Identified][..],
            vec![
                "candidate Failed Import",
                "candidate Identified",
                "candidate Picked Among Several",
                "candidate Track Count Differs",
            ],
        ),
        (
            &[PendingFilter::NeedsYou][..],
            vec!["candidate Several", "candidate Several Tagged"],
        ),
        (
            &[PendingFilter::LookupError][..],
            vec!["candidate Lookup Failed"],
        ),
        (
            &[PendingFilter::ImportError][..],
            vec!["candidate Failed Import"],
        ),
        (
            &[PendingFilter::NeedsYou, PendingFilter::LookupError][..],
            vec![
                "candidate Lookup Failed",
                "candidate Several",
                "candidate Several Tagged",
            ],
        ),
        (
            &[PendingFilter::Identified, PendingFilter::ImportError][..],
            vec![
                "candidate Failed Import",
                "candidate Identified",
                "candidate Picked Among Several",
                "candidate Track Count Differs",
            ],
        ),
    ];
    for (filters, expected) in cases {
        assert_eq!(
            shown(&rows, TriageTab::Pending, checked(filters)),
            expected,
            "{filters:?}"
        );
    }
}

/// A sole release that does not fit the folder is not picked for it, so the
/// row waits on the person as several releases do.
#[test]
fn a_sole_release_that_does_not_fit_the_folder_needs_you() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Unfit")];
    rows.states.insert(
        "hash-Unfit".to_string(),
        with_verdict(several_matches_state(), |verdict| {
            verdict.pressing_count = 1;
            verdict.track_count = Some(10);
        }),
    );
    assert_eq!(
        shown(
            &rows,
            TriageTab::Pending,
            checked(&[PendingFilter::NeedsYou])
        ),
        vec!["candidate Unfit"]
    );
}

/// Done and Skipped rows are past identification, so a filter chosen on
/// Pending leaves them alone.
#[test]
fn the_pending_filter_leaves_done_and_skipped_alone() {
    let rows = every_kind();
    for &filter in PendingFilter::GROUPS.iter().copied().flatten() {
        assert_eq!(
            shown(&rows, TriageTab::Done, checked(&[filter])),
            vec!["candidate Imported"]
        );
        assert_eq!(
            shown(&rows, TriageTab::Skipped, checked(&[filter])),
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
    let texted = |filters: &[PendingFilter], text: &str| {
        flattened(
            &rows,
            &ImportListView {
                pending_filters: checked(filters),
                filter_text: text.to_string(),
                ..ImportListView::default()
            },
        )
    };
    let needs_you = texted(&[PendingFilter::NeedsYou], "album");
    assert_eq!(
        sequence(&rows, &needs_you),
        vec!["candidate Several Tagged"],
        "of the two Needs you rows only the drafted one reads Album"
    );
    assert_eq!(
        needs_you.summary.counts.pending, 10,
        "the tab counts are the whole queue's, whatever the list shows"
    );
    assert!(texted(&[PendingFilter::Identified], "nothing")
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
            pending_filters: checked(&[PendingFilter::Identified]),
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
            pending_filters: checked(&[PendingFilter::Identified]),
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

    let mut imported = keys_for(&members, CandidateAction::Import);
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
            pending_filters: checked(&[PendingFilter::Identified]),
            ..ImportListView::default()
        },
    );
    let members = select_all(&identified, &TriageRuntimeFacts::default());
    let shown: Vec<String> = members.iter().map(|m| m.candidate_key.clone()).collect();

    assert_eq!(keys_for(&members, CandidateAction::Import), shown);
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
                pending_filters: checked(&[PendingFilter::Identifying]),
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
    assert_eq!(
        members.len(),
        2,
        "the filter shows the rows being identified"
    );

    let offers = selection_offers(&members);
    assert!(!offers
        .iter()
        .any(|offer| offer.action == CandidateAction::Import));
    let cancel = offers
        .iter()
        .find(|offer| offer.action == CandidateAction::CancelIdentification)
        .expect("the selection offers to cancel identifying");
    assert_eq!(cancel.count, 2);
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
                pending_filters: checked(&[filter]),
                ..ImportListView::default()
            },
        );
        select_all(&flat, &TriageRuntimeFacts::default())
    };
    let keys = |members: &[SelectionMember], action| keys_for(members, action);

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

/// Every filter is in exactly one group, and the groups keep the order the
/// menu lists them in: identification's, then the import's, each ending with
/// its failure.
#[test]
fn every_filter_is_in_one_group_in_the_menu_s_order() {
    assert_eq!(
        PendingFilter::GROUPS,
        [
            &[
                PendingFilter::Identifying,
                PendingFilter::NeedsYou,
                PendingFilter::Identified,
                PendingFilter::LookupError,
            ][..],
            &[PendingFilter::Importing, PendingFilter::ImportError][..],
        ]
    );
}

/// Checking every state is the same as checking none: the list shows every
/// row, with no state left narrowing it, in whichever order they were checked.
#[test]
fn checking_every_state_is_all() {
    let every: Vec<PendingFilter> = PendingFilter::GROUPS
        .iter()
        .copied()
        .flatten()
        .copied()
        .collect();
    assert_eq!(checked(&every), PendingFilters::default());
    let mut reversed = every.clone();
    reversed.reverse();
    assert_eq!(checked(&reversed), PendingFilters::default());
    assert_eq!(
        every.iter().copied().collect::<PendingFilters>(),
        PendingFilters::default()
    );
    let rows = every_kind();
    assert_eq!(
        shown(&rows, TriageTab::Pending, checked(&every)),
        shown(&rows, TriageTab::Pending, PendingFilters::default())
    );
}

/// Clearing a state leaves the others; clearing the last one shows every row.
#[test]
fn clearing_the_last_state_returns_to_all() {
    let two = checked(&[PendingFilter::NeedsYou, PendingFilter::LookupError]);
    let one = two.with_checked(PendingFilter::NeedsYou, false);
    assert_eq!(one, checked(&[PendingFilter::LookupError]));
    assert_eq!(
        one.with_checked(PendingFilter::LookupError, false),
        PendingFilters::default()
    );
    assert_eq!(
        PendingFilters::default().with_checked(PendingFilter::Identified, false),
        PendingFilters::default()
    );
}

/// Checking a state already checked changes nothing.
#[test]
fn checking_a_checked_state_again_changes_nothing() {
    let once = checked(&[PendingFilter::Importing]);
    assert_eq!(
        once.clone().with_checked(PendingFilter::Importing, true),
        once
    );
}

/// The checked states list in the menu's order, whatever order they were
/// checked in.
#[test]
fn the_checked_states_list_in_the_menu_s_order() {
    let filters = checked(&[
        PendingFilter::ImportError,
        PendingFilter::Identified,
        PendingFilter::Identifying,
    ]);
    assert_eq!(
        filters.into_iter().collect::<Vec<_>>(),
        vec![
            PendingFilter::Identifying,
            PendingFilter::Identified,
            PendingFilter::ImportError,
        ]
    );
}

/// Select All under several states selects exactly the rows any of them
/// keeps — the keys a view change keeps selected.
#[test]
fn select_all_under_several_states_takes_the_rows_any_of_them_keeps() {
    let rows = every_kind();
    let mut keys = shown_candidate_keys(
        &rows,
        &request(ImportListView {
            pending_filters: checked(&[PendingFilter::NeedsYou, PendingFilter::ImportError]),
            ..ImportListView::default()
        }),
    )
    .expect("the queue flattens");
    keys.sort();
    let mut expected: Vec<String> = ["Failed Import", "Several", "Several Tagged"]
        .into_iter()
        .map(key)
        .collect();
    expected.sort();
    assert_eq!(keys, expected);
}

/// A state the runtime answers and one the tables answer each keep their own
/// rows.
#[test]
fn a_live_state_and_a_stored_state_keep_the_rows_of_either() {
    let rows = every_kind();
    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: ImportListView {
                pending_filters: checked(&[PendingFilter::Identifying, PendingFilter::LookupError]),
                ..ImportListView::default()
            },
            live_matches: [key("Tagged")].into_iter().collect(),
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");
    let mut shown = sequence(&rows, &flat);
    shown.sort();
    assert_eq!(shown, vec!["candidate Lookup Failed", "candidate Tagged"]);
}

/// The live matches are the candidates any checked live state keeps by what
/// is running for them, and none while no live state is checked.
#[test]
fn live_matches_are_the_candidates_any_checked_live_state_keeps() {
    let identifying = TriageRuntimeFacts {
        identification: Some(crate::import::IdentificationStatus::Running),
        import: None,
    };
    let importing = TriageRuntimeFacts {
        identification: None,
        import: Some(crate::import::ImportStanding::Queued),
    };
    let facts: std::collections::HashMap<String, TriageRuntimeFacts> = [
        ("identifying".to_string(), identifying),
        ("importing".to_string(), importing),
        ("idle".to_string(), TriageRuntimeFacts::default()),
    ]
    .into_iter()
    .collect();
    let matches = |filters: &[PendingFilter]| {
        checked(filters)
            .live_matches(&facts)
            .into_iter()
            .collect::<Vec<_>>()
    };
    assert_eq!(matches(&[PendingFilter::Identifying]), vec!["identifying"]);
    assert_eq!(
        matches(&[PendingFilter::Identifying, PendingFilter::Importing]),
        vec!["identifying", "importing"]
    );
    assert!(matches(&[PendingFilter::Identified]).is_empty());
    assert!(matches(&[]).is_empty());
}
