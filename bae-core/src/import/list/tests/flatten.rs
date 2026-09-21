//! Ordering, grouping, tabs, the filter, and the chrome.

use super::*;

#[test]
fn rows_order_by_watched_root_then_natural_path() {
    let mut rows = queue();
    rows.watched_folders.push(WatchedFolder {
        path: host_root("/second"),
        name: "second".to_string(),
    });
    rows.candidates = vec![
        candidate("Release 10"),
        candidate("Release 2"),
        ScanCandidateListRow {
            watched_folder_path: host_root("/second"),
            path: format!("{}/Release 1", host_root("/second")),
            ..candidate("Release 1")
        },
    ];

    let flat = flattened(&rows, &view(TriageTab::Pending));

    assert_eq!(
        sequence(&rows, &flat),
        vec![
            "candidate Release 2".to_string(),
            "candidate Release 10".to_string(),
            "candidate Release 1".to_string(),
        ],
        "natural order within a root, and the roots in their stored order"
    );
    assert_eq!(flat.items.len() as u64, 3);
}

#[test]
fn the_descending_order_reverses_the_whole_queue() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release 2"), candidate("Release 10")];

    let flat = flattened(
        &rows,
        &ImportListView {
            order: ImportListOrder::PathDescending,
            ..view(TriageTab::Pending)
        },
    );

    assert_eq!(
        sequence(&rows, &flat),
        vec![
            "candidate Release 10".to_string(),
            "candidate Release 2".to_string(),
        ]
    );
}

/// A tentative candidate is a release approximation the scan found before it
/// knew what enclosed it: not a row, not a count, and not a group.
#[test]
fn a_tentative_candidate_is_neither_a_row_nor_a_count() {
    let mut rows = queue();
    rows.candidates = vec![tentative("Box/CD1")];

    let flat = flattened(&rows, &view(TriageTab::Pending));

    assert!(flat.items.is_empty());
    assert_eq!(flat.summary.counts, TriageTabCounts::default());
    assert!(flat.summary.group_keys.is_empty());
}

#[test]
fn a_release_counts_pending_and_an_invalid_folder_counts_skipped() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release"), invalid("Broken")];

    let flat = flattened(&rows, &view(TriageTab::Pending));

    assert_eq!(
        flat.summary.counts,
        TriageTabCounts {
            pending: 1,
            done: 0,
            skipped: 1,
        }
    );
    assert_eq!(
        sequence(&rows, &flat),
        vec!["candidate Release".to_string()]
    );
    let skipped = flattened(&rows, &view(TriageTab::Skipped));
    assert_eq!(
        sequence(&rows, &skipped),
        vec!["invalid Broken".to_string()]
    );
}

#[test]
fn a_collapsed_group_keeps_its_header_and_drops_its_entries() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Group/Release 1"), candidate("Group/Release 2")];
    let group = FolderReleaseDecisionKey {
        watched_folder_path: root(),
        relative_folder_path: "Group".to_string(),
    };

    let flat = flattened(
        &rows,
        &ImportListView {
            collapsed_groups: BTreeSet::from([group]),
            ..view(TriageTab::Pending)
        },
    );

    assert_eq!(sequence(&rows, &flat), vec!["group Group".to_string()]);
    assert!(!flat.headers[0].expanded);
    assert_eq!(
        flat.headers[0].entry_count, 2,
        "the header still says how much is folded away"
    );
}

#[test]
fn only_entries_beneath_a_group_header_are_group_members() {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Group/Release 1"),
        candidate("Group/Release 2"),
        candidate("Ungrouped"),
    ];

    let flat = flattened(&rows, &view(TriageTab::Pending));

    let memberships = flat
        .items
        .iter()
        .filter_map(|item| match item {
            ItemRef::Candidate {
                is_group_member, ..
            } => Some(*is_group_member),
            ItemRef::Header(_) | ItemRef::Invalid { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(memberships, vec![true, true, false]);
}

#[test]
fn the_filter_matches_the_folder_name_and_the_display_path() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Group/Wanted"), candidate("Other")];

    let by_name = flattened(
        &rows,
        &ImportListView {
            filter_text: "want".to_string(),
            ..view(TriageTab::Pending)
        },
    );
    assert_eq!(
        sequence(&rows, &by_name),
        vec![
            "group Group".to_string(),
            "candidate Group/Wanted".to_string()
        ],
        "an emptied group drops its header, a matching one keeps it"
    );

    let by_path = flattened(
        &rows,
        &ImportListView {
            filter_text: "group/".to_string(),
            ..view(TriageTab::Pending)
        },
    );
    assert_eq!(
        sequence(&rows, &by_path),
        vec![
            "group Group".to_string(),
            "candidate Group/Wanted".to_string()
        ]
    );
}

#[test]
fn the_filter_matches_the_lead_match_title_and_artist() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release"), candidate("Other")];
    rows.states
        .insert("hash-Release".to_string(), several_matches_state());

    for needle in ["album title", "artist name"] {
        let flat = flattened(
            &rows,
            &ImportListView {
                filter_text: needle.to_string(),
                ..view(TriageTab::Pending)
            },
        );
        assert_eq!(
            sequence(&rows, &flat),
            vec!["candidate Release".to_string()],
            "{needle} matches the lead match's columns"
        );
    }
}

