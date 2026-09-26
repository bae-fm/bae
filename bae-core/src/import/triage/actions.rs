use super::{
    IdentificationStatus, NeedsYou, QueueClassification, TriagePlacement, TriageRuntimeFacts,
    TriageSkipAction,
};

/// Commands offered for a candidate at its current lifecycle position —
/// every one a candidate can take, so every surface that acts on candidates
/// offers them from this one list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Read this folder together with others as one release. Offered by each
    /// candidate that could join one; a selection offers it once, over every
    /// member, and only when it holds two or more.
    Combine,
    /// Read this release as the folders it is made of.
    Separate,
    Skip,
    Restore,
    /// Show the candidate's folders where the platform keeps files.
    RevealFolder,
}

impl CandidateAction {
    /// Every action, in the order a surface lists them.
    pub const ALL: [CandidateAction; 12] = [
        CandidateAction::ImportReady,
        CandidateAction::CancelImport,
        CandidateAction::Identify,
        CandidateAction::CancelIdentification,
        CandidateAction::RetryIdentification,
        CandidateAction::ResetToFileMetadata,
        CandidateAction::ClearMetadata,
        CandidateAction::Combine,
        CandidateAction::Separate,
        CandidateAction::Skip,
        CandidateAction::Restore,
        CandidateAction::RevealFolder,
    ];
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
    /// Whether this release is folders a grouping reads as one, which the
    /// candidate offers to read as releases of their own.
    pub separable: bool,
}

impl CandidateActionBasis {
    pub(crate) fn of(
        actionable: bool,
        placement: &TriagePlacement,
        answer: Option<&QueueClassification>,
        separable: bool,
    ) -> Self {
        Self {
            actionable,
            placement: placement.clone(),
            lookup_failed: matches!(
                answer,
                Some(QueueClassification::NeedsYou(NeedsYou::LookupFailed))
            ),
            separable,
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
    ///
    /// How its folders are read is decided before any of that: a release
    /// imported, or with work running for it, is past regrouping; any other
    /// separates when a grouping reads it — even one that cannot be worked on
    /// as it stands, which is what separating fixes — and combines when it can
    /// be acted on. Revealing its folders is always there.
    pub fn actions(&self, live: &TriageRuntimeFacts) -> Vec<CandidateAction> {
        use CandidateAction as A;
        let mut actions = self.commands(live);
        let settled = matches!(self.placement, TriagePlacement::Done)
            || live.importing
            || live.identifying();
        if !settled {
            if self.separable {
                actions.push(A::Separate);
            } else if self.actionable {
                actions.push(A::Combine);
            }
        }
        actions.push(A::RevealFolder);
        actions
    }

    /// The commands that act on what the candidate holds, before how its
    /// folders are read and revealing them.
    fn commands(&self, live: &TriageRuntimeFacts) -> Vec<CandidateAction> {
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
        CandidateActionBasis::of(true, &placement, answer, false)
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
            assert_eq!(
                CandidateActionBasis::of(false, &placement, None, false)
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
            vec![
                CandidateAction::Restore,
                CandidateAction::Combine,
                CandidateAction::RevealFolder
            ]
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
                vec![CandidateAction::CancelImport, CandidateAction::RevealFolder]
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
                vec![
                    CandidateAction::CancelIdentification,
                    CandidateAction::Skip,
                    CandidateAction::RevealFolder
                ]
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
        let grouped = CandidateActionBasis::of(true, &TriagePlacement::Pending, None, true);
        assert!(grouped.actions(&rest).contains(&CandidateAction::Separate));
        assert!(!grouped.actions(&rest).contains(&CandidateAction::Combine));
        let blocked = CandidateActionBasis::of(false, &TriagePlacement::Failed, None, true);
        assert_eq!(
            blocked.actions(&rest),
            vec![CandidateAction::Separate, CandidateAction::RevealFolder]
        );
        let lone = basis(TriagePlacement::Ready, Some(&QueueClassification::Ready));
        assert!(lone.actions(&rest).contains(&CandidateAction::Combine));
        let done = CandidateActionBasis::of(true, &TriagePlacement::Done, None, true);
        assert_eq!(done.actions(&rest), vec![CandidateAction::RevealFolder]);
        let importing = TriageRuntimeFacts {
            identification: None,
            importing: true,
        };
        assert!(!grouped.actions(&importing).contains(&CandidateAction::Separate));
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
