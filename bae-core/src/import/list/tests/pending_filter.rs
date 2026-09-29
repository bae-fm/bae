//! Filtering Found's rows by state, and importing a selection of them.

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
        candidate("Pick Set Aside"),
        candidate("Unread"),
        candidate("Nothing To Look Up"),
        candidate("Broke"),
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
        (
            "Pick Set Aside",
            drafted(CandidateStateListRow {
                metadata_provenance: Some(MetadataProvenance::FileMetadata),
                metadata_author: crate::import::MetadataAuthor::Person,
                ..auto_importable_state("mb-4")
            }),
        ),
        (
            "Unread",
            with_verdict(not_found_state(), |verdict| {
                *verdict = VerdictSummary {
                    unread_document: true,
                    kept_own_draft: false,
                    ..auto_importable_state("mb-6")
                        .verdict
                        .expect("the state has a verdict")
                };
            }),
        ),
        (
            "Nothing To Look Up",
            with_verdict(not_found_state(), |verdict| {
                verdict.kind = VerdictKind::ManualOnly;
                verdict.track_count = Some(11);
            }),
        ),
        (
            "Broke",
            with_verdict(not_found_state(), |verdict| {
                verdict.kind = VerdictKind::Error {
                    failure: crate::signals::InternalFailure {
                        detail: "reading the store: the disk is full".to_string(),
                    },
                };
            }),
        ),
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
fn checked(checked: &[PendingState]) -> PendingFilters {
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

/// Each state keeps exactly its own rows, one state per row, whatever
/// automatic import would or would not take them.
#[test]
fn each_state_keeps_exactly_its_own_rows() {
    let rows = every_kind();
    let cases = [
        (
            PendingState::NotLookedUp,
            vec!["candidate Tagged", "candidate Unidentified"],
        ),
        (PendingState::Identifying, vec![]),
        (
            PendingState::NeedsYou,
            vec![
                "candidate Nothing Found",
                "candidate Nothing To Look Up",
                "candidate Several",
                "candidate Several Tagged",
            ],
        ),
        (
            PendingState::Identified,
            vec![
                "candidate Identified",
                "candidate Picked Among Several",
                "candidate Track Count Differs",
            ],
        ),
        (PendingState::Unmatched, vec!["candidate Pick Set Aside"]),
        (
            PendingState::LookupError,
            vec!["candidate Lookup Failed", "candidate Unread"],
        ),
        (PendingState::Error, vec!["candidate Broke"]),
        (PendingState::Importing, vec![]),
        (PendingState::ImportError, vec!["candidate Failed Import"]),
    ];
    for (state, expected) in cases {
        assert_eq!(
            shown(&rows, TriageTab::Pending, checked(&[state])),
            expected,
            "{state:?}"
        );
    }
    assert_eq!(
        shown(
            &rows,
            TriageTab::Pending,
            checked(&[PendingState::NeedsYou, PendingState::LookupError])
        ),
        vec![
            "candidate Lookup Failed",
            "candidate Nothing Found",
            "candidate Nothing To Look Up",
            "candidate Several",
            "candidate Several Tagged",
            "candidate Unread",
        ]
    );
}

/// Every row on Found is in exactly one state, with what is running for some
/// of them joined: each shows under exactly one filter.
#[test]
fn every_row_is_in_exactly_one_state() {
    let rows = every_kind();
    let identifying = TriageRuntimeFacts {
        identification: Some(crate::import::IdentificationStatus::Running),
        import: None,
    };
    let importing = TriageRuntimeFacts {
        identification: Some(crate::import::IdentificationStatus::Queued),
        import: Some(crate::import::ImportStanding::Running),
    };
    let facts: std::collections::HashMap<String, TriageRuntimeFacts> = [
        (key("Several"), identifying),
        (key("Failed Import"), importing),
    ]
    .into_iter()
    .collect();
    let every = shown(&rows, TriageTab::Pending, PendingFilters::default());
    let mut seen: Vec<String> = Vec::new();
    for &state in PendingState::GROUPS.iter().copied().flatten() {
        let filters = checked(&[state]);
        let flat = flatten(
            &rows,
            &ImportListRequest {
                live_standings: filters.live_standings(&facts),
                view: ImportListView {
                    pending_filters: filters,
                    ..ImportListView::default()
                },
                ..ImportListRequest::default()
            },
        )
        .expect("the queue flattens");
        let shown = sequence(&rows, &flat);
        match state {
            PendingState::Identifying => assert_eq!(shown, vec!["candidate Several"]),
            PendingState::Importing => assert_eq!(shown, vec!["candidate Failed Import"]),
            _ => {}
        }
        seen.extend(shown);
    }
    seen.sort();
    assert_eq!(seen, every, "each row shows under exactly one state");
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
            checked(&[PendingState::NeedsYou])
        ),
        vec!["candidate Unfit"]
    );
}

