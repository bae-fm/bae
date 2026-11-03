//! A durable write the import handle runs on its own task, when that task
//! panics.

use super::*;
use crate::ImportError;

/// The panic comes back to the caller as an internal error, and the crash
/// reporter hears of it once.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_that_panics_fails_its_caller_and_is_reported() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let (parts, _, _tmp) = setup_test_import_reporting_to(diagnostics).await;
    let handle = parts.start().await.expect("start the import service");

    let result: Result<(), ImportError> = handle
        .committed(async {
            if true {
                panic!("the write's bookkeeping broke");
            }
            Ok(())
        })
        .await;

    let Err(ImportError::Internal { detail }) = result else {
        panic!("a panicked write fails as internal, got {result:?}");
    };
    assert_eq!(detail, "a candidate write panicked: the write's bookkeeping broke");
    assert_eq!(bugs.reports(), vec![("a candidate write".to_string(), detail)]);
}
