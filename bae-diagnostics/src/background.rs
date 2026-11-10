//! A panic in work apart from its caller, reported as the bug it is.
//!
//! Five shapes, decided by who waits on the work:
//!
//! - Work a caller awaits runs under `bae_util::task_failure::caught`, and
//!   the caller hands the failure to [`Diagnostics::panicked`] before turning
//!   it into its own error.
//! - Work a caller waits on, on a thread of its own with the stack it needs,
//!   runs under [`Diagnostics::run_on_thread`]: a panic is reported and
//!   handed back to the caller.
//! - Work on the blocking pool runs under [`Diagnostics::blocking`], to its
//!   end even when its caller stops waiting: a panic is reported to a caller
//!   still waiting as the first shape's is, and to the crash reporter once
//!   none is.
//! - Work nobody awaits runs under [`Diagnostics::spawn_reported`] or
//!   [`Diagnostics::spawn_thread_reported`]: a panic is reported, and the
//!   owner's `on_panic` ends what the work served so its reader sees it end.
//!   Work whose owner ends what it served the same way whether it finished or
//!   panicked runs under [`Diagnostics::spawn_ending`]: one `end` hears how
//!   it ended, and what `end` returns is what a joiner of the task gets.
//! - A lock whose guarded state spans operations is taken with
//!   [`lock_or_crash`]: a panic that poisoned it left that state not known to
//!   be valid.
//!
//! A panic's report names where in the code it began, which coven's catch
//! records with the panic (`Bug::location`).

use std::fmt;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};

use bae_util::task_failure::{caught, caught_sync, Bug, TaskFailed};

use super::Diagnostics;

/// coven's bugs in work nobody waits for, such as a store's sync loop and
/// upload worker or work whose caller stopped waiting, reach the crash
/// reporter as core's own do.
impl coven::task::BugSink for Diagnostics {
    fn report(&self, bug: &Bug) {
        self.report_panic(bug.operation, bug);
    }
}

impl Diagnostics {
    /// This reporter as coven's bug sink: where a bug in work nobody waits
    /// for goes.
    pub fn bug_sink(&self) -> coven::task::BugSinkRef {
        Arc::new(self.clone())
    }

    /// Report `failed` when it is a panic, and hand it back for the caller's
    /// own error type. A runtime shutting down under the work is not a bug.
    pub fn bug(&self, failed: TaskFailed) -> TaskFailed {
        match failed {
            TaskFailed::Panicked(bug) => TaskFailed::Panicked(self.panicked(bug)),
            TaskFailed::Shutdown(shutdown) => {
                tracing::debug!(
                    operation = shutdown.operation,
                    outcome = %shutdown.outcome,
                    "work ended unfinished: its runtime is shutting down"
                );
                TaskFailed::Shutdown(shutdown)
            }
        }
    }

