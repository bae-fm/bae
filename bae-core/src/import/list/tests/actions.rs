use super::*;
use crate::import::triage::{CandidateAction, CandidateLiveState};
use crate::import::IdentificationStatus;

/// The Ready set is the tables': a Ready row a run is identifying again stays
/// in it, and only its own live state keeps a bulk import off it while the
/// run is in flight.
#[test]
fn a_ready_candidate_under_identification_stays_in_the_ready_set() {
    let mut rows = queue();
    rows.candidates.push(candidate("Release"));
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));

    let flat = flattened(&rows, &view(TriageTab::Pending));
    let row = row_for(&flat, "Release");
    assert_eq!(row.placement, TriagePlacement::Ready);
    assert!(row.selectable);
    assert_eq!(flat.summary.ready.len(), 1);

    for status in [
        IdentificationStatus::Queued,
        IdentificationStatus::Running,
        IdentificationStatus::Finalizing,
    ] {
        let live = CandidateLiveState::of(
            &row.action_basis,
            TriageRuntimeFacts {
                identification: Some(status),
                importing: false,
            },
        );
        assert_eq!(
            live.actions,
            vec![CandidateAction::CancelIdentification, CandidateAction::Skip]
        );
    }
    assert!(
        CandidateLiveState::of(&row.action_basis, TriageRuntimeFacts::default())
            .actions
            .contains(&CandidateAction::ImportReady)
    );
}
