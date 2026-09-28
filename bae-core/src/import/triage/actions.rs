use super::{
    IdentificationStatus, ImportStanding, TriagePlacement, TriageRuntimeFacts, TriageSkipAction,
};
use crate::identify::VerdictKind;

/// Commands offered for a candidate at its current lifecycle position —
/// every one a candidate can take, so every surface that acts on candidates
/// offers them from this one list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandidateAction {
    /// Import the candidate from its draft, wherever Pending places it.
    Import,
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
        CandidateAction::Import,
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
/// be acted on at all, where it is placed, whether its draft would import, and
/// what its stored lookup came to — none offers identifying it, and a failed
/// one offers a retry whatever the draft over it says.
///
/// The row carries it so the surface drawing the row can hand it back with
/// the row's live-state subscription: the commands a row offers are these
/// facts and what is running for it right now, and only core joins the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateActionBasis {
    pub actionable: bool,
    pub placement: TriagePlacement,
    /// Whether the draft shapes into a release an import can commit.
    pub draft_valid: bool,
    /// What the lookup stored for the candidate's current files came to, or
    /// `None` when none is stored.
    pub lookup: Option<StoredLookup>,
    /// Whether this release is folders a grouping reads as one, which the
    /// candidate offers to read as releases of their own.
    pub separable: bool,
}

/// What a candidate's stored lookup came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredLookup {
    /// It found a release, found none, or left the choice to the person.
    Answered,
    /// A source it asked could not answer.
    Failed,
}

impl CandidateActionBasis {
    /// `lookup` is the shape of the candidate's stored lookup result, or
    /// `None` with none.
    pub(crate) fn of(
        actionable: bool,
        placement: &TriagePlacement,
        draft_valid: bool,
        lookup: Option<VerdictKind>,
        separable: bool,
    ) -> Self {
        Self {
            actionable,
            placement: *placement,
            draft_valid,
            lookup: lookup.map(|kind| match kind {
                VerdictKind::Failed => StoredLookup::Failed,
                VerdictKind::Found | VerdictKind::NotFound | VerdictKind::ManualOnly => {
                    StoredLookup::Answered
                }
            }),
            separable,
        }
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
            || live.importing()
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
        // A running import can be cancelled; one writing its release
        // completes, so nothing is offered for it.
        match live.import {
            Some(ImportStanding::Queued | ImportStanding::Running) => {
                return vec![A::CancelImport]
            }
            Some(ImportStanding::Writing) => return Vec::new(),
            None => {}
        }
        let identifying = live.identifying();
        let placement = &self.placement;
        let mut actions = match placement {
            P::Done | P::Skipped => Vec::new(),
            _ if identifying => vec![A::CancelIdentification],
            P::Pending | P::Failed => {
                let mut actions = Vec::new();
                if self.draft_valid {
                    actions.push(A::Import);
                }
                // A stored lookup is shown as it stood, as Automatic shows it,
                // so only a candidate with none is offered identifying.
                if self.lookup.is_none() {
                    actions.push(A::Identify);
                }
                if self.lookup == Some(StoredLookup::Failed)
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
    use super::super::TriageTab;
    use super::*;

    fn basis(placement: TriagePlacement, lookup: Option<VerdictKind>) -> CandidateActionBasis {
        CandidateActionBasis::of(true, &placement, true, lookup, false)
    }

    fn identifying(status: IdentificationStatus) -> TriageRuntimeFacts {
        TriageRuntimeFacts {
            identification: Some(status),
            import: None,
        }
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
                basis(placement, None)
                    .actions(&rest)
                    .contains(&CandidateAction::Import),
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
            vec![
                CandidateAction::Restore,
                CandidateAction::Combine,
                CandidateAction::RevealFolder
            ]
        );
    }

    /// An import owning the candidate takes every command away but cancelling
    /// it, the skip included, wherever the tables still place it — whether it
    /// is still waiting for the worker or running.
    #[test]
    fn a_queued_or_running_import_offers_only_its_cancel() {
        for standing in [ImportStanding::Queued, ImportStanding::Running] {
            let importing = TriageRuntimeFacts {
                identification: None,
                import: Some(standing),
            };
            for placement in [TriagePlacement::Pending, TriagePlacement::Failed] {
                assert_eq!(
                    basis(placement, None).actions(&importing),
                    vec![CandidateAction::CancelImport, CandidateAction::RevealFolder],
                    "{standing:?}, {placement:?}"
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
            import: Some(ImportStanding::Writing),
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
                basis(TriagePlacement::Pending, Some(VerdictKind::Found))
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
        let grouped = CandidateActionBasis::of(true, &TriagePlacement::Pending, true, None, true);
        assert!(grouped.actions(&rest).contains(&CandidateAction::Separate));
        assert!(!grouped.actions(&rest).contains(&CandidateAction::Combine));
        let blocked = CandidateActionBasis::of(false, &TriagePlacement::Failed, true, None, true);
        assert_eq!(
            blocked.actions(&rest),
            vec![CandidateAction::Separate, CandidateAction::RevealFolder]
        );
        let lone = basis(TriagePlacement::Pending, Some(VerdictKind::Found));
        assert!(lone.actions(&rest).contains(&CandidateAction::Combine));
        let done = CandidateActionBasis::of(true, &TriagePlacement::Done, true, None, true);
        assert_eq!(done.actions(&rest), vec![CandidateAction::RevealFolder]);
        let importing = TriageRuntimeFacts {
            identification: None,
            import: Some(ImportStanding::Running),
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
            assert!(basis(placement, None)
                .actions(&rest)
                .contains(&CandidateAction::Identify));
            for lookup in [
                VerdictKind::Found,
                VerdictKind::NotFound,
                VerdictKind::ManualOnly,
                VerdictKind::Failed,
            ] {
                assert!(
                    !basis(placement, Some(lookup))
                        .actions(&rest)
                        .contains(&CandidateAction::Identify),
                    "{placement:?} with a stored {lookup:?} lookup"
                );
            }
        }
    }

    #[test]
    fn lookup_and_finalization_failures_offer_retry() {
        for placement in [TriagePlacement::Pending, TriagePlacement::Failed] {
            assert!(basis(placement, Some(VerdictKind::Failed))
                .actions(&TriageRuntimeFacts::default())
                .contains(&CandidateAction::RetryIdentification));
        }
        let failure = identifying(IdentificationStatus::FinalizationFailed {
            error: "Provider unavailable".to_owned(),
        });
        assert!(basis(TriagePlacement::Pending, None)
            .actions(&failure)
            .contains(&CandidateAction::RetryIdentification));
        assert!(!basis(TriagePlacement::Pending, None)
            .actions(&TriageRuntimeFacts::default())
            .contains(&CandidateAction::RetryIdentification));
        for lookup in [VerdictKind::Found, VerdictKind::NotFound, VerdictKind::ManualOnly] {
            assert!(
                !basis(TriagePlacement::Pending, Some(lookup))
                    .actions(&TriageRuntimeFacts::default())
                    .contains(&CandidateAction::RetryIdentification),
                "{lookup:?} is a lookup that finished"
            );
        }
    }
}
