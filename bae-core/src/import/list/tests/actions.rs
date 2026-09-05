use super::*;
use crate::import::triage::CandidateAction;

#[test]
fn a_ready_candidate_under_identification_is_excluded_from_bulk_import() {
    let mut rows = queue();
    rows.candidates.push(candidate("Release"));
    rows.states
        .insert("hash-Release".to_string(), ready_state("mb-1"));

    for status in [
        IdentificationStatus::Queued,
        IdentificationStatus::Running,
        IdentificationStatus::Finalizing,
    ] {
        let mut request = request(view(TriageTab::Pending));
        request.runtime_facts.insert(
            key("Release"),
            TriageRuntimeFacts {
                identification: Some(status),
                importing: false,
            },
        );
        let flat = flatten(&rows, &request).expect("the queue flattens");
        let row = row_for(&flat, "Release");
        assert_eq!(row.placement, TriagePlacement::Ready);
        assert_eq!(row.actions, vec![CandidateAction::Skip]);
        assert!(!row.selectable);
        assert!(flat.summary.ready.is_empty());
    }

    let idle = flattened(&rows, &view(TriageTab::Pending));
    assert!(row_for(&idle, "Release")
        .actions
        .contains(&CandidateAction::ImportReady));
    assert_eq!(idle.summary.ready.len(), 1);
}
