use super::*;
use crate::triage::TriageTab;

fn basis(placement: TriagePlacement, lookup: Option<StoredLookup>) -> CandidateActionBasis {
    CandidateActionBasis::of(true, &placement, true, lookup, false)
}

fn identifying(status: IdentificationStatus) -> TriageRuntimeFacts {
    TriageRuntimeFacts { identification: Some(status), import: None }
}

/// Import is offered for a candidate on Pending — a failed attempt
/// included — whose draft would import.
#[test]
fn every_pending_candidate_with_a_valid_draft_offers_import() {
    let rest = TriageRuntimeFacts::default();
    for placement in [
        TriagePlacement::Pending,
        TriagePlacement::Failed,
        TriagePlacement::Skipped,
        TriagePlacement::Done,
    ] {
        let pending = placement.tab() == TriageTab::Pending;
        assert_eq!(
            basis(placement, None).actions(&rest).contains(&CandidateAction::Import),
            pending,
            "{placement:?}"
        );
        assert!(!CandidateActionBasis::of(true, &placement, false, None, false)
            .actions(&rest)
            .contains(&CandidateAction::Import));
        assert_eq!(
            CandidateActionBasis::of(false, &placement, true, None, false)
                .actions(&TriageRuntimeFacts::default()),
            vec![CandidateAction::RevealFolder],
            "a candidate that cannot be acted on still shows where it is"
        );
    }
}

#[test]
fn skipped_candidates_offer_restore_without_replacing_metadata() {
    assert_eq!(
        basis(TriagePlacement::Skipped, None).actions(&TriageRuntimeFacts::default()),
        vec![CandidateAction::Restore, CandidateAction::Combine, CandidateAction::RevealFolder]
    );
}

/// An import owning the candidate takes every command away but cancelling
/// it, the skip included, wherever the tables still place it — whether it
/// is still waiting for the worker or running.
#[test]
fn a_queued_or_running_import_offers_only_its_cancel() {
    for step in [
        crate::ImportStep::Preparing(crate::PrepareStep::Queued),
        crate::ImportStep::Running(crate::ImportPhase::ReadingFiles),
    ] {
        let importing = TriageRuntimeFacts { identification: None, import: Some(step) };
        for placement in [TriagePlacement::Pending, TriagePlacement::Failed] {
            assert_eq!(
                basis(placement, None).actions(&importing),
                vec![CandidateAction::CancelImport, CandidateAction::RevealFolder],
                "{step:?}, {placement:?}"
            );
        }
    }
}

/// An import writing its release completes whatever is asked, so it is
/// not offered a cancel it would refuse.
#[test]
fn an_import_writing_its_release_offers_no_cancel() {
    let writing = TriageRuntimeFacts {
        identification: None,
        import: Some(crate::ImportStep::Running(crate::ImportPhase::Finalizing)),
    };
    assert_eq!(
        basis(TriagePlacement::Pending, None).actions(&writing),
        vec![CandidateAction::RevealFolder]
    );
}

#[test]
fn a_draft_cannot_be_replaced_while_identification_is_running() {
    for status in [
        IdentificationStatus::Queued,
        IdentificationStatus::Running,
        IdentificationStatus::Finalizing,
    ] {
        assert_eq!(
            basis(TriagePlacement::Pending, Some(StoredLookup::Answered))
                .actions(&identifying(status)),
            vec![
                CandidateAction::CancelIdentification,
                CandidateAction::Skip,
                CandidateAction::RevealFolder
            ]
        );
    }
}

/// While the release a run picked unattended is read, the row offers
/// importing once it is stored — whether or not the draft stored now would
/// import, since it is not what imports — until that is asked.
#[test]
fn a_pick_being_read_offers_import_until_it_is_asked() {
    for draft_valid in [true, false] {
        let basis =
            CandidateActionBasis::of(true, &TriagePlacement::Pending, draft_valid, None, false);
        assert_eq!(
            basis.actions(&identifying(IdentificationStatus::ReadingPick { import_asked: false })),
            vec![
                CandidateAction::Import,
                CandidateAction::CancelIdentification,
                CandidateAction::Skip,
                CandidateAction::RevealFolder
            ],
            "draft valid: {draft_valid}"
        );
        assert_eq!(
            basis.actions(&identifying(IdentificationStatus::ReadingPick { import_asked: true })),
            vec![
                CandidateAction::CancelIdentification,
                CandidateAction::Skip,
                CandidateAction::RevealFolder
            ],
            "draft valid: {draft_valid}"
        );
    }
}