    /// Run `work`, named `operation`, on the blocking pool, where it runs to
    /// its end even when its caller stops waiting. A panic in it is reported
    /// either way: here, and handed back as `TaskFailed::Panicked`, to a
    /// caller still waiting; to the crash reporter as coven's bugs in work
    /// nobody waits for go, once none is (`coven::task::blocking_completing`).
    /// The pool dropping it unstarted, as its runtime shuts down, comes back
    /// as `TaskFailed::Shutdown`.
    ///
    /// The work's value is handed back whole, an error in it included: an
    /// error a store call carries a bug in has reached the reporter already,
    /// as the store's call returned.
    pub async fn blocking<T: Send + 'static>(
        &self,
        operation: &'static str,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, TaskFailed> {
        let completing = coven::task::blocking_completing(operation, self.bug_sink(), move || {
            Ok::<T, std::convert::Infallible>(work())
        });
        match completing.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(never)) => match never {},
            Err(failed) => Err(self.bug(failed)),
        }
    }

    /// Start `work`, named `operation`, on the blocking pool with nobody
    /// waiting for it: it runs to its end, and a panic in it goes to the crash
    /// reporter. For work that hands its results on through a channel it owns.
    pub fn blocking_unawaited(
        &self,
        operation: &'static str,
        work: impl FnOnce() + Send + 'static,
    ) {
        drop(coven::task::blocking_completing(operation, self.bug_sink(), move || {
            work();
            Ok::<(), std::convert::Infallible>(())
        }));
    }

    /// Report `bug`, work that panicked, and hand it back for the caller's
    /// own error type: what `caught` and `caught_sync` fail with, and the
    /// panics in coven's errors.
    pub fn panicked(&self, bug: Bug) -> Bug {
        self.report_panic(bug.operation, &bug);
        bug
    }

    /// Report `bug` as `operation`'s, naming where in the code it began.
    fn report_panic(&self, operation: &str, bug: &Bug) {
        self.report_bug(operation, &Located(bug));
    }

    /// Run `task` on `runtime` with nobody awaiting it. On a panic, report it
    /// and call `on_panic`, so the task's owner ends what the task served in a
    /// way its reader sees: sends the completion, fails the buffer, ends the
    /// run. Aborting the returned task aborts the work, and calls nothing.
    pub fn spawn_reported(
        &self,
        runtime: &tokio::runtime::Handle,
        name: &'static str,
        task: impl Future<Output = ()> + Send + 'static,
        on_panic: impl FnOnce(Bug) + Send + 'static,
    ) -> ReportedTask {
        self.spawn_ending(runtime, name, task, move |ended| {
            if let Err(bug) = ended {
                on_panic(bug);
            }
        })
    }

    /// Run `task` on `runtime`, and hand how it ended to `end`: its value, or
    /// its panic, reported first. For work whose owner ends what it served
    /// the same way either way, with something only one of the two may take:
    /// the handler a failed buffer's reader hears, the channel a transfer
    /// says it failed on. What `end` returns is what the task's joiner gets
    /// ([`ReportedTask::outcome`]). A panic in `end` is reported too, and
    /// goes no further. Aborting the returned task aborts the work, and calls
    /// nothing.
    pub fn spawn_ending<T: Send + 'static, U: Send + 'static>(
        &self,
        runtime: &tokio::runtime::Handle,
        name: &'static str,
        task: impl Future<Output = T> + Send + 'static,
        end: impl FnOnce(Result<T, Bug>) -> U + Send + 'static,
    ) -> ReportedTask<U> {
        let diagnostics = self.clone();
        let task = runtime.spawn(async move {
            let ended = caught(name, task).await.map_err(|bug| diagnostics.panicked(bug));
            caught_sync(name, || end(ended))
                .inspect_err(|again| diagnostics.report_panic("end what the work served", again))
        });
        ReportedTask { task }
    }

    /// Start a dedicated thread running `body`. On a panic, report it and
    /// call `on_panic` on that thread, so the thread's owner learns the work
    /// ended.
    pub fn spawn_thread_reported(
        &self,
        name: &'static str,
        body: impl FnOnce() + Send + 'static,
        on_panic: impl FnOnce(Bug) + Send + 'static,
    ) -> std::io::Result<ReportedThread> {
        let diagnostics = self.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "this is the reported thread spawner: the body runs under caught_sync"
        )]
        let thread = std::thread::Builder::new().name(name.to_string()).spawn(move || {
            if let Err(failed) = caught_sync(name, body) {
                diagnostics.end_panicked(failed, on_panic);
            }
        })?;
        Ok(ReportedThread { name, thread })
    }

    /// Run `work`, named `name`, on a thread of its own with a stack of
    /// `stack_size` bytes, and wait for it: for work deeper than the
    /// caller's thread can hold. A panic in it is reported and handed back.
    pub fn run_on_thread<T: Send + 'static>(
        &self,
        name: &'static str,
        stack_size: usize,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, ThreadFailed> {
        let diagnostics = self.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "this is the reported thread spawner: the work runs under caught_sync"
        )]
        let thread = std::thread::Builder::new()
            .name(name.to_string())
            .stack_size(stack_size)
            .spawn(move || caught_sync(name, work).map_err(|bug| diagnostics.panicked(bug)))
            .map_err(|source| ThreadFailed::Start { name, source })?;
        match thread.join() {
            Ok(done) => done.map_err(ThreadFailed::Panicked),
            // The thread catches its work's panic, so this is reporting it
            // panicking.
            Err(payload) => {
                let message = bae_util::task_failure::panic_message(payload.as_ref());
                tracing::error!(thread = name, "reporting a panic panicked: {message}");
                Err(ThreadFailed::Panicked(Bug { operation: name, message, location: None }))
            }
        }
    }

    /// Report a panic, then let its owner end what the work served. A panic in
    /// `on_panic` itself is reported too, and goes no further: it runs where
    /// nothing above it catches.
    fn end_panicked(&self, bug: Bug, on_panic: impl FnOnce(Bug)) {
        let operation = bug.operation;
        let bug = self.panicked(bug);
        if let Err(again) = caught_sync(operation, || on_panic(bug)) {
            self.report_panic("end what the work served", &again);
        }
    }
}

