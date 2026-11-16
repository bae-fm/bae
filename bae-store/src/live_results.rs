//! A coven live query that async code takes results from.
//!
//! A coven live query needs no runtime: its `changed()` future resolves once
//! a write changed what it read or a new request came, and `run_changed()`
//! runs it on the calling thread. An async app awaits `changed()` and runs
//! `run_changed()` on a blocking thread, so no thread waits while a write
//! leaves the result equal (coven's api.md, E4). [`LiveResults`] does
//! that for every async reader of a live query in bae.

use coven::task::{BugSinkRef, TaskFailed};
use std::future::Future;
use std::sync::{Arc, Mutex};

/// What one result of a coven live query is: a value or the query's error,
/// and for a query with a request, the request and revision it answers. It
/// holds for every query, so a type that holds a [`LiveResults`] needs no
/// bounds of its own.
pub trait LiveQueryEvent {
    type Event;
}

impl<T, E> LiveQueryEvent for coven::LiveQuery<T, E> {
    type Event = Result<T, E>;
}

impl<Q, T, E> LiveQueryEvent for coven::ReconfigurableLiveQuery<Q, T, E> {
    type Event = coven::ReconfigurableLiveQueryEvent<Q, T, E>;
}

/// A coven live query, with or without a request: what [`LiveResults`]
/// takes results from.
pub trait CovenLiveQuery: LiveQueryEvent<Event: Send + 'static> + Send + 'static {
    /// Resolves once `run_changed` has something to do.
    fn changed(&self) -> impl Future<Output = ()> + Send + 'static;
    /// Runs the query once if something changed; `None` when nothing did,
    /// when the result equals the last, and once the query has ended.
    fn run_changed(&mut self) -> Option<Self::Event>;
    /// Whether the query answers nothing more.
    fn has_ended(&self) -> bool;
}

impl<T, E> CovenLiveQuery for coven::LiveQuery<T, E>
where
    T: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Send + 'static,
{
    fn changed(&self) -> impl Future<Output = ()> + Send + 'static {
        coven::LiveQuery::changed(self)
    }

    fn run_changed(&mut self) -> Option<Self::Event> {
        coven::LiveQuery::run_changed(self)
    }

    fn has_ended(&self) -> bool {
        coven::LiveQuery::has_ended(self)
    }
}

impl<Q, T, E> CovenLiveQuery for coven::ReconfigurableLiveQuery<Q, T, E>
where
    Q: Clone + PartialEq + Send + Sync + 'static,
    T: Clone + PartialEq + Send + 'static,
    E: From<coven::DbError> + coven::task::CarriesBug + Send + 'static,
{
    fn changed(&self) -> impl Future<Output = ()> + Send + 'static {
        coven::ReconfigurableLiveQuery::changed(self)
    }

    fn run_changed(&mut self) -> Option<Self::Event> {
        coven::ReconfigurableLiveQuery::run_changed(self)
    }

    fn has_ended(&self) -> bool {
        coven::ReconfigurableLiveQuery::has_ended(self)
    }
}

/// The query and a result its run produced after its reader stopped
/// waiting, which the next read returns.
struct Held<L: LiveQueryEvent> {
    query: L,
    unread: Option<L::Event>,
}

/// A live query whose results async code takes one at a time. Each run goes
/// to the blocking pool, and a run whose reader stopped waiting keeps its
/// result for the next read, so a reader that drops [`Self::next`], as a
/// `select!` does, loses no result.
pub struct LiveResults<L: LiveQueryEvent> {
    held: Arc<Mutex<Held<L>>>,
    /// Where a panic of a run no reader waits for goes.
    bugs: BugSinkRef,
}

impl<L: CovenLiveQuery> LiveResults<L> {
    /// `query`'s results, reporting a panic around a run to `bugs`.
    pub fn new(query: L, bugs: BugSinkRef) -> Self {
        Self { held: Arc::new(Mutex::new(Held { query, unread: None })), bugs }
    }

    /// The first result at once, then one each time a write or a new
    /// request changes the result; `None` once the query has ended, a run
    /// panicked outside the query, which goes to the bug sink, or the
    /// runtime shut down.
    pub async fn next(&mut self) -> Option<L::Event> {
        loop {
            let changed = {
                // A poisoned lock ends the query (`Run::Poisoned`).
                let mut held = self.held.lock().ok()?;
                if let Some(event) = held.unread.take() {
                    return Some(event);
                }
                if held.query.has_ended() {
                    return None;
                }
                held.query.changed()
            };
            changed.await;
            let held = Arc::clone(&self.held);
            let ran =
                coven::task::blocking_completing("live query run", self.bugs.clone(), move || {
                    let Ok(mut held) = held.lock() else {
                        return Ok(Run::Poisoned);
                    };
                    if held.unread.is_none() {
                        held.unread = held.query.run_changed();
                    }
                    Ok::<Run, coven::DbError>(Run::Ran)
                })
                .await;
            match ran {
                Ok(Ok(Run::Ran)) => {}
                Ok(Ok(Run::Poisoned)) => return None,
                // The closure returns no error of its own: a failed read is
                // the query's result.
                Ok(Err(_)) => return None,
                Err(TaskFailed::Panicked(bug)) => {
                    coven::task::report(self.bugs.as_ref(), &bug);
                    return None;
                }
                Err(TaskFailed::Shutdown(_)) => return None,
            }
        }
    }

    /// What `read` returns of the query, such as how often it has run.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with<R>(&self, read: impl FnOnce(&L) -> R) -> R {
        read(&self.held.lock().expect("no run of the query panicked").query)
    }
}

/// How a run on the blocking pool ended. A panic in coven's code around the
/// query's own run, which coven catches, poisons the lock that holds the
/// query. The query's state is then unknown, so [`LiveResults::next`] runs
/// it no more and returns `None`; the panic went to the bug sink.
enum Run {
    /// The query ran, or kept the result a reader had not taken yet.
    Ran,
    /// A panic poisoned the lock, and the query did not run.
    Poisoned,
}

#[cfg(test)]
#[path = "live_results_tests.rs"]
mod tests;
