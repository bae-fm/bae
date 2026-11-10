use std::sync::{Arc, Mutex};

use super::*;
use crate::test_panic::{self, reported_panics};
use crate::RecordingBugReporter;

fn reporting() -> (Diagnostics, Arc<RecordingBugReporter>) {
    let reports = Arc::new(RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(reports.clone());
    (diagnostics, reports)
}

/// A panic, which the hook reported as it began, is handed back and not
/// reported again; so is one that reached a join uncaught, with no place.
#[test]
fn bug_hands_back_a_panic_without_reporting_it_again() {
    let (diagnostics, reports) = reporting();
    let failed = TaskFailed::Panicked(test_panic::bug("resize the cover", "zero width"));
    assert_eq!(diagnostics.bug(failed.clone()), failed);
    let uncaught = Bug {
        operation: "resize the cover",
        message: "zero width".into(),
        location: None,
        kind: BugKind::Panic,
    };
    assert_eq!(diagnostics.panicked(uncaught.clone()), uncaught);
    assert!(reports.reports().is_empty());
}

/// A bug code found without a panic, which no hook saw, is reported by
/// `bug`, naming where it was found.
#[test]
fn bug_reports_a_bug_found_without_a_panic() {
    let (diagnostics, reports) = reporting();
    let found = Bug::new("resize the cover", "a width the cover never had");
    let found_on = line!() - 1;
    let failed = TaskFailed::Panicked(found);
    assert_eq!(diagnostics.bug(failed.clone()), failed);
    assert_eq!(
        reports.reports(),
        vec![(
            "resize the cover".to_string(),
            format!(
                "resize the cover panicked: a width the cover never had (at {}:{found_on}:17)",
                file!()
            )
        )]
    );
}

#[test]
fn bug_does_not_report_a_runtime_shutting_down() {
    let (diagnostics, reports) = reporting();
    let failed = TaskFailed::Shutdown(coven::task::Shutdown {
        operation: "resize the cover",
        outcome: coven::task::ShutdownOutcome::NotRun,
    });
    assert_eq!(diagnostics.bug(failed.clone()), failed);
    assert!(reports.reports().is_empty());
}

/// Work on the blocking pool runs to its end after its caller stops waiting,
/// and its panic is reported once, by the hook, naming the work; the sink
/// that hears it once no caller waits only logs it.
#[tokio::test]
async fn blocking_work_whose_caller_stopped_waiting_has_its_panic_reported_once() {
    let hooked = reported_panics();
    let message = "a cover of zero width";
    let reports = Arc::new(crate::HeardBugReports::new());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(reports.clone());
    let (started_tx, started) = tokio::sync::oneshot::channel();
    let (go, wait) = std::sync::mpsc::channel::<()>();
    let mut work = Box::pin(diagnostics.blocking("resize the cover", move || {
        started_tx.send(()).expect("the test waits for the start");
        wait.recv().expect("the test lets the work go on");
        test_panic::panics(message)
    }));
    tokio::select! {
        _ = &mut work => panic!("the work waits for the test"),
        started = started => started.expect("the work started"),
    }
    // The caller stops waiting; the work runs on, and panics.
    drop(work);
    go.send(()).expect("the work is waiting");
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report("resize the cover", message)
    );
    assert!(!hooked.holds_about(message));
    assert!(reports.all_taken(), "the sink only logs a panic");
}

/// Work started with nobody waiting runs to its end, and the hook reports its
/// panic.
#[tokio::test]
async fn blocking_unawaited_work_has_its_panic_reported() {
    let hooked = reported_panics();
    let message = "a body's write came up short";
    Diagnostics::noop().blocking_unawaited("write a body", move || test_panic::panics(message));
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report("write a body", message)
    );
}

/// A panicking task's owner ends its work, and the panic is reported once, by
/// the hook.
#[tokio::test]
async fn spawn_reported_lets_its_owner_end_the_work_of_a_task_that_panicked() {
    let hooked = reported_panics();
    let message = "a queue index out of range";
    let (diagnostics, reports) = reporting();
    let (ended_tx, ended_rx) = tokio::sync::oneshot::channel();
    let task = diagnostics.spawn_reported(
        &tokio::runtime::Handle::current(),
        "follow the queue",
        async {
            tokio::task::yield_now().await;
            test_panic::panics(message)
        },
        move |failed| {
            ended_tx.send(failed).unwrap();
        },
    );
    task.ended().await;
    assert_eq!(ended_rx.await.expect("on_panic ran"), test_panic::bug("follow the queue", message));
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report("follow the queue", message)
    );
    assert!(!hooked.holds_about(message));
    assert!(reports.reports().is_empty(), "the task's owner doesn't report it again");
}

#[tokio::test]
async fn spawn_reported_calls_nothing_when_the_task_finishes_or_is_aborted() {
    let (diagnostics, reports) = reporting();
    let runtime = tokio::runtime::Handle::current();
    let finished = diagnostics.spawn_reported(&runtime, "finish", async {}, |_| {
        panic!("a task that finished did not panic");
    });
    finished.ended().await;
    let aborted = diagnostics.spawn_reported(&runtime, "wait", std::future::pending(), |_| {
        panic!("an aborted task did not panic");
    });
    aborted.abort();
    aborted.ended().await;
    assert!(reports.reports().is_empty());
}

