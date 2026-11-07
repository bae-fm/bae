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

    /// Each time tracks go into Up Next, with how many, for the screens that
    /// confirm an addition.
    pub fn subscribe_queue_additions(&self) -> std::sync::Arc<crate::QueueAdditionsSubscription> {
        crate::screens::queue_additions(
            self.services.subscribe_queue_additions(),
            self.runtime.clone(),
        )
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

    /// Resolve a list of IDs (album or track) to track IDs.
    /// Album IDs are expanded to the primary release's tracks.
    fn resolve_to_track_ids(ids: Vec<String>) -> Vec<String> {
        this.services
            .resolve_to_track_ids(&ids)
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

    /// Move the releases to the cloud, pinned here as the stored import
    /// storage choice says.
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
    fn save_sync_config(config_data: BridgeSaveSyncConfig, device_name: String, setup: Option<std::sync::Arc<crate::BridgeLibrarySetup>>) -> crate::types::BridgeStorageSetup {
        let steps = crate::screens::SetupFeed::new(setup).steps();
        Ok(this
            .services
            .save_s3_config(config_data.into_core()?, &device_name, &|progress| steps.storage(progress))
            .await.map(crate::types::BridgeStorageSetup::from_core)?)
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
        let played = this.services.playback_play_tracks(track_ids, start_index as usize).await?;
        Ok(crate::types::BridgePlayOutcome::from_core(played))
    }

    /// Play the whole library in a new shuffle.
    fn play_library_shuffled() -> crate::types::BridgePlayOutcome {
        let played = this.services.playback_play_library_shuffled().await?;
        Ok(crate::types::BridgePlayOutcome::from_core(played))
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

    fn set_volume(volume: f32) {
        this.services.playback_set_volume(volume);
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

    fn preview_seek_by_ratio(ratio: f64) {
        this.services.playback_preview_seek_by_ratio(ratio);
    }

    fn set_repeat_mode(mode: BridgeRepeatMode) {
        this.services.playback_set_repeat_mode(mode.into_core());
    }

    fn set_shuffle(on: bool) {
        this.services.playback_set_shuffle(on);
    }

    // =========================================================================
    // Queue
    // =========================================================================

    fn add_to_queue(track_ids: Vec<String>) {
        this.services.playback_add_to_queue(track_ids);
    }

    fn add_next(track_ids: Vec<String>) {
        this.services.playback_add_next(track_ids);
    }

    fn add_release_to_queue(release_id: String) {
        this.services.playback_add_release_to_queue(release_id);
    }

    fn add_release_next(release_id: String) {
        this.services.playback_add_release_next(release_id);
    }

    fn insert_in_queue(track_ids: Vec<String>, index: u32) {
        this.services
            .playback_insert_in_queue(track_ids, index as usize);
    }

} }

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    /// Move the entry `entry_id` to sit immediately before `before_entry_id`,
    /// or to the end of its lane without one, and answer the lane's version
    /// the move made: a screen holding the moved row shows the lane's read
    /// again once that read reaches it. A refused move fails with
    /// `QueueChanged`, and the screen shows the lane's read at once.
    ///
    /// The move is sent before the call first waits, so moves sent one after
    /// another, as a drag sends its steps, reach the queue in that order.
    pub async fn reorder_entry(
        self: std::sync::Arc<Self>,
        entry_id: String,
        before_entry_id: Option<String>,
    ) -> Result<u64, BridgeError> {
        let entry_id = parsed_queue_entry_id(&entry_id)?;
        let before = before_entry_id.as_deref().map(parsed_queue_entry_id).transpose()?;
        let answer = self.services.playback_reorder_entry(entry_id, before);
        self.run_exported("AppHandle.reorder_entry", move |_| async move { Ok(answer.await?) })
            .await
    }

    /// Remove the queue entry `entry_id`. An entry no longer queued fails with
    /// `QueueChanged`, and the screen shows the queue as it now is.
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
    /// loading. An entry no longer queued fails with `QueueChanged`.
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
#[path = "queue_additions_tests.rs"]
mod queue_additions_tests;

#[cfg(test)]
#[path = "playback_timeline_tests.rs"]
mod playback_timeline_tests;

#[cfg(test)]
#[path = "screen_pulls_tests.rs"]
mod screen_pulls_tests;
