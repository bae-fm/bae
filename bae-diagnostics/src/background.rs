//! A panic in work apart from its caller, reported as the bug it is.
//!
//! Three shapes, decided by who waits on the work:
//!
//! - Work a caller awaits joins through `bae_util::task_failure` (`blocking`,
//!   `joined`, `caught`), and the caller hands the failure to
//!   [`Diagnostics::bug`] before turning it into its own error.
//! - Work nobody awaits runs under [`Diagnostics::spawn_reported`] or
//!   [`Diagnostics::spawn_thread_reported`]: a panic is reported, and the
//!   owner's `on_panic` ends what the work served so its reader sees it end.
//! - A lock whose guarded state spans operations is taken with
//!   [`lock_or_crash`]: a panic that poisoned it left that state not known to
//!   be valid.

use std::future::Future;
use std::sync::{Mutex, MutexGuard};

use bae_util::task_failure::{caught, caught_sync, Bug, TaskFailed};

use super::Diagnostics;

impl Diagnostics {
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

    /// Report `bug`, work that panicked, and hand it back for the caller's
    /// own error type: what `caught` and `caught_sync` fail with, and the
    /// panics in coven's errors.
    pub fn panicked(&self, bug: Bug) -> Bug {
        self.report_bug(bug.operation, &bug);
        bug
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
        let diagnostics = self.clone();
        ReportedTask {
            task: runtime.spawn(async move {
                if let Err(failed) = caught(name, task).await {
                    diagnostics.end_panicked(failed, on_panic);
                }
            }),
        }
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
        let thread = std::thread::Builder::new().name(name.to_string()).spawn(move || {
            if let Err(failed) = caught_sync(name, body) {
                diagnostics.end_panicked(failed, on_panic);
            }
        })?;
        Ok(ReportedThread { name, thread })
    }

    /// Report a panic, then let its owner end what the work served. A panic in
    /// `on_panic` itself is reported too, and goes no further: it runs where
    /// nothing above it catches.
    fn end_panicked(&self, bug: Bug, on_panic: impl FnOnce(Bug)) {
        let operation = bug.operation;
        let bug = self.panicked(bug);
        if let Err(again) = caught_sync(operation, || on_panic(bug)) {
            self.report_bug("end the work a panic stopped", &again);
        }
    }
}

/// A task [`Diagnostics::spawn_reported`] started. Dropping it leaves the task
/// running.
#[derive(Debug)]
pub struct ReportedTask {
    task: tokio::task::JoinHandle<()>,
}

impl ReportedTask {
    /// End the work at its next await. Its `on_panic` is not called.
    pub fn abort(&self) {
        self.task.abort();
    }

    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    /// Wait until the work has ended: finished, aborted, or panicked and
    /// already reported.
    pub async fn ended(self) {
        // The task catches its work's panic and an abort is an end the
        // caller asked for, so only reporting the panic panicking is left.
        if let Err(error) = self.task.await {
            if error.is_panic() {
                tracing::error!(
                    "reporting a panic panicked: {}",
                    bae_util::task_failure::panic_message(error.into_panic().as_ref())
                );
            }
        }
    }
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
