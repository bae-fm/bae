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

    pub fn subscribe_queue(
        &self,
        callback: Box<dyn crate::types::QueueCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_channel(
            |services| services.subscribe_queue_values(),
            move |value| match value {
                Ok(value) => callback.on_value(BridgeQueueSnapshot::from_core(value)),
                Err(error) => callback.on_error(BridgeError::internal(error)),
            },
        )
    }

    pub fn subscribe_playback_values(
        &self,
        callback: Box<dyn crate::types::PlaybackValuesCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_channel(
            |services| services.subscribe_playback_values(),
            move |value| match value {
                Ok(value) => callback.on_value(BridgePlaybackValues::from_core(value)),
                Err(error) => callback.on_error(BridgeError::internal(error)),
            },
        )
    }

    /// Where playback is in what it plays, now and then as it moves.
    pub fn subscribe_playback_positions(
        &self,
        callback: Box<dyn crate::types::PlaybackPositionsCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_watch(
            |services| services.subscribe_playback_positions(),
            move |value| {
                callback.on_value(crate::types::BridgePlaybackPositions::from_core(
                    value.clone(),
                ))
            },
        )
    }

    /// How many downloads are in each phase, and whether the queue is
    /// paused, now and after each change.
    pub fn subscribe_download_counts(
        &self,
        callback: Box<dyn crate::types::DownloadCountsCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_channel(
            |services| services.subscribe_download_counts(),
            move |value| match value {
                Ok(value) => {
                    callback.on_value(crate::types::BridgeDownloadCounts::from_core(value))
                }
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    /// How far each file being downloaded to keep whole has got, by file:
    /// every one now, then each as it moves, starts or ends.
    pub fn subscribe_pin_file_transfers(
        &self,
        callback: Box<dyn crate::types::FileTransfersCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.live_subscription(move |services| async move {
            let mut files = services.watch_pin_file_transfers();
            while let Some(changes) = files.next().await {
                callback.on_changes(
                    changes
                        .into_iter()
                        .map(crate::types::BridgeFileTransferChange::from_core)
                        .collect(),
                );
            }
        })
    }

    /// The bytes each downloading release has moved, now and then as they
    /// move.
    pub fn subscribe_download_transfers(
        &self,
        callback: Box<dyn crate::types::DownloadTransfersCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.live_subscription(move |services| async move {
            let mut transfers = services.watch_download_transfers();
            while let Some(changes) = transfers.next().await {
                callback.on_changes(
                    changes
                        .into_iter()
                        .map(crate::types::BridgeDownloadTransferChange::from_core)
                        .collect(),
                );
            }
        })
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
            .map_err(BridgeError::database_query)
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
        let sort: Vec<bae_core::db::AlbumSortCriterion> = sort_criteria
            .into_iter()
            .map(BridgeSortCriterion::into_core)
            .collect();
        this.services
            .get_album_index(&sort, &album_id, group_by_artist)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    /// Filesystem path for the user's own external file behind a library file
    /// (including the DiscID re-read of a rip's LOG/CUE evidence; retained
    /// source-audio facts supply its duration). Returns `Ok(None)` if the
    /// file has no readable local location (e.g. cloud-only and not cached).
    /// Returns `Err` on DB failures so callers can distinguish a missing file
    /// from a broken library state. NOT a substitute for a coven byte read.
    fn file_path(file_id: String) -> Option<String> {
        let path = this
            .services
            .file_local_path(&file_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(path.and_then(|p| p.to_str().map(|s| s.to_string())))
    }

    /// Resolve a list of IDs (album or track) to track IDs.
    /// Album IDs are expanded to the primary release's tracks.
    fn resolve_to_track_ids(ids: Vec<String>) -> Vec<String> {
        this.services
            .resolve_to_track_ids(&ids)
            .await
            .map_err(BridgeError::database)
    }

    fn change_cover(release_id: String, selection: BridgeCoverSelection) -> () {
        use bae_core::library::CoverSelection;

        let core_selection = match selection {
            BridgeCoverSelection::ReleaseImage { file_id } => {
                CoverSelection::ReleaseImage { file_id }
            }
            // The library stores the original; its copies are for drawing.
            BridgeCoverSelection::RemoteCover { selection } => CoverSelection::RemoteCover {
                url: selection.image.url,
                source: selection.source.into_core(),
            },
            BridgeCoverSelection::EmbeddedCover { source_file_id } => {
                return Err(BridgeError::internal(format!(
                    "embedded candidate cover {source_file_id} is not a library cover choice"
                )))
            }
        };

        this.services
            .change_cover(&release_id, core_selection)
            .await
            .map_err(|e| BridgeError::internal(format!("{e}")))
    }

    fn set_primary_release(album_id: String, release_id: String) -> () {
        this.services
            .set_album_primary_release(&album_id, &release_id)
            .await
            .map_err(|e| BridgeError::internal(format!("{e}")))
    }

    fn unpin_release(release_id: String) -> () {
        Ok(this.services.unpin_release(&release_id).await?)
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

    fn detach_release_audio(release_id: String, new_path: String) -> () {
        Ok(this.services.detach_release_audio(&release_id, &new_path).await?)
    }

    fn delete_release(release_id: String) -> () {
        Ok(this.services.delete_release(&release_id).await?)
    }

    fn save_sync_config(config_data: BridgeSaveSyncConfig, device_name: String) -> crate::types::BridgeStorageSetup {
        Ok(this
            .services
            .save_s3_config(config_data.into_core()?, &device_name)
            .await.map(crate::types::BridgeStorageSetup::from_core)?)
    }

    fn disconnect_cloud_provider() -> () {
        Ok(this.services.disconnect_cloud_provider().await?)
    }

    /// How many releases live only in the cloud and would become unplayable if
    /// this device disconnected. `0` means nothing is at risk. The UI renders the
    /// warning sentence itself, from `core.sync.cloud_only_releases` and its own
    /// locale's plural rules.
    fn cloud_only_release_count() -> u64 {
        Ok(this.services.cloud_only_release_count().await?)
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

    /// Cancel every download — queued, failed, and the one in flight.
    fn cancel_all_downloads() -> () {
        Ok(this.services.cancel_all_downloads().await?)
    }

    /// Retry every failed download now (flips them back to queued and wakes the
    /// worker).
    fn retry_downloads() -> () {
        Ok(this.services.retry_downloads().await?)
    }
} }

forward! { sync this => {
    // =========================================================================
    // Playback
    // =========================================================================

    /// Play a release from `start_track_id`, or from its first track.
    fn play_release(release_id: String, start_track_id: Option<String>, shuffle: bool) {
        this.services
            .playback_play_release(release_id, start_track_id, shuffle);
    }

    fn play_releases(release_ids: Vec<String>) {
        this.services.playback_play_releases(release_ids);
    }

    fn play_library_shuffled() {
        this.services.playback_play_library_shuffled();
    }

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

    fn set_muted(muted: bool) {
        this.services.playback_set_muted(muted);
    }

    fn preview_play(target: BridgePreviewTarget) {
        this.services.playback_preview_play(target.into_core());
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

    fn remove_entry(entry_id: String) {
        if let Some(entry_id) = queue_entry_id(&entry_id) {
            this.services.playback_remove_entry(entry_id);
        }
    }

    /// Move the entry `entry_id` to sit immediately before `before_entry_id`.
    /// `before_entry_id == None` moves it to the end of the queue.
    fn reorder_entry(entry_id: String, before_entry_id: Option<String>) {
        let Some(entry_id) = queue_entry_id(&entry_id) else {
            return;
        };
        let before = match before_entry_id {
            None => None,
            Some(before) => match queue_entry_id(&before) {
                Some(before) => Some(before),
                None => return,
            },
        };
        this.services.playback_reorder_entry(entry_id, before);
    }

    fn clear_up_next() {
        this.services.playback_clear_up_next();
    }

    fn clear_playing_from() {
        this.services.playback_clear_playing_from();
    }

    fn skip_to_entry(entry_id: String) {
        if let Some(entry_id) = queue_entry_id(&entry_id) {
            this.services.playback_skip_to_entry(entry_id);
        }
    }

} }

/// The queue entry a UI names by the id its queue rows carry. Every id the
/// bridge hands out parses; one that doesn't names no entry, and is logged.
fn queue_entry_id(entry_id: &str) -> Option<QueueEntryId> {
    match entry_id.parse() {
        Ok(entry_id) => Some(entry_id),
        Err(e) => {
            tracing::warn!("queue entry id {entry_id:?} is not one the queue hands out: {e}");
            None
        }
    }
}
