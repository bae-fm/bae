//! Filtering Found's rows by one filter entry, what each entry counts, and
//! importing a selection of them.

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
        release_link: Some(release_link_to("mb-2")),
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
                release_link: None,
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
                release_link: None,
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
        crate::import::ImportFailureReason::error("Import failed"),
    );
    imported(&mut rows, "Imported", "release-1", 1);
    rows.skipped.insert((root(), "Set Aside".to_string()));
    rows
}

/// What is running for three of `every_kind`'s candidates, as the request
/// carries it: a run for Several, an import of Identified, and a run whose
/// answer bae could not store for Tagged.
fn running() -> std::collections::BTreeMap<String, crate::import::LiveStanding> {
    [
        (key("Several"), crate::import::LiveStanding::Identifying),
        (key("Identified"), crate::import::LiveStanding::Importing),
        (
            key("Tagged"),
            crate::import::LiveStanding::Error {
                failure: crate::signals::InternalFailure {
                    detail: "writing the answer: the disk is full".to_string(),
                },
            },
        ),
    ]
    .into_iter()
    .collect()
}

/// `view` flattened with `every_kind`'s candidates running as `running` has
/// them.
fn flattened_running(rows: &ImportQueueRows, view: ImportListView) -> Flattened {
    flatten(
        rows,
        &ImportListRequest {
            view,
            live_standings: running(),
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens")
}

fn under(filter: PendingFilter) -> ImportListView {
    ImportListView {
        pending_filter: filter,
        ..ImportListView::default()
    }
}

fn shown(rows: &ImportQueueRows, tab: TriageTab, filter: PendingFilter) -> Vec<String> {
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

fn sorted(flat: &Flattened, rows: &ImportQueueRows) -> Vec<String> {
    let mut shown = sequence(rows, flat);
    shown.sort();
    shown
}

/// Each state is under the one entry past All the menu puts it under, and
/// All holds every state.
#[test]
fn each_state_is_under_one_entry_past_all() {
    let cases = [
        (PendingState::NeedsYou, PendingFilter::NeedsYou),
        (PendingState::LookupError, PendingFilter::NeedsYou),
        (PendingState::Error, PendingFilter::NeedsYou),
        (PendingState::ImportError, PendingFilter::NeedsYou),
        (PendingState::Identifying, PendingFilter::InProgress),
        (PendingState::Importing, PendingFilter::InProgress),
        (PendingState::Identified, PendingFilter::Identified),
        (PendingState::Unmatched, PendingFilter::Unmatched),
        (PendingState::NotLookedUp, PendingFilter::NotLookedUp),
    ];
    for (state, entry) in cases {
        let holding: Vec<PendingFilter> = PendingFilter::ENTRIES
            .into_iter()
            .filter(|filter| filter.holds(state))
            .collect();
        assert_eq!(holding, vec![PendingFilter::All, entry], "{state:?}");
    }
}

/// The entries a list read counts, with nothing running.
fn entries_at_rest(flat: &Flattened) -> Vec<PendingFilterEntry> {
    flat.found_states
        .entries(&std::collections::BTreeMap::new())
}

/// The entries list in the menu's order, the counted ones as much as the
/// menu's own.
#[test]
fn the_entries_list_in_the_menu_s_order() {
    let order = [
        PendingFilter::All,
        PendingFilter::NeedsYou,
        PendingFilter::InProgress,
        PendingFilter::Identified,
        PendingFilter::Unmatched,
        PendingFilter::NotLookedUp,
    ];
    assert_eq!(PendingFilter::ENTRIES, order);
    assert_eq!(
        entries_at_rest(&flattened(&every_kind(), &view(TriageTab::Pending)))
            .iter()
            .map(|entry| entry.filter)
            .collect::<Vec<_>>(),
        order
    );
}

/// Each entry shows exactly the rows in the states it covers, with what is
/// running for a candidate deciding its state over the tables.
#[test]
fn each_entry_shows_exactly_the_rows_of_its_states() {
    let rows = every_kind();
    let cases = [
        (
            PendingFilter::NeedsYou,
            vec![
                "candidate Broke",
                "candidate Failed Import",
                "candidate Lookup Failed",
                "candidate Nothing Found",
                "candidate Nothing To Look Up",
                "candidate Several Tagged",
                "candidate Tagged",
                "candidate Unread",
            ],
        ),
        (
            PendingFilter::InProgress,
            vec!["candidate Identified", "candidate Several"],
        ),
        (
            PendingFilter::Identified,
            vec![
                "candidate Picked Among Several",
                "candidate Track Count Differs",
            ],
        ),
        (PendingFilter::Unmatched, vec!["candidate Pick Set Aside"]),
        (PendingFilter::NotLookedUp, vec!["candidate Unidentified"]),
    ];
    for (filter, expected) in cases {
        assert_eq!(
            sorted(&flattened_running(&rows, under(filter)), &rows),
            expected,
            "{filter:?}"
        );
    }
    assert_eq!(
        sorted(&flattened_running(&rows, under(PendingFilter::All)), &rows),
        shown(&rows, TriageTab::Pending, PendingFilter::All),
        "All shows every row, whatever is running"
    );
}

/// The entries past All split Found's rows between them: each row shows
/// under exactly one, and together they show what All does.
#[test]
fn every_row_shows_under_exactly_one_entry_past_all() {
    let rows = every_kind();
    let mut seen: Vec<String> = PendingFilter::ENTRIES
        .into_iter()
        .filter(|&filter| filter != PendingFilter::All)
        .flat_map(|filter| sequence(&rows, &flattened_running(&rows, under(filter))))
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        sorted(&flattened_running(&rows, under(PendingFilter::All)), &rows)
    );
}

/// Each entry counts the rows it holds, from a read of the list under All —
/// which reads nothing of what is running — with what is running when the
/// menu opens deciding a row's state over the tables: the count is the rows
/// the list shows under that entry, All's is Found's total, and an entry
/// holding a row can be chosen.
#[test]
fn each_entry_counts_the_rows_it_holds() {
    let rows = every_kind();
    let read = flattened(&rows, &view(TriageTab::Pending));
    let entries = read.found_states.entries(&running());
    let counts: Vec<(PendingFilter, u32)> = entries
        .iter()
        .map(|entry| (entry.filter, entry.count))
        .collect();
    assert_eq!(
        counts,
        vec![
            (PendingFilter::All, 14),
            (PendingFilter::NeedsYou, 8),
            (PendingFilter::InProgress, 2),
            (PendingFilter::Identified, 2),
            (PendingFilter::Unmatched, 1),
            (PendingFilter::NotLookedUp, 1),
        ]
    );
    assert_eq!(read.summary.counts.pending, 14);
    assert!(entries.iter().all(|entry| entry.selectable));
    for entry in &entries {
        let flat = flattened_running(&rows, under(entry.filter));
        assert_eq!(flat.items.len() as u32, entry.count, "{:?}", entry.filter);
        if entry.filter != PendingFilter::All {
            assert_eq!(
                flat.summary.narrowed,
                Some(NarrowedCount {
                    shown: entry.count,
                    total: 14
                }),
                "the list's count under {:?} is the entry's",
                entry.filter
            );
        }
    }
}

/// A read places Found's rows in the states the tables put them in whatever
/// the list shows: the tab on show, the text typed, the entry chosen and what
/// is running leave them alone.
#[test]
fn a_read_places_found_s_rows_whatever_the_list_shows() {
    let mut rows = every_kind();
    // The text filter reads a Done row's library text, which these rows
    // leave out.
    rows.candidates.retain(|row| row.display_path != "Imported");
    rows.imported.clear();
    let at_rest = flattened(&rows, &view(TriageTab::Pending)).found_states;
    let placed = |view: ImportListView| flattened_running(&rows, view).found_states;
    assert_eq!(placed(under(PendingFilter::All)), at_rest);
    assert_eq!(placed(under(PendingFilter::Unmatched)), at_rest);
    assert_eq!(placed(view(TriageTab::Done)), at_rest);
    assert_eq!(placed(view(TriageTab::Skipped)), at_rest);
    assert_eq!(
        placed(ImportListView {
            filter_text: "several".to_string(),
            ..under(PendingFilter::NeedsYou)
        }),
        at_rest
    );
}

/// An entry holding no row cannot be chosen; the others can.
#[test]
fn an_entry_holding_no_row_cannot_be_chosen() {
    let entries = entries_at_rest(&flattened(&every_kind(), &view(TriageTab::Pending)));
    let in_progress = entries
        .iter()
        .find(|entry| entry.filter == PendingFilter::InProgress)
        .expect("In Progress is an entry");
    assert_eq!(in_progress.count, 0, "nothing is running");
    assert!(!in_progress.selectable);
    assert!(entries
        .iter()
        .filter(|entry| entry.filter != PendingFilter::InProgress)
        .all(|entry| entry.selectable));

    assert!(entries_at_rest(&flattened(&queue(), &view(TriageTab::Pending)))
        .iter()
        .all(|entry| entry.count == 0 && !entry.selectable));
}

/// An entry chosen while it held rows stays chosen once it holds none: the
/// list shows nothing under it, and says so against Found's total.
#[test]
fn a_chosen_entry_holding_no_row_stays_chosen() {
    let rows = every_kind();
    let in_progress = flattened_running(&rows, under(PendingFilter::InProgress));
    assert_eq!(in_progress.items.len(), 2);

    let ended = flattened(&rows, &under(PendingFilter::InProgress));
    assert!(ended.items.is_empty());
    assert_eq!(ended.summary.narrowing.pending_filter, PendingFilter::InProgress);
    assert_eq!(
        ended.summary.narrowed,
        Some(NarrowedCount {
            shown: 0,
            total: 14
        })
    );
}

/// A sole release that lists no tracks is not picked for the folder, so the
/// row waits on the person as several releases do.
#[test]
fn a_sole_release_that_lists_no_tracks_needs_you() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Unchecked")];
    rows.states.insert(
        "hash-Unchecked".to_string(),
        with_verdict(several_matches_state(), |verdict| {
            verdict.pressing_count = 1;
            verdict
                .lead
                .as_mut()
                .expect("the state leads with a match")
                .source_tracks = Some(crate::import::search::SourceTracks::Nothing);
        }),
    );
    assert_eq!(
        shown(&rows, TriageTab::Pending, PendingFilter::NeedsYou),
        vec!["candidate Unchecked"]
    );
}

/// Done and Skipped rows are past identification, so an entry chosen on
/// Found leaves them alone.
#[test]
fn the_pending_filter_leaves_done_and_skipped_alone() {
    let rows = every_kind();
    for filter in PendingFilter::ENTRIES {
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

/// The entry composes with the text filter: both have to keep a row.
#[test]
fn the_text_filter_composes_with_the_pending_filter() {
    let mut rows = every_kind();
    rows.candidates.retain(|row| row.display_path != "Imported");
    rows.imported.clear();
    let texted = |filter: PendingFilter, text: &str| {
        flattened(
            &rows,
            &ImportListView {
                pending_filter: filter,
                filter_text: text.to_string(),
                ..ImportListView::default()
            },
        )
    };
    let needs_you = texted(PendingFilter::NeedsYou, "album");
    assert_eq!(
        sorted(&needs_you, &rows),
        vec!["candidate Failed Import", "candidate Several Tagged"],
        "of the Needs You rows only the drafted ones read Album"
    );
    assert_eq!(
        needs_you.summary.counts.pending, 14,
        "the tab counts are the whole queue's, whatever the list shows"
    );
    assert!(texted(PendingFilter::Identified, "nothing").items.is_empty());
}

/// Locating a candidate clears the filters, so a row the entry hides is
/// still found where it sits.
#[test]
fn locating_a_candidate_ignores_the_pending_filter() {
    let rows = every_kind();
    let location = locate_candidate(
        &rows,
        &request(under(PendingFilter::Identified)),
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
    let identified = flattened(&rows, &under(PendingFilter::Identified));
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
    let identified = flattened(&rows, &under(PendingFilter::Identified));
    let members = select_all(&identified, &TriageRuntimeFacts::default());
    let shown: Vec<String> = members.iter().map(|m| m.candidate_key.clone()).collect();

    assert_eq!(keys_for(&members, CandidateAction::Import), shown);
}

/// Rows being identified offer no import: selecting everything In Progress
/// shows while runs go offers to cancel them instead.
#[test]
fn selecting_all_rows_in_progress_offers_their_cancel_and_no_import() {
    let rows = every_kind();
    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: under(PendingFilter::InProgress),
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
    assert_eq!(members.len(), 2, "the entry shows the rows being identified");

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

/// What Needs You's rows offer at rest is each row's own: a draft imports,
/// a failed import imports again, and a lookup that failed or that bae broke
/// on is retried.
#[test]
fn needs_you_rows_offer_what_their_drafts_and_lookups_allow() {
    let rows = every_kind();
    let needs_you = select_all(
        &flattened(&rows, &under(PendingFilter::NeedsYou)),
        &TriageRuntimeFacts::default(),
    );
    let keys = |action| {
        let mut keys = keys_for(&needs_you, action);
        keys.sort();
        keys
    };
    assert_eq!(
        keys(CandidateAction::Import),
        vec![key("Failed Import"), key("Several Tagged")],
        "a draft imports and a failed import imports again; no draft, no import"
    );
    assert_eq!(
        keys(CandidateAction::RetryIdentification),
        vec![key("Broke"), key("Lookup Failed"), key("Unread")],
        "a release the lookup could not read is retried as a failed lookup is"
    );
}

/// While an entry past All narrows the rows, every candidate something is
/// running for is in the state it puts it in; under All, nothing running is
/// read.
#[test]
fn live_standings_are_read_only_while_an_entry_narrows() {
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
    for filter in PendingFilter::ENTRIES {
        let standings: Vec<_> = filter.live_standings(&facts).into_iter().collect();
        if filter == PendingFilter::All {
            assert!(standings.is_empty(), "All read what is running");
        } else {
            assert_eq!(standings, running, "{filter:?}");
        }
    }
}

/// While the text filter or an entry narrows the tab on show, the summary
/// counts the tab's entries the list shows beside the tab's total; while
/// nothing narrows it, there is no count.
#[test]
fn a_narrowed_tab_counts_what_it_shows_of_its_total() {
    let mut rows = every_kind();
    rows.candidates.retain(|row| row.display_path != "Imported");
    rows.imported.clear();
    let narrowed = |tab, filter: PendingFilter, text: &str| {
        flattened(
            &rows,
            &ImportListView {
                tab,
                pending_filter: filter,
                filter_text: text.to_string(),
                ..ImportListView::default()
            },
        )
        .summary
        .narrowed
    };
    assert_eq!(
        narrowed(TriageTab::Pending, PendingFilter::NeedsYou, ""),
        Some(NarrowedCount {
            shown: 8,
            total: 14
        })
    );
    assert_eq!(
        narrowed(TriageTab::Pending, PendingFilter::All, "several"),
        Some(NarrowedCount {
            shown: 1,
            total: 14
        }),
        "only the row with no draft reads its folder's name"
    );
    assert_eq!(
        narrowed(TriageTab::Skipped, PendingFilter::All, "set aside"),
        Some(NarrowedCount { shown: 1, total: 1 })
    );
    assert_eq!(narrowed(TriageTab::Pending, PendingFilter::All, ""), None);
    assert_eq!(
        narrowed(TriageTab::Done, PendingFilter::NeedsYou, ""),
        None,
        "the entry leaves Done alone"
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
        ..under(PendingFilter::NeedsYou)
    };
    let summary = flattened(&rows, &view).summary;
    assert_eq!(
        summary.narrowing,
        crate::import::ImportListNarrowing {
            tab: TriageTab::Pending,
            filter_text: "album".to_string(),
            pending_filter: PendingFilter::NeedsYou,
        }
    );
    assert!(summary.narrowed.is_some());
}

/// The summary says where the first selected row sits in the list it
/// shows, which a list narrowed anew keeps in view; none while no selected
/// row is in it.
#[test]
fn the_summary_places_the_first_selected_row_in_the_list_it_shows() {
    let mut rows = every_kind();
    rows.selected.insert(key("Unidentified"));
    let not_looked_up = flattened(&rows, &under(PendingFilter::NotLookedUp));
    let position = not_looked_up
        .summary
        .first_selected_position
        .expect("the selected row is shown") as usize;
    match not_looked_up.items[position] {
        ItemRef::Candidate { index, .. } => {
            assert_eq!(not_looked_up.rows[index].row.candidate_key, key("Unidentified"))
        }
        other => panic!("the position names a header or invalid row: {other:?}"),
    }
    let needs_you = flattened(&rows, &under(PendingFilter::NeedsYou));
    assert_eq!(needs_you.summary.first_selected_position, None);
}
