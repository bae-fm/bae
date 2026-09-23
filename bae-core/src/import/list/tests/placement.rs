//! Where each candidate's row lands, and what it carries there.
//!
//! One pass places every row from the stored columns plus this process's
//! runtime, so these are the placement rules over row literals: what a stored
//! verdict, a pick, an import and a run in flight each make of a candidate,
//! and in which order they outrank one another.

use super::*;
use crate::identify::NeedsYou;
use crate::import::MetadataAuthor;

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
            cover_thumbnail_url: Some("https://example.test/thumb.jpg".to_string()),
        }]
    );
}
/// A verdict derived from a file shape the candidate has moved past is not the
/// candidate's answer, so nothing places the row but the work queued for it.
#[test]
fn a_verdict_at_a_stale_edit_revision_is_not_the_row_s_answer() {
    let mut rows = queue();
    rows.candidates = vec![ScanCandidateListRow {
        file_edit_revision: 2,
        ..candidate("Release")
    }];
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));

    let flat = flattened_queued(&rows, view(TriageTab::Pending), &["Release"]);

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert_eq!(row.identification, Some(IdentificationStatus::Queued));
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
            identification: Some(IdentificationStatus::Queued),
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
            identification: Some(IdentificationStatus::Queued),
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
            identification: Some(IdentificationStatus::Queued),
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
fn the_identification_rides_on_a_row_with_no_stored_verdict() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    let facts = BTreeMap::from([(
        key("Release"),
        TriageRuntimeFacts {
            identification: Some(IdentificationStatus::Running),
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

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert_eq!(row.identification, Some(IdentificationStatus::Running));
}
/// A row's placement is what its preparation says; the run is a separate fact
/// the row carries beside it. A tag-prefilled draft is ready to import whether
/// or not identification is about to answer the folder again, and the chrome
/// still counts the row as one the queue is waiting on.
#[test]
fn a_ready_row_states_the_identification_queued_for_it() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), prefilled_from_tags_state());

    let flat = flattened_queued(&rows, view(TriageTab::Pending), &["Release"]);

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert_eq!(row.identification, Some(IdentificationStatus::Queued));
    assert_eq!(
        flat.summary
            .first_unidentified
            .as_ref()
            .map(|first| first.candidate_key.as_str()),
        Some(key("Release").as_str())
    );
}
#[test]
fn an_idle_candidate_is_not_queued_by_the_current_automatic_setting() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];

    let flat = flatten(
        &rows,
        &ImportListRequest {
            view: view(TriageTab::Pending),
            ..ImportListRequest::default()
        },
    )
    .expect("the queue flattens");

    assert_eq!(
        row_for(&flat, "Release").placement,
        TriagePlacement::Pending
    );
}
/// A draft a person typed in, read from no catalog and no tags, is their
/// answer as soon as it would import.
#[test]
fn a_valid_draft_a_person_typed_is_ready_and_bulk_importable() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states.insert(
        "hash-Release".to_string(),
        CandidateStateListRow {
            edit_revision: 0,
            verdict: None,
            probed_total_duration_ms: 0,
            metadata_provenance: None,
            metadata_author: MetadataAuthor::Person,
            metadata_draft_valid: true,
            metadata_summary: None,
            selected_cover: None,
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert!(row.selectable);
    assert_eq!(
        flat.summary.ready,
        vec![ReadyRowRef {
            candidate_key: key("Release"),
            cover_thumbnail_url: None,
        }]
    );
}

/// Whatever question the verdict was going to put, a person's pick has
/// answered it: the row is Ready and takes a bulk-import checkbox, rather than
/// keeping the question's tag forever after it was answered.
#[test]
fn a_person_s_pick_answers_whatever_the_verdict_asked() {
    let cases = [
        ("several pressings matched", several_matches_state()),
        ("nothing matched anywhere", not_found_state()),
    ];
    for (name, state) in cases {
        // Without an answer the row states the question.
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

        // The person picks a release; the row is Ready.
        let mut rows = queue();
        rows.candidates = vec![candidate("Release")];
        rows.states
            .insert("hash-Release".to_string(), picked_by_the_person(state));

        let flat = flattened(&rows, &view(TriageTab::Pending));
        let row = row_for(&flat, "Release");
        assert_eq!(row.placement, TriagePlacement::Ready, "{name}: answered");
        assert!(row.selectable, "{name}: a Ready row takes a checkbox");
        assert_eq!(
            row.metadata_provenance,
            Some(MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    Catalog::MusicBrainz,
                    "mb-picked".to_string()
                ),
                partners: vec![],
            }),
            "{name}: the row carries what a bulk import would commit"
        );
    }
}

