//! Every panic in the process reaches the crash reporter once, from the panic
//! hook.
//!
//! The process's panic hook is the one place every panic passes: std calls
//! it once as each panic begins, on whatever thread, and
//! `std::panic::resume_unwind`, which moves a panic that already began, calls
//! it not at all. A catch sees only the panics that unwind to it. One on a
//! thread no catch runs on, such as a dependency's own (an HTTP connection's
//! task, the mDNS daemon, an audio device's callback thread), reaches none;
//! neither does one tokio catches for a task it runs on a thread that is
//! inside a catch, as a current-thread runtime does under `block_on`. So the
//! hook reports every panic, and nothing else reports one: a catch hands its
//! `Bug` back with `BugKind::Panic`, which [`Diagnostics::panicked`] only
//! logs, and only a bug code found without a panic (`BugKind::Found`) is
//! reported where it lands.
//!
//! The report names the operation of the innermost catch on the panicking
//! thread (`coven::task::catching_operation`), what the work was doing, and
//! where the panic began.
//!
//! The hook doesn't call the reporter itself. A panic inside a hook ends the
//! process, and the platform's reporter is foreign code whose throw reaches
//! Rust as a panic; the panicking thread may also hold a lock the report
//! needs. So the hook hands each report to a thread of its own, which calls
//! the reporter.

use std::backtrace::Backtrace;
use std::cell::Cell;
use std::panic::PanicHookInfo;
use std::sync::{mpsc, Arc, Once};

use bae_util::task_failure::{caught_sync, panic_message};

use super::{BugReporter, Diagnostics};

/// The operation a report names for a panic outside every catch.
const OUTSIDE_EVERY_CATCH: &str = "work outside every catch";

/// One panic's crash report: the operation, and the panic's text followed by
/// where it began.
struct PanicReport {
    operation: &'static str,
    detail: String,
}

thread_local! {
    /// Whether this is the reporting thread. A panic on it, the reporter's
    /// own, is only logged: reporting it would hand the reporter a report of
    /// its own failure, again each time it fails.
    static REPORTING: Cell<bool> = const { Cell::new(false) };
}

/// Report every panic in the process to `bugs`, once each, from now on: the
/// hook added here, in front of the one already set, which it calls in turn,
/// logs each panic and hands its report to a thread that calls `bugs`. The
/// first call in a process decides the reporter; a later call changes
/// nothing. Fails only when the reporting thread can't start, and then adds
/// no hook.
pub fn report_panics(bugs: Arc<dyn BugReporter>) -> std::io::Result<()> {
    static INSTALLED: Once = Once::new();
    let mut installed = Ok(());
    INSTALLED.call_once(|| installed = install(bugs));
    installed
}

fn install(bugs: Arc<dyn BugReporter>) -> std::io::Result<()> {
    let (reports, heard) = mpsc::channel::<PanicReport>();
    // Never joined: it reports for the life of the process. Each report runs
    // under a catch of its own, so a reporter that panics on one still hears
    // the next.
    drop(Diagnostics::noop().spawn_thread_reported(
        "bae-panic-reports",
        move || {
            REPORTING.set(true);
            for report in heard {
                if let Err(failed) =
                    caught_sync("report a panic", || bugs.report(report.operation, &report.detail))
                {
                    tracing::error!(
                        operation = report.operation,
                        "the crash reporter panicked on a panic's report: {failed}"
                    );
                }
            }
        },
        |_| {},
    )?);
    let earlier = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        report(panic, &reports);
        earlier(panic);
    }));
    Ok(())
}

/// Log `panic` and hand its report to the reporting thread's queue,
/// `reports`. A panic inside a hook ends the process, so nothing here
/// panics: it reads no thread handle, which panics on a thread whose locals
/// are gone, and reads its own thread's state with `try_with`.
fn report(panic: &PanicHookInfo<'_>, reports: &mpsc::Sender<PanicReport>) {
    let operation = coven::task::catching_operation().unwrap_or(OUTSIDE_EVERY_CATCH);
    let message = panic_message(panic.payload());
    let detail = match panic.location() {
        Some(at) => format!(
            "{operation} panicked: {message} (at {}:{}:{})",
            at.file(),
            at.line(),
            at.column()
        ),
        None => format!("{operation} panicked: {message}"),
    };
    let backtrace = Backtrace::force_capture();
    tracing::error!(operation, %backtrace, "{detail}");
    // A thread whose locals are gone is exiting, so it isn't the reporting
    // thread, which runs for the life of the process.
    let on_reporting_thread = REPORTING.try_with(Cell::get).is_ok_and(|reporting| reporting);
    if on_reporting_thread {
        return;
    }
    if reports.send(PanicReport { operation, detail }).is_err() {
        tracing::error!(
            operation,
            "the panic reporting thread has stopped: this panic goes unreported"
        );
    }
}

#[cfg(test)]
#[path = "panic_reports_tests.rs"]
mod tests;
