use std::sync::{Arc, Mutex};

use super::*;
use crate::test_panic;
use crate::RecordingBugReporter;

fn reporting() -> (Diagnostics, Arc<RecordingBugReporter>) {
    let reports = Arc::new(RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(reports.clone());
    (diagnostics, reports)
}

#[test]
fn bug_reports_a_panic_and_hands_it_back() {
    let (diagnostics, reports) = reporting();
    let failed = TaskFailed::Panicked(test_panic::bug("resize the cover", "zero width"));
    assert_eq!(diagnostics.bug(failed.clone()), failed);
    assert_eq!(reports.reports(), vec![test_panic::report("resize the cover", "zero width")]);
}

/// A bug that can't say where it began, such as a panic that reached a join
/// uncaught, is reported with its text alone.
#[test]
fn a_bug_with_no_place_is_reported_by_its_text() {
    let (diagnostics, reports) = reporting();
    let bug = Bug { operation: "resize the cover", message: "zero width".into(), location: None };
    assert_eq!(diagnostics.panicked(bug.clone()), bug);
    assert_eq!(
        reports.reports(),
        vec![("resize the cover".to_string(), "resize the cover panicked: zero width".to_string())]
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
/// and its panic still reaches the crash reporter. Sabotage: run it under
/// `coven::task::blocking`, and the panic goes nowhere once the caller drops.
#[tokio::test]
async fn blocking_reports_a_panic_its_caller_stopped_waiting_for() {
    let reports = Arc::new(crate::HeardBugReports::new());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(reports.clone());
    let (started_tx, started) = tokio::sync::oneshot::channel();
    let (go, wait) = std::sync::mpsc::channel::<()>();
    let mut work = Box::pin(diagnostics.blocking("resize the cover", move || {
        started_tx.send(()).expect("the test waits for the start");
        wait.recv().expect("the test lets the work go on");
        test_panic::panics("zero width")
    }));
    tokio::select! {
        _ = &mut work => panic!("the work waits for the test"),
        started = started => started.expect("the work started"),
    }
    // The caller stops waiting; the work runs on, and reports.
    drop(work);
    go.send(()).expect("the work is waiting");
    let report = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, reports.next())
        .await
        .expect("the panic is reported");
    assert_eq!(report, test_panic::report("resize the cover", "zero width"));
}

/// Work started with nobody waiting runs to its end, and its panic reaches the
/// crash reporter.
#[tokio::test]
async fn blocking_unawaited_reports_a_panic_nobody_waits_for() {
    let reports = Arc::new(crate::HeardBugReports::new());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(reports.clone());
    diagnostics.blocking_unawaited("write a body", || test_panic::panics("short write"));
    let report = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, reports.next())
        .await
        .expect("the panic is reported");
    assert_eq!(report, test_panic::report("write a body", "short write"));
}

#[tokio::test]
async fn spawn_reported_reports_a_panic_and_lets_its_owner_end_the_work() {
    let (diagnostics, reports) = reporting();
    let (ended_tx, ended_rx) = tokio::sync::oneshot::channel();
    let task = diagnostics.spawn_reported(
        &tokio::runtime::Handle::current(),
        "follow the queue",
        async {
            tokio::task::yield_now().await;
            test_panic::panics("queue index out of range")
        },
        move |failed| {
            ended_tx.send(failed).unwrap();
        },
    );
    task.ended().await;
    assert_eq!(
        ended_rx.await.expect("on_panic ran"),
        test_panic::bug("follow the queue", "queue index out of range")
    );
    assert_eq!(
        reports.reports(),
        vec![test_panic::report("follow the queue", "queue index out of range")]
    );
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
    let (diagnostics, reports) = reporting();
    let task = diagnostics.spawn_reported(
        &tokio::runtime::Handle::current(),
        "fan out progress",
        async { panic!("first") },
        |_| panic!("second"),
    );
    task.ended().await;
    let operations: Vec<String> = reports.reports().into_iter().map(|(op, _)| op).collect();
    assert_eq!(operations, vec!["fan out progress", "end what the work served"]);
}

/// A task that panics is reported, and its `end` hears the panic and hands
/// the joiner what it makes of it.
#[tokio::test]
async fn spawn_ending_reports_a_panic_and_hands_end_s_value_to_the_joiner() {
    let (diagnostics, reports) = reporting();
    let task = diagnostics.spawn_ending(
        &tokio::runtime::Handle::current(),
        "fill a buffer",
        async {
            tokio::task::yield_now().await;
            if true {
                test_panic::panics("window past the end");
            }
            7
        },
        |ended: Result<i32, Bug>| ended,
    );
    assert_eq!(
        task.outcome().await,
        Ok(Err(test_panic::bug("fill a buffer", "window past the end")))
    );
    assert_eq!(reports.reports(), vec![test_panic::report("fill a buffer", "window past the end")]);
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

/// A panic in `end` is reported, and the joiner hears it.
#[tokio::test]
async fn spawn_ending_reports_a_panic_in_end_to_the_joiner() {
    let (diagnostics, reports) = reporting();
    let task = diagnostics.spawn_ending(
        &tokio::runtime::Handle::current(),
        "record the transfer",
        async {},
        |_| -> () { panic!("progress channel gone") },
    );
    assert!(matches!(task.outcome().await, Err(Unfinished::Panicked(_))));
    let operations: Vec<String> = reports.reports().into_iter().map(|(op, _)| op).collect();
    assert_eq!(operations, vec!["end what the work served"]);
}

#[test]
fn spawn_thread_reported_reports_a_panic_and_calls_on_panic_on_that_thread() {
    let (diagnostics, reports) = reporting();
    let (ended_tx, ended_rx) = std::sync::mpsc::channel();
    let thread = diagnostics
        .spawn_thread_reported(
            "bae-decoder",
            || test_panic::panics("packet larger than the frame"),
            move |failed| {
                let thread = std::thread::current().name().map(str::to_string);
                ended_tx.send((failed, thread)).unwrap();
            },
        )
        .expect("spawn the thread");
    thread.join();
    let (failed, thread_name) = ended_rx.recv().expect("on_panic ran");
    assert_eq!(failed, test_panic::bug("bae-decoder", "packet larger than the frame"));
    assert_eq!(thread_name.as_deref(), Some("bae-decoder"));
    assert_eq!(
        reports.reports(),
        vec![test_panic::report("bae-decoder", "packet larger than the frame")]
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

/// Work on a thread of its own that panics is reported, and its caller hears
/// the panic.
#[test]
fn run_on_thread_reports_a_panic_and_hands_it_back() {
    let (diagnostics, reports) = reporting();
    let failed = diagnostics.run_on_thread("open the library", 1024 * 1024, || -> () {
        test_panic::panics("schema out of date")
    });
    assert!(
        matches!(&failed, Err(ThreadFailed::Panicked(bug))
            if *bug == test_panic::bug("open the library", "schema out of date")),
        "{failed:?}"
    );
    assert_eq!(
        reports.reports(),
        vec![test_panic::report("open the library", "schema out of date")]
    );
}

#[test]
fn lock_or_crash_hands_out_an_unpoisoned_lock() {
    let (diagnostics, reports) = reporting();
    let mutex = Mutex::new(5);
    *lock_or_crash(&diagnostics, "the table", &mutex) += 1;
    assert_eq!(*mutex.lock().unwrap(), 6);
    assert!(reports.reports().is_empty());
}
