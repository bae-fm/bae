//! The playback queue's live reads on [`AppServices`]: the queue value, the
//! upcoming tracks as a list past it, and the last addition to Up Next, all
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

    /// One window of the upcoming tracks as the queue draws them, a range of
    /// the order the context lane reads in (`shuffled` picks which). Coven
    /// reruns it as a track in the range comes, goes or moves, and as the
    /// playing track moves.
    pub fn subscribe_upcoming_window(
        &self,
        shuffled: bool,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        super::list_reads::list_window(
            &self.inner.manager,
            self.inner.manager.subscribe_upcoming_window(shuffled, window),
            |read| bae_views::ListWindowValue::of(read, bae_views::ListRows::Upcoming),
        )
    }

    /// How many tracks come after the playing one.
    pub fn subscribe_upcoming_count(
        &self,
        shuffled: bool,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        super::list_reads::list_rows(
            &self.inner.manager,
            self.inner.manager.subscribe_upcoming_count(shuffled),
        )
    }

    /// How many upcoming tracks come before the row at `key`.
    pub fn subscribe_upcoming_position(
        &self,
        shuffled: bool,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        super::list_reads::list_position(
            &self.inner.manager,
            self.inner.manager.subscribe_upcoming_position(shuffled, key),
        )
    }

    delegate_async!(manager, upcoming_key_at => upcoming_key_at(shuffled: bool, position: u64) -> Result<Option<bae_store::list_windows::OrderKey>, bae_library::LibraryError>);
}
