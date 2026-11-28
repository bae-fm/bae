use super::*;
use crate::tables::import_list::selection::selection_params;
use crate::{IdentificationStatus, TriagePlacement, TriageRuntimeFacts, TriageTab};
use bae_store::WriteStatements as _;

/// The columns of one release's `import_list_placement` row that its command
/// columns read. `lookup` is the stored `lookup` column: `"answered"`,
/// `"failed"`, or `None` with no lookup stored.
struct PlacementRow {
    actionable: bool,
    placement: TriagePlacement,
    draft_valid: bool,
    lookup: Option<&'static str>,
    separable: bool,
}

impl PlacementRow {
    fn of(
        actionable: bool,
        placement: &TriagePlacement,
        draft_valid: bool,
        lookup: Option<&'static str>,
        separable: bool,
    ) -> Self {
        Self { actionable, placement: *placement, draft_valid, lookup, separable }
    }

    /// The commands a release offers, read by the list's selection over a
    /// library that places it with these columns and holds a standing of
    /// the given facts.
    async fn actions(&self, live: &TriageRuntimeFacts) -> Vec<CandidateAction> {
        let (tab, pending_state, failed) = match self.placement {
            TriagePlacement::Pending => ("pending", Some("not_looked_up"), false),
            TriagePlacement::Failed => ("pending", Some("import_error"), true),
            TriagePlacement::Done => ("done", None, false),
            TriagePlacement::Skipped => ("skipped", None, false),
        };
        let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
        let key = crate::CandidateKey::from(b"k".to_vec());
        let planted = key.clone();
        let (actionable, draft_valid, lookup, separable) =
            (self.actionable, self.draft_valid, self.lookup, self.separable);
        let live = live.clone();
        db.call(move |sql| {
            crate::tables::import_state::plant_facts_on(sql, &planted, &live)?;
            sql.run(&super::test_reads::PlantPlacementParams {
                key: &planted,
                tab,
                pending_state,
                actionable,
                import_failed: failed,
                draft_valid,
                lookup,
                separable,
            })?;
            Ok(())
        })
        .await
        .unwrap();
        // Two keys naming the release select it twice, so the selection
        // offers Combine too, counting no row unless the release can join one.
        let rows = db.read_rows(selection_params(&[key.clone(), key])).await.unwrap();
        rows.into_iter()
            .filter_map(|row| Some((row.command?, row.offering?)))
            .filter(|(_, offering)| *offering > 0)
            .map(|(command, _)| CandidateAction::ALL[usize::try_from(command).unwrap()])
            .collect()
    }
}

fn placed(placement: TriagePlacement, lookup: Option<&'static str>) -> PlacementRow {
    PlacementRow::of(true, &placement, true, lookup, false)
}

fn identifying(status: IdentificationStatus) -> TriageRuntimeFacts {
    TriageRuntimeFacts { identification: Some(status), import: None }
}

/// Import is offered for a candidate on Pending — a failed attempt
/// included — whose draft would import.
#[tokio::test]
async fn every_pending_candidate_with_a_valid_draft_offers_import() {
    let rest = TriageRuntimeFacts::default();
    for placement in [
        TriagePlacement::Pending,
        TriagePlacement::Failed,
        TriagePlacement::Skipped,
        TriagePlacement::Done,
    ] {
        let pending = placement.tab() == TriageTab::Pending;
        assert_eq!(
            placed(placement, None).actions(&rest).await.contains(&CandidateAction::Import),
            pending,
            "{placement:?}"
        );
        assert!(!PlacementRow::of(true, &placement, false, None, false)
            .actions(&rest)
            .await
            .contains(&CandidateAction::Import));
        assert_eq!(
            PlacementRow::of(false, &placement, true, None, false)
                .actions(&TriageRuntimeFacts::default())
                .await,
            vec![CandidateAction::RevealFolder],
            "a candidate that cannot be acted on still shows where it is"
        );
    }
}

#[tokio::test]
async fn skipped_candidates_offer_restore_without_replacing_metadata() {
    assert_eq!(
        placed(TriagePlacement::Skipped, None).actions(&TriageRuntimeFacts::default()).await,
        vec![CandidateAction::Combine, CandidateAction::Restore, CandidateAction::RevealFolder]
    );
}

/// An import owning the candidate takes every command away but cancelling
/// it, the skip included, wherever the tables still place it — whether it
/// is still waiting for the worker or running.
#[tokio::test]
async fn a_queued_or_running_import_offers_only_its_cancel() {
    for step in [
        crate::ImportStep::Preparing(crate::PrepareStep::Queued),
        crate::ImportStep::Running(crate::ImportPhase::ReadingFiles),
    ] {
        let importing = TriageRuntimeFacts { identification: None, import: Some(step) };
        for placement in [TriagePlacement::Pending, TriagePlacement::Failed] {
            assert_eq!(
                placed(placement, None).actions(&importing).await,
                vec![CandidateAction::CancelImport, CandidateAction::RevealFolder],
                "{step:?}, {placement:?}"
            );
        }
    }
}

/// An import writing its release completes whatever is asked, so it is
/// not offered a cancel it would refuse.
#[tokio::test]
async fn an_import_writing_its_release_offers_no_cancel() {
    let writing = TriageRuntimeFacts {
        identification: None,
        import: Some(crate::ImportStep::Running(crate::ImportPhase::Finalizing)),
    };
    assert_eq!(
        placed(TriagePlacement::Pending, None).actions(&writing).await,
        vec![CandidateAction::RevealFolder]
    );
}

