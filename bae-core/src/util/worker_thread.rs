//! A worker thread and the channel that feeds it, owned together.

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// One worker: an OS thread draining a channel, and the sending end of that
/// channel.
///
/// The two halves are created together by [`WorkerThread::spawn`] — the caller
/// never holds a loose sender it could pair with the wrong thread — and are
/// carried together by every clone of the handle that owns them, so whichever
/// clone runs teardown has both the way to stop the thread and the way to wait
/// for it.
pub(crate) struct WorkerThread<M> {
    /// Names the worker in every warning logged here — a dropped message, a
    /// panic seen at the join.
    name: &'static str,
    tx: mpsc::UnboundedSender<M>,
    /// Taken by whichever clone joins; a later `stop_and_join` finds `None` and
    /// returns, so stopping twice is not an error.
    thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl<M> Clone for WorkerThread<M> {
    // Derived `Clone` would demand `M: Clone`, which neither half needs.
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            tx: self.tx.clone(),
            thread: self.thread.clone(),
        }
    }
}

impl<M> WorkerThread<M> {
    /// Open the channel and hand it to `spawn`, which starts the thread that
    /// drains it. Both ends go in: a worker whose loop sends itself messages —
    /// or hands a sender to what it drives — gets the one paired with its own
    /// receiver rather than a loose one the caller had to keep straight.
    pub(crate) fn spawn(
        name: &'static str,
        spawn: impl FnOnce(
            mpsc::UnboundedSender<M>,
            mpsc::UnboundedReceiver<M>,
        ) -> std::thread::JoinHandle<()>,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            name,
            tx: tx.clone(),
            thread: Arc::new(Mutex::new(Some(spawn(tx, rx)))),
        }
    }

    pub(crate) fn send(&self, message: M) -> Result<(), mpsc::error::SendError<M>> {
        self.tx.send(message)
    }

    /// Fire-and-forget send. A worker that has already shut down is not a
    /// caller's error — the message is logged against the worker's name and
    /// dropped.
    pub(crate) fn dispatch(&self, message: M)
    where
        M: std::fmt::Debug,
    {
        if let Err(err) = self.send(message) {
            tracing::warn!("{} command channel closed; dropped {:?}", self.name, err.0);
        }
    }

    /// Ask the thread to stop, then wait for it to exit.
    ///
    /// `stop` says how this particular worker is told to finish — the message it
    /// recognizes, and any acknowledgement it sends back. It runs only for the
    /// caller that takes the join handle, so a second `stop_and_join` neither
    /// re-sends it nor joins again.
    ///
    /// A thread that panicked already reported itself, and this is called from
    /// teardown paths that must not unwind, so a panic payload is logged rather
    /// than repropagated.
    pub(crate) fn stop_and_join(&self, stop: impl FnOnce(&mpsc::UnboundedSender<M>)) {
        let Some(thread) = self.thread.lock().unwrap().take() else {
            return;
        };
        stop(&self.tx);
        if let Err(panic) = thread.join() {
            tracing::warn!("{} panicked before join: {panic:?}", self.name);
        }
    }

    /// The async twin of [`Self::stop_and_join`], for a caller already on a
    /// runtime: `stop` may await the worker's acknowledgement, and the join runs
    /// off-worker so it doesn't stall a runtime thread. The two share the one
    /// take-once join handle, so whichever runs first stops the thread and the
    /// other returns.
    pub(crate) async fn stop_and_join_async<F: std::future::Future<Output = ()>>(
        &self,
        stop: impl FnOnce(&mpsc::UnboundedSender<M>) -> F,
    ) {
        let Some(thread) = self.thread.lock().unwrap().take() else {
            return;
        };
        stop(&self.tx).await;
        let name = self.name;
        match tokio::task::spawn_blocking(move || thread.join()).await {
            Ok(Ok(())) => {}
            Ok(Err(panic)) => tracing::warn!("{name} panicked before join: {panic:?}"),
            Err(join_err) => tracing::warn!("joining {name} failed: {join_err}"),
        }
    }
}
