//! A live query whose request moves in place, each read of it resolved into
//! what its screen takes. Nothing runs between pulls: each
//! [`next`](LiveRead::next) reads the query until a value for the request
//! last set resolves. Closing the library ends it, since its query's
//! cancellation is the library manager's `until_closed`.

use super::LibraryError;
use bae_store::live_query::{CancellableLiveQuery, LiveQueryCancelled};
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, thiserror::Error)]
pub enum LiveReadError {
    /// The reader cancelled the read, or its library closed.
    #[error("live read cancelled")]
    Cancelled,
    /// A read of the values failed; the read goes on.
    #[error(transparent)]
    Query(#[from] LibraryError),
}

/// The query ending is the read's end.
impl From<LiveQueryCancelled> for LiveReadError {
    fn from(_: LiveQueryCancelled) -> Self {
        Self::Cancelled
    }
}

/// A live read whose request changes in place through [`set`](Self::set):
/// one query for as long as the read is open, never one per request.
pub struct LiveRead<Request, Value> {
    read: Box<dyn Resolved<Request, Value>>,
}

impl<Request, Value> LiveRead<Request, Value>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Value: Send + 'static,
{
    /// `query`'s reads, each resolved by `resolve` from the request it was
    /// read for, that request's revision, and what the query read. A read
    /// `resolve` answers `None` for delivers nothing.
    pub fn new<Projection: Clone + PartialEq + Send + 'static>(
        query: CancellableLiveQuery<Request, Projection>,
        resolve: impl FnMut(&Request, u64, Projection) -> Option<Value> + Send + 'static,
    ) -> Self {
        Self {
            read: Box::new(Resolving {
                query,
                newest: std::sync::Mutex::new(None),
                resolve: std::sync::Mutex::new(Box::new(resolve)),
            }),
        }
    }
}

impl<Request, Value> LiveRead<Request, Value> {
    /// Read `request` from now on. Repeating the standing request reads
    /// nothing again.
    pub fn set(&self, request: Request) -> Result<(), LiveReadError> {
        self.read.set(request)
    }

    /// The next value for the request last set, or the end of the read.
    /// Query errors are values, not the end.
    pub async fn next(&self) -> Result<Value, LiveReadError> {
        self.read.next().await
    }

    /// Stop reading and settle a waiting [`next`](Self::next).
    pub fn cancel(&self) {
        self.read.cancel();
    }
}

/// A live read with the type its query reads hidden, so a read is named by
/// its request and what it delivers alone.
trait Resolved<Request, Value>: Send + Sync {
    fn set(&self, request: Request) -> Result<(), LiveReadError>;

    fn next(&self) -> Pin<Box<dyn Future<Output = Result<Value, LiveReadError>> + Send + '_>>;

    fn cancel(&self);
}

type Resolve<Request, Projection, Value> =
    Box<dyn FnMut(&Request, u64, Projection) -> Option<Value> + Send>;

struct Resolving<Request, Projection, Value> {
    query: CancellableLiveQuery<Request, Projection>,
    /// The revision of the request last set; a read answering another one is
    /// not delivered. `None` while the query's first request is the only
    /// one. Held while a request is set, so no read is judged against the
    /// revision before it.
    newest: std::sync::Mutex<Option<u64>>,
    resolve: std::sync::Mutex<Resolve<Request, Projection, Value>>,
}

impl<Request, Projection, Value> Resolved<Request, Value> for Resolving<Request, Projection, Value>
where
    Request: Clone + PartialEq + Send + Sync + 'static,
    Projection: Clone + PartialEq + Send + 'static,
    Value: Send + 'static,
{
    fn set(&self, request: Request) -> Result<(), LiveReadError> {
        let mut newest = self.newest.lock().expect("live read revision mutex poisoned");
        *newest = Some(self.query.set(request)?);
        Ok(())
    }

    /// Coven delivers a read that finished after its request was replaced,
    /// which keeps a request changing faster than one read from starving
    /// the query, and marks it with the revision it answered: one that isn't
    /// the newest is passed over, and the read for the newest request
    /// follows.
    fn next(&self) -> Pin<Box<dyn Future<Output = Result<Value, LiveReadError>> + Send + '_>> {
        Box::pin(async move {
            loop {
                let event = self.query.next().await?;
                let newest = *self.newest.lock().expect("live read revision mutex poisoned");
                if newest.is_some_and(|newest| event.revision.0 != newest) {
                    continue;
                }
                let projection = event.result.map_err(LibraryError::from)?;
                let value = {
                    let mut resolve =
                        self.resolve.lock().expect("live read resolve mutex poisoned");
                    resolve(&event.request, event.revision.0, projection)
                };
                if let Some(value) = value {
                    return Ok(value);
                }
            }
        })
    }

    fn cancel(&self) {
        self.query.cancel();
    }
}

#[cfg(test)]
#[path = "live_read_tests.rs"]
mod tests;
