use std::future::Future;

use bae_core::diagnostics::Diagnostics;
use bae_util::task_failure::{joined, TaskFailed};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::types::BridgeError;

/// Schedule an operation builder on its owning runtime. The outer async block
/// is constructed on the caller, but `build` is not invoked until a runtime
/// worker polls it.
pub(crate) fn spawn<T, Build, Fut>(runtime: &Handle, build: Build) -> JoinHandle<T>
where
    T: Send + 'static,
    Build: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    runtime.spawn(async move { build().await })
}

/// The runtime the bridge's operations run on, and where a panic in one is
/// reported. A panicked operation is a bug: it is reported to the crash
/// reporter and fails with an internal error, the "Something went wrong" its
/// caller shows.
#[derive(Clone)]
pub(crate) struct OperationRuntime {
    runtime: Handle,
    diagnostics: Diagnostics,
}

impl OperationRuntime {
    pub(crate) fn new(runtime: Handle, diagnostics: Diagnostics) -> Self {
        Self {
            runtime,
            diagnostics,
        }
    }

    /// Run a fallible operation on its owning runtime. Dropping the returned
    /// future aborts the runtime task, which makes UniFFI cancellation own the
    /// work it started instead of detaching it.
    pub(crate) async fn run<T, Build, Fut>(
        self,
        operation: &'static str,
        build: Build,
    ) -> Result<T, BridgeError>
    where
        T: Send + 'static,
        Build: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, BridgeError>> + Send + 'static,
    {
        let task = spawn(&self.runtime, build);
        let _abort = AbortOnDrop(task.abort_handle());
        joined(operation, task)
            .await
            .map_err(|failed| self.failed(failed))?
    }

    /// Run an operation whose explicit domain cancellation must finish even if
    /// the foreign caller disappears. Dropping this outer future detaches the
    /// owned runtime task; the operation itself remains responsible for
    /// reaching its durable terminal state. Retain the runtime owner through
    /// result delivery, including when the operation consumes its last
    /// reference to that owner. A panic is reported whether or not the caller
    /// is still there to hear it.
    pub(crate) async fn run_to_completion<T, Owner, Build, Fut>(
        self,
        owner: Owner,
        operation: &'static str,
        build: Build,
    ) -> Result<T, BridgeError>
    where
        T: Send + 'static,
        Owner: Send + 'static,
        Build: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, BridgeError>> + Send + 'static,
    {
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        let operations = self.clone();
        drop(spawn(&self.runtime, move || async move {
            let _owner = owner;
            let result = joined(operation, spawn(&operations.runtime, build))
                .await
                .map_err(|failed| operations.failed(failed))
                .and_then(std::convert::identity);
            if let Err(Err(error)) = result_tx.send(result) {
                tracing::error!(operation, error = ?error, "persistent runtime operation failed after its caller disappeared");
            }
        }));
        result_rx.await.map_err(|error| {
            BridgeError::internal(format!(
                "persistent runtime operation ended without a result: {operation}: {error}"
            ))
        })?
    }

    /// Run `task` on the runtime with nobody awaiting it. A panic in it is
    /// reported, and `on_panic` ends what it served.
    pub(crate) fn spawn_reported(
        &self,
        name: &'static str,
        task: impl Future<Output = ()> + Send + 'static,
        on_panic: impl FnOnce(TaskFailed) + Send + 'static,
    ) {
        drop(
            self.diagnostics
                .spawn_reported(&self.runtime, name, task, on_panic),
        );
    }

    /// Run `future` to its end on the runtime, for a test driving the
    /// bridge from outside it.
    #[cfg(test)]
    pub(crate) fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    /// The error an operation that did not finish fails with: a panic is
    /// reported as the bug it is; a runtime shutting down under it is not.
    fn failed(&self, failed: TaskFailed) -> BridgeError {
        BridgeError::internal(self.diagnostics.bug(failed))
    }
}

/// Aborts its task when dropped: a finished task ignores it.
struct AbortOnDrop(tokio::task::AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::{mpsc, Arc};

    use bae_core::diagnostics::RecordingBugReporter;
    use tokio::sync::oneshot;

    use super::*;

    struct DropSignal(Option<oneshot::Sender<()>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                sender
                    .send(())
                    .expect("test observes the owned task being dropped");
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

        let (caller, constructed, polled) = result_rx
            .recv()
            .expect("receive test result")
            .expect("owned operation succeeds");
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

        tokio::time::timeout(std::time::Duration::from_secs(1), dropped_rx)
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
        tokio::time::timeout(std::time::Duration::from_secs(1), completed_rx)
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
                BridgeError::Diagnostic {
                    category: crate::types::BridgeErrorCategory::Internal,
                    ..
                }
            ),
            "{error:?}"
        );
        let reports = bugs.reports();
        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0].0, "test operation");
        assert!(
            reports[0].1.contains("the operation broke its guarantee"),
            "{reports:?}"
        );
        owned.shutdown_background();
    }

    /// Sends the operation of each bug reported, for a test to wait on.
    struct HeardBugs(std::sync::Mutex<mpsc::Sender<String>>);

    impl bae_core::diagnostics::BugReporter for HeardBugs {
        fn report(&self, operation: &str, _detail: &str) {
            self.0
                .lock()
                .unwrap()
                .send(operation.to_string())
                .expect("the test hears the report");
        }
    }

    /// A lifecycle operation that panics after its caller left is still
    /// reported: nobody else will hear of it.
    #[tokio::test]
    async fn a_persistent_operation_panicking_after_its_caller_left_is_reported() {
        let owned = owned_runtime();
        let (reported, reports) = mpsc::channel();
        let operations = OperationRuntime::new(
            owned.handle().clone(),
            Diagnostics::noop()
                .reporting_bugs_to(Arc::new(HeardBugs(std::sync::Mutex::new(reported)))),
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

        let operation = tokio::task::spawn_blocking(move || {
            reports.recv_timeout(std::time::Duration::from_secs(5))
        })
        .await
        .expect("the wait joins")
        .expect("the panic is reported");
        assert_eq!(operation, "test persistent operation");
        owned.shutdown_background();
    }
}