/// A bug as its report tells it: its text, then where in the code it began,
/// when a catch saw it begin.
struct Located<'a>(&'a Bug);

impl fmt::Display for Located<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0.location {
            Some(location) => write!(f, "{} (at {location})", self.0),
            None => write!(f, "{}", self.0),
        }
    }
}

/// A task [`Diagnostics::spawn_reported`] or [`Diagnostics::spawn_ending`]
/// started, which hands its joiner what its `end` returned. Dropping it leaves
/// the task running.
#[derive(Debug)]
pub struct ReportedTask<U = ()> {
    task: tokio::task::JoinHandle<Result<U, Bug>>,
}

/// Why a joiner of a [`ReportedTask`] gets nothing from its `end`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unfinished {
    /// The task was aborted, or its runtime dropped it as it shut down,
    /// before `end` ran.
    #[error("the work was stopped before it ended")]
    Stopped,
    /// `end` panicked, and was reported.
    #[error(transparent)]
    Panicked(Bug),
}

impl<U> ReportedTask<U> {
    /// End the work at its next await. Its `end` is not called.
    pub fn abort(&self) {
        self.task.abort();
    }

    /// A handle that ends the work as [`Self::abort`] does, for an owner that
    /// hands the task's join to someone else.
    pub fn abort_handle(&self) -> tokio::task::AbortHandle {
        self.task.abort_handle()
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    /// Wait until the work has ended: finished, aborted, or panicked and
    /// already reported.
    pub async fn ended(self) {
        // A stop is an end the caller or the runtime asked for, and a panic in
        // `end` was reported where it happened.
        if let Err(unfinished) = self.outcome().await {
            tracing::debug!("a reported task ended unfinished: {unfinished}");
        }
    }

    /// Wait until the work has ended, and take what its `end` returned.
    pub async fn outcome(self) -> Result<U, Unfinished> {
        match self.task.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(bug)) => Err(Unfinished::Panicked(bug)),
            // The task catches its work's panic and `end`'s, so only
            // reporting a panic panicking is left.
            Err(error) if error.is_panic() => {
                let message = bae_util::task_failure::panic_message(error.into_panic().as_ref());
                tracing::error!("reporting a panic panicked: {message}");
                Err(Unfinished::Panicked(Bug {
                    operation: "report a panic",
                    message,
                    location: None,
                }))
            }
            Err(_) => Err(Unfinished::Stopped),
        }
    }
}

/// Why [`Diagnostics::run_on_thread`] has no value to hand back.
#[derive(Debug, thiserror::Error)]
pub enum ThreadFailed {
    /// The system refused the thread.
    #[error("start the {name} thread: {source}")]
    Start {
        name: &'static str,
        #[source]
        source: std::io::Error,
    },
    /// The work panicked, and was reported.
    #[error(transparent)]
    Panicked(Bug),
}

/// A thread [`Diagnostics::spawn_thread_reported`] started. Dropping it
/// leaves the thread running.
#[derive(Debug)]
pub struct ReportedThread {
    name: &'static str,
    thread: std::thread::JoinHandle<()>,
}

impl ReportedThread {
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// The thread, for an owner that unparks it.
    pub fn thread(&self) -> &std::thread::Thread {
        self.thread.thread()
    }

    /// Wait for the thread to exit. A panic in its body was reported on the
    /// thread; one that escaped even that (the crash reporter itself
    /// panicking) is logged here.
    pub fn join(self) {
        if let Err(payload) = self.thread.join() {
            tracing::error!(
                thread = self.name,
                "reporting a panic panicked: {}",
                bae_util::task_failure::panic_message(payload.as_ref())
            );
        }
    }
}

/// Lock `mutex`, whose guarded state spans operations. If a panic poisoned
/// it, that state was left partway through a change and nothing after can
/// trust it: report `what` and abort the process.
pub fn lock_or_crash<'a, T>(
    diagnostics: &Diagnostics,
    what: &'static str,
    mutex: &'a Mutex<T>,
) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(_) => {
            diagnostics
                .report_bug(what, &"a panic while it was held left it partway through a change");
            std::process::abort()
        }
    }
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
