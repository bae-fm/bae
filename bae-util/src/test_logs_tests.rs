#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;

/// The line a site logs.
fn warn_from_the_site() {
    tracing::warn!("the site under capture");
}

/// Another thread reaching a site first, with no capture of its own, while
/// this thread captures, keeps the site's lines from neither: this
/// thread's capture takes them, and the other's are dropped.
#[test]
fn a_capture_keeps_lines_from_a_site_another_thread_reaches_first() {
    let (start, started) = std::sync::mpsc::channel();
    let (reached, reached_rx) = std::sync::mpsc::channel();
    let other = std::thread::spawn(move || {
        started.recv().unwrap();
        warn_from_the_site();
        reached.send(()).unwrap();
    });
    let logs = capture_warn_logs(|| {
        start.send(()).unwrap();
        reached_rx.recv().unwrap();
        warn_from_the_site();
    });
    other.join().unwrap();
    assert_eq!(
        logs.matches("the site under capture").count(),
        1,
        "the capture takes its own thread's line alone: {logs:?}"
    );
}
