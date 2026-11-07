//! Stores still closing after their library's handle was released on its
//! runtime, or the `OpenedLibrary` holding them was released unopened.

use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;

/// Each store whose library's handle was released on its own runtime, or
/// whose `OpenedLibrary` was released unopened, closing on a thread of its
/// own until it has. A call that opens or removes one of them
/// waits for its thread first, so it never finds the store still held.
///
/// Nothing done while the list is locked can panic — a push, or a take, split
/// and put back — so a poisoned lock says nothing about the list, and is not
/// a panic for a released handle's `Drop`, which adds to it, to raise.
pub(crate) struct ClosingLibraries(Mutex<Vec<(coven::StoreId, JoinHandle<()>)>>);

impl ClosingLibraries {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    pub(crate) fn add(&self, store: coven::StoreId, closing: JoinHandle<()>) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).push((store, closing));
    }

    /// Wait until every library of `store` still closing has closed. Blocks,
    /// so it runs off any async runtime. A close that failed or panicked was
    /// reported on its own thread, as a bug of the handle released; it is not
    /// this call's failure.
    pub(crate) fn wait(&self, store: coven::StoreId) {
        let waiting: Vec<_> = {
            let mut closing = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            let (waiting, others) = std::mem::take(&mut *closing)
                .into_iter()
                .partition(|(closing, _)| *closing == store);
            *closing = others;
            waiting
        };
        for (_, closing) in waiting {
            if let Err(panic) = closing.join() {
                tracing::error!(
                    "reporting a library close's failure panicked: {}",
                    bae_util::task_failure::panic_message(panic.as_ref())
                );
            }
        }
    }
}
