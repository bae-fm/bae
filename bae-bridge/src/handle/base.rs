use super::*;

impl AppHandle {
    pub(crate) fn start(
        services: AppServices,
        ui_event_bus: bae_core::ui::UiEventBus,
        runtime: tokio::runtime::Runtime,
    ) -> Self {
        #[cfg(feature = "desktop")]
        let desktop =
            bae_desktop::DesktopServices::start(services.clone(), runtime.handle().clone());
        #[cfg(feature = "cast")]
        let cast = bae_cast::CastController::start(
            services.clone(),
            ui_event_bus.clone(),
            runtime.handle().clone(),
            bae_core::renderer::RendererDiscovery::for_host(),
        );

        Self {
            services,
            ui_event_bus,
            #[cfg(feature = "desktop")]
            desktop,
            #[cfg(feature = "cast")]
            cast,
            runtime,
        }
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    // =========================================================================
    // Library
    // =========================================================================

    pub async fn get_album_page(
        &self,
        sort_criteria: Vec<BridgeSortCriterion>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeAlbum>, BridgeError> {
        let sort: Vec<bae_core::db::AlbumSortCriterion> = sort_criteria
            .into_iter()
            .map(BridgeSortCriterion::into_core)
            .collect();
        let albums = self
            .services
            .get_album_page(&sort, offset, limit)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;

        Ok(albums.into_iter().map(BridgeAlbum::from_core).collect())
    }

    /// 0-based position of `album_id` under the given sort, matching
    /// `get_album_page`'s ordering, or `None` if the album isn't present.
    /// Lets the grid load the page containing an album and scroll to it
    /// without depending on that page already being fetched.
    pub async fn get_album_index(
        &self,
        sort_criteria: Vec<BridgeSortCriterion>,
        album_id: String,
    ) -> Result<Option<u64>, BridgeError> {
        let sort: Vec<bae_core::db::AlbumSortCriterion> = sort_criteria
            .into_iter()
            .map(BridgeSortCriterion::into_core)
            .collect();
        self.services
            .get_album_index(&sort, &album_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    pub async fn get_album_count(&self) -> Result<u64, BridgeError> {
        self.services
            .get_album_count()
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    pub async fn get_composer_count(&self) -> Result<u64, BridgeError> {
        self.services
            .get_composer_count()
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    pub async fn get_composer_page(
        &self,
        sort_criteria: Vec<BridgeComposerSortCriterion>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeComposerSummary>, BridgeError> {
        let sort: Vec<bae_core::db::ComposerSortCriterion> = sort_criteria
            .into_iter()
            .map(BridgeComposerSortCriterion::into_core)
            .collect();
        let composers = self
            .services
            .get_composer_page(&sort, offset, limit)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(composers
            .into_iter()
            .map(BridgeComposerSummary::from_core)
            .collect())
    }

    pub async fn get_composer_detail(
        &self,
        artist_id: String,
    ) -> Result<Option<BridgeComposerDetail>, BridgeError> {
        let detail = self
            .services
            .get_composer_detail(&artist_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(detail.map(BridgeComposerDetail::from_core))
    }

    pub async fn get_work_detail(
        &self,
        work_id: String,
    ) -> Result<Option<BridgeWorkDetail>, BridgeError> {
        let detail = self
            .services
            .get_work_detail(&work_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(detail.map(BridgeWorkDetail::from_core))
    }

    pub async fn get_artist_count(&self) -> Result<u64, BridgeError> {
        self.services
            .get_artist_count()
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    pub async fn get_artist_page(
        &self,
        sort_criteria: Vec<BridgeArtistSortCriterion>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeArtistSummary>, BridgeError> {
        let sort: Vec<bae_core::db::ArtistSortCriterion> = sort_criteria
            .into_iter()
            .map(BridgeArtistSortCriterion::into_core)
            .collect();
        let artists = self
            .services
            .get_artist_page(&sort, offset, limit)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(artists
            .into_iter()
            .map(BridgeArtistSummary::from_core)
            .collect())
    }

    pub async fn get_artist_detail(
        &self,
        artist_id: String,
    ) -> Result<Option<BridgeArtistDetail>, BridgeError> {
        let detail = self
            .services
            .get_artist_detail(&artist_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(detail.map(BridgeArtistDetail::from_core))
    }

    /// Filesystem path for the user's own external file behind a library file
    /// (the DiscID re-read of a rip's LOG/CUE/audio). Returns `Ok(None)` if the
    /// file has no readable local location (e.g. cloud-only and not cached).
    /// Returns `Err` on DB failures so callers can distinguish a missing file
    /// from a broken library state. NOT a substitute for a coven byte read.
    pub async fn file_path(&self, file_id: String) -> Result<Option<String>, BridgeError> {
        let path = self
            .services
            .file_local_path(&file_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(path.and_then(|p| p.to_str().map(|s| s.to_string())))
    }

    pub async fn get_album_detail(
        &self,
        album_id: String,
    ) -> Result<BridgeAlbumDetail, BridgeError> {
        let detail = self
            .services
            .find_album_detail(&album_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?
            .ok_or_else(|| BridgeError::NotFound {
                entity: crate::types::BridgeEntityKind::Album,
                id: album_id.to_string(),
            })?;

        Ok(BridgeAlbumDetail::from_core(detail))
    }

    /// Fat release detail (tracks, files, gallery) for the album-detail
    /// view. Returns `Ok(None)` when the release doesn't exist.
    pub async fn find_release_detail(
        &self,
        release_id: String,
    ) -> Result<Option<BridgeRelease>, BridgeError> {
        let detail = self
            .services
            .find_release_detail(&release_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(detail.map(BridgeRelease::from_core))
    }

    /// One page of the Storage Manager list, pre-sorted and
    /// pre-filtered. Rows carry both halves (release + parent album) so
    /// the UI can populate its normalized slices from a single call.
    pub async fn storage_page(
        &self,
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<BridgeStoragePage, BridgeError> {
        let core_sort = sort.into_core();
        let core_filter = filter.into_core();
        let page = self
            .services
            .get_storage_page(&core_sort, core_filter, offset, limit)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;

        Ok(BridgeStoragePage::from_core(page))
    }

    /// Count of storage rows matching `filter`. Matches
    /// `storage_page`'s `total_count` for the same filter.
    pub async fn storage_count(&self, filter: BridgeStorageFilter) -> Result<u64, BridgeError> {
        let core_filter = filter.into_core();
        self.services
            .get_storage_count(core_filter)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    /// Sum of `total_size` over every storage row matching `filter` — the
    /// storage-manager footer's "Total:" figure, independent of how many
    /// pages of the filtered list are loaded.
    pub async fn storage_total_size(
        &self,
        filter: BridgeStorageFilter,
    ) -> Result<u64, BridgeError> {
        let core_filter = filter.into_core();
        self.services
            .get_storage_total_size(core_filter)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    pub async fn search_library(&self, query: String) -> Result<BridgeSearchResults, BridgeError> {
        // Trim and blank-check are core policy, not the surface's: a blank query is
        // not a search and yields empty results, never a `LIKE '%%'` match-all.
        let results = match bae_core::library::LibrarySearchQuery::parse(&query) {
            Some(query) => self
                .services
                .search_library(&query)
                .await
                .map_err(|e| BridgeError::database(format!("{e}")))?,
            None => bae_core::album_detail::SearchResults::default(),
        };
        Ok(BridgeSearchResults {
            albums: results
                .albums
                .into_iter()
                .map(BridgeAlbumSearchResult::from_core)
                .collect(),
            artists: results
                .artists
                .into_iter()
                .map(BridgeArtistSummary::from_core)
                .collect(),
            tracks: results
                .tracks
                .into_iter()
                .map(BridgeTrackSearchResult::from_core)
                .collect(),
            composers: results
                .composers
                .into_iter()
                .map(BridgeComposerSummary::from_core)
                .collect(),
            works: results
                .works
                .into_iter()
                .map(BridgeWorkSummary::from_core)
                .collect(),
        })
    }

    // =========================================================================
    // Playback
    // =========================================================================

    pub fn play_release(&self, release_id: String, start_track_index: Option<u32>, shuffle: bool) {
        self.services.playback_play_release(
            release_id,
            start_track_index.map(|i| i as usize),
            shuffle,
        );
    }

    pub fn play_releases(&self, release_ids: Vec<String>) {
        self.services.playback_play_releases(release_ids);
    }

    pub fn play_library_shuffled(&self) {
        self.services.playback_play_library_shuffled();
    }

    pub fn pause(&self) {
        self.services.playback_pause();
    }

    pub fn resume(&self) {
        self.services.playback_resume();
    }

    pub fn stop(&self) {
        self.services.playback_stop();
    }

    pub fn next_track(&self) {
        self.services.playback_next();
    }

    pub fn previous_track(&self) {
        self.services.playback_previous();
    }

    pub fn seek_by_ratio(&self, ratio: f64) {
        self.services.playback_seek_by_ratio(ratio);
    }

    pub fn set_volume(&self, volume: f32) {
        self.services.playback_set_volume(volume);
    }

    pub async fn get_volume(&self) -> f32 {
        self.services.playback_get_volume().await
    }

    pub fn set_muted(&self, muted: bool) {
        self.services.playback_set_muted(muted);
    }

    pub fn preview_play(&self, path: String) {
        self.services.playback_preview_play(path);
    }

    pub fn preview_stop(&self) {
        self.services.playback_preview_stop();
    }

    pub fn preview_toggle_pause(&self) {
        self.services.playback_preview_toggle_pause();
    }

    pub fn preview_seek_by_ratio(&self, ratio: f64) {
        self.services.playback_preview_seek_by_ratio(ratio);
    }

    pub fn set_repeat_mode(&self, mode: BridgeRepeatMode) {
        let core_mode = mode.into_core();
        self.services.playback_set_repeat_mode(core_mode);
    }

    pub fn set_shuffle(&self, on: bool) {
        self.services.playback_set_shuffle(on);
    }

    // =========================================================================
    // Queue
    // =========================================================================

    pub fn add_to_queue(&self, track_ids: Vec<String>) {
        self.services.playback_add_to_queue(track_ids);
    }

    pub fn add_next(&self, track_ids: Vec<String>) {
        self.services.playback_add_next(track_ids);
    }

    pub fn add_release_to_queue(&self, release_id: String) {
        self.services.playback_add_release_to_queue(release_id);
    }

    pub fn add_release_next(&self, release_id: String) {
        self.services.playback_add_release_next(release_id);
    }

    pub async fn get_queue_snapshot(&self) -> Result<BridgeQueueSnapshot, BridgeError> {
        let snapshot = self
            .services
            .get_queue_snapshot()
            .await
            .map_err(BridgeError::internal)?;
        Ok(BridgeQueueSnapshot::from_core(snapshot))
    }

    /// A page of the context's upcoming tail past the window
    /// `get_queue_snapshot`/`QueueUpdated` already carry. `offset` 0 is the
    /// first not-yet-played entry after the current track — the same
    /// coordinate space as `BridgePlaybackContext.upcoming`. The reply's
    /// `revision` lets the caller drop a page answered for a since-superseded
    /// queue state.
    pub async fn get_queue_upcoming_page(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<BridgeQueueUpcomingPage, BridgeError> {
        let page = self
            .services
            .get_queue_upcoming_page(offset, limit)
            .await
            .map_err(BridgeError::internal)?;
        Ok(BridgeQueueUpcomingPage::from_core(page))
    }

    /// Resolve a list of IDs (album or track) to track IDs.
    /// Album IDs are expanded to the primary release's tracks.
    pub async fn resolve_to_track_ids(&self, ids: Vec<String>) -> Result<Vec<String>, BridgeError> {
        self.services
            .resolve_to_track_ids(&ids)
            .await
            .map_err(BridgeError::database)
    }

    pub fn insert_in_queue(&self, track_ids: Vec<String>, index: u32) {
        self.services
            .playback_insert_in_queue(track_ids, index as usize);
    }

    pub fn remove_entry(&self, entry_id: String) {
        self.services.playback_remove_entry(QueueEntryId(entry_id));
    }

    /// Move the entry `entry_id` to sit immediately before `before_entry_id`.
    /// `before_entry_id == None` moves it to the end of the queue.
    pub fn reorder_entry(&self, entry_id: String, before_entry_id: Option<String>) {
        self.services
            .playback_reorder_entry(QueueEntryId(entry_id), before_entry_id.map(QueueEntryId));
    }

    pub fn clear_up_next(&self) {
        self.services.playback_clear_up_next();
    }

    pub fn clear_playing_from(&self) {
        self.services.playback_clear_playing_from();
    }

    pub fn skip_to_entry(&self, entry_id: String) {
        self.services.playback_skip_to_entry(QueueEntryId(entry_id));
    }

    // =========================================================================
    // Settings
    // =========================================================================

    pub fn get_config(&self) -> BridgeConfig {
        BridgeConfig::from_core(&self.services.get_config())
    }

    pub fn set_pause_between_sides(&self, enabled: bool) -> Result<(), BridgeError> {
        self.services
            .set_pause_between_sides(enabled)
            .map_err(BridgeError::config)
    }

    /// How many blob uploads run at once. Rejected outside 1..=8. Takes effect the
    /// next time the library's coven handle opens.
    pub fn set_max_concurrent_uploads(&self, n: u32) -> Result<(), BridgeError> {
        self.services
            .set_max_concurrent_uploads(n)
            .map_err(BridgeError::config)
    }

    /// How many blob downloads a pin fetches at once. Rejected outside 1..=8.
    /// Takes effect the next time the library's coven handle opens.
    pub fn set_max_concurrent_downloads(&self, n: u32) -> Result<(), BridgeError> {
        self.services
            .set_max_concurrent_downloads(n)
            .map_err(BridgeError::config)
    }

    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. The write fires a config
    /// invalidation, which is what re-renders the bar — no app keeps its own copy.
    pub fn set_show_remaining_time(&self, enabled: bool) -> Result<(), BridgeError> {
        self.services
            .set_show_remaining_time(enabled)
            .map_err(BridgeError::config)
    }

    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. The write fires a
    /// config invalidation, which re-renders the page — no app keeps its own
    /// copy.
    pub fn set_library_full_width(&self, enabled: bool) -> Result<(), BridgeError> {
        self.services
            .set_library_full_width(enabled)
            .map_err(BridgeError::config)
    }

    pub fn set_save_presets(
        &self,
        presets: Vec<crate::types::BridgeSavePreset>,
    ) -> Result<(), BridgeError> {
        self.services
            .set_save_presets(
                presets
                    .into_iter()
                    .map(crate::types::BridgeSavePreset::into_core)
                    .collect(),
            )
            .map_err(BridgeError::config)
    }

    pub fn set_default_track_save_preset(&self, preset_id: String) -> Result<(), BridgeError> {
        self.services
            .set_default_track_save_preset(preset_id)
            .map_err(BridgeError::config)
    }

    pub fn set_default_release_save_preset(&self, preset_id: String) -> Result<(), BridgeError> {
        self.services
            .set_default_release_save_preset(preset_id)
            .map_err(BridgeError::config)
    }

    /// Whether the encryption key is loaded — `init` successfully read it from
    /// the keyring and built the sync manager. Reflects the cached init-time
    /// result, not a fresh keyring read. `false` here for an
    /// `encryption_key_stored` library means the keyring got wiped and the
    /// user needs to enter the key again.
    pub fn has_encryption_key(&self) -> bool {
        self.services.has_encryption()
    }

    pub fn rename_library(&self, library_id: String, name: String) -> Result<(), BridgeError> {
        // Trim + non-blank is core policy, applied here rather than in each app.
        let name = bae_core::library_name::LibraryName::parse(&name)
            .map_err(|e| BridgeError::config(e.to_string()))?;
        self.services.rename_library(&library_id, &name)?;
        Ok(())
    }

    pub fn lock_active_library(&self) -> Result<(), BridgeError> {
        self.services.forget_encryption_key()?;
        Ok(())
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, BridgeError> {
        Ok(self.services.get_discogs_token()?)
    }

    // Discogs token writes live on the desktop-only import service (see the
    // `feature = "desktop"` impl block); mobile reads status but never writes.

    // =========================================================================
    // Cover art
    // =========================================================================

    pub async fn change_cover(
        &self,
        album_id: String,
        release_id: String,
        selection: BridgeCoverSelection,
    ) -> Result<(), BridgeError> {
        use bae_core::library::CoverSelection;

        let core_selection = match selection {
            BridgeCoverSelection::ReleaseImage { file_id } => {
                CoverSelection::ReleaseImage { file_id }
            }
            BridgeCoverSelection::RemoteCover { selection } => CoverSelection::RemoteCover {
                url: selection.url,
                source: selection.source.into_core(),
            },
        };

        self.services
            .change_cover(&album_id, &release_id, core_selection)
            .await
            .map_err(|e| BridgeError::internal(format!("{e}")))
    }

    pub async fn set_primary_release(
        &self,
        album_id: String,
        release_id: String,
    ) -> Result<(), BridgeError> {
        self.services
            .set_album_primary_release(&album_id, &release_id)
            .await
            .map_err(|e| BridgeError::internal(format!("{e}")))
    }

    // =========================================================================
    // Storage
    // =========================================================================

    pub async fn unpin_release(&self, release_id: String) -> Result<(), BridgeError> {
        self.services.unpin_release(&release_id).await?;
        Ok(())
    }

    pub async fn make_release_remote(
        &self,
        release_id: String,
        pin: bool,
    ) -> Result<(), BridgeError> {
        self.services.make_release_remote(&release_id, pin).await?;
        Ok(())
    }

    pub async fn make_release_local(
        &self,
        release_id: String,
        new_path: String,
    ) -> Result<(), BridgeError> {
        self.services
            .make_release_local(&release_id, &new_path)
            .await?;
        Ok(())
    }

    pub async fn delete_release(&self, release_id: String) -> Result<(), BridgeError> {
        self.services.delete_release(&release_id).await?;
        Ok(())
    }

    // =========================================================================
    // Sync / membership
    // =========================================================================

    pub async fn save_sync_config(
        &self,
        config_data: BridgeSaveSyncConfig,
    ) -> Result<(), BridgeError> {
        use bae_core::sync::S3ConfigData;
        self.services
            .save_s3_config(S3ConfigData {
                bucket: config_data.bucket,
                region: config_data.region,
                endpoint: config_data.endpoint,
                key_prefix: config_data.key_prefix,
                access_key: config_data.access_key,
                secret_key: config_data.secret_key,
                storage: crate::types::BridgeHomeStorage::into_core(config_data.storage),
            })
            .await?;
        Ok(())
    }

    pub fn disconnect_cloud_provider(&self) -> Result<(), BridgeError> {
        self.services.disconnect_cloud_provider()?;
        Ok(())
    }

    /// How many releases live only in the cloud and would become unplayable if
    /// this device disconnected. `0` means nothing is at risk. The UI renders the
    /// warning sentence itself, from `core.sync.cloud_only_releases` and its own
    /// locale's plural rules.
    pub async fn cloud_only_release_count(&self) -> Result<u64, BridgeError> {
        Ok(self.services.cloud_only_release_count().await?)
    }

    pub async fn generate_restore_code(&self) -> Result<String, BridgeError> {
        Ok(self.services.generate_restore_code().await?)
    }

    /// The library's membership (devices, with this device flagged, and whether
    /// the running device is an owner). Reads the membership chain from cloud
    /// storage.
    pub async fn get_members(&self) -> Result<crate::types::BridgeMembership, BridgeError> {
        let membership = self.services.get_members().await?;
        Ok(crate::types::BridgeMembership::from_core(membership))
    }

    /// Approve a joining device by its public key (from its join-request code),
    /// returning the invite code to hand back to that device.
    pub async fn invite_member(
        &self,
        public_key_hex: String,
        provider_account_email: Option<String>,
    ) -> Result<String, BridgeError> {
        Ok(self
            .services
            .invite_member(&public_key_hex, provider_account_email.as_deref())
            .await?)
    }

    /// Mint the scannable invite for a device that asked to join, returning the
    /// payload bytes to render as a QR code.
    ///
    /// `join_request_code` is the joining device's own `generate_join_request`
    /// code, handed over first — the offer is signed for that device's key, so
    /// this payload cannot be minted without it.
    ///
    /// Minting only publishes the offer: `drive_device_join` must then run on
    /// this device, while the code is on screen, to admit the joiner.
    pub async fn begin_device_invite(
        &self,
        join_request_code: String,
    ) -> Result<Vec<u8>, BridgeError> {
        Ok(self
            .services
            .begin_device_invite(&join_request_code)
            .await?)
    }

    /// Run this device's side of a join it invited, until the joining device is
    /// in or the attempt ends. Call this while the invite is displayed; it
    /// returns `true` when the device joined, `false` when the attempt ended
    /// without it.
    pub async fn drive_device_join(&self, invite: Vec<u8>) -> Result<bool, BridgeError> {
        use bae_core::library::DeviceJoinOutcome;
        Ok(matches!(
            self.services.drive_device_join(invite).await?,
            DeviceJoinOutcome::Joined
        ))
    }

    /// Withdraw an invite this device minted, so a joining device that already
    /// scanned it is told to stop.
    pub async fn cancel_device_invite(&self, invite: Vec<u8>) -> Result<(), BridgeError> {
        Ok(self.services.cancel_device_invite(invite).await?)
    }

    /// Remove a device from the library and rotate the library key.
    pub async fn remove_member(&self, public_key_hex: String) -> Result<(), BridgeError> {
        self.services.remove_member(&public_key_hex).await?;
        Ok(())
    }

    /// Forget the active local library on this device: delete its key, clear the
    /// active pointer, and remove its data directory (the owner's cloud copy is
    /// untouched). The caller must drop this handle right after — the database
    /// lives in the removed directory — and re-open / onboard from scratch.
    pub fn forget_library(&self) -> Result<(), BridgeError> {
        self.services.forget_library()?;

        info!("Forgot local library");
        Ok(())
    }

    pub fn trigger_sync(&self) {
        self.services.trigger_sync();
    }

    pub fn is_sync_ready(&self) -> bool {
        self.services.is_sync_ready()
    }

    pub fn get_sync_status(&self) -> BridgeSyncStatusSnapshot {
        crate::types::BridgeSyncStatusSnapshot::from_core(self.services.get_sync_status())
    }

    /// The current cloud outbox processing snapshot. Outbox invalidations tell
    /// the Storage Manager to read it again.
    pub async fn get_outbox_snapshot(
        &self,
    ) -> Result<crate::types::BridgeOutboxSnapshot, BridgeError> {
        let snapshot = self
            .services
            .outbox_snapshot()
            .await
            .map_err(BridgeError::internal)?;
        Ok(crate::types::BridgeOutboxSnapshot::from_core(snapshot))
    }

    /// Retry failed uploads now: drain coven's upload queue immediately instead
    /// of waiting for the next sync cycle.
    pub async fn retry_outbox(&self) -> Result<(), BridgeError> {
        self.services
            .retry_outbox_now()
            .await
            .map_err(BridgeError::internal)
    }

    /// Cancel whatever transition a release is mid-flight — a pin (download), a
    /// remote upload, or an unmanage — leaving it in its prior state. The UI
    /// calls this from the storage row and the queue pane without knowing which
    /// is running; a no-op if nothing is in progress.
    pub async fn cancel_release_transition(&self, release_id: String) -> Result<(), BridgeError> {
        self.services
            .cancel_release_transition(&release_id)
            .await
            .map_err(BridgeError::internal)
    }

    /// Pause or resume the cloud-upload pipeline. While paused, new enqueues
    /// still land in the outbox but the sync cycle won't drain them; the
    /// snapshot's `paused` field flips so the UI can render the toggle.
    pub async fn set_sync_paused(&self, paused: bool) {
        self.services.set_sync_paused(paused).await;
    }

    // ── Download (pin) queue ─────────────────────────────────────────

    /// The current download-queue snapshot. Download invalidations tell the
    /// Downloads pane to read it again.
    pub fn get_download_snapshot(&self) -> crate::types::BridgeDownloadSnapshot {
        crate::types::BridgeDownloadSnapshot::from_core(self.services.download_snapshot())
    }

    /// Enqueue releases to pin for offline. They join the in-memory serial
    /// download queue; the worker drains them one at a time. The DB lookups
    /// (resolving each release's title/size for its pane row) happen here; the
    /// deep cloud download runs on the queue worker.
    pub async fn queue_pin_releases(&self, release_ids: Vec<String>) {
        self.services.enqueue_pins(release_ids).await;
    }

    /// Pause or resume the download queue. In-flight downloads finish; the queue
    /// stops starting new ones until resumed.
    pub fn set_downloads_paused(&self, paused: bool) {
        self.services.set_downloads_paused(paused);
    }

    /// Cancel a release's download — drops a queued/failed entry or aborts the
    /// in-flight one (a partial download never lands, so the release stays
    /// cloud-only).
    pub fn cancel_download(&self, release_id: String) {
        self.services.cancel_download(&release_id);
    }

    /// Retry every failed download now (flips them back to queued and wakes the
    /// worker).
    pub fn retry_downloads(&self) {
        self.services.retry_downloads();
    }
}

/// Playback-state persistence, deliberately **not** `cancellable`.
///
/// Both of these write playback state to disk and must run to completion: dropping the
/// future partway leaves the queue, current track, and position unwritten, so the next cold
/// launch silently restores stale state. `BaeApp`'s `ShutdownRace` relies on this — it lets
/// the losing task keep running rather than cancelling it.
#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    /// Graceful shutdown: saves playback state to disk, then stops the playback service.
    pub async fn shutdown(&self) {
        #[cfg(feature = "desktop")]
        {
            self.desktop.shutdown_mcp();
            self.desktop.shutdown_subsonic();
            self.services.playback_shutdown().await;
        }
        #[cfg(not(feature = "desktop"))]
        self.services.playback_shutdown().await;
    }

    /// Persist the current playback state without stopping playback. Mobile
    /// calls this when the app is backgrounded so the queue, current track, and
    /// position survive a later cold launch — it can't call `shutdown`, which
    /// would stop the background audio.
    pub async fn save_playback_state(&self) {
        self.services.playback_save_state().await;
    }
}

#[cfg(feature = "desktop")]
impl BridgeMcpServerStatus {
    pub(super) fn from_core(status: bae_desktop::McpServerStatus) -> Self {
        match status {
            bae_desktop::McpServerStatus::Disabled => BridgeMcpServerStatus::Disabled,
            bae_desktop::McpServerStatus::Running { url } => BridgeMcpServerStatus::Running { url },
            bae_desktop::McpServerStatus::Error { error } => BridgeMcpServerStatus::Error {
                error: crate::types::BridgeMcpServerError::from_core(error),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeMcpServerError {
    pub(super) fn from_core(error: bae_desktop::McpServerError) -> Self {
        use crate::types::BridgeMcpServerError;
        match error {
            bae_desktop::McpServerError::InvalidConfig { detail } => {
                BridgeMcpServerError::InvalidConfig { detail }
            }
            bae_desktop::McpServerError::TokenUnavailable { detail } => {
                BridgeMcpServerError::TokenUnavailable { detail }
            }
            bae_desktop::McpServerError::BindFailed { detail } => {
                BridgeMcpServerError::BindFailed { detail }
            }
            bae_desktop::McpServerError::ServerFailed { detail } => {
                BridgeMcpServerError::ServerFailed { detail }
            }
        }
    }
}

#[cfg(feature = "cast")]
impl crate::types::BridgeCastStatus {
    pub(super) fn from_core(status: bae_cast::CastStatus) -> Self {
        use crate::types::BridgeCastStatus;
        match status {
            bae_cast::CastStatus::NotCasting => BridgeCastStatus::NotCasting,
            bae_cast::CastStatus::Casting { device_name } => {
                BridgeCastStatus::Casting { device_name }
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeSubsonicServerStatus {
    pub(super) fn from_core(status: bae_desktop::SubsonicServerStatus) -> Self {
        use crate::types::BridgeSubsonicServerStatus;
        match status {
            bae_desktop::SubsonicServerStatus::Disabled => BridgeSubsonicServerStatus::Disabled,
            bae_desktop::SubsonicServerStatus::Running { url } => {
                BridgeSubsonicServerStatus::Running { url }
            }
            bae_desktop::SubsonicServerStatus::Error { error } => {
                BridgeSubsonicServerStatus::Error {
                    error: crate::types::BridgeSubsonicServerError::from_core(error),
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeSubsonicServerError {
    pub(super) fn from_core(error: bae_desktop::SubsonicServerError) -> Self {
        use crate::types::BridgeSubsonicServerError;
        match error {
            bae_desktop::SubsonicServerError::InvalidConfig { detail } => {
                BridgeSubsonicServerError::InvalidConfig { detail }
            }
            bae_desktop::SubsonicServerError::CredentialUnavailable { detail } => {
                BridgeSubsonicServerError::CredentialUnavailable { detail }
            }
            bae_desktop::SubsonicServerError::BindFailed { detail } => {
                BridgeSubsonicServerError::BindFailed { detail }
            }
            bae_desktop::SubsonicServerError::ServerFailed { detail } => {
                BridgeSubsonicServerError::ServerFailed { detail }
            }
        }
    }
}

// =========================================================================
// Release gallery (all platforms)
// =========================================================================
