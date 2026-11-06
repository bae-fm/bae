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
/// relay thread.
pub(crate) struct EventRelay {
    state: Arc<RelayState>,
    relay: Thread,
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
        self.relay.unpark();
    }
}

/// Start a relay thread, and hand back its two sides.
pub(crate) fn event_relay() -> std::io::Result<(RelayWake, EventRelay)> {
    let state = Arc::new(RelayState { ready: tokio::sync::Notify::new(), closed: false.into() });
    let relayed = state.clone();
    let relay = std::thread::Builder::new()
        .name("playback audio events".to_string())
        .spawn(move || loop {
            std::thread::park();
            if relayed.closed.load(Ordering::Acquire) {
                return;
            }
            relayed.ready.notify_one();
        })?
        .thread()
        .clone();
    Ok((RelayWake { relay: relay.clone() }, EventRelay { state, relay }))
}

#[cfg(test)]
#[path = "event_relay_tests.rs"]
mod tests;
