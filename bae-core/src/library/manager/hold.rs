//! Where a test holds work whose timing it doesn't otherwise control: while
//! held, each run of the work waits; dropping the returned guard lets them
//! through.

use std::sync::{Arc, Condvar, Mutex};

#[derive(Default)]
pub(crate) struct Hold {
    held: Mutex<bool>,
    released: Condvar,
}

impl Hold {
    /// Wait, blocking this thread, until no test holds the work.
    pub(crate) fn wait(&self) {
        let _released = self
            .released
            .wait_while(self.held.lock().unwrap(), |held| *held)
            .unwrap();
    }

    /// Hold the work until the returned guard drops.
    pub(crate) fn hold(self: &Arc<Self>) -> Held {
        *self.held.lock().unwrap() = true;
        Held(self.clone())
    }
}

/// Lets the held work through when dropped.
pub(crate) struct Held(Arc<Hold>);

impl Drop for Held {
    fn drop(&mut self) {
        *self.0.held.lock().unwrap() = false;
        self.0.released.notify_all();
    }
}
