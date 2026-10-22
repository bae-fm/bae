//! The test process's one tracing subscriber, and the captures tests read
//! their own thread's log lines from.
//!
//! tracing caches, per log site, whether any subscriber wants it, the first
//! time any thread reaches the site. A capture installed as its thread's own
//! default subscriber loses lines to that cache: while it is the only
//! subscriber registered, tracing-core asks only the default of the thread
//! reaching the site, so another test's thread reaching it first, with no
//! subscriber of its own, marks it unwanted for every thread, the capturing
//! one included. So the process has one global subscriber, and a capture is a
//! buffer on its thread that the subscriber writes into: every site asks it
//! on every line whether the thread it runs on is capturing.

use std::cell::RefCell;
use std::future::Future;
use std::io::{self, Write};
use std::sync::Once;
use tracing::subscriber::Interest;
use tracing::Metadata;
use tracing_subscriber::layer::{Context, Filter, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

thread_local! {
    /// The capture running on this thread.
    static CAPTURE: RefCell<Option<Capture>> = const { RefCell::new(None) };
}

/// The level a capture takes lines at, and the lines it took.
struct Capture {
    level: tracing::Level,
    bytes: Vec<u8>,
}

/// Install the process's subscriber, once: warnings and bae-core's debug
/// lines go to the running test's output, and each line a capturing thread
/// logs at its capture's level or above goes into that capture.
pub(crate) fn install() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_test_writer()
                    .with_ansi(false)
                    .with_filter(EnvFilter::new("warn,bae_core=debug")),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(|| CaptureWriter)
                    .with_filter(Capturing),
            )
            .init();
    });
}

/// Whether the thread a line is logged on is capturing at its level. Asked
/// on every line, never cached per site: which threads capture changes from
/// one line to the next.
struct Capturing;

impl<S> Filter<S> for Capturing {
    fn enabled(&self, metadata: &Metadata<'_>, _: &Context<'_, S>) -> bool {
        CAPTURE.with(|capture| {
            capture
                .borrow()
                .as_ref()
                .is_some_and(|capture| *metadata.level() <= capture.level)
        })
    }

    fn callsite_enabled(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::sometimes()
    }
}

/// Appends to the capture of the thread it writes on.
struct CaptureWriter;

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        CAPTURE.with(|capture| {
            if let Some(capture) = capture.borrow_mut().as_mut() {
                capture.bytes.extend_from_slice(buf);
            }
        });
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// This thread's capture, from its start until it is read or dropped — a
/// test that panics mid-capture leaves its thread not capturing.
struct CaptureGuard;

impl CaptureGuard {
    fn start(level: tracing::Level) -> Self {
        install();
        CAPTURE.with(|capture| {
            let earlier = capture.replace(Some(Capture {
                level,
                bytes: Vec::new(),
            }));
            assert!(earlier.is_none(), "a thread runs one capture at a time");
        });
        Self
    }

    fn logs(self) -> String {
        let capture = CAPTURE
            .with(|capture| capture.borrow_mut().take())
            .expect("the capture runs until it is read");
        String::from_utf8(capture.bytes).unwrap()
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        CAPTURE.with(|capture| capture.borrow_mut().take());
    }
}

/// Everything `run` logs on this thread at `level` or above, for a test
/// asserting that a decision the code makes quietly is at least visible in
/// the log.
pub(crate) fn capture_logs_at(level: tracing::Level, run: impl FnOnce()) -> String {
    let capture = CaptureGuard::start(level);
    run();
    capture.logs()
}

pub(crate) fn capture_warn_logs(run: impl FnOnce()) -> String {
    capture_logs_at(tracing::Level::WARN, run)
}

/// Everything `run` logs at warning or above on this thread, which a
/// current-thread runtime polls it on.
pub(crate) async fn capture_warn_logs_async<F, Fut>(run: F) -> String
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let capture = CaptureGuard::start(tracing::Level::WARN);
    run().await;
    capture.logs()
}

#[cfg(test)]
mod tests {
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
}
