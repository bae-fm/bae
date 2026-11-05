//! The playback queue's live reads on [`AppServices`]: the queue value, the
//! upcoming tail's windows past it, and the last addition to Up Next, all
//! read from the queue tables.

use super::*;

impl AppServices {
    /// The queue as its screens show it: the first value at once, then one
    /// after each write that changes it.
    pub fn subscribe_queue_values(
        &self,
    ) -> bae_store::live_query::LiveValues<bae_playback::ResolvedQueueSnapshot> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_queue_value())
    }

    /// The last time tracks went into Up Next, as the screens confirming
    /// each addition read it: the first value at once, then one per
    /// addition. `None` until there is one since playback started with the
    /// library.
    pub fn subscribe_queue_additions(
        &self,
    ) -> bae_store::live_query::LiveValues<Option<bae_playback::QueueAddition>> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_queue_addition())
    }

    /// The context's upcoming tail, read in the windows the subscription is
    /// asked for; see [`bae_library::subscribe_queue_upcoming`].
    pub fn subscribe_queue_upcoming(&self) -> bae_library::LiveQueueUpcoming {
        bae_library::subscribe_queue_upcoming(&self.inner.manager)
    }
}
