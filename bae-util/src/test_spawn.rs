//! How a test starts a task or a thread.
//!
//! bae's own work starts through a reported spawner (`Diagnostics`'s
//! `spawn_reported`, `spawn_thread_reported`, `blocking` and the rest), which
//! hears a panic and ends what the work served. A test has no crash reporter
//! to report to, and what it starts ends with the test, so a test starts its
//! tasks and threads here, and these functions are the only test code that
//! makes the spawn calls `clippy.toml` disallows.

use std::future::Future;

/// Start `future` as a task on the calling test's runtime.
pub fn task<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[expect(
        clippy::disallowed_methods,
        reason = "test code: the task ends with its test, which has no crash reporter to report to"
    )]
    tokio::spawn(future)
}

/// Start `future` as a task on `runtime`, for a test that drives a runtime of
/// its own from outside it.
pub fn task_on<F>(runtime: &tokio::runtime::Handle, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    #[expect(
        clippy::disallowed_methods,
        reason = "test code: the task ends with its test, which has no crash reporter to report to"
    )]
    runtime.spawn(future)
}

/// Start `work` on a new thread.
pub fn thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> std::thread::JoinHandle<T> {
    #[expect(
        clippy::disallowed_methods,
        reason = "test code: the thread ends with its test, which has no crash reporter to report to"
    )]
    std::thread::spawn(work)
}

/// Start `work` on a new thread that `builder` sets up, for a test that needs
/// the thread's name or stack size.
pub fn thread_with<T: Send + 'static>(
    builder: std::thread::Builder,
    work: impl FnOnce() -> T + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<T>> {
    #[expect(
        clippy::disallowed_methods,
        reason = "test code: the thread ends with its test, which has no crash reporter to report to"
    )]
    builder.spawn(work)
}

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
