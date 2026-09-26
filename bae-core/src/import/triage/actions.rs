use super::{
    IdentificationStatus, NeedsYou, QueueClassification, TriagePlacement, TriageRuntimeFacts,
    TriageSkipAction,
};

/// Commands offered for a candidate at its current lifecycle position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateAction {
    ImportReady,
    Identify,
    /// Stop the identification that is waiting, running, or being written.
    CancelIdentification,
    /// Stop the import that is waiting for the worker or running.
    CancelImport,
    RetryIdentification,
    ResetToFileMetadata,
    ClearMetadata,
    Skip,
    Restore,
}

/// What the tables say a candidate's commands are decided from: whether it can
/// be acted on at all, where it is placed, and whether its stored lookup
/// failed — which offers a retry whatever the draft over it says.
///
/// The row carries it so the surface drawing the row can hand it back with
/// the row's live-state subscription: the commands a row offers are these
/// facts and what is running for it right now, and only core joins the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateActionBasis {
    pub actionable: bool,
    pub placement: TriagePlacement,
    pub lookup_failed: bool,
}

impl CandidateActionBasis {
    pub(crate) fn of(
        actionable: bool,
        placement: &TriagePlacement,
        answer: Option<&QueueClassification>,
    ) -> Self {
        Self {
            actionable,
            placement: placement.clone(),
            lookup_failed: matches!(
                answer,
                Some(QueueClassification::NeedsYou(NeedsYou::LookupFailed))
            ),
        }
    }

    /// Whether a bulk import may take this row when nothing is running for
    /// it — the Ready set the list counts and selects from the tables alone.
    pub fn importable_at_rest(&self) -> bool {
        self.actions(&TriageRuntimeFacts::default())
            .contains(&CandidateAction::ImportReady)
    }

    /// The commands these facts offer with `live` running for the candidate.
    ///
    /// An import owning the candidate leaves only cancelling it, not even a
    /// skip: the attempt is what decides it now. A run in flight leaves only cancelling
    /// it and the skip — the run is about to write the answer every other
    /// command would overwrite.
    pub fn actions(&self, live: &TriageRuntimeFacts) -> Vec<CandidateAction> {
        use CandidateAction as A;
        use TriagePlacement as P;
        if !self.actionable {
            return Vec::new();
        }
        if live.importing {
            return vec![A::CancelImport];
        }
        let identifying = live.identifying();
        let placement = &self.placement;
        let mut actions = match placement {
            P::Done | P::Skipped => Vec::new(),
            _ if identifying => vec![A::CancelIdentification],
            P::Pending | P::Ready | P::NeedsYou { .. } | P::Failed => {
                let mut actions = Vec::new();
                if matches!(placement, P::Ready) {
                    actions.push(A::ImportReady);
                }
                actions.push(A::Identify);
                if self.lookup_failed
                    || matches!(
                        live.identification,
                        Some(IdentificationStatus::FinalizationFailed { .. })
                    )
                {
                    actions.push(A::RetryIdentification);
                }
                actions.extend([A::ResetToFileMetadata, A::ClearMetadata]);
                actions
            }
        };
        if let Some(skip) = placement.skip_action() {
            actions.push(match skip {
                TriageSkipAction::Skip => A::Skip,
                TriageSkipAction::Unskip => A::Restore,
            });
        }
        actions
    }
}

/// What is running for one candidate right now, and the commands its row
/// offers with it: the part of a row that changes without a write.
///
/// Read per candidate, beside the list rather than through it — a run moving
/// from queued to running moves no row, so it reruns no list read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CandidateLiveState {
    pub facts: TriageRuntimeFacts,
    pub actions: Vec<CandidateAction>,
}

impl CandidateLiveState {
    pub fn of(basis: &CandidateActionBasis, facts: TriageRuntimeFacts) -> Self {
        Self {
            actions: basis.actions(&facts),
            facts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basis(
        placement: TriagePlacement,
        answer: Option<&QueueClassification>,
    ) -> CandidateActionBasis {
        CandidateActionBasis::of(true, &placement, answer)
    }

    fn identifying(status: IdentificationStatus) -> TriageRuntimeFacts {
        TriageRuntimeFacts {
            identification: Some(status),
            importing: false,
        }
    }

    #[test]
    fn only_ready_candidates_offer_unattended_import() {
        for placement in [
            TriagePlacement::Pending,
            TriagePlacement::Ready,
            TriagePlacement::Failed,
            TriagePlacement::Skipped,
            TriagePlacement::Done,
        ] {
            assert_eq!(
                basis(placement.clone(), None).importable_at_rest(),
                placement == TriagePlacement::Ready
            );
            assert!(CandidateActionBasis::of(false, &placement, None)
                .actions(&TriageRuntimeFacts::default())
                .is_empty());
        }
    }

    #[test]
    fn skipped_candidates_offer_restore_without_replacing_metadata() {
        assert_eq!(
            basis(TriagePlacement::Skipped, None).actions(&TriageRuntimeFacts::default()),
            vec![CandidateAction::Restore]
        );
    }

    /// An import owning the candidate takes every command away but cancelling
    /// it, the skip included, wherever the tables still place it.
    #[test]
    fn a_running_import_offers_only_its_cancel() {
        let importing = TriageRuntimeFacts {
            identification: None,
            importing: true,
        };
        for placement in [TriagePlacement::Ready, TriagePlacement::Pending] {
            assert_eq!(
                basis(placement, None).actions(&importing),
                vec![CandidateAction::CancelImport]
            );
        }
    }

    #[test]
    fn a_ready_draft_cannot_be_replaced_while_identification_is_running() {
        for status in [
            IdentificationStatus::Queued,
            IdentificationStatus::Running,
            IdentificationStatus::Finalizing,
        ] {
            assert_eq!(
                basis(TriagePlacement::Ready, Some(&QueueClassification::Ready))
                    .actions(&identifying(status)),
                vec![CandidateAction::CancelIdentification, CandidateAction::Skip]
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
                vec![CandidateAction::CancelIdentification, CandidateAction::Skip]
            );
        }
    }

    #[test]
    fn lookup_and_finalization_failures_offer_retry() {
        let failed_lookup = QueueClassification::NeedsYou(NeedsYou::LookupFailed);
        for placement in [
            TriagePlacement::NeedsYou {
                reason: NeedsYou::LookupFailed,
            },
            TriagePlacement::Ready,
        ] {
            assert!(basis(placement, Some(&failed_lookup))
                .actions(&TriageRuntimeFacts::default())
                .contains(&CandidateAction::RetryIdentification));
        }
        let failure = identifying(IdentificationStatus::FinalizationFailed {
            error: "Provider unavailable".to_owned(),
        });
        assert!(basis(TriagePlacement::Ready, None)
            .actions(&failure)
            .contains(&CandidateAction::RetryIdentification));
        assert!(!basis(TriagePlacement::Pending, None)
            .actions(&TriageRuntimeFacts::default())
            .contains(&CandidateAction::RetryIdentification));
    }
}
