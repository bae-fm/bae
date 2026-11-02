//! A live read that a task of its own produces: the task watches the
//! request the UI sets, merges it with whatever else the values depend on,
//! and sends each value. The subscriptions that merge a live query with the
//! playback queue, the upload queue, pin markers, or sync state are all this.

use super::LibraryError;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum LiveReadError {
    /// The reader cancelled the read, or its library closed.
    #[error("live read cancelled")]
    Cancelled,
    /// A read of the values failed; the read goes on.
    #[error(transparent)]
    Query(#[from] LibraryError),
    /// The task producing the values broke, which was reported as a bug: the
    /// read has ended.
    #[error("the live read ended: {0}")]
    Failed(#[source] LibraryError),
}

/// What a live read's task sends its reader.
pub(crate) enum LiveReadMessage<Value> {
    /// A value, or a query error the read goes on after.
    Read(Result<Value, LibraryError>),
    /// The task broke, and the read ends.
    Failed(LibraryError),
}

/// The end of a live read its task sends on.
pub(crate) struct LiveReadSender<Value>(mpsc::UnboundedSender<LiveReadMessage<Value>>);

impl<Value> Clone for LiveReadSender<Value> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<Value> LiveReadSender<Value> {
    /// Send a value or a query error. Fails once the reader is gone.
    pub(crate) fn send(
        &self,
        value: Result<Value, LibraryError>,
    ) -> Result<(), mpsc::error::SendError<LiveReadMessage<Value>>> {
        self.0.send(LiveReadMessage::Read(value))
    }
}

/// A live read's channel: the task's end and the [`LiveRead`]'s.
pub(crate) fn live_read_channel<Value>(
) -> (LiveReadSender<Value>, mpsc::UnboundedReceiver<LiveReadMessage<Value>>) {
    let (sender, values) = mpsc::unbounded_channel();
    (LiveReadSender(sender), values)
}

/// Whoever reads what a subscription's task sends, told when the task broke
/// so the read ends with that failure rather than as if it were cancelled.
pub(crate) trait SubscriptionReader: Send + 'static {
    fn fail(&self, error: LibraryError);
}

impl<Value: Send + 'static> SubscriptionReader for LiveReadSender<Value> {
    fn fail(&self, error: LibraryError) {
        if self.0.send(LiveReadMessage::Failed(error)).is_err() {
            tracing::debug!("a live read's task broke after its reader left");
        }
    }
}

/// A subscription read as a channel of results: the failure is its last
/// result, and the channel ends after it.
impl<Value: Send + 'static> SubscriptionReader
    for mpsc::UnboundedSender<Result<Value, LibraryError>>
{
    fn fail(&self, error: LibraryError) {
        if self.send(Err(error)).is_err() {
            tracing::debug!("a subscription's task broke after its reader left");
        }
    }
}

/// A live read whose request changes in place through [`set`](Self::set):
/// one task for as long as the read is open, never one per request.
pub struct LiveRead<Request, Value> {
    request: tokio::sync::watch::Sender<Request>,
    values: tokio::sync::Mutex<mpsc::UnboundedReceiver<LiveReadMessage<Value>>>,
    cancellation: CancellationToken,
    /// Stops the task that produces the values.
    task: CancellationToken,
}

impl<Request: PartialEq, Value> LiveRead<Request, Value> {
    /// Owns the task `task` stops, which reads the request `request` holds
    /// and sends each value on `values`.
    pub(crate) fn new(
        request: tokio::sync::watch::Sender<Request>,
        values: mpsc::UnboundedReceiver<LiveReadMessage<Value>>,
        task: CancellationToken,
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
    /// the end; a task that broke ends the read with its failure.
    pub async fn next(&self) -> Result<Value, LiveReadError> {
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(LiveReadError::Cancelled),
            message = async { self.values.lock().await.recv().await } => match message {
                Some(LiveReadMessage::Read(value)) => value.map_err(LiveReadError::Query),
                Some(LiveReadMessage::Failed(error)) => Err(LiveReadError::Failed(error)),
                None => Err(LiveReadError::Cancelled),
            },
        }
    }

    /// Stop reading and settle a waiting [`next`](Self::next).
    pub fn cancel(&self) {
        self.cancellation.cancel();
        self.task.cancel();
    }
}

impl<Request, Value> Drop for LiveRead<Request, Value> {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.task.cancel();
    }
}
