use super::*;

#[test]
fn an_imported_candidate_reports_imported_while_its_execution_still_retires() {
    let standing = CandidateStanding {
        skipped: false,
        imported: true,
        claimed: true,
    };
    assert!(!standing.answerable());
    match standing.editable() {
        Err(super::super::ImportError::CandidateAlreadyImported) => {}
        Err(error) => panic!("the durable imported state must take precedence: {error:?}"),
        Ok(()) => panic!("an imported candidate must not become editable"),
    }
}
