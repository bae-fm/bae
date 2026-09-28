//! Where each candidate's row lands, and what it carries there.
//!
//! One pass places every row from the stored columns alone, so these are the
//! placement rules over row literals: what a skip, an import's rows and a
//! failed attempt each make of a candidate, and in which order they outrank
//! one another. What identification found places nothing; neither does what
//! is running for a candidate, which is the row's live state.

use super::*;
use crate::import::{CandidateAction, CandidateLiveState, MetadataAuthor};

/// Whether the row, with nothing running for it, offers to import.
fn offers_import(row: &TriageRow) -> bool {
    CandidateLiveState::of(&row.action_basis, TriageRuntimeFacts::default())
        .actions
        .contains(&CandidateAction::Import)
}

/// A stored verdict places nothing: an identified row is Pending, offers to
/// import, and hands its lead's cover to Pending's warm-up.
#[test]
fn an_identified_row_is_pending_and_offers_import() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), auto_importable_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert!(offers_import(row));
    assert_eq!(
        flat.summary.pending_covers,
        vec![crate::import::cover_art::RemoteImageSet::original(
            "https://example.test/front.jpg".to_string(),
        )]
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
        .insert("hash-Release".to_string(), auto_importable_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert!(!offers_import(row), "a draft for other files is not this row's");
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
/// release the row is Pending, and the import is the row's live state — which
/// offers nothing but its cancel while it runs.
#[test]
fn a_claimed_import_leaves_the_row_pending() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), auto_importable_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));

    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);
    assert_eq!(row.import_status, None);
    assert_eq!(flat.summary.counts.pending, 1);
    let importing = crate::import::CandidateLiveState::of(
        &row.action_basis,
        TriageRuntimeFacts {
            identification: None,
            import: Some(crate::import::ImportStanding::Running),
        },
    );
    assert_eq!(
        importing.actions,
        vec![
            crate::import::CandidateAction::CancelImport,
            crate::import::CandidateAction::RevealFolder
        ]
    );
}
/// The failure is a row, so it survives the session that produced it: a
/// relaunched queue still says why the attempt failed. It stays on Pending —
/// nothing was imported — and importing it again is the ordinary import.
#[test]
fn a_failed_import_stays_pending_and_reads_its_error_from_its_row() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), auto_importable_state("mb-1"));
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
    assert!(offers_import(row));
}
/// Retrying is the ordinary import. Queuing it clears the failure row, which
/// leaves the row Pending while the import runs; when the import lands, the
/// release outranks any leftover failure and the row is Done.
#[test]
fn retrying_a_failed_import_moves_it_back_to_pending_then_to_done() {
    let mut rows = queue();
    rows.candidates = vec![candidate("Release")];
    rows.states
        .insert("hash-Release".to_string(), auto_importable_state("mb-1"));
    rows.failures
        .insert("hash-Release".to_string(), "boom".to_string());
    assert_eq!(
        row_for(&flattened(&rows, &view(TriageTab::Pending)), "Release").placement,
        TriagePlacement::Failed
    );

    rows.failures.clear();
    let flat = flattened(&rows, &view(TriageTab::Pending));
    assert_eq!(row_for(&flat, "Release").placement, TriagePlacement::Pending);
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
/// Import is offered by one rule, whoever wrote the draft and whatever
/// identification found: a draft that would import offers it, and one that
/// would not does not.
#[test]
fn a_row_offers_import_exactly_when_its_draft_would_import() {
    let cases = [
        ("the tags seeded it", prefilled_from_tags_state(), true),
        (
            "a person typed it",
            CandidateStateListRow {
                edit_revision: 0,
                verdict: None,
                metadata_provenance: None,
                metadata_author: MetadataAuthor::Person,
                metadata_draft_valid: true,
                metadata_summary: None,
            },
            true,
        ),
        ("a person picked among several", picked_by_the_person(several_matches_state()), true),
        (
            "identification picked, the track count differs",
            with_verdict(auto_importable_state("mb-1"), |verdict| verdict.track_count = Some(10)),
            true,
        ),
        ("several matched and none is picked", several_matches_state(), false),
        ("nothing matched", not_found_state(), false),
        (
            "a person's draft would not import",
            CandidateStateListRow {
                metadata_author: MetadataAuthor::Person,
                metadata_draft_valid: false,
                ..auto_importable_state("mb-1")
            },
            false,
        ),
    ];
    for (name, state, offers) in cases {
        let mut rows = queue();
        rows.candidates = vec![candidate("Release")];
        rows.states.insert("hash-Release".to_string(), state);

        let flat = flattened(&rows, &view(TriageTab::Pending));
        let row = row_for(&flat, "Release");
        assert_eq!(row.placement, TriagePlacement::Pending, "{name}");
        assert_eq!(offers_import(row), offers, "{name}");
    }
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
