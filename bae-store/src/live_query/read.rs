//! A live query whose request moves in place, each read of it resolved into
//! what its screen takes. Nothing runs between pulls: each
//! [`next`](LiveRead::next) reads the query until a value that answers the
//! request last set resolves.

use super::{CancellableLiveQuery, LiveQueryError};
use crate::LibraryPageWindows;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;

/// What a [`LiveRead`] asks its query for. Coven delivers a read that
/// finished after its request was replaced, which keeps a request changing
/// faster than one read from starving the query; whether that read still
/// answers what the reader shows is a fact about the request.
pub trait LiveRequest: Clone + PartialEq + Send + Sync + 'static {
    /// Whether a read for `self` answers the reader once `current` is the
    /// request set.
    fn still_answers(&self, current: &Self) -> bool;
}

/// A read for another id, or none, shows something the reader no longer
/// asks for.
impl<Id: Clone + PartialEq + Send + Sync + 'static> LiveRequest for Option<Id> {
    fn still_answers(&self, current: &Self) -> bool {
        self == current
    }
}

/// A read for another set of ids answers about ids the reader no longer
/// asks about.
impl LiveRequest for BTreeSet<String> {
    fn still_answers(&self, current: &Self) -> bool {
        self == current
    }
}

/// A read for another window answers about rows the reader no longer asks
/// for.
impl LiveRequest for crate::ListWindow {
    fn still_answers(&self, current: &Self) -> bool {
        self == current
    }
}

/// Each window a read names holds that window's rows as of the read, whatever
/// windows are set since.
impl LiveRequest for LibraryPageWindows {
    fn still_answers(&self, _current: &Self) -> bool {
        true
    }
}

/// The read a value came from: the request it answered, that request's
/// revision, and why the query ran.
pub struct ReadFor<'a, Request> {
    pub request: &'a Request,
    pub revision: u64,
    pub cause: coven::LiveQueryCause,
}

/// A live read whose request changes in place through [`set`](Self::set):
/// one query for as long as the read is open, never one per request. Its
/// query's cancellation is the library's, so closing the library ends it.
/// `E` is the error its query fails with.
pub struct LiveRead<Request, Value, E = crate::DbError> {
    read: Box<dyn Resolved<Request, Value, E>>,
}

impl<Request, Value, E> LiveRead<Request, Value, E>
where
    Request: LiveRequest,
    Value: Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Send + 'static,
{
    /// `query`'s reads that answer the request last set, each resolved by
    /// `resolve` from the read it came from and what the query read. A read
    /// `resolve` answers `None` for delivers nothing.
    pub fn new<Projection: Clone + PartialEq + Send + 'static>(
        query: CancellableLiveQuery<Request, Projection, E>,
        resolve: impl FnMut(ReadFor<'_, Request>, Projection) -> Option<Value> + Send + 'static,
    ) -> Self {
        Self {
            read: Box::new(Resolving {
                query,
                current: std::sync::Mutex::new(None),
                resolve: std::sync::Mutex::new(Box::new(resolve)),
            }),
        }
    }
}

impl<Request, Value, E> LiveRead<Request, Value, E> {
    /// Read `request` from now on, and return the revision each read for it
    /// carries. Repeating the standing request keeps its revision and reads
    /// nothing again.
    pub fn set(&self, request: Request) -> Result<u64, LiveQueryError<E>> {
        self.read.set(request)
    }

    /// The next value that answers the request last set, or the end of the
    /// read. Query errors are values, not the end.
    pub async fn next(&self) -> Result<Value, LiveQueryError<E>> {
        self.read.next().await
    }

    /// Stop reading, settle a waiting [`next`](Self::next), and drop the
    /// query once no read holds it, rather than when the reader lets this go.
    pub async fn cancel(&self) {
        self.read.cancel().await;
    }
}

type Pending<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A live read with the type its query reads hidden, so a read is named by
/// its request and what it delivers alone.
trait Resolved<Request, Value, E>: Send + Sync {
    fn set(&self, request: Request) -> Result<u64, LiveQueryError<E>>;

    fn next(&self) -> Pending<'_, Result<Value, LiveQueryError<E>>>;

    fn cancel(&self) -> Pending<'_, ()>;
}

type Resolve<Request, Projection, Value> =
    Box<dyn FnMut(ReadFor<'_, Request>, Projection) -> Option<Value> + Send>;

struct Resolving<Request, Projection, Value, E> {
    query: CancellableLiveQuery<Request, Projection, E>,
    /// The request last set; a read for another one is delivered only when
    /// it still answers this one. `None` while the query's first request is
    /// the only one. Held while a request is set, so no read is judged
    /// against the request before it.
    current: std::sync::Mutex<Option<Request>>,
    resolve: std::sync::Mutex<Resolve<Request, Projection, Value>>,
}

impl<Request, Projection, Value, E> Resolved<Request, Value, E>
    for Resolving<Request, Projection, Value, E>
where
    Request: LiveRequest,
    Projection: Clone + PartialEq + Send + 'static,
    Value: Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Send + 'static,
{
    fn set(&self, request: Request) -> Result<u64, LiveQueryError<E>> {
        let mut current = self.current.lock().expect("live read request mutex poisoned");
        let revision = self.query.set(request.clone())?;
        *current = Some(request);
        Ok(revision)
    }

    fn next(&self) -> Pending<'_, Result<Value, LiveQueryError<E>>> {
        Box::pin(async move {
            loop {
                let event = self.query.next().await?;
                let answers = self
                    .current
                    .lock()
                    .expect("live read request mutex poisoned")
                    .as_ref()
                    .is_none_or(|current| event.request.still_answers(current));
                if !answers {
                    continue;
                }
                let projection = event.result.map_err(LiveQueryError::Query)?;
                let read = ReadFor {
                    request: &event.request,
                    revision: event.revision.0,
                    cause: event.cause,
                };
                let value = {
                    let mut resolve =
                        self.resolve.lock().expect("live read resolve mutex poisoned");
                    resolve(read, projection)
                };
                if let Some(value) = value {
                    return Ok(value);
                }
            }
        })
    }

    fn cancel(&self) -> Pending<'_, ()> {
        Box::pin(self.query.close())
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
