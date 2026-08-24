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
            claim: IdentityChoice::Release {
                release_ref: crate::import::MetadataRef::new("mb-1", MetadataSource::MusicBrainz),
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

    let flat = flatten(&rows, &view(TriageTab::Pending), &facts).expect("the queue flattens");

    assert_eq!(
        row_for(&flat, "Release").placement,
        TriagePlacement::Importing
    );
    assert_eq!(flat.summary.counts.pending, 1);
    assert!(flat.summary.ready.is_empty());
}

/// The failure is a row, so it survives the session that produced it: a
/// relaunched queue still places the candidate as Done and still says why.
#[test]
fn a_failed_import_reads_its_error_from_its_row() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.failures
        .insert("hash-Release".to_string(), "boom".to_string());

    let flat = flattened(&rows, &view(TriageTab::Done));

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Done);
    assert_eq!(
        row.import_status,
        Some(TriageImportStatus::Error {
            error: "boom".to_string()
        })
    );
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

    let flat = flatten(&rows, &view(TriageTab::Pending), &facts).expect("the queue flattens");

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

    let flat = flatten(&rows, &view(TriageTab::Pending), &facts).expect("the queue flattens");

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
fn the_first_unidentified_key_is_the_first_row_still_waiting_on_a_verdict() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release 1"), candidate("Release 2")];
    rows.states
        .insert("hash-Release 1".to_string(), ready_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));

    assert_eq!(
        flat.summary.first_unidentified_key.as_deref(),
        Some(key("Release 2").as_str())
    );
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
            row.claim,
            Some(IdentityChoice::Release {
                release_ref: crate::import::MetadataRef::new(
                    "mb-picked",
                    MetadataSource::MusicBrainz
                ),
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
            pick: Some(IdentityPick::Unknown),
            ..several_matches_state()
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert_eq!(row.claim, Some(IdentityChoice::Unknown));
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
    assert_eq!(row.claim, None);
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
    let flat = flatten(&rows, &view(TriageTab::Pending), &running).expect("the queue flattens");
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
