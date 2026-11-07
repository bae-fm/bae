use std::future::pending;
use std::sync::{mpsc, Arc};

use bae_diagnostics::RecordingBugReporter;
use tokio::sync::oneshot;

use super::*;
use bae_util::test_channels::HANG_GUARD;

struct DropSignal(Option<oneshot::Sender<()>>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            sender.send(()).expect("test observes the owned task being dropped");
        }
    }
}

fn owned_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name("owned-operation-test")
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("build owned runtime")
}

/// Operations on `owned` whose panics reach the returned reporter.
fn reporting_operations(
    owned: &tokio::runtime::Runtime,
) -> (OperationRuntime, Arc<RecordingBugReporter>) {
    let bugs = Arc::new(RecordingBugReporter::default());
    let operations = OperationRuntime::new(
        owned.handle().clone(),
        Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    );
    (operations, bugs)
}

#[test]
fn run_constructs_and_polls_on_the_owned_runtime() {
    let owned = owned_runtime();
    let operations = OperationRuntime::new(owned.handle().clone(), Diagnostics::noop());
    let (result_tx, result_rx) = mpsc::sync_channel(1);

    std::thread::Builder::new()
        .name("foreign-operation-test".to_string())
        .stack_size(544 * 1024)
        .spawn(move || {
            let caller = std::thread::current().id();
            let result = owned.block_on(operations.run("test operation", move || {
                let constructed = std::thread::current().id();
                async move {
                    let polled = std::thread::current().id();
                    Ok((caller, constructed, polled))
                }
            }));
            result_tx.send(result).expect("return test result");
        })
        .expect("spawn foreign caller")
        .join()
        .expect("foreign caller does not panic");

    let (caller, constructed, polled) =
        result_rx.recv().expect("receive test result").expect("owned operation succeeds");
    assert_ne!(caller, constructed);
    assert_eq!(constructed, polled);
}

#[tokio::test]
async fn dropping_run_aborts_the_owned_task() {
    let owned = owned_runtime();
    let operations = OperationRuntime::new(owned.handle().clone(), Diagnostics::noop());
    let (started_tx, started_rx) = oneshot::channel();
    let (dropped_tx, dropped_rx) = oneshot::channel();

    let caller = tokio::spawn(operations.run("test operation", move || async move {
        let _drop_signal = DropSignal(Some(dropped_tx));
        started_tx.send(()).expect("test receives task start");
        pending::<Result<(), BridgeError>>().await
    }));
    started_rx.await.expect("owned task starts");
    caller.abort();

    tokio::time::timeout(HANG_GUARD, dropped_rx)
        .await
        .expect("cancellation aborts the owned task")
        .expect("owned task holds the drop signal");
    owned.shutdown_background();
}

#[tokio::test]
async fn dropping_run_to_completion_keeps_the_owned_task_alive() {
    let owned = owned_runtime();
    let operations = OperationRuntime::new(owned.handle().clone(), Diagnostics::noop());
    let (started_tx, started_rx) = oneshot::channel();
    let (continue_tx, continue_rx) = oneshot::channel();
    let (completed_tx, completed_rx) = oneshot::channel();

    let caller = tokio::spawn(operations.run_to_completion(
        (),
        "test persistent operation",
        move || async move {
            started_tx.send(()).expect("test receives task start");
            continue_rx.await.expect("test releases operation");
            completed_tx.send(()).expect("test observes completion");
            Ok(())
        },
    ));
    started_rx.await.expect("owned task starts");
    caller.abort();
    continue_tx.send(()).expect("release owned operation");
    tokio::time::timeout(HANG_GUARD, completed_rx)
        .await
        .expect("owned operation completes after caller disappears")
        .expect("completion signal remains connected");
    owned.shutdown_background();
}

/// A panic in an operation fails it with the internal error the host
/// shows as "Something went wrong", and reaches the crash reporter once.
#[tokio::test]
async fn a_panicking_operation_fails_as_internal_and_is_reported() {
    let owned = owned_runtime();
    let (operations, bugs) = reporting_operations(&owned);

    let result = operations
        .run("test operation", || async {
            if true {
                panic!("the operation broke its guarantee");
            }
            Ok(())
        })
        .await;

    let error = result.expect_err("a panicked operation fails");
    assert!(
        matches!(
            error,
            BridgeError::Diagnostic { category: crate::types::BridgeErrorCategory::Internal, .. }
        ),
        "{error:?}"
    );
    let reports = bugs.reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0].0, "test operation");
    assert!(reports[0].1.contains("the operation broke its guarantee"), "{reports:?}");
    owned.shutdown_background();
}

/// A lifecycle operation that panics after its caller left is still
/// reported: nobody else will hear of it.
#[tokio::test]
async fn a_persistent_operation_panicking_after_its_caller_left_is_reported() {
    let owned = owned_runtime();
    let reports = Arc::new(bae_diagnostics::HeardBugReports::default());
    let operations = OperationRuntime::new(
        owned.handle().clone(),
        Diagnostics::noop().reporting_bugs_to(reports.clone()),
    );
    let (started_tx, started_rx) = oneshot::channel();
    let (continue_tx, continue_rx) = oneshot::channel::<()>();

    let caller = tokio::spawn(operations.run_to_completion(
        (),
        "test persistent operation",
        move || async move {
            started_tx.send(()).expect("test receives task start");
            continue_rx.await.expect("test releases operation");
            if true {
                panic!("the lifecycle operation broke its guarantee");
            }
            Ok(())
        },
    ));
    started_rx.await.expect("owned task starts");
    caller.abort();
    continue_tx.send(()).expect("release owned operation");

    let (operation, _) =
        tokio::time::timeout(HANG_GUARD, reports.next()).await.expect("the panic is reported");
    assert_eq!(operation, "test persistent operation");
    owned.shutdown_background();
}
