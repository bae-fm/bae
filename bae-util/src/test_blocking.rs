//! Blocking work a test runs off its runtime.

use std::future::Future;

/// Start `work` on the blocking pool at once, and give back a future of its
/// value. A test awaits it, so a panic in the work fails the test as it would
/// inline; bae's own blocking work runs under `Diagnostics::blocking`, whose
/// panic reaches the crash reporter whether or not its caller still waits.
pub fn on_blocking_pool<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = T> + Send {
    #[expect(
        clippy::disallowed_methods,
        reason = "a test awaits its blocking work, so a panic in it fails the test"
    )]
    let task = tokio::task::spawn_blocking(work);
    async move {
        match task.await {
            Ok(value) => value,
            Err(error) => match error.try_into_panic() {
                Ok(payload) => std::panic::resume_unwind(payload),
                Err(error) => panic!("a test's blocking work ended unfinished: {error}"),
            },
        }
    }
}
