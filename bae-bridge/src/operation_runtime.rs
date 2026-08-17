use std::future::Future;

use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::types::BridgeError;

/// Schedule an operation builder on its owning runtime. The outer async block
/// is constructed on the caller, but `build` is not invoked until a runtime
/// worker polls it.
pub(crate) fn spawn<T, Build, Fut>(runtime: Handle, build: Build) -> JoinHandle<T>
where
    T: Send + 'static,
    Build: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    runtime.spawn(async move { build().await })
}

/// Run a fallible operation on its owning runtime. Dropping the returned future
/// aborts the runtime task, which makes UniFFI cancellation own the work it
/// started instead of detaching it.
pub(crate) async fn run<T, Build, Fut>(runtime: Handle, build: Build) -> Result<T, BridgeError>
where
    T: Send + 'static,
    Build: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, BridgeError>> + Send + 'static,
{
    let mut task = AbortOnDrop::new(spawn(runtime, build));
    (&mut task.task).await.map_err(join_error)?
}

/// Run an operation whose explicit domain cancellation must finish even if the
/// foreign caller disappears. Dropping this outer future detaches the owned
/// runtime task; the operation itself remains responsible for reaching its
/// durable terminal state.
pub(crate) async fn run_to_completion<T, Build, Fut>(
    runtime: Handle,
    operation: &'static str,
    build: Build,
) -> Result<T, BridgeError>
where
    T: Send + 'static,
    Build: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, BridgeError>> + Send + 'static,
{
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    let operation_runtime = runtime.clone();
    drop(spawn(runtime, move || async move {
        let result = spawn(operation_runtime, build)
            .await
            .map_err(join_error)
            .and_then(std::convert::identity);
        if let Err(Err(error)) = result_tx.send(result) {
            tracing::error!(operation, error = ?error, "persistent runtime operation failed after its caller disappeared");
        }
    }));
    result_rx.await.map_err(|_| {
        BridgeError::internal(format!(
            "persistent runtime operation ended without a result: {operation}"
        ))
    })?
}

fn join_error(error: tokio::task::JoinError) -> BridgeError {
    BridgeError::internal(format!(
        "owned runtime operation failed to complete: {error}"
    ))
}

struct AbortOnDrop<T> {
    task: JoinHandle<T>,
}

impl<T> AbortOnDrop<T> {
    fn new(task: JoinHandle<T>) -> Self {
        Self { task }
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::mpsc;

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

    #[test]
    fn run_constructs_and_polls_on_the_owned_runtime() {
        let owned = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("owned-operation-test")
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .expect("build owned runtime");
        let handle = owned.handle().clone();
        let (result_tx, result_rx) = mpsc::sync_channel(1);

        std::thread::Builder::new()
            .name("foreign-operation-test".to_string())
            .stack_size(544 * 1024)
            .spawn(move || {
                let caller = std::thread::current().id();
                let result = owned.block_on(run(handle, move || {
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
        let owned = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .expect("build owned runtime");
        let handle = owned.handle().clone();
        let (started_tx, started_rx) = oneshot::channel();
        let (dropped_tx, dropped_rx) = oneshot::channel();

        let caller = tokio::spawn(run(handle, move || async move {
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
        let owned = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .expect("build owned runtime");
        let handle = owned.handle().clone();
        let (started_tx, started_rx) = oneshot::channel();
        let (continue_tx, continue_rx) = oneshot::channel();
        let (completed_tx, completed_rx) = oneshot::channel();

        let caller = tokio::spawn(run_to_completion(
            handle,
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
}