#[test]
fn the_filter_matches_an_invalid_folder() {
    let mut rows = queue();
    rows.candidates = vec![invalid("Broken")];

    let broken = flattened(
        &rows,
        &ImportListView {
            filter_text: "broken".to_string(),
            ..view(TriageTab::Skipped)
        },
    );
    assert_eq!(sequence(&rows, &broken), vec!["invalid Broken".to_string()]);
}

/// A group header asks how the folder under it is read, and offers to read it
/// the other way. Neither question survives the import, so Done and Skipped are
/// flat lists of releases and only Pending groups.
#[test]
fn only_pending_groups_its_rows() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Group/Release 1"), candidate("Group/Release 2")];
    imported(&mut rows, "Group/Release 1", "rel-1", 100);
    rows.skipped.insert((root(), "Group/Release 2".to_string()));

    let pending = flattened(&rows, &view(TriageTab::Pending));
    assert!(
        pending.items.is_empty(),
        "both rows left Pending: {:?}",
        sequence(&rows, &pending)
    );

    let done = flattened(&rows, &view(TriageTab::Done));
    assert_eq!(
        sequence(&rows, &done),
        vec!["candidate Group/Release 1".to_string()]
    );

    let skipped = flattened(&rows, &view(TriageTab::Skipped));
    assert_eq!(
        sequence(&rows, &skipped),
        vec!["candidate Group/Release 2".to_string()]
    );

    assert!(
        done.summary.group_keys.is_empty(),
        "a folder whose rows are all past import has no header to retain state against"
    );
}

/// Done is ordered by what the cloud is still doing, then by when the import
/// happened — newest first. The path decides nothing: a folder in the library
/// is finished, and the alphabet answers neither question.
#[test]
fn done_orders_by_upload_then_newest_import_first() {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("A settled old"),
        candidate("B working"),
        candidate("C settled new"),
        candidate("D queued"),
    ];
    imported(&mut rows, "A settled old", "rel-settled-old", 100);
    imported(&mut rows, "B working", "rel-working", 200);
    imported(&mut rows, "C settled new", "rel-settled-new", 300);
    imported(&mut rows, "D queued", "rel-queued", 400);

    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: view(TriageTab::Done),
            upload_standing: BTreeMap::from([
                ("rel-working".to_string(), UploadStanding::Working),
                ("rel-queued".to_string(), UploadStanding::Queued),
            ]),
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");

    assert_eq!(
        sequence(&rows, &flat),
        vec![
            "candidate B working".to_string(),
            "candidate D queued".to_string(),
            "candidate C settled new".to_string(),
            "candidate A settled old".to_string(),
        ]
    );
}

#[test]
fn the_first_unidentified_row_has_its_position_in_the_current_view() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release 1"), candidate("Release 2")];
    rows.states
        .insert("hash-Release 1".to_string(), ready_state("mb-1"));

    let flat = flattened_queued(&rows, view(TriageTab::Pending), &["Release 2"]);

    let target = flat
        .summary
        .first_unidentified
        .expect("the queue has an unidentified row");
    assert_eq!(target.candidate_key, key("Release 2"));
    assert_eq!(target.stable_key, format!("candidate:{}", key("Release 2")));
    assert_eq!(target.group_key, None);
    assert_eq!(target.visible_position, Some(1));
}

#[test]
fn the_first_unidentified_position_is_absent_outside_the_current_view() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release 1"), candidate("Release 2")];
    rows.states
        .insert("hash-Release 1".to_string(), ready_state("mb-1"));

    let filtered = flattened_queued(
        &rows,
        ImportListView {
            filter_text: "Release 1".to_string(),
            ..view(TriageTab::Pending)
        },
        &["Release 2"],
    );
    let other_tab = flattened_queued(&rows, view(TriageTab::Done), &["Release 2"]);

    assert_eq!(
        filtered
            .summary
            .first_unidentified
            .expect("identification still has a target")
            .visible_position,
        None
    );
    assert_eq!(
        other_tab
            .summary
            .first_unidentified
            .expect("identification still has a target")
            .visible_position,
        None
    );
}

