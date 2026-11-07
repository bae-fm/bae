use std::sync::{Arc, Mutex};

use super::*;
use crate::RecordingBugReporter;

fn reporting() -> (Diagnostics, Arc<RecordingBugReporter>) {
    let reports = Arc::new(RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(reports.clone());
    (diagnostics, reports)
}

#[test]
fn bug_reports_a_panic_and_hands_it_back() {
    let (diagnostics, reports) = reporting();
    let failed = TaskFailed::Panicked(Bug {
        operation: "resize the cover",
        message: "zero width".to_string(),
    });
    assert_eq!(diagnostics.bug(failed.clone()), failed);
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
        panic!("zero width");
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
    assert_eq!(
        report,
        ("resize the cover".to_string(), "resize the cover panicked: zero width".to_string())
    );
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
            panic!("queue index out of range");
        },
        move |failed| {
            ended_tx.send(failed).unwrap();
        },
    );
    task.ended().await;
    assert_eq!(
        ended_rx.await.expect("on_panic ran"),
        Bug { operation: "follow the queue", message: "queue index out of range".to_string() }
    );
    assert_eq!(
        reports.reports(),
        vec![(
            "follow the queue".to_string(),
            "follow the queue panicked: queue index out of range".to_string()
        )]
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
    assert_eq!(operations, vec!["fan out progress", "end the work a panic stopped"]);
}

#[test]
fn spawn_thread_reported_reports_a_panic_and_calls_on_panic_on_that_thread() {
    let (diagnostics, reports) = reporting();
    let (ended_tx, ended_rx) = std::sync::mpsc::channel();
    let thread = diagnostics
        .spawn_thread_reported(
            "bae-decoder",
            || panic!("packet larger than the frame"),
            move |failed| {
                let thread = std::thread::current().name().map(str::to_string);
                ended_tx.send((failed, thread)).unwrap();
            },
        )
        .expect("spawn the thread");
    thread.join();
    let (failed, thread_name) = ended_rx.recv().expect("on_panic ran");
    assert_eq!(
        failed,
        Bug { operation: "bae-decoder", message: "packet larger than the frame".to_string() }
    );
    assert_eq!(thread_name.as_deref(), Some("bae-decoder"));
    assert_eq!(reports.reports().len(), 1);
}

#[test]
fn lock_or_crash_hands_out_an_unpoisoned_lock() {
    let (diagnostics, reports) = reporting();
    let mutex = Mutex::new(5);
    *lock_or_crash(&diagnostics, "the table", &mutex) += 1;
    assert_eq!(*mutex.lock().unwrap(), 6);
    assert!(reports.reports().is_empty());
}
