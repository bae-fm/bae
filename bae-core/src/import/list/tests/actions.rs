use super::*;
use crate::import::triage::{CandidateAction, CandidateLiveState};
use crate::import::IdentificationStatus;

/// A row a run is identifying again stays where the tables place it, and only
/// its own live state keeps an import off it while the run is in flight.
#[test]
fn a_candidate_under_identification_offers_only_its_cancel_and_skip() {
    let mut rows = queue();
    rows.candidates.push(candidate("Release"));
    rows.states
        .insert("hash-Release".to_string(), auto_importable_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Pending);

    for status in [
        IdentificationStatus::Queued,
        IdentificationStatus::Running,
        IdentificationStatus::Finalizing,
    ] {
        let live = CandidateLiveState::of(
            &row.action_basis,
            TriageRuntimeFacts {
                identification: Some(status),
                import: None,
            },
        );
        assert_eq!(
            live.actions,
            vec![
                CandidateAction::CancelIdentification,
                CandidateAction::Skip,
                CandidateAction::RevealFolder
            ]
        );
    }
    assert!(
        CandidateLiveState::of(&row.action_basis, TriageRuntimeFacts::default())
            .actions
            .contains(&CandidateAction::Import)
    );
}
