//! Work the manager runs apart from its caller that panics: reported once as
//! a bug, and failing what it served.

use super::*;
use crate::library::{LiveRead, LiveReadError};

/// A live read whose task panics ends with the failure, which its reader
/// tells apart from its own cancel, and the panic is reported once.
#[tokio::test]
async fn a_live_read_whose_task_panics_ends_with_its_failure() {
    let (manager, _temp_dir, bugs) = setup_test_manager_reporting_bugs().await;
    let (requests, _) = tokio::sync::watch::channel(());
    let (values, read) = crate::library::live_read_channel::<()>();
    let task = crate::library::CancellationToken::new();
    manager.spawn_stoppable_subscription_task(
        "test subscription",
        task.clone(),
        values.clone(),
        async move {
            let _values = values;
            panic!("the subscription broke");
        },
    );
    let read = LiveRead::new(requests, read, task);

    let ended = read.next().await;

    assert!(
        matches!(ended, Err(LiveReadError::Failed(LibraryError::Internal(_)))),
        "{ended:?}"
    );
    assert_eq!(bugs.reports().len(), 1, "{:?}", bugs.reports());
}

/// A file operation that panics fails as something that went wrong, is
/// reported once, and leaves the library able to close.
#[tokio::test]
async fn a_file_operation_that_panics_fails_and_is_reported() {
    let (manager, _temp_dir, bugs) = setup_test_manager_reporting_bugs().await;

    let error = manager
        .run_file_operation("test operation", |_closing| async { broken_operation() })
        .await
        .expect_err("a panicked operation fails");

    assert!(matches!(error, LibraryError::Internal(_)), "{error}");
    assert_eq!(bugs.reports().len(), 1, "{:?}", bugs.reports());
    manager.close().await.expect("the library closes");
}

fn broken_operation() -> Result<(), LibraryError> {
    panic!("the operation broke")
}