/// A candidate nobody has answered yet is Pending, and a run in flight for
/// it leaves only cancelling the run and the skip — the run is about to
/// write the answer every other command would overwrite.
#[test]
fn active_identification_cannot_be_overwritten_by_a_bulk_action() {
    for status in [
        IdentificationStatus::Queued,
        IdentificationStatus::Running,
        IdentificationStatus::Finalizing,
    ] {
        assert_eq!(
            basis(TriagePlacement::Pending, None).actions(&identifying(status)),
            vec![
                CandidateAction::CancelIdentification,
                CandidateAction::Skip,
                CandidateAction::RevealFolder
            ]
        );
    }
}

/// How a candidate's folders are read is offered only while nothing is
/// running for it and it is not imported: a grouping separates — even one
/// that cannot be worked on, which separating fixes — and anything else
/// that can be acted on combines.
#[test]
fn a_candidate_offers_separating_or_combining_while_it_is_settled_nowhere() {
    let rest = TriageRuntimeFacts::default();
    let grouped = CandidateActionBasis::of(true, &TriagePlacement::Pending, true, None, true);
    assert!(grouped.actions(&rest).contains(&CandidateAction::Separate));
    assert!(!grouped.actions(&rest).contains(&CandidateAction::Combine));
    let blocked = CandidateActionBasis::of(false, &TriagePlacement::Failed, true, None, true);
    assert_eq!(
        blocked.actions(&rest),
        vec![CandidateAction::Separate, CandidateAction::RevealFolder]
    );
    let lone = basis(TriagePlacement::Pending, Some(StoredLookup::Answered));
    assert!(lone.actions(&rest).contains(&CandidateAction::Combine));
    let done = CandidateActionBasis::of(true, &TriagePlacement::Done, true, None, true);
    assert_eq!(done.actions(&rest), vec![CandidateAction::RevealFolder]);
    let importing = TriageRuntimeFacts {
        identification: None,
        import: Some(crate::ImportStep::Running(crate::ImportPhase::ReadingFiles)),
    };
    assert!(!grouped.actions(&importing).contains(&CandidateAction::Separate));
}

/// A candidate is offered identifying only while no lookup is stored for
/// its files — the rule Automatic follows, which shows a stored one as it
/// stood. A failed lookup is retried rather than identified again.
#[test]
fn only_a_candidate_with_no_stored_lookup_offers_identify() {
    let rest = TriageRuntimeFacts::default();
    for placement in [TriagePlacement::Pending, TriagePlacement::Failed] {
        assert!(basis(placement, None).actions(&rest).contains(&CandidateAction::Identify));
        for lookup in [StoredLookup::Answered, StoredLookup::Failed] {
            assert!(
                !basis(placement, Some(lookup)).actions(&rest).contains(&CandidateAction::Identify),
                "{placement:?} with a stored {lookup:?} lookup"
            );
        }
    }
}

#[test]
fn lookup_and_finalization_failures_offer_retry() {
    for placement in [TriagePlacement::Pending, TriagePlacement::Failed] {
        assert!(basis(placement, Some(StoredLookup::Failed))
            .actions(&TriageRuntimeFacts::default())
            .contains(&CandidateAction::RetryIdentification));
    }
    let failure = identifying(IdentificationStatus::FinalizationFailed {
        failure: crate::SaveFailure::NotWritten { error: "Provider unavailable".to_owned() },
    });
    assert!(basis(TriagePlacement::Pending, None)
        .actions(&failure)
        .contains(&CandidateAction::RetryIdentification));
    assert!(!basis(TriagePlacement::Pending, None)
        .actions(&TriageRuntimeFacts::default())
        .contains(&CandidateAction::RetryIdentification));
    assert!(
        !basis(TriagePlacement::Pending, Some(StoredLookup::Answered))
            .actions(&TriageRuntimeFacts::default())
            .contains(&CandidateAction::RetryIdentification),
        "a lookup that finished offers no retry"
    );
}

/// An answer whose release cannot be read into the draft fails the same
/// way on every run, so neither identifying nor retrying is offered.
#[test]
fn an_answer_that_fails_again_offers_no_retry() {
    let failure = identifying(IdentificationStatus::FinalizationFailed {
        failure: crate::SaveFailure::Inapplicable {
            error: "the release lists a track with no title".to_owned(),
        },
    });
    let actions = basis(TriagePlacement::Pending, None).actions(&failure);
    assert!(!actions.contains(&CandidateAction::RetryIdentification));
    assert!(!actions.contains(&CandidateAction::Identify));
}
