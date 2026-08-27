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

#[test]
fn a_stored_verdict_that_classifies_ready_makes_a_selectable_row() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert!(row.selectable);
    assert_eq!(
        flat.summary.ready,
        vec![ReadyRowRef {
            candidate_key: key("Release"),
            metadata_seed: MetadataSeed::ExternalRelease {
                source: MetadataSource::MusicBrainz,
                release_id: "mb-1".to_string(),
            },
            cover_thumbnail_url: Some("https://example.test/thumb.jpg".to_string()),
        }]
    );
}

/// A verdict derived from a file shape the candidate has moved past is not the
/// candidate's answer: the row is still waiting on identification.
#[test]
fn a_verdict_at_a_stale_edit_revision_is_not_the_row_s_answer() {
    let mut rows = queue();
    rows.candidates = vec![ScanCandidateListRow {
        file_edit_revision: 2,
        ..candidate("Release")
    }];
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));

    assert!(matches!(
        row_for(&flat, "Release").placement,
        TriagePlacement::NeedsYou {
            reason: crate::import::NeedsYouReason::StillIdentifying { .. },
            ..
        }
    ));
}

#[test]
fn an_imported_content_hash_puts_its_row_in_done() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.imported.insert(
        "hash-Release".to_string(),
        ImportedRelease {
            release_id: "rel-1".to_string(),
            album_id: "alb-1".to_string(),
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Done));

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Done);
    assert!(matches!(
        row.import_status,
        Some(TriageImportStatus::Complete { .. })
    ));
    assert_eq!(flat.summary.counts.done, 1);
}

#[test]
fn a_skipped_candidate_lands_in_skipped() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.skipped.insert((root(), "Release".to_string()));

    let flat = flattened(&rows, &view(TriageTab::Skipped));

    assert_eq!(
        row_for(&flat, "Release").placement,
        TriagePlacement::Skipped
    );
    assert_eq!(flat.summary.counts.skipped, 1);
}

/// The three runtime facts a placement reads, each on its own.
#[test]
fn a_claimed_import_places_the_row_as_importing() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));
    let facts = BTreeMap::from([(
        key("Release"),
        TriageRuntimeFacts {
            phase: IdentifyPhase::Queued,
            importing: true,
        },
    )]);

    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: view(TriageTab::Pending),
            runtime_facts: facts,
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");

    assert_eq!(
        row_for(&flat, "Release").placement,
        TriagePlacement::Importing
    );
    assert_eq!(flat.summary.counts.pending, 1);
    assert!(flat.summary.ready.is_empty());
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

/// The failure is a row, so it survives the session that produced it: a
/// relaunched queue still says why the attempt failed. It stays Pending —
/// nothing was imported, and the folder is waiting on another attempt — and it
/// is not Ready, so a bulk import does not sweep it back up.
#[test]
fn a_failed_import_stays_pending_and_reads_its_error_from_its_row() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));
    rows.failures
        .insert("hash-Release".to_string(), "boom".to_string());

    let flat = flattened(&rows, &view(TriageTab::Pending));

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Failed);
    assert_eq!(
        row.import_status,
        Some(TriageImportStatus::Error {
            error: "boom".to_string()
        })
    );
    assert_eq!(flat.summary.counts.pending, 1);
    assert_eq!(flat.summary.counts.done, 0);
    assert!(
        !row.selectable,
        "the attempt that just failed is not what makes a row safe to sweep up"
    );
    assert!(flat.summary.ready.is_empty());
}

/// Retrying is the ordinary import: the run claims the candidate, and the row
/// leaves the failure for Importing without the failure row being cleared
/// first. When it lands, the release outranks the leftover failure and the row
/// is Done.
#[test]
fn retrying_a_failed_import_moves_it_through_importing_to_done() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));
    rows.failures
        .insert("hash-Release".to_string(), "boom".to_string());
    let running = BTreeMap::from([(
        key("Release"),
        TriageRuntimeFacts {
            phase: IdentifyPhase::Queued,
            importing: true,
        },
    )]);

    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: view(TriageTab::Pending),
            runtime_facts: running,
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");
    assert_eq!(
        row_for(&flat, "Release").placement,
        TriagePlacement::Importing
    );
    assert_eq!(flat.summary.counts.pending, 1);

    rows.imported.insert(
        "hash-Release".to_string(),
        ImportedRelease {
            release_id: "rel-1".to_string(),
            album_id: "alb-1".to_string(),
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Done));
    assert_eq!(row_for(&flat, "Release").placement, TriagePlacement::Done);
    assert_eq!(flat.summary.counts.done, 1);
    assert_eq!(flat.summary.counts.pending, 0);
}

