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
    /// Names the thread in the warnings [`Self::stop_and_join`] logs.
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
    /// Open the channel and hand its receiving end to `spawn`, which starts the
    /// thread that drains it.
    pub(crate) fn spawn(
        name: &'static str,
        spawn: impl FnOnce(mpsc::UnboundedReceiver<M>) -> std::thread::JoinHandle<()>,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            name,
            tx,
            thread: Arc::new(Mutex::new(Some(spawn(rx)))),
        }
    }

    pub(crate) fn send(&self, message: M) -> Result<(), mpsc::error::SendError<M>> {
        self.tx.send(message)
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
}
