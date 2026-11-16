use super::*;

#[uniffi::export]
impl AppHandle {
    // =========================================================================
    // Library
    // =========================================================================

    // =========================================================================
    // Playback
    // =========================================================================

    // =========================================================================
    // Queue
    // =========================================================================

    /// The queue as its screens show it, now and after each change.
    pub fn subscribe_queue(&self) -> std::sync::Arc<crate::QueueSubscription> {
        crate::screens::queue(self.services.subscribe_queue_values(), self.runtime.clone())
    }

    /// What playback is doing, apart from where it is in what it plays: the
    /// first value at once, then one after each change.
    pub fn subscribe_playback_values(&self) -> std::sync::Arc<crate::PlaybackValuesSubscription> {
        crate::screens::playback_values(
            self.services.subscribe_playback_values(),
            self.runtime.clone(),
        )
    }

    /// Where playback is, for a surface that moves the position on itself:
    /// the timeline now, then each time playback leaves the line it holds.
    pub fn subscribe_playback_timeline(
        &self,
    ) -> std::sync::Arc<crate::PlaybackTimelineSubscription> {
        crate::screens::playback_timeline(
            self.services.subscribe_playback_timeline(),
            self.runtime.clone(),
        )
    }

    /// How many downloads are in each phase, and whether the queue is
    /// paused, now and after each change.
    pub fn subscribe_download_counts(&self) -> std::sync::Arc<crate::DownloadCountsSubscription> {
        crate::screens::download_counts(
            self.services.subscribe_download_counts(),
            self.runtime.clone(),
        )
    }
}

