#![expect(
    clippy::disallowed_methods,
    reason = "test code: a thread and a task no catch runs on, as a dependency's are"
)]

use std::sync::Arc;

use bae_util::task_failure::{caught, Bug};

use crate::test_panic::{self, reported_panics};
use crate::{Diagnostics, RecordingBugReporter};

/// A panic on a thread bae didn't spawn, such as a dependency's own, reaches
/// no catch of bae's. The panic hook reports it, once, naming no operation.
/// Sabotage: report only from catches, and nothing reports it.
#[tokio::test]
async fn a_panic_on_a_thread_bae_did_not_spawn_is_reported_once() {
    let hooked = reported_panics();
    let message = "a dependency's thread lost its socket";
    let thread = std::thread::spawn(move || test_panic::panics(message));
    assert!(thread.join().is_err(), "the thread panicked");
    let operation = "work outside every catch";
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report(operation, message)
    );
    assert!(!hooked.holds_about(message), "the panic is reported once");
}

/// A task tokio runs and catches on a thread that is inside a catch of bae's,
/// as a current-thread runtime runs a dependency's tasks under `block_on`,
/// never reaches that catch. The hook reports it, once, naming the catch the
/// thread was in.
#[tokio::test]
async fn a_panic_tokio_catches_inside_a_catch_is_reported_once() {
    let hooked = reported_panics();
    let message = "a connection's task read past its frame";
    let thread = Diagnostics::noop()
        .spawn_thread_reported(
            "bae-uploads",
            move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .expect("build the thread's runtime");
                let task = runtime.block_on(async {
                    tokio::spawn(async move { test_panic::panics(message) }).await
                });
                assert!(task.expect_err("the task panicked").is_panic());
            },
            |bug| panic!("the thread's own work did not panic: {bug}"),
        )
        .expect("spawn the thread");
    thread.join();
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report("bae-uploads", message)
    );
    assert!(!hooked.holds_about(message), "the panic is reported once");
}

/// A panic a catch hands back is reported once, by the hook as it began; the
/// sink the catch's owner tells of it only logs it.
#[tokio::test]
async fn a_caught_panic_is_reported_once() {
    let hooked = reported_panics();
    let message = "the queue's index ran past its end";
    let reports = Arc::new(RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(reports.clone());
    let failed = caught("follow the queue", async move { test_panic::panics(message) })
        .await
        .expect_err("the work panicked");
    assert_eq!(diagnostics.panicked(failed.clone()), failed);
    assert_eq!(
        test_panic::report_about(hooked, message).await,
        test_panic::report("follow the queue", message)
    );
    assert!(!hooked.holds_about(message), "the hook reports the panic once");
    assert!(reports.reports().is_empty(), "the catch's owner doesn't report it again");
}

/// A bug code found without a panic passes no hook, so where it lands
/// reports it.
#[test]
fn a_bug_found_without_a_panic_is_reported_where_it_lands() {
    let reports = Arc::new(RecordingBugReporter::default());
    let diagnostics = Diagnostics::noop().reporting_bugs_to(reports.clone());
    let found = Bug::new("release reserved names", "a name no one reserved");
    let found_on = line!() - 1;
    assert_eq!(diagnostics.panicked(found.clone()), found);
    assert_eq!(
        reports.reports(),
        [(
            "release reserved names".to_string(),
            format!(
                "release reserved names panicked: a name no one reserved (at {}:{found_on}:17)",
                file!()
            )
        )]
    );
}