/// A release for this content hash means an attempt already succeeded, so a
/// leftover failure row is behind it.
#[test]
fn an_imported_release_outranks_a_leftover_failure() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.failures
        .insert("hash-Release".to_string(), "boom".to_string());
    rows.imported.insert(
        "hash-Release".to_string(),
        ImportedRelease {
            release_id: "rel-1".to_string(),
            album_id: "alb-1".to_string(),
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Done));

    assert!(matches!(
        row_for(&flat, "Release").import_status,
        Some(TriageImportStatus::Complete { .. })
    ));
}

/// A claimed import outranks both stored answers: the folder is not in the
/// library until the running attempt says it is.
#[test]
fn a_running_import_outranks_the_release_it_has_not_finished_writing() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.imported.insert(
        "hash-Release".to_string(),
        ImportedRelease {
            release_id: "rel-1".to_string(),
            album_id: "alb-1".to_string(),
        },
    );
    let facts = BTreeMap::from([(
        key("Release"),
        TriageRuntimeFacts {
            phase: IdentifyPhase::Queued,
            importing: true,
        },
    )]);

    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: view(TriageTab::Pending),
            runtime_facts: facts,
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Importing);
    assert_eq!(row.import_status, Some(TriageImportStatus::Importing));
}

#[test]
fn the_identify_phase_rides_on_a_row_with_no_stored_verdict() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    let facts = BTreeMap::from([(
        key("Release"),
        TriageRuntimeFacts {
            phase: IdentifyPhase::Running,
            importing: false,
        },
    )]);

    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: view(TriageTab::Pending),
            runtime_facts: facts,
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");

    assert_eq!(
        row_for(&flat, "Release").placement,
        TriagePlacement::NeedsYou {
            group: crate::import::NeedsYouGroup::StillIdentifying,
            reason: crate::import::NeedsYouReason::StillIdentifying {
                phase: IdentifyPhase::Running,
            },
        }
    );
}

#[test]
fn the_first_unidentified_row_has_its_position_in_the_current_view() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release 1"), candidate("Release 2")];
    rows.states
        .insert("hash-Release 1".to_string(), ready_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));

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

    let filtered = flattened(
        &rows,
        &ImportListView {
            filter_text: "Release 1".to_string(),
            ..view(TriageTab::Pending)
        },
    );
    let other_tab = flattened(&rows, &view(TriageTab::Done));

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

    let flat = flattened(&rows, &view(TriageTab::Pending));
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

// ── A pick is the answer ────────────────────────────────────────────

/// Whatever question the verdict was going to put, a stored pick has answered
/// it: the row is Ready and takes a bulk-import checkbox, rather than keeping
/// the question's tag forever after it was answered.
#[test]
fn a_pick_answers_whatever_the_verdict_asked() {
    let cases = [
        ("several pressings matched", several_matches_state()),
        ("nothing matched anywhere", not_found_state()),
    ];
    for (name, mut state) in cases {
        // Without a pick the row states the question.
        let mut rows = queue();
        rows.candidates = vec![candidate("Release")];
        rows.states
            .insert("hash-Release".to_string(), state.clone());
        assert!(
            matches!(
                row_for(&flattened(&rows, &view(TriageTab::Pending)), "Release").placement,
                TriagePlacement::NeedsYou { .. }
            ),
            "{name}: unanswered, the row asks"
        );

        // The user picks a release; the row is Ready.
        state.pick = Some(release_pick("mb-picked"));
        let mut rows = queue();
        rows.candidates = vec![candidate("Release")];
        rows.states.insert("hash-Release".to_string(), state);

        let flat = flattened(&rows, &view(TriageTab::Pending));
        let row = row_for(&flat, "Release");
        assert_eq!(row.placement, TriagePlacement::Ready, "{name}: answered");
        assert!(row.selectable, "{name}: a Ready row takes a checkbox");
        assert_eq!(
            row.metadata_seed,
            Some(MetadataSeed::ExternalRelease {
                source: MetadataSource::MusicBrainz,
                release_id: "mb-picked".to_string(),
            }),
            "{name}: the row carries what a bulk import would commit"
        );
    }
}