/// Done and Skipped rows are past identification, so a filter chosen on
/// Pending leaves them alone.
#[test]
fn the_pending_filter_leaves_done_and_skipped_alone() {
    let rows = every_kind();
    for &filter in PendingState::GROUPS.iter().copied().flatten() {
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
    let texted = |filters: &[PendingState], text: &str| {
        flattened(
            &rows,
            &ImportListView {
                pending_filters: checked(filters),
                filter_text: text.to_string(),
                ..ImportListView::default()
            },
        )
    };
    let needs_you = texted(&[PendingState::NeedsYou], "album");
    assert_eq!(
        sequence(&rows, &needs_you),
        vec!["candidate Several Tagged"],
        "of the Needs You rows only the drafted one reads Album"
    );
    assert_eq!(
        needs_you.summary.counts.pending, 14,
        "the tab counts are the whole queue's, whatever the list shows"
    );
    assert!(texted(&[PendingState::Identified], "nothing")
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
            pending_filters: checked(&[PendingState::Identified]),
            ..ImportListView::default()
        }),
        &key("Unidentified"),
    )
    .expect("the queue flattens")
    .expect("the candidate is found");
    assert_eq!(location.tab, TriageTab::Pending);
}

/// Selecting every identified row and importing them takes each one — a pick
/// among several releases and a track count that differs as much as an
/// auto-importable row — and a tags-only row selected beside them imports too.
#[test]
fn importing_a_selection_takes_every_row_with_a_draft_to_import() {
    let rows = every_kind();
    let identified = flattened(
        &rows,
        &ImportListView {
            pending_filters: checked(&[PendingState::Identified]),
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
            pending_filters: checked(&[PendingState::Identified]),
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
                pending_filters: checked(&[PendingState::Identifying]),
                ..ImportListView::default()
            },
            live_standings: [key("Identified"), key("Tagged")]
                .into_iter()
                .map(|key| (key, crate::import::LiveStanding::Identifying))
                .collect(),
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

/// What each state's rows offer at rest is their own: Needs You imports
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

    let needs_you = offers_of(PendingState::NeedsYou);
    assert_eq!(
        keys(&needs_you, CandidateAction::Import),
        vec![key("Several Tagged")],
        "the tags' draft imports; no draft, no import"
    );

    let lookup_error = offers_of(PendingState::LookupError);
    assert!(keys(&lookup_error, CandidateAction::Import).is_empty());
    let mut retried = keys(&lookup_error, CandidateAction::RetryIdentification);
    retried.sort();
    assert_eq!(
        retried,
        vec![key("Lookup Failed"), key("Unread")],
        "a release the lookup could not read is retried as a failed lookup is"
    );

    let import_error = offers_of(PendingState::ImportError);
    assert_eq!(
        keys(&import_error, CandidateAction::Import),
        vec![key("Failed Import")]
    );
}

/// Every state is in exactly one group, and the groups keep the order the
/// menu lists them in: identification's, then the import's, each ending with
/// its failure.
#[test]
fn every_state_is_in_one_group_in_the_menu_s_order() {
    assert_eq!(
        PendingState::GROUPS,
        [
            &[
                PendingState::NotLookedUp,
                PendingState::Identifying,
                PendingState::NeedsYou,
                PendingState::Identified,
                PendingState::Unmatched,
                PendingState::LookupError,
                PendingState::Error,
            ][..],
            &[PendingState::Importing, PendingState::ImportError][..],
        ]
    );
}

/// Checking every state is the same as checking none: the list shows every
/// row, with no state left narrowing it, in whichever order they were checked.
#[test]
fn checking_every_state_is_all() {
    let every: Vec<PendingState> = PendingState::GROUPS
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
    let two = checked(&[PendingState::NeedsYou, PendingState::LookupError]);
    let one = two.with_checked(PendingState::NeedsYou, false);
    assert_eq!(one, checked(&[PendingState::LookupError]));
    assert_eq!(
        one.with_checked(PendingState::LookupError, false),
        PendingFilters::default()
    );
    assert_eq!(
        PendingFilters::default().with_checked(PendingState::Identified, false),
        PendingFilters::default()
    );
}

/// Checking a state already checked changes nothing.
#[test]
fn checking_a_checked_state_again_changes_nothing() {
    let once = checked(&[PendingState::Importing]);
    assert_eq!(
        once.clone().with_checked(PendingState::Importing, true),
        once
    );
}

/// The checked states list in the menu's order, whatever order they were
/// checked in.
#[test]
fn the_checked_states_list_in_the_menu_s_order() {
    let filters = checked(&[
        PendingState::ImportError,
        PendingState::Identified,
        PendingState::Identifying,
    ]);
    assert_eq!(
        filters.into_iter().collect::<Vec<_>>(),
        vec![
            PendingState::Identifying,
            PendingState::Identified,
            PendingState::ImportError,
        ]
    );
}

/// Select All under several states selects exactly the rows in any of them —
/// the keys a view change keeps selected.
#[test]
fn select_all_under_several_states_takes_the_rows_any_of_them_keeps() {
    let rows = every_kind();
    let mut keys = shown_candidate_keys(
        &rows,
        &request(ImportListView {
            pending_filters: checked(&[PendingState::NeedsYou, PendingState::ImportError]),
            ..ImportListView::default()
        }),
    )
    .expect("the queue flattens");
    keys.sort();
    let mut expected: Vec<String> = [
        "Failed Import",
        "Nothing Found",
        "Nothing To Look Up",
        "Several",
        "Several Tagged",
    ]
    .into_iter()
    .map(key)
    .collect();
    expected.sort();
    assert_eq!(keys, expected);
}

/// A state what is running decides and one the tables decide each keep their
/// own rows.
#[test]
fn a_live_state_and_a_stored_state_keep_the_rows_of_either() {
    let rows = every_kind();
    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: ImportListView {
                pending_filters: checked(&[PendingState::Identifying, PendingState::LookupError]),
                ..ImportListView::default()
            },
            live_standings: [(key("Tagged"), crate::import::LiveStanding::Identifying)]
                .into_iter()
                .collect(),
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");
    let mut shown = sequence(&rows, &flat);
    shown.sort();
    assert_eq!(
        shown,
        vec![
            "candidate Lookup Failed",
            "candidate Tagged",
            "candidate Unread"
        ]
    );
}

/// While a state narrows the rows, every candidate something is running for
/// is in the state it puts it in; while none does, nothing running is read.
#[test]
fn live_standings_are_read_only_while_a_state_narrows() {
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
    let standings = |states: &[PendingState]| {
        checked(states)
            .live_standings(&facts)
            .into_iter()
            .collect::<Vec<_>>()
    };
    let running = vec![
        (
            "identifying".to_string(),
            crate::import::LiveStanding::Identifying,
        ),
        (
            "importing".to_string(),
            crate::import::LiveStanding::Importing,
        ),
    ];
    assert_eq!(standings(&[PendingState::Identifying]), running);
    assert_eq!(standings(&[PendingState::Identified]), running);
    assert!(standings(&[]).is_empty());
}

/// The summary names the states narrowing the tab on show, in the menu's
/// order: the view's own on Pending, and none on the tabs they leave alone.
#[test]
fn the_summary_names_the_states_narrowing_the_tab_on_show() {
    let rows = every_kind();
    let narrowing = |tab, filters: &[PendingState]| {
        flattened(
            &rows,
            &ImportListView {
                tab,
                pending_filters: checked(filters),
                ..ImportListView::default()
            },
        )
        .summary
        .pending_filters
        .into_iter()
        .collect::<Vec<_>>()
    };
    assert_eq!(
        narrowing(
            TriageTab::Pending,
            &[PendingState::ImportError, PendingState::NeedsYou]
        ),
        vec![PendingState::NeedsYou, PendingState::ImportError]
    );
    assert!(narrowing(TriageTab::Pending, &[]).is_empty());
    assert!(narrowing(TriageTab::Done, &[PendingState::NeedsYou]).is_empty());
    assert!(narrowing(TriageTab::Skipped, &[PendingState::NeedsYou]).is_empty());
}

/// While the text filter or a state narrows the tab on show, the summary
/// counts the tab's entries the list shows beside the tab's total; while
/// nothing narrows it, there is no count.
#[test]
fn a_narrowed_tab_counts_what_it_shows_of_its_total() {
    let mut rows = every_kind();
    rows.candidates.retain(|row| row.display_path != "Imported");
    rows.imported.clear();
    let narrowed = |tab, filters: &[PendingState], text: &str| {
        flattened(
            &rows,
            &ImportListView {
                tab,
                pending_filters: checked(filters),
                filter_text: text.to_string(),
                ..ImportListView::default()
            },
        )
        .summary
        .narrowed
    };
    assert_eq!(
        narrowed(
            TriageTab::Pending,
            &[PendingState::NeedsYou, PendingState::LookupError],
            ""
        ),
        Some(NarrowedCount {
            shown: 6,
            total: 14
        })
    );
    assert_eq!(
        narrowed(TriageTab::Pending, &[], "several"),
        Some(NarrowedCount {
            shown: 1,
            total: 14
        }),
        "only the row with no draft reads its folder's name"
    );
    assert_eq!(
        narrowed(TriageTab::Skipped, &[], "set aside"),
        Some(NarrowedCount { shown: 1, total: 1 })
    );
    assert_eq!(narrowed(TriageTab::Pending, &[], ""), None);
    assert_eq!(
        narrowed(TriageTab::Done, &[PendingState::NeedsYou], ""),
        None,
        "the states leave Done alone"
    );
}

/// The summary says which filter its count was made under, so a count is
/// only ever shown beside the filter it is for.
#[test]
fn the_summary_names_the_filter_its_count_is_for() {
    let mut rows = every_kind();
    rows.candidates.retain(|row| row.display_path != "Imported");
    rows.imported.clear();
    let view = ImportListView {
        filter_text: "album".to_string(),
        pending_filters: checked(&[PendingState::NeedsYou]),
        ..ImportListView::default()
    };
    let summary = flattened(&rows, &view).summary;
    assert_eq!(
        summary.narrowing,
        crate::import::ImportListNarrowing {
            tab: TriageTab::Pending,
            filter_text: "album".to_string(),
            pending_filters: checked(&[PendingState::NeedsYou]),
        }
    );
    assert!(summary.narrowed.is_some());
}
