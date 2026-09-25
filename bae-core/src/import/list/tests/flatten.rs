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
            folder: format!("{}/Release 1", host_root("/second")),
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

/// A row with no draft shows its folder's name and nothing else, so that is
/// what finds it; its path is not on screen and finds nothing.
#[test]
fn the_filter_finds_an_undrafted_row_by_its_folder_name_not_its_path() {
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
    assert!(sequence(&rows, &by_path).is_empty());
}

/// A drafted row shows its draft's title and artists — not the verdict's
/// lead, and not its folder — so those are what find it.
#[test]
fn the_filter_finds_a_drafted_row_by_the_draft_it_shows() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Folder"), candidate("Other")];
    rows.states.insert(
        "hash-Folder".to_string(),
        CandidateStateListRow {
            metadata_summary: Some(crate::import::TriageMetadataSummary {
                album_title: "Album".to_string(),
                album_artist_assignments: vec![crate::import::ArtistAssignment::named("Artist")],
            }),
            ..several_matches_state()
        },
    );

    let found = |needle: &str| {
        let flat = flattened(
            &rows,
            &ImportListView {
                filter_text: needle.to_string(),
                ..view(TriageTab::Pending)
            },
        );
        sequence(&rows, &flat)
    };
    for needle in ["album", "ARTIST"] {
        assert_eq!(
            found(needle),
            vec!["candidate Folder".to_string()],
            "{needle} is on the row"
        );
    }
    for needle in ["album title", "artist name", "folder"] {
        assert!(
            found(needle).is_empty(),
            "{needle} is the verdict's lead or the folder, which the row does not show"
        );
    }
}

/// A Done row shows the library release it became — its title, artist and
/// year — so those are what find it, and the candidate's own draft does not.
#[test]
fn the_filter_finds_a_done_row_by_its_library_release() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Folder"), candidate("Other")];
    rows.states
        .insert("hash-Folder".to_string(), ready_state("mb-1"));
    rows.states.insert(
        "hash-Other".to_string(),
        CandidateStateListRow {
            metadata_summary: Some(crate::import::TriageMetadataSummary {
                album_title: "Draft".to_string(),
                album_artist_assignments: vec![],
            }),
            ..ready_state("mb-2")
        },
    );
    imported(&mut rows, "Folder", "rel-1", 100);
    imported(&mut rows, "Other", "rel-2", 200);
    rows.imported_text = Some(std::collections::HashMap::from([
        (
            "rel-1".to_string(),
            crate::import::ImportedReleaseText {
                title: "Album".to_string(),
                artist: Some("Artist".to_string()),
                year: Some(1999),
            },
        ),
        (
            "rel-2".to_string(),
            crate::import::ImportedReleaseText {
                title: "Other Album".to_string(),
                artist: None,
                year: None,
            },
        ),
    ]));

    let found = |needle: &str| {
        let flat = flattened(
            &rows,
            &ImportListView {
                filter_text: needle.to_string(),
                ..view(TriageTab::Done)
            },
        );
        sequence(&rows, &flat)
    };
    for needle in ["artist", "1999"] {
        assert_eq!(
            found(needle),
            vec!["candidate Folder".to_string()],
            "{needle} is on the row"
        );
    }
    assert_eq!(found("album").len(), 2);
    assert!(found("draft").is_empty(), "the candidate's draft is not shown");
    assert!(found("folder").is_empty(), "the folder is not shown");
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

fn first_among(rows: &ImportQueueRows, view: ImportListView, keys: &[&str]) -> Option<String> {
    let keys = keys.iter().map(|display_path| key(display_path)).collect();
    first_candidate_among(rows, &request(view), &keys).expect("the queue orders")
}

/// The first of several keys is the one the queue reaches first, not the one
/// named first.
#[test]
fn the_first_candidate_among_keys_follows_the_queue_order() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release 1"), candidate("Release 2")];
    rows.states
        .insert("hash-Release 1".to_string(), ready_state("mb-1"));

    assert_eq!(
        first_among(&rows, view(TriageTab::Pending), &["Release 2"]),
        Some(key("Release 2"))
    );
    assert_eq!(
        first_among(
            &rows,
            view(TriageTab::Pending),
            &["Release 2", "Release 1"]
        ),
        Some(key("Release 1"))
    );
    assert_eq!(
        first_among(&rows, view(TriageTab::Pending), &["Elsewhere"]),
        None
    );
}

/// The queue's order is the whole queue's: a filter that hides the row, or
/// another tab on screen, does not.
#[test]
fn the_first_candidate_among_keys_ignores_the_filter_and_the_tab() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release 1"), candidate("Release 2")];

    let filtered = ImportListView {
        filter_text: "Release 1".to_string(),
        ..view(TriageTab::Pending)
    };
    assert_eq!(
        first_among(&rows, filtered, &["Release 2"]),
        Some(key("Release 2"))
    );
    assert_eq!(
        first_among(&rows, view(TriageTab::Done), &["Release 2"]),
        Some(key("Release 2"))
    );
}

#[test]
fn the_first_candidate_among_grouped_keys_is_the_groups_first_member() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Group/Release 1"), candidate("Group/Release 2")];

    assert_eq!(
        first_among(
            &rows,
            view(TriageTab::Pending),
            &["Group/Release 2", "Group/Release 1"]
        ),
        Some(key("Group/Release 1"))
    );
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
fn the_summary_carries_the_watched_folders() {
    let rows = queue();

    let flat = flattened(&rows, &view(TriageTab::Pending));

    assert_eq!(flat.summary.watched_folders.len(), 1);
}

// ── Applied metadata provenance is the answer ──────────────────────

/// A group header is where a folder read as several releases offers to be read
/// as one. A header that is only a path component the rows share offers
/// nothing: the folder that holds them — `Singles/Label`, the nearest one
/// nothing is stored for — is where that choice belongs, not every folder
/// above it.
#[test]
fn only_a_group_over_a_folder_read_as_several_offers_to_combine() {
    let mut rows = queue();
    rows.candidates = vec![
        candidate("Box/Disc 1"),
        candidate("Box/Disc 2"),
        candidate("Singles/Label/Series/One"),
        candidate("Singles/Label/Series/Two"),
    ];
    rows.folder_readings.insert((root(), "Box".to_string()), false);
    rows.folder_readings
        .insert((root(), "Singles/Label/Series".to_string()), false);

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
