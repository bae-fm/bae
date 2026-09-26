//! Where each candidate's row lands, and what it carries there.
//!
//! One pass places every row from the stored columns alone, so these are the
//! placement rules over row literals: what a stored verdict, a pick, a skip
//! and an import's rows each make of a candidate, and in which order they
//! outrank one another. What is running for a candidate places nothing; it is
//! the row's live state.

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
            cover: Some(crate::import::cover_art::RemoteImageSet::original(
                "https://example.test/front.jpg".to_string(),
            )),
        }]
    );
}
/// A verdict derived from a file shape the candidate has moved past is not the
/// candidate's answer, so nothing places the row.
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

    assert_eq!(
        row_for(&flat, "Release").placement,
        TriagePlacement::Pending
    );
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
/// An import running for a candidate places nothing: until it writes the
/// release, the tables put the row where its draft does, and the import is the
/// row's live state — which offers no command at all while it runs.
#[test]
fn a_claimed_import_leaves_the_row_where_its_draft_puts_it() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert_eq!(row.import_status, None);
    assert_eq!(flat.summary.counts.pending, 1);
    let importing = crate::import::CandidateLiveState::of(
        &row.action_basis,
        TriageRuntimeFacts {
            identification: None,
            importing: true,
        },
    );
    assert!(importing.actions.is_empty());
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
/// Retrying is the ordinary import. Queuing it clears the failure row, which
/// leaves the row where its draft puts it while the import runs; when the
/// import lands, the release outranks any leftover failure and the row is
/// Done.
#[test]
fn retrying_a_failed_import_moves_it_back_to_its_draft_then_to_done() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));
    rows.failures
        .insert("hash-Release".to_string(), "boom".to_string());
    assert_eq!(
        row_for(&flattened(&rows, &view(TriageTab::Pending)), "Release").placement,
        TriagePlacement::Failed
    );

    rows.failures.clear();
    let flat = flattened(&rows, &view(TriageTab::Pending));
    assert_eq!(row_for(&flat, "Release").placement, TriagePlacement::Ready);
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
/// A row's placement is what its preparation says; a run queued for it is
/// its live state. A tag-prefilled draft is ready to import whether or not
/// identification is about to answer the folder again.
#[test]
fn a_ready_row_stays_ready_with_identification_queued_for_it() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), prefilled_from_tags_state());

    let flat = flattened(&rows, &view(TriageTab::Pending));

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert!(row.selectable);
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
            metadata_provenance: None,
            metadata_author: MetadataAuthor::Person,
            metadata_draft_valid: true,
            metadata_summary: None,
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
            cover: None,
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
    let cases = [
        (
            queue(),
            with_verdict(ready_state("mb-1"), |verdict| {
                verdict.track_count = Some(10);
            }),
            NeedsYou::TrackCountDisagrees {
                local: 10,
                source: 11,
            },
        ),
        (
            queue(),
            with_verdict(ready_state("mb-1"), |verdict| {
                verdict.lead = Some(LeadMatch {
                    source_tracks: None,
                    ..lead("mb-1")
                });
            }),
            NeedsYou::SourceTracksUnknown,
        ),
        (
            queue(),
            with_verdict(ready_state("mb-1"), |verdict| {
                verdict.lead = Some(LeadMatch {
                    source_tracks: Some(SourceTracks::Nothing),
                    ..lead("mb-1")
                });
            }),
            NeedsYou::SourceTracksUnknown,
        ),
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
/// and the row falls back to Pending.
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

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert_eq!(row.metadata_provenance, None);
}

/// A pick does not outrank the two facts above it: a skipped candidate stays
/// skipped, and an imported one stays done.
#[test]
fn a_pick_does_not_outrank_skipped_or_done() {
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
        .insert("hash-Release".to_string(), picked);
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
