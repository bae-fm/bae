//! Work the manager runs apart from its caller that panics: reported once as
//! a bug, and failing what it served.

use super::*;
use crate::library::{LiveRead, LiveReadError};

/// A preference write that panics fails as something that went wrong, is
/// reported once, and leaves the next write free to run.
#[tokio::test]
async fn a_preference_write_that_panics_is_reported_and_the_next_one_runs() {
    let (manager, _temp_dir, bugs) = setup_test_manager_reporting_bugs().await;

    let error = manager
        .update_preferences(|_| panic!("the edit broke"))
        .await
        .expect_err("a write that panicked fails");

    assert!(
        matches!(error, crate::config::ConfigError::Unfinished(_)),
        "{error}"
    );
    assert!(matches!(
        LibraryError::from(error).category(),
        crate::ui::UiErrorCategory::Internal
    ));
    assert_eq!(bugs.reports().len(), 1, "{:?}", bugs.reports());
    manager
        .set_show_remaining_time(true)
        .await
        .expect("the next write runs");
    assert!(manager.get_config().prefs.show_remaining_time);
}

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

/// A subscription read as a channel of results whose task panics sends the
/// failure as its last result, then ends.
#[tokio::test]
async fn a_channel_subscription_whose_task_panics_sends_its_failure_last() {
    let (manager, _temp_dir, bugs) = setup_test_manager_reporting_bugs().await;
    let (values, mut read) = tokio::sync::mpsc::unbounded_channel::<Result<(), LibraryError>>();
    manager.spawn_subscription_task("test subscription", values.clone(), async move {
        let _values = values;
        panic!("the subscription broke");
    });

    let failure = read.recv().await;

    assert!(
        matches!(failure, Some(Err(LibraryError::Internal(_)))),
        "{failure:?}"
    );
    assert!(
        read.recv().await.is_none(),
        "the failure is the last result"
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
