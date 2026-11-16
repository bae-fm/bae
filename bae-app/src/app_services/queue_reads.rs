//! The playback queue's live reads on [`AppServices`]: the queue value and
//! the list of tracks after the playing one, both read from the queue
//! tables.

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

    /// Returns the tracks a system media session lists after the playing one:
    /// the Up Next tracks, then the context lane's tracks in queue order, at
    /// most `limit` in all. The stream yields the first value at once, then a
    /// new value after each write that changes these tracks.
    pub fn subscribe_session_up_next(
        &self,
        limit: u64,
    ) -> bae_store::live_query::LiveValues<Vec<bae_playback::QueueItem>> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_session_up_next(limit))
    }

    /// One window of the tracks after the playing one, a range of the context
    /// lane's position order, or of its shuffled order when `shuffled` is
    /// true. Coven reruns it when a write adds, removes or moves a track in
    /// the range, and when the playing track changes.
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