forward! { async this => {
    /// How many downloads are in each phase now, and whether the queue is
    /// paused.
    fn get_download_counts() -> crate::types::BridgeDownloadCounts {
        this.services
            .download_counts()
            .await
            .map(crate::types::BridgeDownloadCounts::from_core)
            .map_err(BridgeError::from)
    }

    /// One window of the albums a search finds, read once: at most `limit`
    /// albums starting `offset` albums in. A blank query finds nothing.
    fn read_search_album_page(
        query: String,
        offset: u64,
        limit: u64,
    ) -> Vec<crate::types::BridgeAlbumSearchResult> {
        let Some(query) = bae_views::LibrarySearchQuery::parse(&query) else {
            return Ok(Vec::new());
        };
        Ok(this
            .services
            .search_album_page(&query, offset, limit)
            .await
            .map_err(BridgeError::from)?
            .into_iter()
            .map(crate::types::BridgeAlbumSearchResult::from_core)
            .collect())
    }

    /// One track of a release as a browse item shows it, read once; `None`
    /// when the release has no such track.
    fn read_release_track(
        release_id: String,
        track_id: String,
    ) -> Option<crate::types::BridgeBrowseTrack> {
        Ok(this
            .services
            .find_release_track(&release_id, &track_id)
            .await
            .map_err(BridgeError::from)?
            .map(crate::types::BridgeBrowseTrack::from_core))
    }

    /// The track a spoken "play" of the search plays, read once, or `None`
    /// when the search finds no track to play.
    fn read_search_top_playable(query: String) -> Option<crate::types::BridgeTopPlayable> {
        let Some(query) = bae_views::LibrarySearchQuery::parse(&query) else {
            return Ok(None);
        };
        Ok(this
            .services
            .search_library(&query)
            .await
            .map_err(BridgeError::from)?
            .top_playable
            .map(crate::types::BridgeTopPlayable::from_core))
    }

    /// One album's detail, read once; `None` when the album doesn't exist.
    fn read_album_detail(album_id: String) -> Option<crate::types::BridgeAlbumDetail> {
        Ok(this
            .services
            .find_album_detail(&album_id)
            .await
            .map_err(BridgeError::from)?
            .map(crate::types::BridgeAlbumDetail::from_core))
    }

    /// One composer's page above its rows, read once; `None` when the composer
    /// doesn't exist.
    fn read_composer_detail(artist_id: String) -> Option<crate::types::BridgeComposerDetail> {
        Ok(this
            .services
            .get_composer_detail(&artist_id)
            .await
            .map_err(BridgeError::from)?
            .map(crate::types::BridgeComposerDetail::from_core))
    }

    /// One work's detail, read once; `None` when the work doesn't exist.
    fn read_work_detail(work_id: String) -> Option<crate::types::BridgeWorkDetail> {
        Ok(this
            .services
            .get_work_detail(&work_id)
            .await
            .map_err(BridgeError::from)?
            .map(crate::types::BridgeWorkDetail::from_core))
    }

    /// The sort values of the row at `position` of `list`, read by offset, or
    /// of its last row when the list ends before it; none for an empty list.
    /// A screen reads this key to start its windows at a position.
    fn list_key_at(list: BridgeList, position: u64) -> Option<Vec<BridgeSortValue>> {
        let key = match list {
            BridgeList::AlbumGrid { sort, group_by_artist } => this
                .services
                .album_key_at(&BridgeList::album_sort(sort), group_by_artist, position)
                .await
                .map_err(BridgeError::from)?,
            BridgeList::Artists { sort } => this
                .services
                .artist_key_at(&BridgeList::artist_sort(sort), position)
                .await
                .map_err(BridgeError::from)?,
            BridgeList::Composers { sort } => this
                .services
                .composer_key_at(&BridgeList::composer_sort(sort), position)
                .await
                .map_err(BridgeError::from)?,
            BridgeList::ComposerPage { artist_id } => this
                .services
                .composer_detail_key_at(&artist_id, position)
                .await
                .map_err(BridgeError::from)?,
            BridgeList::Downloads => this
                .services
                .download_key_at(position)
                .await
                .map_err(BridgeError::from)?,
            BridgeList::Upcoming { shuffled } => this
                .services
                .upcoming_key_at(shuffled, position)
                .await
                .map_err(BridgeError::from)?,
            BridgeList::ArtistSearch { text } => this
                .services
                .artist_search_key_at(BridgeList::artist_search_query(&text), position)
                .await
                .map_err(BridgeError::from)?,
            BridgeList::Storage { sort, filter, search } => this
                .services
                .storage_key_at(&BridgeList::storage_sort(sort), filter.into_core(), &search, position)
                .await
                .map_err(BridgeError::from)?,
        };
        Ok(key.as_ref().map(BridgeSortValue::of_key))
    }

    /// 0-based position of `album_id` under the given sort, or `None` if the
    /// album isn't present.
    /// Lets the grid load the page containing an album and scroll to it
    /// without depending on that page already being fetched.
    fn get_album_index(
        sort_criteria: Vec<BridgeSortCriterion>,
        album_id: String,
        group_by_artist: bool,
    ) -> Option<u64> {
        let sort: Vec<bae_views::AlbumSortCriterion> = sort_criteria
            .into_iter()
            .map(BridgeSortCriterion::into_core)
            .collect();
        this.services
            .get_album_index(&sort, &album_id, group_by_artist)
            .await
            .map_err(BridgeError::from)
    }

    /// The albums of `album_ids` the grid lists, each once, in the grid's
    /// order under `sort_criteria` and `group_by_artist`: what an action on
    /// the grid's selection acts on, in that order.
    fn albums_in_browse_order(
        sort_criteria: Vec<BridgeSortCriterion>,
        album_ids: Vec<String>,
        group_by_artist: bool,
    ) -> Vec<BridgeAlbum> {
        let sort: Vec<bae_views::AlbumSortCriterion> = sort_criteria
            .into_iter()
            .map(BridgeSortCriterion::into_core)
            .collect();
        Ok(this
            .services
            .albums_in_browse_order(&sort, album_ids, group_by_artist)
            .await
            .map_err(BridgeError::from)?
            .into_iter()
            .map(BridgeAlbum::from_core)
            .collect())
    }

    /// The tracks of `track_ids` the library holds, each once, in the order
    /// their release lists them. An action on tracks the person picked uses
    /// this order, whatever order the person picked them in.
    fn tracks_in_release_order(track_ids: Vec<String>) -> Vec<String> {
        this.services.track_ids_in_release_order(track_ids).await.map_err(BridgeError::from)
    }

    /// Resolve a list of IDs (album or track) to track IDs.
    /// Album IDs are expanded to the primary release's tracks.
    fn resolve_to_track_ids(ids: Vec<String>) -> Vec<String> {
        this.services
            .resolve_to_track_ids(&ids)
            .await
            .map_err(BridgeError::from)
    }

    /// Whether the Storage Manager list under `filter` and the `search` text
    /// holds every one of the releases.
    fn storage_holds_all(
        filter: crate::types::BridgeStorageFilter,
        search: String,
        release_ids: Vec<String>,
    ) -> bool {
        this.services
            .storage_holds_all(filter.into_core(), &search, release_ids.into_iter().collect())
            .await
            .map_err(BridgeError::from)
    }

    fn set_primary_release(album_id: String, release_id: String) -> () {
        this.services
            .set_album_primary_release(&album_id, &release_id)
            .await
            .map_err(BridgeError::from)
    }

    /// Unpin the releases, every one in one write.
    fn unpin_releases(release_ids: Vec<String>) -> () {
        Ok(this.services.unpin_releases(&release_ids).await?)
    }

    /// Move the releases to the cloud.
    fn upload_releases(release_ids: Vec<String>) -> BridgeUploadReleasesOutcome {
        this.services
            .upload_releases(&release_ids)
            .await
            .map(BridgeUploadReleasesOutcome::from_core)
            .map_err(BridgeError::from)
    }

    fn delete_release(release_id: String) -> () {
        Ok(this.services.delete_release(&release_id).await?)
    }

    fn delete_album(album_id: String) -> () {
        Ok(this.services.delete_album(&album_id).await?)
    }

    /// Set S3 up as the library's storage, each step coven runs written into
    /// `setup`'s stream when the app shows the setup.
    fn save_sync_config(config_data: BridgeSaveSyncConfig, setup: Option<std::sync::Arc<crate::BridgeLibrarySetup>>) -> crate::types::BridgeStorageSetup {
        let steps = crate::screens::SetupFeed::new(setup).steps();
        this.save_storage(config_data, &steps).await
    }

    /// This person's restore code to type, and for the QR the code that also
    /// holds the S3 key this device holds.
    fn generate_restore_code() -> crate::types::BridgeRestoreCodeShare {
        Ok(crate::types::BridgeRestoreCodeShare::from_core(this.services.generate_restore_code().await?))
    }

    /// Enqueue releases to pin for offline. They join the serial download
    /// queue; the worker drains them one at a time. Fails, queueing none of
    /// them, when a release's storage can't be read; the deep cloud download
    /// runs on the queue worker.
    fn queue_pin_releases(release_ids: Vec<String>) -> () {
        Ok(this.services.enqueue_pins(release_ids).await?)
    }

    // ── Download (pin) queue ─────────────────────────────────────────

    /// Pause or resume the download queue. In-flight downloads finish; the queue
    /// stops starting new ones until resumed.
    fn set_downloads_paused(paused: bool) -> () {
        Ok(this.services.set_downloads_paused(paused).await?)
    }

    /// Cancel a release's download — drops a queued/failed entry or aborts the
    /// in-flight one (a partial download never lands, so the release stays
    /// cloud-only).
    fn cancel_download(release_id: String) -> () {
        Ok(this.services.cancel_download(&release_id).await?)
    }

    /// Retry one release's failed download now (flips it back to queued and
    /// wakes the worker); other failed downloads stay failed.
    fn retry_download(release_id: String) -> () {
        Ok(this.services.retry_download(&release_id).await?)
    }

    /// Retry every failed download now (flips them back to queued and wakes the
    /// worker).
    fn retry_downloads() -> () {
        Ok(this.services.retry_downloads().await?)
    }

    // =========================================================================
    // Playback: each play answers with what it did
    // =========================================================================

    /// Play a release from `start_track_id`, or from its first track, or
    /// shuffled.
    fn play_release(
        release_id: String,
        start_track_id: Option<String>,
        shuffle: bool,
    ) -> crate::types::BridgePlayOutcome {
        let played =
            this.services.playback_play_release(release_id, start_track_id, shuffle).await?;
        Ok(crate::types::BridgePlayOutcome::from_core(played))
    }

    /// Play several releases one after another, in the order given.
    fn play_releases(release_ids: Vec<String>) -> crate::types::BridgePlayOutcome {
        let played = this.services.playback_play_releases(release_ids).await?;
        Ok(crate::types::BridgePlayOutcome::from_core(played))
    }

    /// Play `track_ids` in that order, from the one at `start_index`.
    fn play_tracks(track_ids: Vec<String>, start_index: u32) -> crate::types::BridgePlayOutcome {
        let played = this.services.playback_play_tracks(track_ids, bae_playback::tables::TracksStart::Index(start_index as usize)).await?;
        Ok(crate::types::BridgePlayOutcome::from_core(played))
    }

    /// Play the whole library in a new shuffle.
    fn play_library_shuffled() -> crate::types::BridgePlayOutcome {
        let played = this.services.playback_play_library_shuffled().await?;
        Ok(crate::types::BridgePlayOutcome::from_core(played))
    }

    /// The person dismissed the failure notice: clear the failure that the
    /// `playback_now` row holds, so the notice shows again only for a later
    /// failure.
    fn dismiss_playback_failure() -> () {
        Ok(this.services.dismiss_playback_failure().await?)
    }

} }

