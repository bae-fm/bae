//! Libraries still closing after their handle was released on their runtime.

use std::sync::Mutex;
use std::thread::JoinHandle;

/// Each library whose handle was released on its own runtime, closing on a
/// thread of its own until it has. A call that opens or removes one of them
/// waits for its thread first, so it never finds the store still held.
#[derive(Default)]
pub(crate) struct ClosingLibraries(Mutex<Vec<(coven::StoreId, JoinHandle<()>)>>);

impl ClosingLibraries {
    pub(crate) fn add(&self, store: coven::StoreId, closing: JoinHandle<()>) {
        self.0
            .lock()
            .expect("closing libraries lock")
            .push((store, closing));
    }

    /// Wait until every library of `store` still closing has closed. Blocks,
    /// so it runs off any async runtime.
    pub(crate) fn wait(&self, store: coven::StoreId) {
        let waiting: Vec<_> = {
            let mut closing = self.0.lock().expect("closing libraries lock");
            let (waiting, others) = std::mem::take(&mut *closing)
                .into_iter()
                .partition(|(closing, _)| *closing == store);
            *closing = others;
            waiting
        };
        for (_, closing) in waiting {
            if let Err(panic) = closing.join() {
                std::panic::resume_unwind(panic);
            }
        }
    }
}
