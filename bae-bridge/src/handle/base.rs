use super::*;

#[uniffi::export]
impl AppHandle {
    // =========================================================================
    // Library
    // =========================================================================

    pub fn subscribe_album_page(
        &self,
        sort_criteria: Vec<BridgeSortCriterion>,
        offset: u64,
        limit: u64,
        callback: Box<dyn crate::types::AlbumPageCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let sort = sort_criteria
            .into_iter()
            .map(BridgeSortCriterion::into_core)
            .collect::<Vec<_>>();
        self.subscribe_live_query(
            move |services| services.subscribe_album_page(&sort, offset, limit),
            move |services, value| match value {
                Ok(raw) => {
                    let (rows, total_count) = services.resolve_album_page(raw);
                    callback.on_value(crate::types::BridgeAlbumPage {
                        rows: rows.into_iter().map(BridgeAlbum::from_core).collect(),
                        total_count,
                    });
                }
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    pub fn subscribe_composer_page(
        &self,
        sort_criteria: Vec<BridgeComposerSortCriterion>,
        offset: u64,
        limit: u64,
        callback: Box<dyn crate::types::ComposerPageCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let sort = sort_criteria
            .into_iter()
            .map(BridgeComposerSortCriterion::into_core)
            .collect::<Vec<_>>();
        self.subscribe_live_query(
            move |services| services.subscribe_composer_page(&sort, offset, limit),
            move |services, value| match value {
                Ok(raw) => {
                    let (rows, total_count) = services.resolve_composer_page(raw);
                    callback.on_value(crate::types::BridgeComposerPage {
                        rows: rows
                            .into_iter()
                            .map(BridgeComposerSummary::from_core)
                            .collect(),
                        total_count,
                    });
                }
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    pub fn subscribe_composer_detail(
        &self,
        artist_id: String,
        callback: Box<dyn crate::types::ComposerDetailCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_live_query(
            move |services| services.subscribe_composer_detail(&artist_id),
            move |services, value| match value {
                Ok(projection) => callback.on_value(
                    services
                        .resolve_composer_detail_projection(projection)
                        .map(BridgeComposerDetail::from_core),
                ),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    pub fn subscribe_work_detail(
        &self,
        work_id: String,
        callback: Box<dyn crate::types::WorkDetailCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_live_query(
            move |services| services.subscribe_work_detail(&work_id),
            move |services, value| match value {
                Ok(projection) => callback.on_value(
                    services
                        .resolve_work_detail_projection(projection)
                        .map(BridgeWorkDetail::from_core),
                ),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    pub fn subscribe_artist_page(
        &self,
        sort_criteria: Vec<BridgeArtistSortCriterion>,
        offset: u64,
        limit: u64,
        callback: Box<dyn crate::types::ArtistPageCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let sort = sort_criteria
            .into_iter()
            .map(BridgeArtistSortCriterion::into_core)
            .collect::<Vec<_>>();
        self.subscribe_live_query(
            move |services| services.subscribe_artist_page(&sort, offset, limit),
            move |services, value| match value {
                Ok(raw) => {
                    let (rows, total_count) = services.resolve_artist_page(raw);
                    callback.on_value(crate::types::BridgeArtistPage {
                        rows: rows
                            .into_iter()
                            .map(BridgeArtistSummary::from_core)
                            .collect(),
                        total_count,
                    });
                }
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    pub fn subscribe_artist_detail(
        &self,
        artist_id: String,
        callback: Box<dyn crate::types::ArtistDetailCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_live_query(
            move |services| services.subscribe_artist_detail(&artist_id),
            move |services, value| match value {
                Ok(projection) => callback.on_value(
                    services
                        .resolve_artist_detail_projection(projection)
                        .map(BridgeArtistDetail::from_core),
                ),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    pub fn subscribe_album_detail(
        &self,
        album_id: String,
        callback: Box<dyn crate::types::AlbumDetailCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_channel(
            move |services, runtime| services.subscribe_album_detail_values(runtime, album_id),
            move |value| match value {
                Ok(value) => callback.on_value(value.map(BridgeAlbumDetail::from_core)),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    pub fn subscribe_release_detail(
        &self,
        release_id: String,
        callback: Box<dyn crate::types::ReleaseDetailCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_channel(
            move |services, runtime| services.subscribe_release_detail_values(runtime, release_id),
            move |value| match value {
                Ok(value) => callback.on_value(value.map(BridgeRelease::from_core)),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    pub fn subscribe_storage_projection(
        &self,
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
        offset: u64,
        limit: u64,
        callback: Box<dyn crate::types::StorageProjectionCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_channel(
            move |services, runtime| {
                services.subscribe_storage_values(
                    runtime,
                    sort.into_core(),
                    filter.into_core(),
                    offset,
                    limit,
                )
            },
            move |value| match value {
                Ok(value) => callback.on_value(crate::types::BridgeStorageProjection {
                    page: BridgeStoragePage::from_core(value.page),
                    total_size: value.total_size,
                }),
                Err(error) => callback.on_error(BridgeError::database_query(error)),
            },
        )
    }

    pub fn subscribe_library_search(
        &self,
        query: String,
        callback: Box<dyn crate::types::LibrarySearchCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.live_subscription(move |services, _| async move {
            let parsed = bae_core::library::LibrarySearchQuery::parse(&query);
            let Some(parsed) = parsed else {
                callback.on_value(BridgeSearchResults::from_core(
                    bae_core::album_detail::SearchResults::default(),
                ));
                std::future::pending::<()>().await;
                return;
            };
            let mut values = services.subscribe_library_search(&parsed);
            loop {
                match values.next().await {
                    Ok(projection) => callback.on_value(BridgeSearchResults::from_core(
                        services.resolve_library_search_projection(projection),
                    )),
                    Err(error) => callback.on_error(BridgeError::database_query(error)),
                }
            }
        })
    }

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
            |services, runtime| services.subscribe_queue_values(runtime),
            move |value| match value {
                Ok(value) => callback.on_value(BridgeQueueSnapshot::from_core(value)),
                Err(error) => callback.on_error(BridgeError::internal(error)),
            },
        )
    }

    pub fn subscribe_queue_upcoming_page(
        &self,
        offset: u32,
        limit: u32,
        callback: Box<dyn crate::types::QueueUpcomingCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_channel(
            move |services, runtime| {
                services.subscribe_queue_upcoming_values(runtime, offset, limit)
            },
            move |value| match value {
                Ok(value) => callback.on_value(BridgeQueueUpcomingPage::from_core(value)),
                Err(error) => callback.on_error(BridgeError::internal(error)),
            },
        )
    }

    pub fn subscribe_playback_values(
        &self,
        callback: Box<dyn crate::types::PlaybackValuesCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_watch(
            |services| services.subscribe_playback_values(),
            move |value| callback.on_value(BridgePlaybackValues::from_core(value.clone())),
        )
    }

    pub fn subscribe_downloads(
        &self,
        callback: Box<dyn crate::types::DownloadCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        self.subscribe_watch(
            |services| services.subscribe_download_values(),
            move |value| {
                callback.on_value(crate::types::BridgeDownloadSnapshot::from_core(
                    value.clone(),
                ))
            },
        )
    }
}

forward! { async this => {
    /// 0-based position of `album_id` under the given sort, or `None` if the
    /// album isn't present.
    /// Lets the grid load the page containing an album and scroll to it
    /// without depending on that page already being fetched.
    fn get_album_index(
        sort_criteria: Vec<BridgeSortCriterion>,
        album_id: String,
    ) -> Option<u64> {
        let sort: Vec<bae_core::db::AlbumSortCriterion> = sort_criteria
            .into_iter()
            .map(BridgeSortCriterion::into_core)
            .collect();
        this.services
            .get_album_index(&sort, &album_id)
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

    /// Existing library artists matching a name or exact stored ID. A blank
    /// query is not a search and returns no suggestions.
    fn search_artists(query: String) -> Vec<BridgeArtistSearchResult> {
        let Some(query) = bae_core::library::LibrarySearchQuery::parse(&query) else {
            return Ok(Vec::new());
        };
        this.services
            .search_artists(&query)
            .await
            .map(|results| {
                results
                    .into_iter()
                    .map(BridgeArtistSearchResult::from_core)
                    .collect()
            })
            .map_err(BridgeError::database)
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
            BridgeCoverSelection::RemoteCover { selection } => CoverSelection::RemoteCover {
                url: selection.url,
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

    fn make_releases_remote(
        release_ids: Vec<String>,
        pin: bool,
    ) -> BridgeMakeReleasesRemoteOutcome {
        this.services
            .make_releases_remote(&release_ids, pin)
            .await
            .map(BridgeMakeReleasesRemoteOutcome::from_core)
            .map_err(BridgeError::from)
    }

    fn make_release_local(release_id: String, new_path: String) -> () {
        Ok(this.services.make_release_local(&release_id, &new_path).await?)
    }

    fn delete_release(release_id: String) -> () {
        Ok(this.services.delete_release(&release_id).await?)
    }

    fn save_sync_config(config_data: BridgeSaveSyncConfig) -> () {
        use bae_core::sync::S3ConfigData;
        Ok(this
            .services
            .save_s3_config(S3ConfigData {
                bucket: config_data.bucket,
                region: config_data.region,
                endpoint: config_data.endpoint,
                key_prefix: config_data.key_prefix,
                access_key: config_data.access_key,
                secret_key: config_data.secret_key,
                storage: crate::types::BridgeHomeStorage::into_core(config_data.storage),
            })
            .await?)
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

    fn generate_restore_code() -> String {
        Ok(this.services.generate_restore_code().await?)
    }

    /// The library's membership (devices, with this device flagged, and whether
    /// the running device is an owner). Reads the membership chain from cloud
    /// storage.
    fn get_members() -> crate::types::BridgeMembership {
        let membership = this.services.get_members().await?;
        Ok(crate::types::BridgeMembership::from_core(membership))
    }

    /// Remove a device from the library and rotate the library key.
    fn remove_member(public_key_hex: String) -> () {
        Ok(this.services.remove_member(&public_key_hex).await?)
    }

    /// Forget the active local library on this device: delete its key, clear the
    /// active pointer, and remove its data directory (the owner's cloud copy is
    /// untouched). The caller must drop this handle right after — the database
    /// lives in the removed directory — and re-open / onboard from scratch.
    fn forget_library() -> () {
        this.services.forget_library().await?;
        info!("Forgot local library");
        Ok(())
    }

    /// Enqueue releases to pin for offline. They join the in-memory serial
    /// download queue; the worker drains them one at a time. The DB lookups
    /// (resolving each release's title/size for its pane row) happen here; the
    /// deep cloud download runs on the queue worker.
    fn queue_pin_releases(release_ids: Vec<String>) -> () {
        this.services.enqueue_pins(release_ids).await;
        Ok(())
    }
} }

forward! { sync this => {
    // =========================================================================
    // Playback
    // =========================================================================

    fn play_release(release_id: String, start_track_index: Option<u32>, shuffle: bool) {
        this.services.playback_play_release(
            release_id,
            start_track_index.map(|i| i as usize),
            shuffle,
        );
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
        this.services.playback_remove_entry(QueueEntryId(entry_id));
    }

    /// Move the entry `entry_id` to sit immediately before `before_entry_id`.
    /// `before_entry_id == None` moves it to the end of the queue.
    fn reorder_entry(entry_id: String, before_entry_id: Option<String>) {
        this.services
            .playback_reorder_entry(QueueEntryId(entry_id), before_entry_id.map(QueueEntryId));
    }

    fn clear_up_next() {
        this.services.playback_clear_up_next();
    }

    fn clear_playing_from() {
        this.services.playback_clear_playing_from();
    }

    fn skip_to_entry(entry_id: String) {
        this.services.playback_skip_to_entry(QueueEntryId(entry_id));
    }

    // ── Download (pin) queue ─────────────────────────────────────────

    /// The current download-queue snapshot.
    fn get_download_snapshot() -> crate::types::BridgeDownloadSnapshot {
        crate::types::BridgeDownloadSnapshot::from_core(this.services.download_snapshot())
    }

    /// Pause or resume the download queue. In-flight downloads finish; the queue
    /// stops starting new ones until resumed.
    fn set_downloads_paused(paused: bool) {
        this.services.set_downloads_paused(paused);
    }

    /// Cancel a release's download — drops a queued/failed entry or aborts the
    /// in-flight one (a partial download never lands, so the release stays
    /// cloud-only).
    fn cancel_download(release_id: String) {
        this.services.cancel_download(&release_id);
    }

    /// Retry every failed download now (flips them back to queued and wakes the
    /// worker).
    fn retry_downloads() {
        this.services.retry_downloads();
    }
} }