/// Identification applying its own pick is not an answer: the Ready rule's
/// checks decide the row, so every disagreement they find lands it in Needs
/// you with that disagreement as its reason.
#[test]
fn identification_s_own_pick_is_judged_by_the_ready_rule() {
    let mut in_library = queue();
    in_library.lead_statuses.insert(
        "mb-1".to_string(),
        crate::db::LibraryStatus {
            release_in_library: true,
            ..crate::db::LibraryStatus::absent("mb-1")
        },
    );
    let cases = [
        (
            queue(),
            CandidateStateListRow {
                verdict: Some(VerdictSummary {
                    track_count: Some(10),
                    ..ready_state("mb-1").verdict.expect("a verdict")
                }),
                ..ready_state("mb-1")
            },
            NeedsYou::TrackCountDisagrees {
                local: 10,
                source: 11,
            },
        ),
        (
            queue(),
            CandidateStateListRow {
                probed_total_duration_ms: 1_200_000,
                ..ready_state("mb-1")
            },
            NeedsYou::DurationsDisagree {
                probed_ms: 1_200_000,
                source_ms: 2_400_000,
                // Half a second per track, eleven tracks.
                tolerance_ms: 5_500,
            },
        ),
        (
            queue(),
            CandidateStateListRow {
                verdict: Some(VerdictSummary {
                    lead: Some(LeadMatch {
                        source_tracks: None,
                        ..lead("mb-1")
                    }),
                    ..ready_state("mb-1").verdict.expect("a verdict")
                }),
                ..ready_state("mb-1")
            },
            NeedsYou::SourceLengthsUnknown,
        ),
        (
            queue(),
            CandidateStateListRow {
                probed_total_duration_ms: 0,
                ..ready_state("mb-1")
            },
            NeedsYou::LocalDurationUnknown,
        ),
        (in_library, ready_state("mb-1"), NeedsYou::AlreadyInLibrary),
    ];
    for (mut rows, state, reason) in cases {
        assert_eq!(state.metadata_author, MetadataAuthor::Identification);
        assert!(state.metadata_draft_valid);
        rows.candidates = vec![candidate("Release")];
        rows.states.insert("hash-Release".to_string(), state);

        let flat = flattened(&rows, &view(TriageTab::Pending));
        let row = row_for(&flat, "Release");
        assert_eq!(
            row.placement,
            TriagePlacement::NeedsYou {
                reason: reason.clone()
            },
            "{reason:?}"
        );
        assert!(!row.selectable, "{reason:?}: a question is not swept up");
        assert!(flat.summary.ready.is_empty(), "{reason:?}");
    }
}

/// A draft the folder's tags seeded is what the person chose to start from:
/// once it would import, it is Ready whatever the verdict asks.
#[test]
fn a_valid_draft_the_tags_seeded_answers_the_row() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states.insert(
        "hash-Release".to_string(),
        CandidateStateListRow {
            metadata_provenance: Some(MetadataProvenance::FileMetadata),
            metadata_author: MetadataAuthor::Prefill,
            metadata_draft_valid: true,
            ..several_matches_state()
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert!(row.selectable);
    assert_eq!(
        row.metadata_provenance,
        Some(MetadataProvenance::FileMetadata)
    );
}

/// Ready means a bulk import can commit the row. A tag-seeded draft that
/// would not import is not Ready: the verdict's question stands, and with no
/// verdict the row is Pending.
#[test]
fn a_draft_the_tags_seeded_that_would_not_import_is_not_ready() {
    let invalid = CandidateStateListRow {
        metadata_provenance: Some(MetadataProvenance::FileMetadata),
        metadata_author: MetadataAuthor::Prefill,
        metadata_draft_valid: false,
        ..several_matches_state()
    };
    let cases = [
        (
            invalid.clone(),
            TriagePlacement::NeedsYou {
                reason: NeedsYou::SeveralMatches { count: 3 },
            },
        ),
        (
            CandidateStateListRow {
                verdict: None,
                ..invalid
            },
            TriagePlacement::Pending,
        ),
    ];
    for (state, expected) in cases {
        let mut rows = queue();
        rows.candidates = vec![candidate("Release")];
        rows.states.insert("hash-Release".to_string(), state);

        let flat = flattened(&rows, &view(TriageTab::Pending));
        let row = row_for(&flat, "Release");
        assert_eq!(row.placement, expected);
        assert!(!row.selectable);
        assert!(flat.summary.ready.is_empty());
    }
}

/// Not even a verdict with nothing to ask makes an invalid draft Ready: a
/// person's draft that would not import leaves the row Pending.
#[test]
fn a_verdict_with_nothing_to_ask_does_not_make_an_invalid_draft_ready() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states.insert(
        "hash-Release".to_string(),
        CandidateStateListRow {
            metadata_author: MetadataAuthor::Person,
            metadata_draft_valid: false,
            ..ready_state("mb-1")
        },
    );

    let flat = flattened(&rows, &view(TriageTab::Pending));
    assert_eq!(
        row_for(&flat, "Release").placement,
        TriagePlacement::Pending
    );
}

/// A pick belongs to the file shape it was chosen against. Editing the folder
/// moves the candidate past that shape, so the pick is not its answer any more
/// and the row falls back to Pending with its queued work beside it.
#[test]
fn a_pick_at_a_stale_edit_revision_does_not_answer_the_row() {
    let mut rows = queue();
    rows.candidates = vec![ScanCandidateListRow {
        file_edit_revision: 2,
        ..candidate("Release")
    }];
    rows.states.insert(
        "hash-Release".to_string(),
        picked_by_the_person(several_matches_state()),
    );

    let flat = flattened_queued(&rows, view(TriageTab::Pending), &["Release"]);
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert_eq!(row.identification, Some(IdentificationStatus::Queued));
    assert_eq!(row.metadata_provenance, None);
}

/// A pick does not outrank the three facts above it: a skipped candidate stays
/// skipped, an imported one stays done, and a running import keeps the row.
#[test]
fn a_pick_does_not_outrank_skipped_done_or_importing() {
    let picked = picked_by_the_person(several_matches_state());

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
            identification: Some(IdentificationStatus::Queued),
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

/// `state` after the person picked `mb-picked` for it: a valid draft read
/// from that release, written by them.
fn picked_by_the_person(state: CandidateStateListRow) -> CandidateStateListRow {
    CandidateStateListRow {
        metadata_provenance: Some(external_release_seed("mb-picked")),
        metadata_author: MetadataAuthor::Person,
        metadata_draft_valid: true,
        ..state
    }
}
