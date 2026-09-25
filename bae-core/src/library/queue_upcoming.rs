//! The context's upcoming tail past the queue value's own first window, read
//! through one live subscription whose windows move as the queue scrolls.

use super::{LibraryError, LibraryPageWindow, LibraryPageWindows};
use tokio_util::sync::CancellationToken;

/// One requested window of the upcoming tail, with the entries in it.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueUpcomingWindow {
    pub window: LibraryPageWindow,
    pub items: Vec<crate::queue::QueueItem>,
}

/// Every requested window as of one queue revision. Offsets are into the
/// not-yet-played tail — the same coordinate space as
/// [`crate::queue::ResolvedContext::upcoming`] — and a window reaching past
/// the tail's end holds what remains of it.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueUpcomingSnapshot {
    /// The `PlaybackQueue` revision the windows were sliced from. A UI shows
    /// them only while its queue value carries the same revision.
    pub revision: u64,
    pub windows: Vec<QueueUpcomingWindow>,
}

#[derive(Debug, thiserror::Error)]
pub enum QueueUpcomingSubscriptionError {
    #[error("queue upcoming subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] LibraryError),
}

/// A live read of the upcoming tail's requested windows. The windows change
/// in place through [`set_windows`](Self::set_windows) and the queue's own
/// revisions move the same subscription: neither opens another query.
pub struct QueueUpcomingSubscription {
    windows: tokio::sync::watch::Sender<LibraryPageWindows>,
    values: tokio::sync::Mutex<
        tokio::sync::mpsc::UnboundedReceiver<Result<QueueUpcomingSnapshot, LibraryError>>,
    >,
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl QueueUpcomingSubscription {
    /// Owns `task`, which reads the windows `windows` holds and sends each
    /// value on `values`.
    pub(crate) fn new(
        windows: tokio::sync::watch::Sender<LibraryPageWindows>,
        values: tokio::sync::mpsc::UnboundedReceiver<Result<QueueUpcomingSnapshot, LibraryError>>,
        task: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            windows,
            values: tokio::sync::Mutex::new(values),
            cancellation: CancellationToken::new(),
            task,
        }
    }

    /// Read `windows` from now on. Repeating the standing windows reads
    /// nothing again.
    pub fn set_windows(
        &self,
        windows: LibraryPageWindows,
    ) -> Result<(), QueueUpcomingSubscriptionError> {
        if self.cancellation.is_cancelled() {
            return Err(QueueUpcomingSubscriptionError::Cancelled);
        }
        self.windows.send_if_modified(|current| {
            if *current == windows {
                return false;
            }
            *current = windows;
            true
        });
        Ok(())
    }

    /// The next value, or the end of the subscription. Query errors are
    /// values, not the end.
    pub async fn next(&self) -> Result<QueueUpcomingSnapshot, QueueUpcomingSubscriptionError> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(QueueUpcomingSubscriptionError::Cancelled),
            value = async { self.values.lock().await.recv().await } => match value {
                Some(value) => value.map_err(QueueUpcomingSubscriptionError::Query),
                None => Err(QueueUpcomingSubscriptionError::Cancelled),
            },
        }
    }

    /// Stop reading and settle a waiting [`next`](Self::next).
    pub fn cancel(&self) {
        self.cancellation.cancel();
        self.task.abort();
    }
}

impl Drop for QueueUpcomingSubscription {
    fn drop(&mut self) {
        self.cancel();
    }
}
