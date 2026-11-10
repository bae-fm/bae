//! A panic at one known place, for a test that checks where a panic's report
//! says it began, and the reports the panic hook makes in a test process.

use std::sync::{Arc, OnceLock};

use bae_util::task_failure::{Bug, BugKind};

use crate::HeardBugReports;

/// The line of the `panic!` in [`panics`].
pub const PANIC_LINE: u32 = line!() + 4;

/// Panics with `message`, on line [`PANIC_LINE`] of this file, at column 5.
pub fn panics(message: &str) -> ! {
    panic!("{message}")
}

/// The bug `operation` gives once [`panics`] panics in it with `message`.
pub fn bug(operation: &'static str, message: &str) -> Bug {
    Bug {
        operation,
        message: message.to_string(),
        location: Some(coven::task::SourceLocation {
            file: file!().to_string(),
            line: PANIC_LINE,
            column: 5,
        }),
        kind: BugKind::Panic,
    }
}

/// The crash report of `operation`'s panic in [`panics`] with `message`: the
/// operation, and the bug's text followed by where it began. The panic hook
/// makes it, naming the innermost catch the panic began in.
pub fn report(operation: &str, message: &str) -> (String, String) {
    let at = format!("{}:{PANIC_LINE}:5", file!());
    (operation.to_string(), format!("{operation} panicked: {message} (at {at})"))
}

/// The crash reports the panic hook makes in this test process
/// (`report_panics`), added the first time a test asks and kept for the rest
/// of the process. Tests run side by side in one process, so a test takes
/// the reports about its own panic's message (`HeardBugReports::next_about`).
pub fn reported_panics() -> &'static HeardBugReports {
    static REPORTS: OnceLock<Arc<HeardBugReports>> = OnceLock::new();
    REPORTS.get_or_init(|| {
        let reports = Arc::new(HeardBugReports::new());
        crate::report_panics(reports.clone()).expect("start the panic reporting thread");
        reports
    })
}

/// Wait for the report about `message` in `reports`, the panic hook's
/// reports from [`reported_panics`], and take it: the test asks for
/// `reports` before its panic, which the hook reports only once added.
pub async fn report_about(reports: &HeardBugReports, message: &str) -> (String, String) {
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, reports.next_about(message))
        .await
        .expect("the panic hook reports the panic")
}
