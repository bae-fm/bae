use std::future::Future;

use bae_diagnostics::Diagnostics;
use bae_util::task_failure::{caught, joined, Bug, TaskFailed};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::types::BridgeError;

/// Schedule an operation builder, named `operation`, on its owning runtime.
/// The outer async block is constructed on the caller, but `build` is not
/// invoked until a runtime worker polls it. The operation runs under a catch
/// of its panic, so the join hands the panic back as a `Bug` naming where it
/// began (`joined`).
pub(crate) fn spawn<T, Build, Fut>(
    runtime: &Handle,
    operation: &'static str,
    build: Build,
) -> JoinHandle<Result<T, Bug>>
where
    T: Send + 'static,
    Build: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    #[expect(
        clippy::disallowed_methods,
        reason = "its callers join the task at once and report its panic through Diagnostics::bug"
    )]
    runtime.spawn(caught(operation, async move { build().await }))
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
        Self { runtime, diagnostics }
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
        let task = spawn(&self.runtime, operation, build);
        let _abort = AbortOnDrop(task.abort_handle());
        joined(operation, task).await.map_err(|failed| self.failed(failed))?
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
        drop(self.diagnostics.spawn_ending(
            &self.runtime,
            operation,
            async move { build().await },
            move |ended| {
                let _owner = owner;
                // A panic was reported as it happened.
                let result = ended.unwrap_or_else(|bug| {
                    Err(BridgeError::unfinished(TaskFailed::Panicked(bug)))
                });
                if let Err(Err(error)) = result_tx.send(result) {
                    tracing::error!(operation, error = ?error, "persistent runtime operation failed after its caller disappeared");
                }
            },
        ));
        result_rx.await.map_err(|error| {
            BridgeError::internal(format!(
                "persistent runtime operation ended without a result: {operation}: {error}"
            ))
        })?
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
        BridgeError::unfinished(self.diagnostics.bug(failed))
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
#[path = "operation_runtime_tests.rs"]
mod tests;
