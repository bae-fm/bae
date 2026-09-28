//! A gate a test holds a worker thread behind, so it can act while that work
//! is in flight.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

type Opened = Arc<(Mutex<bool>, Condvar)>;

/// Holds every [`Held::pass`] until opened. Dropping it opens it, so a test
/// that fails first never leaves a worker blocked.
pub(crate) struct Gate(Opened);

/// The worker's side of a [`Gate`].
pub(crate) struct Held {
    entered: Sender<()>,
    opened: Opened,
}

/// A closed gate, its worker side, and a report of each pass entering.
pub(crate) fn closed() -> (Gate, Held, Receiver<()>) {
    let opened = Opened::default();
    let (entered, entries) = std::sync::mpsc::channel();
    (Gate(opened.clone()), Held { entered, opened }, entries)
}

impl Gate {
    pub(crate) fn open(&self) {
        let (open, changed) = &*self.0;
        *open.lock().unwrap() = true;
        changed.notify_all();
    }
}

impl Drop for Gate {
    fn drop(&mut self) {
        self.open();
    }
}

impl Held {
    /// Report entering, then wait until the gate is open.
    pub(crate) fn pass(&self) {
        let _ = self.entered.send(());
        let (open, changed) = &*self.opened;
        let _open = changed
            .wait_while(open.lock().unwrap(), |open| !*open)
            .unwrap();
    }
}
