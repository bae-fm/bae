//! How the playback service learns that an output pushed audio events.
//!
//! The device's real-time callback pushes position ticks, completions and
//! starvation reports into a lock-free ring, and must neither allocate, lock
//! nor block, so it can't wake an async task itself: that takes the runtime's
//! scheduler lock. It unparks a relay thread instead, the same operation it
//! already does every buffer to wake the decoder (an atomic swap, plus a
//! semaphore signal only while the relay is parked). The relay, an ordinary
//! thread, notifies the task that drains the ring. A push while the relay is
//! busy leaves the unpark's token, so the relay notifies again; a notification
//! with no task waiting is kept for its next wait. No push goes unseen.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::Thread;

use bae_diagnostics::{Diagnostics, ReportedThread};

struct RelayState {
    ready: tokio::sync::Notify,
    closed: AtomicBool,
}

/// The pushing side's wake, safe on the real-time callback.
pub(crate) struct RelayWake {
    relay: Thread,
}

impl RelayWake {
    /// Tell the drain an event was pushed. Called once per pushed event, never
    /// per sample.
    pub(crate) fn wake(&self) {
        self.relay.unpark();
    }
}

/// The draining side: waits until events were pushed. Dropping it ends the
/// relay thread, and waits for it to end.
pub(crate) struct EventRelay {
    state: Arc<RelayState>,
    /// The relay thread, taken by the drop that joins it.
    relay: Option<ReportedThread>,
}

impl EventRelay {
    /// Complete once events were pushed since the last wait completed, or
    /// at once if they were pushed with no wait in progress.
    pub(crate) async fn ready(&self) {
        self.state.ready.notified().await;
    }
}

impl Drop for EventRelay {
    fn drop(&mut self) {
        self.state.closed.store(true, Ordering::Release);
        let Some(relay) = self.relay.take() else {
            return;
        };
        relay.thread().unpark();
        // The relay thread runs only the loop below, which never drops this,
        // so the join is never on itself.
        relay.join();
    }
}

/// Start a relay thread, and hand back its two sides. A panic on the thread
/// is reported to `diagnostics`.
pub(crate) fn event_relay(diagnostics: &Diagnostics) -> std::io::Result<(RelayWake, EventRelay)> {
    let state = Arc::new(RelayState { ready: tokio::sync::Notify::new(), closed: false.into() });
    let relayed = state.clone();
    let relay = diagnostics.spawn_thread_reported(
        "playback audio events",
        move || loop {
            std::thread::park();
            if relayed.closed.load(Ordering::Acquire) {
                return;
            }
            relayed.ready.notify_one();
        },
        // The relay only parks, reads a flag and notifies; the report is
        // all there is to do.
        |_| {},
    )?;
    Ok((RelayWake { relay: relay.thread().clone() }, EventRelay { state, relay: Some(relay) }))
}

#[cfg(test)]
#[path = "event_relay_tests.rs"]
mod tests;