forward! { sync this => {
    // =========================================================================
    // Playback
    // =========================================================================

    fn pause() {
        this.services.playback_pause();
    }

    fn resume() {
        this.services.playback_resume();
    }

    /// Stop a running side-pause countdown and stay paused at the boundary —
    /// the prompt's Close. Play still starts the next side.
    fn cancel_side_pause_countdown() {
        this.services.playback_cancel_side_pause_countdown();
    }

    fn stop() {
        this.services.playback_stop();
    }

    fn next_track() {
        this.services.playback_next();
    }

    fn previous_track() {
        this.services.playback_previous();
    }

    fn seek_by_ratio(ratio: f64) {
        this.services.playback_seek_by_ratio(ratio);
    }

    fn set_volume_live(volume: f32) {
        this.services.playback_set_volume_live(volume);
    }

    fn set_muted(muted: bool) {
        this.services.playback_set_muted(muted);
    }

    fn preview_play(target: BridgePreviewTarget) -> Result<(), BridgeError> {
        this.services.playback_preview_play(target.into_core()?);
        Ok(())
    }

    fn preview_stop() {
        this.services.playback_preview_stop();
    }

    fn preview_toggle_pause() {
        this.services.playback_preview_toggle_pause();
    }

    /// Pause the active preview, or play it on; a preview already so stays so.
    fn preview_set_paused(paused: bool) {
        this.services.playback_preview_set_paused(paused);
    }

    fn preview_seek_by_ratio(ratio: f64) {
        this.services.playback_preview_seek_by_ratio(ratio);
    }
} }

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    /// Move the entry `entry_id` to sit immediately before `before_entry_id`,
    /// or to the end of its lane without one, and answer whether it moved:
    /// `false` when the entry, or the one it goes before, is no longer queued
    /// there, so nothing moved. The lane's list shows the new order when its
    /// live query runs again after the write.
    ///
    /// The move is sent before the call first waits, so moves sent one after
    /// another, as a drag sends its steps, reach the queue in that order.
    pub async fn reorder_entry(
        self: std::sync::Arc<Self>,
        entry_id: String,
        before_entry_id: Option<String>,
    ) -> Result<bool, BridgeError> {
        let entry_id = parsed_queue_entry_id(&entry_id)?;
        let before = before_entry_id.as_deref().map(parsed_queue_entry_id).transpose()?;
        let answer = self.services.playback_reorder_entry(entry_id, before);
        self.run_exported("AppHandle.reorder_entry", move |_| async move { Ok(answer.await?) })
            .await
    }

    /// Add tracks to the end of Up Next, and return how many tracks it added,
    /// so the screen that called it can show that number. `add_to_queue` sends
    /// the command before it first waits, so commands sent one after another
    /// reach the queue in that order.
    pub async fn add_to_queue(
        self: std::sync::Arc<Self>,
        track_ids: Vec<String>,
    ) -> Result<u32, BridgeError> {
        let answer = self.services.playback_add_to_queue(track_ids);
        self.run_exported(
            "AppHandle.add_to_queue",
            move |_| async move { Ok(answer.await? as u32) },
        )
        .await
    }

    /// Add tracks to the front of Up Next, and return how many tracks it
    /// added.
    pub async fn add_next(
        self: std::sync::Arc<Self>,
        track_ids: Vec<String>,
    ) -> Result<u32, BridgeError> {
        let answer = self.services.playback_add_next(track_ids);
        self.run_exported("AppHandle.add_next", move |_| async move { Ok(answer.await? as u32) })
            .await
    }

    /// Add a release's tracks to the end of Up Next, and return how many
    /// tracks it added.
    pub async fn add_release_to_queue(
        self: std::sync::Arc<Self>,
        release_id: String,
    ) -> Result<u32, BridgeError> {
        let answer = self.services.playback_add_release_to_queue(release_id);
        self.run_exported("AppHandle.add_release_to_queue", move |_| async move {
            Ok(answer.await? as u32)
        })
        .await
    }

    /// Add a release's tracks to the front of Up Next, and return how many
    /// tracks it added.
    pub async fn add_release_next(
        self: std::sync::Arc<Self>,
        release_id: String,
    ) -> Result<u32, BridgeError> {
        let answer = self.services.playback_add_release_next(release_id);
        self.run_exported(
            "AppHandle.add_release_next",
            move |_| async move { Ok(answer.await? as u32) },
        )
        .await
    }

    /// Insert tracks into Up Next before the entry at `index`, and return how
    /// many tracks it added.
    pub async fn insert_in_queue(
        self: std::sync::Arc<Self>,
        track_ids: Vec<String>,
        index: u32,
    ) -> Result<u32, BridgeError> {
        let answer = self.services.playback_insert_in_queue(track_ids, index as usize);
        self.run_exported(
            "AppHandle.insert_in_queue",
            move |_| async move { Ok(answer.await? as u32) },
        )
        .await
    }

    /// Set the volume a slider's drag ends on, answering once playback has
    /// stored it, so the slider can hold the dragged level until then.
    pub async fn set_volume(self: std::sync::Arc<Self>, volume: f32) -> Result<(), BridgeError> {
        let answer = self.services.playback_set_volume(volume);
        self.run_exported("AppHandle.set_volume", move |_| async move { Ok(answer.await?) }).await
    }

    /// Set how the queue repeats, answering once it is set.
    pub async fn set_repeat_mode(
        self: std::sync::Arc<Self>,
        mode: BridgeRepeatMode,
    ) -> Result<(), BridgeError> {
        let answer = self.services.playback_set_repeat_mode(mode.into_core());
        self.run_exported("AppHandle.set_repeat_mode", move |_| async move { Ok(answer.await?) })
            .await
    }

    /// Shuffle the context lane or put it back in order, answering once it is
    /// done.
    pub async fn set_shuffle(self: std::sync::Arc<Self>, on: bool) -> Result<(), BridgeError> {
        let answer = self.services.playback_set_shuffle(on);
        self.run_exported("AppHandle.set_shuffle", move |_| async move { Ok(answer.await?) }).await
    }

    /// Remove the queue entry `entry_id`. An entry no longer queued is already
    /// gone, and the screen shows the queue as it now is.
    ///
    /// The command is sent before the call first waits, so commands sent one
    /// after another reach the queue in that order.
    pub async fn remove_entry(
        self: std::sync::Arc<Self>,
        entry_id: String,
    ) -> Result<(), BridgeError> {
        let entry_id = parsed_queue_entry_id(&entry_id)?;
        let answer = self.services.playback_remove_entry(entry_id);
        self.run_exported("AppHandle.remove_entry", move |_| async move { Ok(answer.await?) }).await
    }

    /// Skip to the queue entry `entry_id`, answering once its track is
    /// loading. An entry no longer queued is not skipped to, and the screen
    /// shows the queue as it now is.
    pub async fn skip_to_entry(
        self: std::sync::Arc<Self>,
        entry_id: String,
    ) -> Result<(), BridgeError> {
        let entry_id = parsed_queue_entry_id(&entry_id)?;
        let answer = self.services.playback_skip_to_entry(entry_id);
        self.run_exported("AppHandle.skip_to_entry", move |_| async move { Ok(answer.await?) })
            .await
    }

    /// Empty Up Next, the manual lane, leaving the playing context.
    pub async fn clear_up_next(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let answer = self.services.playback_clear_up_next();
        self.run_exported("AppHandle.clear_up_next", move |_| async move { Ok(answer.await?) })
            .await
    }

    /// Drop the context lane; the playing track keeps playing.
    pub async fn clear_playing_from(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let answer = self.services.playback_clear_playing_from();
        self.run_exported("AppHandle.clear_playing_from", move |_| async move { Ok(answer.await?) })
            .await
    }
}

/// `entry_id` as the queue's id, or why it isn't one the queue hands out: a
/// bug of the screen that sent it.
fn parsed_queue_entry_id(entry_id: &str) -> Result<QueueEntryId, BridgeError> {
    entry_id.parse().map_err(|e| {
        BridgeError::diagnostic(
            crate::types::BridgeErrorCategory::Internal,
            format!("queue entry id {entry_id:?} is not one the queue hands out: {e}"),
        )
    })
}

#[cfg(test)]
#[path = "base_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "playback_values_tests.rs"]
mod playback_values_tests;

#[cfg(test)]
#[path = "queue_adds_tests.rs"]
mod queue_adds_tests;

#[cfg(test)]
#[path = "playback_timeline_tests.rs"]
mod playback_timeline_tests;

#[cfg(test)]
#[path = "screen_pulls_tests.rs"]
mod screen_pulls_tests;
