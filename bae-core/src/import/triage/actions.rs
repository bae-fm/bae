use super::{
    CandidateAnswer, IdentificationStatus, NeedsYou, QueueClassification, TriagePlacement,
    TriageSkipAction,
};

/// Commands offered for a candidate at its current lifecycle position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateAction {
    ImportReady,
    Identify,
    RetryIdentification,
    UseFileMetadata,
    ClearMetadata,
    Skip,
    Restore,
}

pub(crate) fn candidate_actions(
    actionable: bool,
    placement: &TriagePlacement,
    identification: Option<&IdentificationStatus>,
    answer: &CandidateAnswer,
) -> Vec<CandidateAction> {
    use CandidateAction as A;
    use TriagePlacement as P;
    if !actionable {
        return Vec::new();
    }
    let identifying = matches!(
        identification,
        Some(
            IdentificationStatus::Queued
                | IdentificationStatus::Running
                | IdentificationStatus::Finalizing
        )
    );
    let mut actions = match placement {
        P::Importing | P::Done | P::Skipped => Vec::new(),
        _ if identifying => Vec::new(),
        P::Identification {
            status:
                IdentificationStatus::Queued
                | IdentificationStatus::Running
                | IdentificationStatus::Finalizing,
        } => Vec::new(),
        P::Pending
        | P::Ready
        | P::NeedsYou { .. }
        | P::Failed
        | P::Identification {
            status: IdentificationStatus::FinalizationFailed { .. },
        } => {
            let mut actions = Vec::new();
            if matches!(placement, P::Ready) {
                actions.push(A::ImportReady);
            }
            actions.push(A::Identify);
            if matches!(
                answer,
                CandidateAnswer::Classified(QueueClassification::NeedsYou(NeedsYou::LookupFailed))
            ) || matches!(
                identification,
                Some(IdentificationStatus::FinalizationFailed { .. })
            ) || matches!(
                placement,
                P::NeedsYou {
                    reason: NeedsYou::LookupFailed
                } | P::Identification {
                    status: IdentificationStatus::FinalizationFailed { .. }
                }
            ) {
                actions.push(A::RetryIdentification);
            }
            actions.extend([A::UseFileMetadata, A::ClearMetadata]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ready_candidates_offer_unattended_import() {
        for placement in [
            TriagePlacement::Pending,
            TriagePlacement::Ready,
            TriagePlacement::Failed,
            TriagePlacement::Skipped,
            TriagePlacement::Done,
            TriagePlacement::Importing,
        ] {
            assert_eq!(
                candidate_actions(true, &placement, None, &CandidateAnswer::Unidentified)
                    .contains(&CandidateAction::ImportReady),
                placement == TriagePlacement::Ready
            );
            assert!(
                candidate_actions(false, &placement, None, &CandidateAnswer::Unidentified)
                    .is_empty()
            );
        }
    }

    #[test]
    fn skipped_candidates_offer_restore_without_replacing_metadata() {
        assert_eq!(
            candidate_actions(
                true,
                &TriagePlacement::Skipped,
                None,
                &CandidateAnswer::Unidentified
            ),
            vec![CandidateAction::Restore]
        );
    }

    #[test]
    fn a_ready_draft_cannot_be_replaced_while_identification_is_running() {
        for status in [
            IdentificationStatus::Queued,
            IdentificationStatus::Running,
            IdentificationStatus::Finalizing,
        ] {
            assert_eq!(
                candidate_actions(
                    true,
                    &TriagePlacement::Ready,
                    Some(&status),
                    &CandidateAnswer::Classified(QueueClassification::Ready)
                ),
                vec![CandidateAction::Skip]
            );
        }
    }

    #[test]
    fn active_identification_cannot_be_overwritten_by_a_bulk_action() {
        for status in [
            IdentificationStatus::Queued,
            IdentificationStatus::Running,
            IdentificationStatus::Finalizing,
        ] {
            assert_eq!(
                candidate_actions(
                    true,
                    &TriagePlacement::Identification {
                        status: status.clone()
                    },
                    Some(&status),
                    &CandidateAnswer::Identification(status.clone())
                ),
                vec![CandidateAction::Skip]
            );
        }
    }

    #[test]
    fn lookup_and_finalization_failures_offer_retry() {
        let failed_lookup =
            CandidateAnswer::Classified(QueueClassification::NeedsYou(NeedsYou::LookupFailed));
        for placement in [
            TriagePlacement::NeedsYou {
                reason: NeedsYou::LookupFailed,
            },
            TriagePlacement::Ready,
        ] {
            assert!(candidate_actions(true, &placement, None, &failed_lookup)
                .contains(&CandidateAction::RetryIdentification));
        }
        let failure = IdentificationStatus::FinalizationFailed {
            error: "Provider unavailable".to_owned(),
        };
        assert!(candidate_actions(
            true,
            &TriagePlacement::Ready,
            Some(&failure),
            &CandidateAnswer::Identification(failure.clone())
        )
        .contains(&CandidateAction::RetryIdentification));
        assert!(!candidate_actions(
            true,
            &TriagePlacement::Pending,
            None,
            &CandidateAnswer::Unidentified
        )
        .contains(&CandidateAction::RetryIdentification));
    }
}
