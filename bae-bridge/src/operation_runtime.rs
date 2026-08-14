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
    (&mut task.task).await.map_err(|error| {
        BridgeError::internal(format!(
            "owned runtime operation failed to complete: {error}"
        ))
    })?
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
}