/// Reading the folder as its own tags is an answer too — there is no release
/// to name, and nothing left to ask.
#[test]
fn an_unknown_pick_answers_the_row() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states.insert(
        "hash-Release".to_string(),
        CandidateStateListRow {
            pick: Some(MetadataSeed::FileTags),
            ..several_matches_state()
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert_eq!(row.metadata_seed, Some(MetadataSeed::FileTags));
}

/// A pick belongs to the file shape it was made against. Editing the folder
/// moves the candidate past that shape, so the pick is not its answer any more
/// and the row goes back to waiting on identification.
#[test]
fn a_pick_at_a_stale_edit_revision_does_not_answer_the_row() {
    let mut rows = queue();
    rows.candidates = vec![ScanCandidateListRow {
        file_edit_revision: 2,
        ..candidate("Release")
    }];
    rows.states.insert(
        "hash-Release".to_string(),
        CandidateStateListRow {
            pick: Some(release_pick("mb-picked")),
            ..several_matches_state()
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert!(matches!(
        row.placement,
        TriagePlacement::NeedsYou {
            reason: crate::import::NeedsYouReason::StillIdentifying { .. },
            ..
        }
    ));
    assert_eq!(row.metadata_seed, None);
}

/// A pick does not outrank the three facts above it: a skipped candidate stays
/// skipped, an imported one stays done, and a running import keeps the row.
#[test]
fn a_pick_does_not_outrank_skipped_done_or_importing() {
    let picked = CandidateStateListRow {
        pick: Some(release_pick("mb-picked")),
        ..several_matches_state()
    };

    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), picked.clone());
    rows.skipped
        .insert((rows.watched_folders[0].path.clone(), "Release".to_string()));
    assert_eq!(
        row_for(&flattened(&rows, &view(TriageTab::Skipped)), "Release").placement,
        TriagePlacement::Skipped
    );

    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), picked.clone());
    rows.imported.insert(
        "hash-Release".to_string(),
        ImportedRelease {
            release_id: "rel-1".to_string(),
            album_id: "alb-1".to_string(),
        },
    );
    assert_eq!(
        row_for(&flattened(&rows, &view(TriageTab::Done)), "Release").placement,
        TriagePlacement::Done
    );

    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states.insert("hash-Release".to_string(), picked);
    let running = BTreeMap::from([(
        key("Release"),
        TriageRuntimeFacts {
            phase: IdentifyPhase::Queued,
            importing: true,
        },
    )]);
    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: view(TriageTab::Pending),
            runtime_facts: running,
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");
    assert_eq!(
        row_for(&flat, "Release").placement,
        TriagePlacement::Importing
    );
}

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

/// A folder nothing has settled either way, holding several releases, is
/// combinable too — the scan names it on every row below it, and the header
/// for it is where the choice belongs.
#[test]
fn a_folder_the_scan_named_as_an_ancestor_offers_to_combine() {
    let mut rows = queue();
    rows.candidates = vec![
        ScanCandidateListRow {
            combine_ancestor_relative_path: Some("Wrapper".to_string()),
            ..candidate("Wrapper/Box/Disc 1")
        },
        ScanCandidateListRow {
            combine_ancestor_relative_path: Some("Wrapper".to_string()),
            ..candidate("Wrapper/Box/Disc 2")
        },
    ];

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let header = flat.headers.first().expect("a header for Wrapper");
    assert_eq!(header.group.name, "Wrapper");
    assert!(header.group.combinable);
}
