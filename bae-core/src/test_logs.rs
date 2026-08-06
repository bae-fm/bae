use std::future::Future;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

struct LogWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn subscriber_at(
    level: tracing::Level,
    bytes: Arc<Mutex<Vec<u8>>>,
) -> impl tracing::Subscriber + Send + Sync + 'static {
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_ansi(false)
        .with_writer(move || LogWriter {
            bytes: bytes.clone(),
        })
        .finish()
}

fn logs_from(bytes: Arc<Mutex<Vec<u8>>>) -> String {
    let bytes = bytes.lock().unwrap().clone();
    String::from_utf8(bytes).unwrap()
}

/// Everything `run` logs at `level` or above, for a test asserting that a
/// decision the code makes quietly is at least visible in the log.
pub(crate) fn capture_logs_at(level: tracing::Level, run: impl FnOnce()) -> String {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let subscriber = subscriber_at(level, bytes.clone());

    tracing::subscriber::with_default(subscriber, run);

    logs_from(bytes)
}

pub(crate) fn capture_warn_logs(run: impl FnOnce()) -> String {
    capture_logs_at(tracing::Level::WARN, run)
}

pub(crate) async fn capture_warn_logs_async<F, Fut>(run: F) -> String
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let subscriber = subscriber_at(tracing::Level::WARN, bytes.clone());

    let guard = tracing::subscriber::set_default(subscriber);
    run().await;
    drop(guard);

    logs_from(bytes)
}