#[test]
fn the_first_grouped_unidentified_position_follows_its_header() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Group/Release 1"), candidate("Group/Release 2")];

    let flat = flattened_queued(
        &rows,
        view(TriageTab::Pending),
        &["Group/Release 1", "Group/Release 2"],
    );
    let target = flat
        .summary
        .first_unidentified
        .expect("the queue has an unidentified row");

    assert_eq!(target.candidate_key, key("Group/Release 1"));
    assert_eq!(
        target
            .group_key
            .expect("the candidate is grouped")
            .relative_folder_path,
        "Group"
    );
    assert_eq!(target.visible_position, Some(1));
}

#[test]
fn candidate_location_opens_only_its_group() {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Earlier/Release 1"),
        candidate("Earlier/Release 2"),
        candidate("Target/Release 1"),
    ];
    let earlier = FolderReleaseDecisionKey {
        watched_folder_path: root(),
        relative_folder_path: "Earlier".to_string(),
    };
    let target = FolderReleaseDecisionKey {
        watched_folder_path: root(),
        relative_folder_path: "Target".to_string(),
    };
    let request = ImportListRequest {
        view: ImportListView {
            filter_text: "does not match".to_string(),
            collapsed_groups: BTreeSet::from([earlier.clone(), target.clone()]),
            ..view(TriageTab::Pending)
        },
        ..ImportListRequest::default()
    };

    let location = locate_candidate(&rows, &request, &key("Target/Release 1"))
        .expect("the queue locates")
        .expect("the candidate is in Pending");

    assert_eq!(location.tab, TriageTab::Pending);
    assert_eq!(location.group_key, Some(target));
    assert_eq!(location.visible_position, 2);
}

#[test]
fn candidate_location_follows_an_import_from_pending_to_done() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));
    imported(&mut rows, "Release", "mb-1", 100);

    let location = locate_candidate(&rows, &request(view(TriageTab::Pending)), &key("Release"))
        .expect("the queue locates")
        .expect("the imported candidate is in Done");
    assert_eq!(location.tab, TriageTab::Done);
    assert_eq!(location.group_key, None);
    assert_eq!(location.visible_position, 0);
}

/// The Ready set is filtered — it is what a bulk import of what is on screen
/// would act on — while the counts and the group keys are the whole queue's.
#[test]
fn the_summary_filters_ready_and_keeps_the_counts_whole() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Group/Wanted"), candidate("Group/Other")];
    rows.states
        .insert("hash-Group/Wanted".to_string(), ready_state("mb-1"));
    rows.states
        .insert("hash-Group/Other".to_string(), ready_state("mb-2"));

    let flat = flattened(
        &rows,
        &ImportListView {
            filter_text: "wanted".to_string(),
            ..view(TriageTab::Pending)
        },
    );

    assert_eq!(flat.summary.counts.pending, 2);
    assert_eq!(
        flat.summary
            .ready
            .iter()
            .map(|row| row.candidate_key.clone())
            .collect::<Vec<_>>(),
        vec![key("Group/Wanted")]
    );
    assert_eq!(flat.summary.group_keys.len(), 1);
}

#[test]
fn the_summary_carries_the_watched_folders_and_their_scan_statuses() {
    let rows = queue();

    let flat = flattened(&rows, &view(TriageTab::Pending));

    assert_eq!(flat.summary.watched_folders.len(), 1);
    assert_eq!(flat.summary.folder_scan_statuses.len(), 1);
}

// ── Applied metadata provenance is the answer ──────────────────────

/// A group header is where a folder read as several releases offers to be read
/// as one. A header that is only a path component the rows share offers
/// nothing — there is no such folder to combine, and asking would be a
/// question with no answer behind it.
#[test]
fn only_a_group_over_a_folder_read_as_several_offers_to_combine() {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Box/Disc 1"),
        candidate("Box/Disc 2"),
        candidate("Singles/One"),
        candidate("Singles/Two"),
    ];
    rows.separated_folders.insert((root(), "Box".to_string()));

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let group_of = |name: &str| {
        flat.headers
            .iter()
            .find(|header| header.group.name == name)
            .unwrap_or_else(|| panic!("a header for {name}"))
            .group
            .clone()
    };
    assert!(group_of("Box").combinable);
    assert!(!group_of("Singles").combinable);
}