#[tokio::test]
async fn a_draft_cannot_be_replaced_while_identification_is_running() {
    for status in [
        IdentificationStatus::Queued,
        IdentificationStatus::Running,
        IdentificationStatus::Finalizing,
    ] {
        assert_eq!(
            placed(TriagePlacement::Pending, Some("answered")).actions(&identifying(status)).await,
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
#[tokio::test]
async fn a_pick_being_read_offers_import_until_it_is_asked() {
    for draft_valid in [true, false] {
        let row = PlacementRow::of(true, &TriagePlacement::Pending, draft_valid, None, false);
        assert_eq!(
            row.actions(&identifying(IdentificationStatus::ReadingPick { import_asked: false }))
                .await,
            vec![
                CandidateAction::Import,
                CandidateAction::CancelIdentification,
                CandidateAction::Skip,
                CandidateAction::RevealFolder
            ],
            "draft valid: {draft_valid}"
        );
        assert_eq!(
            row.actions(&identifying(IdentificationStatus::ReadingPick { import_asked: true }))
                .await,
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
#[tokio::test]
async fn active_identification_cannot_be_overwritten_by_a_bulk_action() {
    for status in [
        IdentificationStatus::Queued,
        IdentificationStatus::Running,
        IdentificationStatus::Finalizing,
    ] {
        assert_eq!(
            placed(TriagePlacement::Pending, None).actions(&identifying(status)).await,
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
#[tokio::test]
async fn a_candidate_offers_separating_or_combining_while_it_is_settled_nowhere() {
    let rest = TriageRuntimeFacts::default();
    let grouped = PlacementRow::of(true, &TriagePlacement::Pending, true, None, true);
    assert!(grouped.actions(&rest).await.contains(&CandidateAction::Separate));
    assert!(!grouped.actions(&rest).await.contains(&CandidateAction::Combine));
    let blocked = PlacementRow::of(false, &TriagePlacement::Failed, true, None, true);
    assert_eq!(
        blocked.actions(&rest).await,
        vec![CandidateAction::Separate, CandidateAction::RevealFolder]
    );
    let lone = placed(TriagePlacement::Pending, Some("answered"));
    assert!(lone.actions(&rest).await.contains(&CandidateAction::Combine));
    let done = PlacementRow::of(true, &TriagePlacement::Done, true, None, true);
    assert_eq!(done.actions(&rest).await, vec![CandidateAction::RevealFolder]);
    let importing = TriageRuntimeFacts {
        identification: None,
        import: Some(crate::ImportStep::Running(crate::ImportPhase::ReadingFiles)),
    };
    assert!(!grouped.actions(&importing).await.contains(&CandidateAction::Separate));
}

/// A candidate is offered identifying only while no lookup is stored for
/// its files — the rule Automatic follows, which shows a stored one as it
/// stood. A failed lookup is retried rather than identified again.
#[tokio::test]
async fn only_a_candidate_with_no_stored_lookup_offers_identify() {
    let rest = TriageRuntimeFacts::default();
    for placement in [TriagePlacement::Pending, TriagePlacement::Failed] {
        assert!(placed(placement, None).actions(&rest).await.contains(&CandidateAction::Identify));
        for lookup in ["answered", "failed"] {
            assert!(
                !placed(placement, Some(lookup))
                    .actions(&rest)
                    .await
                    .contains(&CandidateAction::Identify),
                "{placement:?} with a stored {lookup:?} lookup"
            );
        }
    }
}

#[tokio::test]
async fn lookup_and_finalization_failures_offer_retry() {
    for placement in [TriagePlacement::Pending, TriagePlacement::Failed] {
        assert!(placed(placement, Some("failed"))
            .actions(&TriageRuntimeFacts::default())
            .await
            .contains(&CandidateAction::RetryIdentification));
    }
    let failure = identifying(IdentificationStatus::FinalizationFailed {
        failure: crate::SaveFailure::NotWritten { error: "Provider unavailable".to_owned() },
    });
    assert!(placed(TriagePlacement::Pending, None)
        .actions(&failure)
        .await
        .contains(&CandidateAction::RetryIdentification));
    assert!(!placed(TriagePlacement::Pending, None)
        .actions(&TriageRuntimeFacts::default())
        .await
        .contains(&CandidateAction::RetryIdentification));
    assert!(
        !placed(TriagePlacement::Pending, Some("answered"))
            .actions(&TriageRuntimeFacts::default())
            .await
            .contains(&CandidateAction::RetryIdentification),
        "a lookup that finished offers no retry"
    );
}

/// An answer whose release cannot be read into the draft fails the same
/// way on every run, so neither identifying nor retrying is offered.
#[tokio::test]
async fn an_answer_that_fails_again_offers_no_retry() {
    let failure = identifying(IdentificationStatus::FinalizationFailed {
        failure: crate::SaveFailure::Inapplicable {
            error: "the release lists a track with no title".to_owned(),
        },
    });
    let actions = placed(TriagePlacement::Pending, None).actions(&failure).await;
    assert!(!actions.contains(&CandidateAction::RetryIdentification));
    assert!(!actions.contains(&CandidateAction::Identify));
}
