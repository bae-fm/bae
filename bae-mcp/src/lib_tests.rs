use std::sync::Arc;

use bae_diagnostics::{Diagnostics, RecordingBugReporter};
use rmcp::model::{CallToolResponse, ErrorCode};
use rmcp::ErrorData;

use super::answer_or_report;

/// A tool call whose work broke its own guarantee.
async fn panicking_tool_call() -> Result<CallToolResponse, ErrorData> {
    panic!("the tool broke its guarantee")
}

/// rmcp runs a tool call on a task nothing joins, so its panic is reported
/// where the call is answered, and the agent gets an internal error instead
/// of no answer.
#[tokio::test]
async fn a_tool_call_that_panics_is_reported_and_answered_with_an_internal_error() {
    let bugs = Arc::new(RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(bugs.clone());

    let answer = answer_or_report(&diagnostics, panicking_tool_call()).await;

    let error = answer.expect_err("a panicked tool call has no answer");
    assert_eq!(error.code, ErrorCode::INTERNAL_ERROR);
    assert_eq!(
        bugs.reports(),
        [(
            "answer an MCP tool call".to_string(),
            "answer an MCP tool call panicked: the tool broke its guarantee".to_string()
        )]
    );
}
