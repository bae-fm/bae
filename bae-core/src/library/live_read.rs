//! A live read that a task of its own produces: the task watches the
//! request the UI sets, merges it with whatever else the values depend on,
//! and sends each value. The subscriptions that merge a live query with the
//! playback queue, the upload queue, pin markers, or sync state are all this.

use super::LibraryError;
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum LiveReadError {
    #[error("live read cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] LibraryError),
}

/// A live read whose request changes in place through [`set`](Self::set):
/// one task for as long as the read is open, never one per request.
pub struct LiveRead<Request, Value> {
    request: tokio::sync::watch::Sender<Request>,
    values: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<Result<Value, LibraryError>>>,
    cancellation: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl<Request: PartialEq, Value> LiveRead<Request, Value> {
    /// Owns `task`, which reads the request `request` holds and sends each
    /// value on `values`.
    pub(crate) fn new(
        request: tokio::sync::watch::Sender<Request>,
        values: tokio::sync::mpsc::UnboundedReceiver<Result<Value, LibraryError>>,
        task: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            request,
            values: tokio::sync::Mutex::new(values),
            cancellation: CancellationToken::new(),
            task,
        }
    }

    /// Read `request` from now on. Repeating the standing request reads
    /// nothing again.
    pub fn set(&self, request: Request) -> Result<(), LiveReadError> {
        if self.cancellation.is_cancelled() {
            return Err(LiveReadError::Cancelled);
        }
        self.request.send_if_modified(|current| {
            if *current == request {
                return false;
            }
            *current = request;
            true
        });
        Ok(())
    }

    /// The next value, or the end of the read. Query errors are values, not
    /// the end.
    pub async fn next(&self) -> Result<Value, LiveReadError> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(LiveReadError::Cancelled),
            value = async { self.values.lock().await.recv().await } => match value {
                Some(value) => value.map_err(LiveReadError::Query),
                None => Err(LiveReadError::Cancelled),
            },
        }
    }

    /// Stop reading and settle a waiting [`next`](Self::next).
    pub fn cancel(&self) {
        self.cancellation.cancel();
        self.task.abort();
    }
}

impl<Request, Value> Drop for LiveRead<Request, Value> {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.task.abort();
    }
}
