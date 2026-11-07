//! A worker thread and the channel that feeds it, owned together.

use std::sync::{Arc, Mutex};

use coven::task::{blocking_completing, caught_sync, report, Bug, BugSinkRef, TaskFailed};
use tokio::sync::{mpsc, watch};

use crate::task_failure::panic_message;

/// One worker: an OS thread draining a channel, the sending end of that
/// channel, and how the thread ended once it has.
///
/// The two halves are created together by [`WorkerThread::spawn`] — the caller
/// never holds a loose sender it could pair with the wrong thread — and are
/// carried together by every clone of the handle that owns them, so whichever
/// clone runs teardown has both the way to stop the thread and the way to wait
/// for it.
pub struct WorkerThread<M> {
    /// Names the worker's thread, the warning a dropped message logs, and the
    /// bug a panic in the thread is reported as.
    name: &'static str,
    /// Where a panic that escaped the thread's own catch is reported, seen at
    /// the join.
    bugs: BugSinkRef,
    tx: mpsc::UnboundedSender<M>,
    /// How the thread ended, once it has. The thread records it in the same
    /// step that closes the channel, so a send finds either the channel open
    /// or the end.
    end: watch::Receiver<Option<WorkerEnd>>,
    /// Taken by whichever clone joins; a later `stop_and_join` finds `None` and
    /// returns, so stopping twice is not an error.
    thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

/// How a worker's thread ended.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorkerEnd {
    /// Its body returned.
    #[error("the worker's thread had returned")]
    Returned,
    /// Its body panicked with this bug, which reached the worker's bug sink as
    /// it happened.
    #[error("the worker's thread panicked")]
    Panicked(#[source] Bug),
}

/// A message sent after the worker's thread ended, given back with how the
/// thread ended.
#[derive(Debug)]
pub struct WorkerEnded<M> {
    pub message: M,
    pub end: WorkerEnd,
}

impl<M> Clone for WorkerThread<M> {
    // Derived `Clone` would demand `M: Clone`, which no half needs.
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            bugs: self.bugs.clone(),
            tx: self.tx.clone(),
            end: self.end.clone(),
            thread: self.thread.clone(),
        }
    }
}

impl<M: Send + 'static> WorkerThread<M> {
    /// Open the channel and start the thread named `name`, which runs `body`
    /// to drain it. The body gets a sender paired with its own receiver — for
    /// a worker whose loop sends itself messages, or hands a sender to what it
    /// drives — and borrows the receiver, which the thread keeps: the channel
    /// closes only once the thread has recorded how the body ended: returned
    /// or panicked. A panic in `body` goes to `bugs` as it happens, on the
    /// thread, which then ends; the join does not report it again.
    ///
    /// Panics if the system refuses the thread, as `std::thread::spawn` does.
    pub fn spawn(
        name: &'static str,
        bugs: BugSinkRef,
        body: impl FnOnce(mpsc::UnboundedSender<M>, &mut mpsc::UnboundedReceiver<M>) + Send + 'static,
    ) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let body_tx = tx.clone();
        let (ended, end) = watch::channel(None);
        let heard = bugs.clone();
        let thread = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let (recorded, panicked) = match caught_sync(name, || body(body_tx, &mut rx)) {
                    Ok(()) => (WorkerEnd::Returned, None),
                    Err(bug) => (WorkerEnd::Panicked(bug.clone()), Some(bug)),
                };
                ended.send_modify(move |end| {
                    *end = Some(recorded);
                    drop(rx);
                });
                if let Some(bug) = panicked {
                    report(heard.as_ref(), &bug);
                }
            })
            .unwrap_or_else(|error| panic!("start the {name} thread: {error}"));
        Self { name, bugs, tx, end, thread: Arc::new(Mutex::new(Some(thread))) }
    }
}

impl<M> WorkerThread<M> {
    /// Hand `message` to the thread, or give it back with how the thread
    /// ended.
    pub fn send(&self, message: M) -> Result<(), WorkerEnded<M>> {
        // Held across the send, so the thread can't close the channel
        // meanwhile.
        let end = self.end.borrow();
        if let Some(end) = end.as_ref() {
            return Err(WorkerEnded { message, end: end.clone() });
        }
        if self.tx.send(message).is_err() {
            unreachable!("the {} channel closed before its thread recorded its end", self.name);
        }
        Ok(())
    }

    /// How the thread ended, once it has.
    pub async fn ended(&self) -> WorkerEnd {
        let mut end = self.end.clone();
        let ended = match end.wait_for(Option::is_some).await.as_deref() {
            Ok(Some(end)) => end.clone(),
            _ => unreachable!("the {} thread let go of its end before recording it", self.name),
        };
        ended
    }

    /// Fire-and-forget send. A worker that has already ended is not a
    /// caller's error — the message is logged against the worker's name and
    /// how it ended, and dropped.
    pub fn dispatch(&self, message: M)
    where
        M: std::fmt::Debug,
    {
        if let Err(ended) = self.send(message) {
            tracing::warn!("{} ended ({:?}); dropped {:?}", self.name, ended.end, ended.message);
        }
    }

    /// Ask the thread to stop, then wait for it to exit.
    ///
    /// `stop` says how this particular worker is told to finish — the message it
    /// recognizes, and any acknowledgement it sends back. It runs only for the
    /// caller that takes the join handle, so a second `stop_and_join` neither
    /// re-sends it nor joins again.
    ///
    /// The thread reported its own panic as it happened; one that escaped
    /// that catch is reported to the worker's bug sink here, not raised
    /// again: this runs on teardown paths that must not unwind.
    pub fn stop_and_join(&self, stop: impl FnOnce(&mpsc::UnboundedSender<M>)) {
        let Some(thread) = self.thread.lock().unwrap().take() else {
            return;
        };
        stop(&self.tx);
        if let Err(bug) = joined_thread(self.name, thread) {
            report(self.bugs.as_ref(), &bug);
        }
    }

    /// The async twin of [`Self::stop_and_join`], for a caller already on a
    /// runtime: `stop` may await the worker's acknowledgement, and the join runs
    /// off-worker so it doesn't stall a runtime thread. The two share the one
    /// take-once join handle, so whichever runs first stops the thread and the
    /// other returns. The join runs to its end even when this caller stops
    /// waiting, and a panic that escaped the thread's own catch reaches the
    /// bug sink either way.
    pub async fn stop_and_join_async<Stopped: std::future::Future<Output = ()>>(
        &self,
        stop: impl FnOnce(&mpsc::UnboundedSender<M>) -> Stopped,
    ) {
        let Some(thread) = self.thread.lock().unwrap().take() else {
            return;
        };
        stop(&self.tx).await;
        let name = self.name;
        let joined = blocking_completing("join a worker thread", self.bugs.clone(), move || {
            joined_thread(name, thread)
        });
        match joined.await {
            Ok(Ok(())) => {}
            Ok(Err(bug)) | Err(TaskFailed::Panicked(bug)) => report(self.bugs.as_ref(), &bug),
            Err(failed @ TaskFailed::Shutdown(_)) => {
                tracing::warn!("joining {name} failed: {failed}")
            }
        }
    }
}

/// Wait for the worker `name`'s thread to exit, and the bug its panic was if
/// one escaped the thread's own catch.
fn joined_thread(name: &'static str, thread: std::thread::JoinHandle<()>) -> Result<(), Bug> {
    thread
        .join()
        .map_err(|payload| Bug { operation: name, message: panic_message(payload.as_ref()) })
}

#[cfg(test)]
#[path = "worker_thread_tests.rs"]
mod tests;