#[tokio::test]
async fn a_panic_in_on_panic_is_reported_and_goes_no_further() {
    let hooked = reported_panics();
    let first = "progress fanned out to a closed screen";
    let second = "the screen's end was already sent";
    let task = Diagnostics::noop().spawn_reported(
        &tokio::runtime::Handle::current(),
        "fan out progress",
        async move { test_panic::panics(first) },
        move |_| test_panic::panics(second),
    );
    task.ended().await;
    assert_eq!(
        test_panic::report_about(hooked, first).await,
        test_panic::report("fan out progress", first)
    );
    assert_eq!(
        test_panic::report_about(hooked, second).await,
        test_panic::report("end what the work served", second)
    );
}

#[tokio::test]
async fn spawn_thread_reported_calls_on_panic_on_the_thread_that_panicked() {
    let hooked = reported_panics();
    let message = "a packet larger than the frame";
    let (diagnostics, reports) = reporting();
    let (ended_tx, ended_rx) = std::sync::mpsc::channel();
    let thread = diagnostics
        .spawn_thread_reported(
            "bae-decoder",
            move || test_panic::panics(message),
            move |failed| {
                let thread = std::thread::current().name().map(str::to_string);
                ended_tx.send((failed, thread)).unwrap();
            },
        )
        .expect("spawn the thread");
    thread.join();
    let (failed, thread_name) = ended_rx.recv().expect("on_panic ran");
    assert_eq!(failed, test_panic::bug("bae-decoder", message));
    assert_eq!(thread_name.as_deref(), Some("bae-decoder"));
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report("bae-decoder", message)
    );
    assert!(reports.reports().is_empty(), "the thread's owner doesn't report it again");
}

/// A task that panics hands its `end` the panic, which the hook reported,
/// and the joiner gets what `end` makes of it; the task reports nothing.
#[tokio::test]
async fn spawn_ending_hands_a_panic_to_end_and_end_s_value_to_the_joiner() {
    let hooked = reported_panics();
    let message = "a window past the buffer's end";
    let (diagnostics, reports) = reporting();
    let task = diagnostics.spawn_ending(
        &tokio::runtime::Handle::current(),
        "fill a buffer",
        async move {
            tokio::task::yield_now().await;
            if true {
                test_panic::panics(message);
            }
            7
        },
        |ended: Result<i32, Bug>| ended,
    );
    assert_eq!(task.outcome().await, Ok(Err(test_panic::bug("fill a buffer", message))));
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report("fill a buffer", message)
    );
    assert!(reports.reports().is_empty(), "the task doesn't report it again");
}

/// A task that finishes hands its value to `end`, and reports nothing.
#[tokio::test]
async fn spawn_ending_hands_a_finished_value_to_end() {
    let (diagnostics, reports) = reporting();
    let task = diagnostics.spawn_ending(
        &tokio::runtime::Handle::current(),
        "count the files",
        async { 7 },
        |ended: Result<i32, Bug>| ended.map(|count| count * 2),
    );
    assert_eq!(task.outcome().await, Ok(Ok(14)));
    assert!(reports.reports().is_empty());
}

/// An aborted task's `end` never runs, and its joiner hears it was stopped.
#[tokio::test]
async fn spawn_ending_tells_the_joiner_of_an_abort() {
    let (diagnostics, reports) = reporting();
    let task = diagnostics.spawn_ending(
        &tokio::runtime::Handle::current(),
        "wait",
        std::future::pending::<()>(),
        |_| panic!("an aborted task's end does not run"),
    );
    task.abort();
    assert_eq!(task.outcome().await, Err(Unfinished::Stopped));
    assert!(reports.reports().is_empty());
}

/// A panic in `end` is reported by the hook, naming the ending, and the
/// joiner hears it.
#[tokio::test]
async fn spawn_ending_hands_a_panic_in_end_to_the_joiner() {
    let hooked = reported_panics();
    let message = "the transfer's progress channel is gone";
    let task = Diagnostics::noop().spawn_ending(
        &tokio::runtime::Handle::current(),
        "record the transfer",
        async {},
        move |_| -> () { test_panic::panics(message) },
    );
    assert!(matches!(task.outcome().await, Err(Unfinished::Panicked(_))));
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report("end what the work served", message)
    );
}

/// Work on a thread of its own hands its value back, run with the stack it
/// asked for: a frame a default stack can't hold fits.
#[test]
fn run_on_thread_hands_back_the_work_s_value_on_its_stack() {
    let (diagnostics, reports) = reporting();
    let value = diagnostics.run_on_thread("deep work", 16 * 1024 * 1024, || {
        let frame = std::hint::black_box([1u8; 8 * 1024 * 1024]);
        (frame[0], std::thread::current().name().map(str::to_string))
    });
    assert_eq!(value.expect("the work ran"), (1, Some("deep work".to_string())));
    assert!(reports.reports().is_empty());
}

/// Work on a thread of its own that panics hands the panic back to its
/// caller; the hook reported it, so nothing reports it again.
#[tokio::test]
async fn run_on_thread_hands_back_a_panic() {
    let hooked = reported_panics();
    let message = "the schema is out of date";
    let (diagnostics, reports) = reporting();
    let failed = diagnostics.run_on_thread("open the library", 1024 * 1024, move || -> () {
        test_panic::panics(message)
    });
    assert!(
        matches!(&failed, Err(ThreadFailed::Panicked(bug))
            if *bug == test_panic::bug("open the library", message)),
        "{failed:?}"
    );
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report("open the library", message)
    );
    assert!(reports.reports().is_empty());
}

#[test]
fn lock_or_crash_hands_out_an_unpoisoned_lock() {
    let (diagnostics, reports) = reporting();
    let mutex = Mutex::new(5);
    *lock_or_crash(&diagnostics, "the table", &mutex) += 1;
    assert_eq!(*mutex.lock().unwrap(), 6);
    assert!(reports.reports().is_empty());
}
