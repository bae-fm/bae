#[cfg(feature = "desktop")]
use std::ops::Deref;

#[cfg(not(feature = "desktop"))]
use bae_core::library::AppServices;
use bae_core::playback::QueueEntryId;
#[cfg(feature = "desktop")]
use bae_core::signals::ExtractionSource;
#[cfg(feature = "desktop")]
use bae_core::util::rate_limiter::CallPriority;
use tracing::info;

#[cfg(feature = "oauth-providers")]
use crate::types::BridgeCloudProvider;
#[cfg(feature = "desktop")]
use crate::types::BridgeDiscogsSaveOutcome;
#[cfg(any(feature = "cloudkit", feature = "oauth-providers"))]
use crate::types::BridgeHomeStorage;
#[cfg(feature = "desktop")]
use crate::types::BridgeRemoteCover;
use crate::types::{
    BridgeAlbum, BridgeAlbumDetail, BridgeAlbumSearchResult, BridgeArtistDetail,
    BridgeArtistSortCriterion, BridgeArtistSummary, BridgeComposerDetail,
    BridgeComposerSortCriterion, BridgeComposerSummary, BridgeComposerWorkGroup, BridgeConfig,
    BridgeCoverSelection, BridgeError, BridgeFile, BridgeGalleryItem, BridgeGallerySource,
    BridgeMetadataSource, BridgeQueueSnapshot, BridgeQueueUpcomingPage, BridgeRelease,
    BridgeReleaseRoleSummary, BridgeReleaseSummary, BridgeRepeatMode, BridgeSaveSyncConfig,
    BridgeSearchResults, BridgeSortCriterion, BridgeStorageFilter, BridgeStoragePage,
    BridgeStorageRow, BridgeStorageSort, BridgeSyncStatusSnapshot, BridgeTrack, BridgeTrackGroup,
    BridgeTrackRoleSummary, BridgeTrackSearchResult, BridgeWorkDetail, BridgeWorkReleaseSummary,
    BridgeWorkSummary, BridgeWorkTrackSummary,
};
#[cfg(feature = "desktop")]
use crate::types::{
    BridgeImportCandidateSnapshot, BridgeImportCandidatesSnapshot, BridgeMcpServerStatus,
    BridgeStorageMode, BridgeSubsonicServerStatus,
};

#[derive(uniffi::Object)]
#[cfg(feature = "desktop")]
pub struct AppHandle {
    pub(crate) app: bae_desktop::DesktopApp,
}

#[derive(uniffi::Object)]
#[cfg(not(feature = "desktop"))]
pub struct AppHandle {
    pub(crate) services: AppServices,
    pub(crate) ui_event_bus: bae_core::ui::UiEventBus,
    /// Last, so it outlives the services that run on it — see
    /// `bae_core::app::RunningApp`.
    pub(crate) runtime: tokio::runtime::Runtime,
}

#[cfg(feature = "desktop")]
impl Deref for AppHandle {
    type Target = bae_desktop::DesktopApp;

    fn deref(&self) -> &Self::Target {
        &self.app
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
            .library_manager()
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
            .library_manager()
            .get_album_index(&sort, &album_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    pub async fn get_album_count(&self) -> Result<u64, BridgeError> {
        self.services
            .library_manager()
            .get_album_count()
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    pub async fn get_composer_count(&self) -> Result<u64, BridgeError> {
        self.services
            .library_manager()
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
            .library_manager()
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
            .library_manager()
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
            .library_manager()
            .get_work_detail(&work_id)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(detail.map(BridgeWorkDetail::from_core))
    }

    pub async fn get_artist_count(&self) -> Result<u64, BridgeError> {
        self.services
            .library_manager()
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
            .library_manager()
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
            .library_manager()
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
            .library_manager()
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
            .library_manager()
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
            .library_manager()
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
            .library_manager()
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
            .library_manager()
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
            .library_manager()
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
                .library_manager()
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
        self.services.playback().play_release(
            release_id,
            start_track_index.map(|i| i as usize),
            shuffle,
        );
    }

    pub fn play_releases(&self, release_ids: Vec<String>) {
        self.services.playback().play_releases(release_ids);
    }

    pub fn play_library_shuffled(&self) {
        self.services.playback().play_library_shuffled();
    }

    pub fn pause(&self) {
        self.services.playback().pause();
    }

    pub fn resume(&self) {
        self.services.playback().resume();
    }

    pub fn stop(&self) {
        self.services.playback().stop();
    }

    pub fn next_track(&self) {
        self.services.playback().next();
    }

    pub fn previous_track(&self) {
        self.services.playback().previous();
    }

    pub fn seek_by_ratio(&self, ratio: f64) {
        self.services.playback().seek_by_ratio(ratio);
    }

    pub fn set_volume(&self, volume: f32) {
        self.services.playback().set_volume(volume);
    }

    pub async fn get_volume(&self) -> f32 {
        self.services.playback().get_volume().await
    }

    pub fn set_muted(&self, muted: bool) {
        self.services.playback().set_muted(muted);
    }

    pub fn preview_play(&self, path: String) {
        self.services.playback().preview_play(path);
    }

    pub fn preview_stop(&self) {
        self.services.playback().preview_stop();
    }

    pub fn preview_toggle_pause(&self) {
        self.services.playback().preview_toggle_pause();
    }

    pub fn preview_seek_by_ratio(&self, ratio: f64) {
        self.services.playback().preview_seek_by_ratio(ratio);
    }

    pub fn set_repeat_mode(&self, mode: BridgeRepeatMode) {
        let core_mode = mode.into_core();
        self.services.playback().set_repeat_mode(core_mode);
    }

    pub fn set_shuffle(&self, on: bool) {
        self.services.playback().set_shuffle(on);
    }

    // =========================================================================
    // Queue
    // =========================================================================

    pub fn add_to_queue(&self, track_ids: Vec<String>) {
        self.services.playback().add_to_queue(track_ids);
    }

    pub fn add_next(&self, track_ids: Vec<String>) {
        self.services.playback().add_next(track_ids);
    }

    pub fn add_release_to_queue(&self, release_id: String) {
        self.services.playback().add_release_to_queue(release_id);
    }

    pub fn add_release_next(&self, release_id: String) {
        self.services.playback().add_release_next(release_id);
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
            .library_manager()
            .resolve_to_track_ids(&ids)
            .await
            .map_err(BridgeError::database)
    }

    pub fn insert_in_queue(&self, track_ids: Vec<String>, index: u32) {
        self.services
            .playback()
            .insert_in_queue(track_ids, index as usize);
    }

    pub fn remove_entry(&self, entry_id: String) {
        self.services
            .playback()
            .remove_entry(QueueEntryId(entry_id));
    }

    /// Move the entry `entry_id` to sit immediately before `before_entry_id`.
    /// `before_entry_id == None` moves it to the end of the queue.
    pub fn reorder_entry(&self, entry_id: String, before_entry_id: Option<String>) {
        self.services
            .playback()
            .reorder_entry(QueueEntryId(entry_id), before_entry_id.map(QueueEntryId));
    }

    pub fn clear_up_next(&self) {
        self.services.playback().clear_up_next();
    }

    pub fn clear_playing_from(&self) {
        self.services.playback().clear_playing_from();
    }

    pub fn skip_to_entry(&self, entry_id: String) {
        self.services
            .playback()
            .skip_to_entry(QueueEntryId(entry_id));
    }

    // =========================================================================
    // Settings
    // =========================================================================

    pub fn get_config(&self) -> BridgeConfig {
        BridgeConfig::from_core(&self.services.library_manager().get_config())
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
            .library_manager()
            .set_max_concurrent_uploads(n)
            .map_err(BridgeError::config)
    }

    /// How many blob downloads a pin fetches at once. Rejected outside 1..=8.
    /// Takes effect the next time the library's coven handle opens.
    pub fn set_max_concurrent_downloads(&self, n: u32) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .set_max_concurrent_downloads(n)
            .map_err(BridgeError::config)
    }

    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. The write fires a config
    /// invalidation, which is what re-renders the bar — no app keeps its own copy.
    pub fn set_show_remaining_time(&self, enabled: bool) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .set_show_remaining_time(enabled)
            .map_err(BridgeError::config)
    }

    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. The write fires a
    /// config invalidation, which re-renders the page — no app keeps its own
    /// copy.
    pub fn set_library_full_width(&self, enabled: bool) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .set_library_full_width(enabled)
            .map_err(BridgeError::config)
    }

    pub fn set_save_presets(
        &self,
        presets: Vec<crate::types::BridgeSavePreset>,
    ) -> Result<(), BridgeError> {
        self.services
            .library_manager()
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
            .library_manager()
            .set_default_track_save_preset(preset_id)
            .map_err(BridgeError::config)
    }

    pub fn set_default_release_save_preset(&self, preset_id: String) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .set_default_release_save_preset(preset_id)
            .map_err(BridgeError::config)
    }

    /// Whether the encryption key is loaded — `init` successfully read it from
    /// the keyring and built the sync manager. Reflects the cached init-time
    /// result, not a fresh keyring read. `false` here for an
    /// `encryption_key_stored` library means the keyring got wiped and the
    /// user needs to enter the key again.
    pub fn has_encryption_key(&self) -> bool {
        self.services.library_manager().has_encryption()
    }

    pub fn rename_library(&self, library_id: String, name: String) -> Result<(), BridgeError> {
        // Trim + non-blank is core policy, applied here rather than in each app.
        let name = bae_core::library_name::LibraryName::parse(&name)
            .map_err(|e| BridgeError::config(e.to_string()))?;
        self.services
            .library_manager()
            .rename_library(&library_id, &name)?;
        Ok(())
    }

    pub fn lock_active_library(&self) -> Result<(), BridgeError> {
        self.services.library_manager().forget_encryption_key()?;
        Ok(())
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, BridgeError> {
        Ok(self.services.library_manager().get_discogs_token()?)
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
            .library_manager()
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
            .library_manager()
            .set_album_primary_release(&album_id, &release_id)
            .await
            .map_err(|e| BridgeError::internal(format!("{e}")))
    }

    // =========================================================================
    // Storage
    // =========================================================================

    pub async fn unpin_release(&self, release_id: String) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .unpin_release(&release_id)
            .await?;
        Ok(())
    }

    pub async fn make_release_remote(
        &self,
        release_id: String,
        pin: bool,
    ) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .make_release_remote(&release_id, pin)
            .await?;
        Ok(())
    }

    pub async fn make_release_local(
        &self,
        release_id: String,
        new_path: String,
    ) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .make_release_local(&release_id, &new_path)
            .await?;
        Ok(())
    }

    pub async fn delete_release(&self, release_id: String) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .delete_release(&release_id)
            .await?;
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
            .library_manager()
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
        self.services
            .library_manager()
            .disconnect_cloud_provider()?;
        Ok(())
    }

    /// How many releases live only in the cloud and would become unplayable if
    /// this device disconnected. `0` means nothing is at risk. The UI renders the
    /// warning sentence itself, from `core.sync.cloud_only_releases` and its own
    /// locale's plural rules.
    pub async fn cloud_only_release_count(&self) -> Result<u64, BridgeError> {
        Ok(self
            .services
            .library_manager()
            .cloud_only_release_count()
            .await?)
    }

    pub async fn generate_restore_code(&self) -> Result<String, BridgeError> {
        Ok(self
            .services
            .library_manager()
            .generate_restore_code()
            .await?)
    }

    /// The library's membership (devices, with this device flagged, and whether
    /// the running device is an owner). Reads the membership chain from cloud
    /// storage.
    pub async fn get_members(&self) -> Result<crate::types::BridgeMembership, BridgeError> {
        let membership = self.services.library_manager().get_members().await?;
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
            .library_manager()
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
            .library_manager()
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
            self.services
                .library_manager()
                .drive_device_join(invite)
                .await?,
            DeviceJoinOutcome::Joined
        ))
    }

    /// Withdraw an invite this device minted, so a joining device that already
    /// scanned it is told to stop.
    pub async fn cancel_device_invite(&self, invite: Vec<u8>) -> Result<(), BridgeError> {
        Ok(self
            .services
            .library_manager()
            .cancel_device_invite(invite)
            .await?)
    }

    /// Remove a device from the library and rotate the library key.
    pub async fn remove_member(&self, public_key_hex: String) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .remove_member(&public_key_hex)
            .await?;
        Ok(())
    }

    /// Forget the active local library on this device: delete its key, clear the
    /// active pointer, and remove its data directory (the owner's cloud copy is
    /// untouched). The caller must drop this handle right after — the database
    /// lives in the removed directory — and re-open / onboard from scratch.
    pub fn forget_library(&self) -> Result<(), BridgeError> {
        self.services.library_manager().forget_library()?;

        info!("Forgot local library");
        Ok(())
    }

    pub fn trigger_sync(&self) {
        self.services.library_manager().trigger_sync();
    }

    pub fn is_sync_ready(&self) -> bool {
        self.services.library_manager().is_sync_ready()
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
            .library_manager()
            .outbox_snapshot()
            .await
            .map_err(BridgeError::internal)?;
        Ok(crate::types::BridgeOutboxSnapshot::from_core(snapshot))
    }

    /// Retry failed uploads now: drain coven's upload queue immediately instead
    /// of waiting for the next sync cycle.
    pub async fn retry_outbox(&self) -> Result<(), BridgeError> {
        self.services
            .library_manager()
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
            .library_manager()
            .cancel_release_transition(&release_id)
            .await
            .map_err(BridgeError::internal)
    }

    /// Pause or resume the cloud-upload pipeline. While paused, new enqueues
    /// still land in the outbox but the sync cycle won't drain them; the
    /// snapshot's `paused` field flips so the UI can render the toggle.
    pub async fn set_sync_paused(&self, paused: bool) {
        self.services
            .library_manager()
            .set_sync_paused(paused)
            .await;
    }

    // ── Download (pin) queue ─────────────────────────────────────────

    /// The current download-queue snapshot. Download invalidations tell the
    /// Downloads pane to read it again.
    pub fn get_download_snapshot(&self) -> crate::types::BridgeDownloadSnapshot {
        crate::types::BridgeDownloadSnapshot::from_core(
            self.services.library_manager().download_snapshot(),
        )
    }

    /// Enqueue releases to pin for offline. They join the in-memory serial
    /// download queue; the worker drains them one at a time. The DB lookups
    /// (resolving each release's title/size for its pane row) happen here; the
    /// deep cloud download runs on the queue worker.
    pub async fn queue_pin_releases(&self, release_ids: Vec<String>) {
        self.services
            .library_manager()
            .enqueue_pins(release_ids)
            .await;
    }

    /// Pause or resume the download queue. In-flight downloads finish; the queue
    /// stops starting new ones until resumed.
    pub fn set_downloads_paused(&self, paused: bool) {
        self.services.library_manager().set_downloads_paused(paused);
    }

    /// Cancel a release's download — drops a queued/failed entry or aborts the
    /// in-flight one (a partial download never lands, so the release stays
    /// cloud-only).
    pub fn cancel_download(&self, release_id: String) {
        self.services.library_manager().cancel_download(&release_id);
    }

    /// Retry every failed download now (flips them back to queued and wakes the
    /// worker).
    pub fn retry_downloads(&self) {
        self.services.library_manager().retry_downloads();
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
            self.app.shutdown_mcp();
            self.app.shutdown_subsonic();
            self.services.playback().shutdown().await;
        }
        #[cfg(not(feature = "desktop"))]
        self.services.playback().shutdown().await;
    }

    /// Persist the current playback state without stopping playback. Mobile
    /// calls this when the app is backgrounded so the queue, current track, and
    /// position survive a later cold launch — it can't call `shutdown`, which
    /// would stop the background audio.
    pub async fn save_playback_state(&self) {
        self.services.playback().save_state().await;
    }
}

#[cfg(feature = "desktop")]
impl BridgeMcpServerStatus {
    fn from_core(status: bae_desktop::McpServerStatus) -> Self {
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
    fn from_core(error: bae_desktop::McpServerError) -> Self {
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

#[cfg(feature = "desktop")]
impl crate::types::BridgeCastStatus {
    fn from_core(status: bae_desktop::CastStatus) -> Self {
        use crate::types::BridgeCastStatus;
        match status {
            bae_desktop::CastStatus::NotCasting => BridgeCastStatus::NotCasting,
            bae_desktop::CastStatus::Casting { device_name } => {
                BridgeCastStatus::Casting { device_name }
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeSubsonicServerStatus {
    fn from_core(status: bae_desktop::SubsonicServerStatus) -> Self {
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
    fn from_core(error: bae_desktop::SubsonicServerError) -> Self {
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

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    /// Bytes of a curated library image (a release cover or an artist portrait),
    /// read through coven's locality-aware read (local store while Local,
    /// cache/cloud while Remote). `None` when no such image exists. The
    /// `BridgeImageRef` carries the image kind, so core reads the known image
    /// namespace directly. A read error surfaces, not masked.
    pub async fn fetch_library_image_bytes(
        &self,
        image: crate::types::BridgeImageRef,
    ) -> Result<Option<Vec<u8>>, BridgeError> {
        self.services
            .library_manager()
            .read_image_blob(&image.into_core())
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }

    /// Bytes of one slot in a release's image strip. The lightbox calls this for
    /// EVERY item, passing the `source` it received; core dispatches the read on
    /// the variant (the cover by its image ref, a release's own image file by
    /// file id), so the UI never picks the byte source itself. A read error
    /// surfaces, not masked.
    pub async fn fetch_release_image_bytes(
        &self,
        release_id: String,
        source: BridgeGallerySource,
    ) -> Result<Vec<u8>, BridgeError> {
        self.services
            .library_manager()
            .read_gallery_bytes(&release_id, &source.into_core())
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))
    }
}

// =========================================================================
// Apple-only: CloudKit
// =========================================================================

#[cfg(feature = "cloudkit")]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub async fn use_cloudkit(&self, storage: BridgeHomeStorage) -> Result<(), BridgeError> {
        let storage = crate::types::BridgeHomeStorage::into_core(storage);
        self.services
            .library_manager()
            .use_cloudkit(storage)
            .await?;
        Ok(())
    }
}

// =========================================================================
// OAuth-only: consumer-cloud sign-in (Google Drive, Dropbox, OneDrive)
// =========================================================================

#[cfg(feature = "oauth-providers")]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub async fn sign_in_cloud_provider(
        &self,
        provider: BridgeCloudProvider,
        storage: BridgeHomeStorage,
    ) -> Result<(), BridgeError> {
        // OAuth + cloud-folder setup then starts sync. Cancellation (the Swift
        // Task dropping this future) tears down the OAuth listener via coven's
        // own drop guard.
        let core_provider = crate::types::BridgeCloudProvider::into_core(provider);
        let storage = crate::types::BridgeHomeStorage::into_core(storage);
        self.services
            .library_manager()
            .sign_in_cloud_provider(core_provider, storage)
            .await?;
        Ok(())
    }
}

// =========================================================================
// Desktop-only: Import, Cover fetching
// =========================================================================

#[cfg(feature = "desktop")]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    pub fn set_mcp_server_config(&self, enabled: bool, port: u16) -> Result<(), BridgeError> {
        self.app
            .set_mcp_config(bae_core::config::McpConfig { enabled, port })
            .map_err(BridgeError::config)
    }

    pub fn get_mcp_server_status(&self) -> BridgeMcpServerStatus {
        BridgeMcpServerStatus::from_core(self.app.mcp_server_status())
    }

    pub fn get_mcp_token(&self) -> Result<String, BridgeError> {
        Ok(self.services.library_manager().ensure_mcp_token()?)
    }

    pub fn generate_mcp_token(&self) -> String {
        bae_core::library::generate_mcp_token()
    }

    pub fn set_mcp_token(&self, token: String) -> Result<(), BridgeError> {
        self.services.library_manager().set_mcp_token(token)?;
        Ok(())
    }

    pub fn set_subsonic_server_config(
        &self,
        enabled: bool,
        port: u16,
        username: String,
        bind_address: String,
    ) -> Result<(), BridgeError> {
        self.app
            .set_subsonic_config(bae_core::config::SubsonicConfig {
                enabled,
                port,
                username,
                bind_address,
            })
            .map_err(BridgeError::config)
    }

    pub fn get_subsonic_server_status(&self) -> BridgeSubsonicServerStatus {
        BridgeSubsonicServerStatus::from_core(self.app.subsonic_server_status())
    }

    pub fn set_subsonic_password(&self, password: String) -> Result<(), BridgeError> {
        self.app
            .set_subsonic_password(&password)
            .map_err(BridgeError::config)
    }

    /// Start browsing for Cast devices (call when the device picker opens).
    pub fn start_cast_discovery(&self) {
        self.app.start_cast_discovery();
    }

    /// Stop browsing for Cast devices (call when the device picker closes).
    pub fn stop_cast_discovery(&self) {
        self.app.stop_cast_discovery();
    }

    /// The current list of discovered remote-renderer devices (Cast and UPnP).
    /// Requery on a `CastDevices` invalidation.
    pub fn get_cast_devices(&self) -> Vec<crate::types::BridgeCastDevice> {
        self.app
            .cast_devices()
            .into_iter()
            .map(crate::types::BridgeCastDevice::from_core)
            .collect()
    }

    /// Cast playback to the device with `device_id`.
    pub fn cast_to(&self, device_id: String) -> Result<(), BridgeError> {
        self.app.cast_to(&device_id).map_err(|e| {
            let detail = e.to_string();
            match e {
                // AirPlay receivers the sender can't drive get their own localized
                // picker line, not the generic internal-error one.
                bae_desktop::CastError::AirPlayPinRequired
                | bae_desktop::CastError::AirPlayEncryptionUnsupported => BridgeError::diagnostic(
                    crate::types::BridgeErrorCategory::AirPlayUnsupported,
                    detail,
                ),
                _ => BridgeError::internal(detail),
            }
        })
    }

    /// Stop casting and return playback to local output.
    pub fn stop_casting(&self) {
        self.app.stop_casting();
    }

    /// Whether playback is currently on a Cast device, and which. Refresh on a
    /// `CastStatusChanged` event.
    pub fn get_cast_status(&self) -> crate::types::BridgeCastStatus {
        crate::types::BridgeCastStatus::from_core(self.app.cast_status())
    }

    /// Validate then persist a Discogs API token, returning what happened so the
    /// UI can react (keep the draft on `Rejected`, show the optimistic-save note
    /// on `Unvalidated`). Lives on the import service, which only runs on desktop
    /// (identification). Mobile reads token status via `get_config` but never
    /// writes.
    pub async fn save_discogs_token(
        &self,
        token: String,
    ) -> Result<BridgeDiscogsSaveOutcome, BridgeError> {
        self.services
            .import()
            .save_discogs_token(&token)
            .await
            .map(BridgeDiscogsSaveOutcome::from_core)
            .map_err(BridgeError::config)
    }

    /// Re-check a stored `Unvalidated` key against Discogs. No-op when no key is
    /// stored or it's already settled. Called at app launch and settings-tab
    /// open for the offline-saved case.
    pub async fn revalidate_discogs_token(&self) -> Result<(), BridgeError> {
        self.services
            .import()
            .revalidate_discogs_token()
            .await
            .map_err(BridgeError::config)
    }

    pub fn remove_discogs_token(&self) -> Result<(), BridgeError> {
        self.services
            .import()
            .remove_discogs_token()
            .map_err(BridgeError::config)
    }

    /// Register the platform artwork analyzer. Called once at app boot
    /// (e.g. from `BaeApp`'s startup path) by the platforms that have one.
    /// Extraction owns artwork OCR and streams its barcode/text signals to
    /// identify. A platform that never calls this has no artwork analyzer, and
    /// extraction treats artwork as no signal source at all — the barcode signal
    /// reports `Absent` (never read) rather than an empty scan.
    pub fn register_artwork_analyzer(
        &self,
        analyzer: Box<dyn crate::types::ArtworkAnalyzerCallback>,
    ) {
        let adapter = std::sync::Arc::new(crate::signals::ArtworkAnalyzerAdapter::new(analyzer));
        self.services.extraction().register_analyzer(adapter);
    }

    /// Start identifying a folder candidate. Identify subscribes first, then
    /// extraction streams the candidate's `Signals` (disc ID, barcodes,
    /// classified text) that identify looks up and the UI surfaces. Events
    /// flow through the unified import event channel → bus → reducer → store,
    /// and the verdict this reaches is persisted like the background sweep's —
    /// core decides all of that, so this stays one call.
    pub fn auto_identify_folder(&self, candidate_key: String) {
        self.services.identify_folder_candidate(candidate_key);
    }

    /// Start re-identifying an existing library release. Extraction resolves
    /// the release's disc ID and artwork from the library. Events stream
    /// through the same identify channel — the UI consumes them by candidate
    /// key the same way it does for folder imports.
    pub fn auto_identify_release(&self, candidate_key: String, release_id: String) {
        self.services
            .identify()
            .start(candidate_key.clone(), CallPriority::Interactive);
        self.services.extraction().start(
            candidate_key,
            ExtractionSource::Release { release_id },
            CallPriority::Interactive,
        );
    }

    /// Stop a candidate's identify pipeline: cancels the identify driver and
    /// the in-flight signal extraction (artwork OCR) for `candidate_key`. The
    /// inverse of `auto_identify_folder` / `auto_identify_release`; a no-op for
    /// a key with nothing running. Called when the UI tears the candidate down
    /// (the re-identify sheet closing).
    pub fn cancel_auto_identify(&self, candidate_key: String) {
        self.services.identify().cancel(&candidate_key);
        self.services.extraction().cancel(&candidate_key);
    }

    /// Toggle a signal in a candidate's toolbar — include or exclude it from
    /// triangulation. The identify driver flips the signal and re-combines
    /// over the surviving signals, emitting the resulting state through the
    /// same event channel. Idempotent: a no-op when the candidate isn't
    /// running.
    pub fn toggle_signal_for_candidate(
        &self,
        candidate_key: String,
        signal: crate::types::BridgeExcludedSignal,
    ) {
        self.services
            .identify()
            .toggle_signal(&candidate_key, signal.into_core());
    }

    /// Re-run a candidate's lookups from the toolbar. A live driver resets to
    /// triangulating and re-dispatches from its retained signals, preserving
    /// exclusions; a candidate showing a resumed stored verdict has no driver,
    /// and a fresh interactive run replaces the stored answer.
    pub fn rerun_identify_for_candidate(&self, candidate_key: String) {
        self.services.rerun_identify(candidate_key);
    }

    /// Decide a candidate's identity — the pressing the user picked, or the
    /// decision to read the folder's own tags. Persists the choice and comes
    /// back with everything the pane seeds from it, the same payload
    /// [`Self::candidate_decided_identity`] serves, so a fresh launch renders
    /// exactly what this click rendered. Identification writes the same
    /// record itself when a verdict settles on exactly one match; this is the
    /// path for the choices only a person can make.
    pub async fn pick_candidate_identity(
        &self,
        candidate_key: String,
        pick: crate::types::BridgeIdentityPick,
    ) -> Result<crate::types::BridgeDecidedIdentity, BridgeError> {
        let answer = self
            .services
            .import()
            .pick_candidate_identity(candidate_key, pick.into_core())
            .await
            .map_err(BridgeError::import)?;
        Ok(crate::types::BridgeDecidedIdentity::from_core(answer))
    }

    /// The candidate's decided identity read back, or `None` while nothing is
    /// decided — what selecting a row asks, and the whole of "resume": a
    /// stored decision answers exactly like the click that made it did.
    pub async fn candidate_decided_identity(
        &self,
        candidate_key: String,
    ) -> Result<Option<crate::types::BridgeDecidedIdentity>, BridgeError> {
        let answer = self
            .services
            .import()
            .candidate_answer(candidate_key)
            .await
            .map_err(BridgeError::import)?;
        Ok(answer.map(crate::types::BridgeDecidedIdentity::from_core))
    }

    /// Re-identify commit. Translates the user's `IdentityChoice` into a fully
    /// cross-linked identity vec + metadata pointer, then writes via
    /// `set_identity` — the outcome is indistinguishable from re-importing the
    /// release with the same choice.
    ///
    /// Returns the album id the release lives on after the commit, which may have
    /// changed if the new identity vec didn't fit the source album
    /// (`set_identity` move semantics). Reseeding metadata is the caller's call:
    /// `reset_metadata_to_source` + `update_release_metadata_user_edit`.
    pub async fn re_identify_release(
        &self,
        release_id: String,
        identity_choice: crate::types::BridgeIdentityChoice,
    ) -> Result<String, BridgeError> {
        let core_choice = identity_choice.into_core();
        let library_manager = self.services.library_manager();
        library_manager
            .re_identify_release(&release_id, core_choice)
            .await
            .map_err(BridgeError::import)?;
        library_manager
            .get_album_id_for_release(&release_id)
            .await
            .map_err(BridgeError::database)
    }

    /// The current watched-folder list. The UI fetches this when the import
    /// view appears to render the group headers.
    pub fn watched_folders(&self) -> Vec<crate::types::BridgeWatchedFolder> {
        self.services
            .import()
            .watched_folders()
            .into_iter()
            .map(crate::types::BridgeWatchedFolder::from_core)
            .collect()
    }

    pub fn get_import_candidates(&self) -> BridgeImportCandidatesSnapshot {
        crate::types::BridgeImportCandidatesSnapshot::from_core(
            self.services.get_import_candidates(),
        )
    }

    /// The import sidebar: one pre-shaped row per candidate and the four tab
    /// counts. Read it again on an `ImportCandidateList` invalidation — it
    /// consults the stored verdicts and the live library, so it is a query
    /// rather than something pushed a row at a time.
    pub async fn get_import_triage_queue(
        &self,
    ) -> Result<crate::types::BridgeTriageQueue, BridgeError> {
        self.services
            .import_triage_queue()
            .await
            .map(crate::types::BridgeTriageQueue::from_core)
            .map_err(BridgeError::database)
    }

    pub fn get_candidate(&self, key: String) -> Option<BridgeImportCandidateSnapshot> {
        self.services
            .get_candidate(&key)
            .map(crate::types::BridgeImportCandidateSnapshot::from_core)
    }

    pub async fn add_watched_folder(&self, path: String) -> Result<(), BridgeError> {
        self.services
            .import()
            .add_watched_folder(path)
            .await
            .map_err(BridgeError::import)
    }

    pub async fn remove_watched_folder(&self, path: String) -> Result<(), BridgeError> {
        self.services
            .import()
            .remove_watched_folder(path)
            .await
            .map_err(BridgeError::import)
    }

    pub async fn refresh_watched_folder(&self, path: String) -> Result<(), BridgeError> {
        self.services
            .import()
            .refresh_watched_folder(path)
            .await
            .map_err(BridgeError::import)
    }

    pub async fn set_folder_release_decision(
        &self,
        key: crate::types::BridgeFolderReleaseDecisionKey,
        decision: crate::types::BridgeFolderReleaseDecision,
    ) -> Result<(), BridgeError> {
        self.services
            .import()
            .set_folder_release_decision(key.into_core(), decision.into_core())
            .await
            .map_err(BridgeError::import)
    }

    /// Mark the candidate at `path` skipped or unskipped. Persists the change;
    /// the candidate invalidation makes the import view read the new row.
    pub async fn set_candidate_skipped(
        &self,
        path: String,
        skipped: bool,
    ) -> Result<(), BridgeError> {
        self.services
            .import()
            .set_candidate_skipped(path, skipped)
            .await
            .map_err(BridgeError::import)
    }

    /// What the track sheet `sheet_file_id` on candidate `candidate_key` can be
    /// bound to: the folder's audio, each file offered or refused with the
    /// reason. Empty when the sheet names one file per track rather than one
    /// for the disc — a single choice cannot express that layout — and when the
    /// folder holds no audio.
    ///
    /// Core probes each file to decide, so ask for this when a picker opens
    /// rather than holding it alongside the candidate.
    pub async fn sheet_binding_options(
        &self,
        candidate_key: String,
        sheet_file_id: String,
    ) -> Result<Vec<crate::types::BridgeSheetBindingOption>, BridgeError> {
        Ok(self
            .services
            .import()
            .sheet_binding_options(candidate_key, sheet_file_id)
            .await
            .map_err(BridgeError::import)?
            .into_iter()
            .map(crate::types::BridgeSheetBindingOption::from_core)
            .collect())
    }

    /// Bind a candidate's track sheet to one of its audio files, or clear the
    /// binding with `audio_file_id: None`.
    ///
    /// Clearing leaves the sheet describing nothing; it does not restore what
    /// the scan proposed. `audio_file_id` must be one the matching
    /// [`Self::sheet_binding_options`] call offered — a refused one is rejected
    /// here rather than at commit.
    ///
    /// Persists the decision and clears the candidate's stored identify
    /// verdict, because a bound sheet is a different disc. The candidate
    /// invalidation makes the import view read the new roles.
    pub async fn set_sheet_binding(
        &self,
        candidate_key: String,
        sheet_file_id: String,
        audio_file_id: Option<String>,
    ) -> Result<(), BridgeError> {
        self.services
            .import()
            .set_sheet_binding(candidate_key, sheet_file_id, audio_file_id)
            .await
            .map_err(BridgeError::import)
    }

    /// Say which disc of the release one of a candidate's track sheets holds,
    /// or take it out of the tracklist with `BridgeSheetDisc::Ignored`.
    ///
    /// Cue filenames are arbitrary — `CD1.cue` may hold disc two — so the
    /// assignment is the truth about which cue is which disc, and no UI reads
    /// it off a name. Discs count from one.
    ///
    /// Persists the decision and clears the candidate's stored identify
    /// verdict, because a re-assigned or ignored sheet is a different
    /// tracklist. The candidate invalidation makes the import view read it.
    pub async fn set_sheet_disc(
        &self,
        candidate_key: String,
        sheet_file_id: String,
        disc: crate::types::BridgeSheetDisc,
    ) -> Result<(), BridgeError> {
        self.services
            .import()
            .set_sheet_disc(candidate_key, sheet_file_id, disc.into_core())
            .await
            .map_err(BridgeError::import)
    }

    /// Put one of a candidate's files in a role, or put it back in the one the
    /// scan proposed. `choice` must be one of that file's
    /// `BridgeCandidateFile::alternatives`.
    ///
    /// This is what a slot's Exclude action calls: taking a file out of the
    /// tracklist is a fact about the folder, so it is stored rather than kept
    /// in whichever pane happens to be open — a pane that dropped the row
    /// locally would have it back the next time a release was picked.
    ///
    /// Persists the decision and clears the candidate's stored identify
    /// verdict, because a folder with one fewer track is a different disc. The
    /// candidate invalidation makes the import view read the new roles.
    pub async fn set_file_role(
        &self,
        candidate_key: String,
        file_id: String,
        choice: crate::types::BridgeFileRoleChoice,
    ) -> Result<(), BridgeError> {
        self.services
            .import()
            .set_file_role(candidate_key, file_id, choice.into_core())
            .await
            .map_err(BridgeError::import)
    }

    /// Scan every watched folder. Import invalidations tell the UI to read the
    /// current candidate list.
    pub fn scan_watched_folders(&self) -> Result<(), BridgeError> {
        self.services
            .import()
            .scan_watched_folders()
            .map_err(BridgeError::import)
    }

    /// Search for releases with library status check in one call. Cancelled
    /// when the Swift caller drops the awaiting Task — uniffi forwards
    /// cancellation to the Rust future, which drops the in-flight HTTP
    /// request before it completes.
    pub async fn search_for_candidate(
        &self,
        query: crate::types::BridgeSearchQuery,
    ) -> Result<crate::types::BridgeCandidateSearchResults, BridgeError> {
        use bae_core::import::SearchQuery;

        let (core_query, tab, bridge_source) = match query {
            crate::types::BridgeSearchQuery::General {
                artist,
                album,
                source,
            } => (
                SearchQuery::General {
                    artist,
                    album,
                    source: source.into_core(),
                },
                crate::types::BridgeSearchQueryKind::General,
                source,
            ),
            crate::types::BridgeSearchQuery::CatalogNumber {
                catalog_number,
                source,
            } => (
                SearchQuery::CatalogNumber {
                    catalog_number,
                    source: source.into_core(),
                },
                crate::types::BridgeSearchQueryKind::CatalogNumber,
                source,
            ),
            crate::types::BridgeSearchQuery::Barcode { barcode, source } => (
                SearchQuery::Barcode {
                    barcode,
                    source: source.into_core(),
                },
                crate::types::BridgeSearchQueryKind::Barcode,
                source,
            ),
        };

        let grouped = self
            .services
            .import()
            .search_with_status(core_query)
            .await
            .map_err(BridgeError::import)?;

        Ok(crate::types::BridgeCandidateSearchResults::from_core(
            grouped,
            tab,
            bridge_source,
        ))
    }

    /// The claim line for picking `result` under `candidate_key`, without a
    /// prefetch. Re-identify's path: it commits straight from the picked row,
    /// so the header states the claim from that row plus the candidate's own
    /// identify evidence. The import confirm pane gets the same line back from
    /// `prefetch_release` instead, since it is fetching the release anyway.
    pub fn claim_for_pick(
        &self,
        candidate_key: String,
        result: crate::types::BridgeMetadataResult,
    ) -> crate::types::BridgeClaimLine {
        let release = bae_core::import::ClaimRelease {
            release_ref: bae_core::import::MetadataRef::new(
                result.release_id,
                result.source.into_core(),
            ),
            year: result.year,
            format: result.format,
            country: result.country,
            catalog_number: result.catalog_number,
            // A picked row carries no tracklist: the search endpoint returns
            // none, and re-identify already refuses a release whose track count
            // differs from the library release's, so there is nothing the
            // metadata-from line could tell the user by repeating it.
            track_count: None,
        };
        crate::types::BridgeClaimLine::from_core(
            self.services
                .import()
                .claim_for_pick(&candidate_key, &release),
        )
    }

    // The bridge boundary surface is wide on purpose — uniffi flattens
    // primitives across the FFI per Swift idiom, not per Rust idiom.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_import(
        &self,
        candidate_key: String,
        selected_cover: Option<BridgeCoverSelection>,
        storage_mode: BridgeStorageMode,
        pin: bool,
        identity_choice: crate::types::BridgeIdentityChoice,
        user_edit: Option<crate::types::BridgeReleaseUserEdit>,
    ) -> Result<(), BridgeError> {
        let cover = selected_cover.map(crate::types::BridgeCoverSelection::into_core);

        let user_edit = user_edit.map(crate::types::BridgeReleaseUserEdit::into_core);

        self.services
            .import()
            .start_import(
                &candidate_key,
                cover,
                storage_mode.into_core(),
                pin,
                identity_choice.into_core(),
                user_edit,
            )
            .await
            .map(|_| ())
            .map_err(BridgeError::import)
    }

    /// Project the embedded tags of a folder's audio files into the
    /// editor's user-edit shape. Used by the "Add as Unknown"
    /// affordance: the UI calls this to populate the editor before
    /// the user verifies/edits and commits with
    /// `BridgeIdentityChoice::Unknown`.
    pub async fn preview_file_tags_for_folder(
        &self,
        candidate_key: String,
    ) -> Result<crate::types::BridgeReleaseUserEdit, BridgeError> {
        let edit = self
            .services
            .import()
            .preview_file_tags_for_folder(candidate_key)
            .await
            .map_err(BridgeError::import)?;
        Ok(crate::types::BridgeReleaseUserEdit::from_core(edit))
    }

    /// The mapping table for a candidate nobody has picked a release for:
    /// every source unit the folder offers, with what it becomes left open.
    pub fn candidate_mapping(
        &self,
        candidate_key: String,
    ) -> Result<crate::types::BridgeMappingTable, BridgeError> {
        let mapping = self
            .services
            .import()
            .candidate_mapping(&candidate_key)
            .map_err(BridgeError::import)?;
        Ok(crate::types::BridgeMappingTable::from_core(mapping))
    }

    /// Apply a user-supplied metadata edit (from the edit-metadata sheet) to a
    /// release. Writes the user's edited values directly without touching
    /// identity, `metadata_source`, or cached source payloads.
    pub async fn update_release_metadata_user_edit(
        &self,
        release_id: String,
        edit: crate::types::BridgeReleaseUserEdit,
    ) -> Result<(), BridgeError> {
        let core_edit = crate::types::BridgeReleaseUserEdit::into_core(edit);
        self.services
            .library_manager()
            .apply_release_metadata_user_edit(&release_id, &core_edit)
            .await
            .map_err(BridgeError::import)
    }

    /// Seed the EditMetadataSheet's raw form from a library release's current
    /// metadata. bae-core does the projection (current state → wire edit → raw
    /// form); this is pure type translation around the result.
    pub async fn seed_release_edit(
        &self,
        release_id: String,
    ) -> Result<crate::types::BridgeRawReleaseEdit, BridgeError> {
        let raw = self
            .services
            .library_manager()
            .release_edit_seed(&release_id)
            .await
            .map_err(BridgeError::import)?;
        Ok(crate::types::BridgeRawReleaseEdit::from_core(raw))
    }

    /// Re-project a release's metadata from its `metadata_source` /
    /// `metadata_source_release_id` pointer. Returns the projected
    /// `ReleaseUserEdit` without writing — the editor populates its
    /// form with the result; the user re-edits or saves via
    /// `update_release_metadata_user_edit`. Identity rows and the
    /// metadata-source columns are not touched.
    pub async fn reset_metadata_to_source(
        &self,
        release_id: String,
    ) -> Result<crate::types::BridgeReleaseUserEdit, BridgeError> {
        let edit = self
            .services
            .library_manager()
            .reset_metadata_to_source(&release_id)
            .await
            .map_err(BridgeError::import)?;
        Ok(crate::types::BridgeReleaseUserEdit::from_core(edit))
    }

    pub async fn fetch_remote_covers(
        &self,
        release_id: String,
    ) -> Result<Vec<BridgeRemoteCover>, BridgeError> {
        let covers = self
            .services
            .import()
            .fetch_remote_covers(&release_id)
            .await
            .map_err(BridgeError::import)?;
        Ok(covers
            .into_iter()
            .map(crate::types::BridgeRemoteCover::from_core)
            .collect())
    }

    /// Bytes of provider art at `url` — art from Cover Art Archive or Discogs
    /// that isn't in the library yet, so there is no image ref to read it by.
    /// Core owns the network: this is the only image fetch that leaves the
    /// device, and its byte cache is core's. The returned validator identifies
    /// the content, so a UI holding a decoded copy replaces it only when the
    /// bytes at the URL actually change.
    pub async fn fetch_remote_image_bytes(
        &self,
        url: String,
    ) -> Result<crate::types::BridgeRemoteImage, BridgeError> {
        self.services
            .import()
            .fetch_remote_image_bytes(url)
            .await
            .map(crate::types::BridgeRemoteImage::from_core)
            .map_err(BridgeError::import)
    }
}

// =========================================================================
// Export (desktop-only)
// =========================================================================

// Gated on the target, not on `desktop`: bae-core gates `mod export` itself on the
// target, so the export queue and the track exporter exist on every non-mobile
// build whether or not this crate's `desktop` feature is on.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    // ── Export queue ─────────────────────────────────────────────────

    /// The current export-queue snapshot. Export invalidations tell the
    /// Exporting pane to read it again.
    pub fn get_output_snapshot(&self) -> crate::types::BridgeOutputSnapshot {
        crate::types::BridgeOutputSnapshot::from_core(
            self.services.library_manager().output_snapshot(),
        )
    }

    /// Enqueue a verbatim release export to `target_dir`. It joins the in-memory
    /// serial output queue; the worker drains it one release at a time. The
    /// storage-summary lookup (resolving the pane row's title/size) happens here;
    /// the deep cloud read + copy runs on the queue worker.
    pub async fn enqueue_export(
        &self,
        release_id: String,
        target_dir: String,
    ) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .enqueue_export(&release_id, std::path::PathBuf::from(target_dir))
            .await
            .map_err(BridgeError::export)
    }

    /// Enqueue a release-level save to `target_dir` under the preset named by
    /// `preset_id`. The preset is resolved and captured whole at enqueue time,
    /// so a later config edit can't change or break this queued save.
    pub async fn enqueue_release_save(
        &self,
        release_id: String,
        target_dir: String,
        preset_id: String,
    ) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .enqueue_release_save(
                &release_id,
                std::path::PathBuf::from(target_dir),
                &preset_id,
            )
            .await
            .map_err(BridgeError::save)
    }

    /// Pause or resume the export queue. The in-flight export finishes; the queue
    /// stops starting new ones until resumed.
    pub fn set_outputs_paused(&self, paused: bool) {
        self.services.library_manager().set_outputs_paused(paused);
    }

    /// Cancel a release's export — drops a queued/failed entry or aborts the
    /// in-flight one (a partial copy never lands its destination file).
    pub fn cancel_output(&self, release_id: String) {
        self.services.library_manager().cancel_output(&release_id);
    }

    /// Retry every failed export now (flips them back to queued and wakes the
    /// worker).
    pub fn retry_outputs(&self) {
        self.services.library_manager().retry_outputs();
    }

    // ── Track save ───────────────────────────────────────────────────

    /// Save one track to `output_path` under the preset named by `preset_id`
    /// (must apply to track saves). Always a constructed file — decoded, encoded
    /// to the preset codec, tagged, cover embedded — never a verbatim copy.
    pub async fn save_track(
        &self,
        track_id: String,
        output_path: String,
        preset_id: String,
    ) -> Result<(), BridgeError> {
        self.services
            .library_manager()
            .save_track(&track_id, std::path::Path::new(&output_path), &preset_id)
            .await
            .map_err(|e| BridgeError::save(format!("{e}")))
    }

    /// The default filename stem (no extension) a single-track "Save As…"
    /// suggests for `track_id` under the preset named by `preset_id`, rendered
    /// from that preset's token pattern. Reads only the database — no audio or
    /// cover — while seeding a save panel.
    pub async fn save_track_suggested_name(
        &self,
        track_id: String,
        preset_id: String,
    ) -> Result<String, BridgeError> {
        self.services
            .library_manager()
            .save_track_suggested_name(&track_id, &preset_id)
            .await
            .map_err(|e| BridgeError::save(format!("{e}")))
    }
}

// =========================================================================
// UI events (all platforms — the synced library drives the UI everywhere)
// =========================================================================

#[uniffi::export]
impl AppHandle {
    /// Subscribe to the unified UI event stream. One subscription for everything.
    ///
    /// Mobile receives library/config/sync/playback events (desktop-only
    /// import/identify events simply never fire there, since those services
    /// aren't started).
    pub fn subscribe_ui_events(&self, callback: Box<dyn crate::types::UiEventCallback>) {
        let rx = self.ui_event_bus.subscribe();
        self.runtime.spawn(pump_ui_events(rx, callback));
    }
}

/// Forward bus events to the platform callback until the bus closes.
///
/// Falling behind (`Lagged`) drops events but must not kill the subscription —
/// this loop is the UI's only event feed for the whole library session, and the
/// callback is a synchronous FFI call, so a slow consumer during a burst is
/// exactly when lag happens. Dropped events can't be replayed, so the pump
/// synthesizes every coarse invalidation instead and the UI re-reads each domain
/// rather than freezing on the last delivered snapshot. (Mirrors the core bus's
/// own lag recovery in `UiEventBus::wire_library_events`.) Dropped playback
/// pushes aren't recoverable by invalidation, but the next progress tick
/// refreshes them within a second.
async fn pump_ui_events(
    mut rx: tokio::sync::broadcast::Receiver<bae_core::ui::UiBusEvent>,
    callback: Box<dyn crate::types::UiEventCallback>,
) {
    use crate::types::{BridgeInvalidation, BridgeUiEvent};

    loop {
        match rx.recv().await {
            Ok(event) => {
                if let Some(bridge_event) = convert_ui_event(event) {
                    callback.on_event(bridge_event);
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(
                    "UI event subscription lagged; dropped {n} events; re-invalidating all domains"
                );
                for invalidation in [
                    BridgeInvalidation::AlbumList,
                    BridgeInvalidation::ComposerList,
                    BridgeInvalidation::ArtistList,
                    BridgeInvalidation::Queue,
                    BridgeInvalidation::Config,
                    BridgeInvalidation::SyncStatus,
                    BridgeInvalidation::Outbox,
                    BridgeInvalidation::DownloadQueue,
                    BridgeInvalidation::OutputQueue,
                    BridgeInvalidation::ImportCandidateList,
                    BridgeInvalidation::WatchedFolders,
                ] {
                    callback.on_event(BridgeUiEvent::Invalidated { invalidation });
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

impl crate::types::BridgeUploadReleaseGroup {
    fn from_core(g: bae_core::library::UploadReleaseGroup) -> Self {
        let bae_core::library::UploadReleaseGroup {
            release_id,
            display_title,
            files,
            progress,
        } = g;
        Self {
            release_id,
            display_title,
            files: files
                .into_iter()
                .map(crate::types::BridgeUploadFileOp::from_core)
                .collect(),
            progress: crate::types::BridgeUploadProgress::from_core(progress),
        }
    }
}

impl crate::types::BridgeUploadFileOp {
    /// Flatten core's per-file `UploadState` into `state` + `bytes_done` +
    /// `last_error`, so the UI reads plain fields instead of switching on
    /// associated data.
    fn from_core(f: bae_core::library::UploadFileOp) -> Self {
        use bae_core::library::UploadState;
        let bae_core::library::UploadFileOp {
            file_id,
            display_name,
            bytes_total,
            state,
        } = f;
        let (state, bytes_done, last_error) = match state {
            UploadState::Queued => (crate::types::BridgeUploadFileState::Queued, 0, None),
            UploadState::Active { bytes_done } => (
                crate::types::BridgeUploadFileState::Uploading,
                bytes_done,
                None,
            ),
            UploadState::Failed { last_error } => (
                crate::types::BridgeUploadFileState::Retrying,
                0,
                Some(last_error),
            ),
            UploadState::Done => (crate::types::BridgeUploadFileState::Done, bytes_total, None),
        };
        Self {
            file_id,
            display_name,
            bytes_done,
            bytes_total,
            state,
            last_error,
        }
    }
}

impl crate::types::BridgeDeleteOp {
    fn from_core(op: bae_core::library::DeleteOp) -> Self {
        let bae_core::library::DeleteOp {
            namespace,
            blob_id,
            created_at,
        } = op;
        Self {
            namespace,
            blob_id,
            created_at,
        }
    }
}

impl crate::types::BridgeOutboxSnapshot {
    fn from_core(snapshot: bae_core::library::OutboxSnapshot) -> Self {
        // Derived aggregates borrow `&snapshot`; compute them before the move.
        let per_release = snapshot
            .per_release_progress()
            .into_iter()
            .map(|(release_id, progress)| {
                (
                    release_id,
                    crate::types::BridgeUploadProgress::from_core(progress),
                )
            })
            .collect();
        let pending_deletes = snapshot.pending_delete_count();
        let summary_parts = snapshot
            .summary_parts()
            .into_iter()
            .map(crate::types::BridgeCountLabel::from_core)
            .collect();

        let bae_core::library::OutboxSnapshot {
            upload_groups,
            deletes,
            total,
            paused,
            throughput_bps,
            eta_seconds,
        } = snapshot;

        crate::types::BridgeOutboxSnapshot {
            upload_groups: upload_groups
                .into_iter()
                .map(crate::types::BridgeUploadReleaseGroup::from_core)
                .collect(),
            deletes: deletes
                .into_iter()
                .map(crate::types::BridgeDeleteOp::from_core)
                .collect(),
            per_release,
            total: crate::types::BridgeUploadProgress::from_core(total),
            pending_deletes,
            summary_parts,
            paused,
            throughput_bps,
            eta_seconds,
        }
    }
}

impl crate::types::BridgeUploadProgress {
    fn from_core(p: bae_core::library::UploadProgress) -> Self {
        // `activity()` borrows `&p`; compute it before destructuring `p`.
        let activity = p
            .activity()
            .map(crate::types::BridgeUploadActivity::from_core);
        let bae_core::library::UploadProgress {
            queued,
            active,
            failed,
            // Completed files feed the cumulative byte fractions; the count
            // itself has no UI consumer (finished releases aren't rendered,
            // and pending groups list their done files individually).
            done: _,
            bytes_done,
            bytes_total,
        } = p;
        crate::types::BridgeUploadProgress {
            queued,
            active,
            failed,
            bytes_done,
            bytes_total,
            activity,
        }
    }
}

impl crate::types::BridgeUploadActivity {
    fn from_core(a: bae_core::library::UploadActivity) -> Self {
        use crate::types::BridgeUploadActivity;
        use bae_core::library::UploadActivity;
        match a {
            UploadActivity::Uploading => BridgeUploadActivity::Uploading,
            UploadActivity::Retrying => BridgeUploadActivity::Retrying,
            UploadActivity::Queued => BridgeUploadActivity::Queued,
        }
    }
}

impl crate::types::BridgeDownloadTransferProgress {
    pub(crate) fn from_core(p: bae_core::library::DownloadTransferProgress) -> Self {
        let bae_core::library::DownloadTransferProgress {
            bytes_done,
            bytes_total,
            fraction,
        } = p;
        Self {
            bytes_done,
            bytes_total,
            fraction,
        }
    }
}

impl crate::types::BridgeDownloadState {
    fn from_core(state: bae_core::library::DownloadState) -> Self {
        use crate::types::BridgeDownloadState;
        use bae_core::library::DownloadState;
        match state {
            DownloadState::Queued => BridgeDownloadState::Queued,
            DownloadState::Active { progress } => BridgeDownloadState::Active {
                progress: crate::types::BridgeDownloadTransferProgress::from_core(progress),
            },
            DownloadState::Failed { error } => BridgeDownloadState::Failed { error },
        }
    }
}

impl crate::types::BridgeDownloadOp {
    fn from_core(op: bae_core::library::DownloadOp) -> Self {
        let bae_core::library::release_queue::ReleaseQueueOp {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            // Downloads carry no operation-specific payload.
            payload: (),
            state,
        } = op;
        crate::types::BridgeDownloadOp {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            state: crate::types::BridgeDownloadState::from_core(state),
        }
    }
}

/// Shared projection for the download and export queue snapshots, which are both
/// aliases of the same generic `ReleaseQueueSnapshot`. Parameterized over the
/// per-op and per-progress converters so each snapshot keeps its own named fields.
fn project_release_queue_snapshot<Extra, Progress, Op, Prog>(
    snapshot: bae_core::library::release_queue::ReleaseQueueSnapshot<Extra, Progress>,
    op_from_core: impl Fn(bae_core::library::release_queue::ReleaseQueueOp<Extra, Progress>) -> Op,
    progress_from_core: impl Fn(bae_core::library::release_queue::ReleaseQueueProgress) -> Prog,
) -> (Vec<Op>, Prog, bool) {
    let bae_core::library::release_queue::ReleaseQueueSnapshot { ops, total, paused } = snapshot;
    (
        ops.into_iter().map(op_from_core).collect(),
        progress_from_core(total),
        paused,
    )
}

/// Shared projection for the download and export per-state counts, both aliases
/// of the same generic `ReleaseQueueProgress`.
fn release_queue_progress_counts(
    p: bae_core::library::release_queue::ReleaseQueueProgress,
) -> (u32, u32, u32) {
    let bae_core::library::release_queue::ReleaseQueueProgress {
        queued,
        active,
        failed,
    } = p;
    (queued, active, failed)
}

impl crate::types::BridgeDownloadSnapshot {
    fn from_core(snapshot: bae_core::library::DownloadSnapshot) -> Self {
        let summary_parts = snapshot
            .total
            .summary_parts("core.queue.downloading")
            .into_iter()
            .map(crate::types::BridgeCountLabel::from_core)
            .collect();
        let (downloads, total, paused) = project_release_queue_snapshot(
            snapshot,
            crate::types::BridgeDownloadOp::from_core,
            crate::types::BridgeDownloadProgress::from_core,
        );
        crate::types::BridgeDownloadSnapshot {
            downloads,
            total,
            summary_parts,
            paused,
        }
    }
}

impl crate::types::BridgeDownloadProgress {
    fn from_core(p: bae_core::library::DownloadProgress) -> Self {
        let (queued, active, failed) = release_queue_progress_counts(p);
        crate::types::BridgeDownloadProgress {
            queued,
            active,
            failed,
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputState {
    fn from_core(state: bae_core::library::OutputState) -> Self {
        use crate::types::BridgeOutputState;
        use bae_core::library::OutputState;
        match state {
            OutputState::Queued => BridgeOutputState::Queued,
            OutputState::Active { progress } => BridgeOutputState::Active { percent: progress },
            OutputState::Failed { error } => BridgeOutputState::Failed { error },
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputOp {
    fn from_core(op: bae_core::library::OutputOp) -> Self {
        let bae_core::library::release_queue::ReleaseQueueOp {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            payload,
            state,
        } = op;
        let bae_core::library::output_snapshot::OutputRequest { target_dir, kind } = payload;
        crate::types::BridgeOutputOp {
            release_id,
            target_dir: target_dir.to_string_lossy().to_string(),
            title,
            file_count,
            total_size,
            created_at,
            state: crate::types::BridgeOutputState::from_core(state),
            kind: crate::types::BridgeOutputKind::from_core(&kind),
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputSnapshot {
    fn from_core(snapshot: bae_core::library::OutputSnapshot) -> Self {
        let summary_parts = snapshot
            .total
            .summary_parts("core.queue.output")
            .into_iter()
            .map(crate::types::BridgeCountLabel::from_core)
            .collect();
        let (outputs, total, paused) = project_release_queue_snapshot(
            snapshot,
            crate::types::BridgeOutputOp::from_core,
            crate::types::BridgeOutputProgress::from_core,
        );
        crate::types::BridgeOutputSnapshot {
            outputs,
            total,
            summary_parts,
            paused,
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl crate::types::BridgeOutputProgress {
    fn from_core(p: bae_core::library::OutputProgress) -> Self {
        let (queued, active, failed) = release_queue_progress_counts(p);
        crate::types::BridgeOutputProgress {
            queued,
            active,
            failed,
        }
    }
}

impl crate::types::BridgeQueueEntry {
    fn from_core(i: bae_core::queue::QueueItem) -> Self {
        let bae_core::queue::QueueItem {
            entry_id,
            track_id,
            title,
            artist_names,
            duration_ms,
            album_title,
            cover_image,
        } = i;
        crate::types::BridgeQueueEntry {
            entry_id,
            track_id,
            title,
            artist_names,
            duration_clock: crate::types::BridgeDurationClock::from_millis(duration_ms),
            album_title,
            cover_image: cover_image.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl crate::types::BridgePlaybackContext {
    fn from_core(context: bae_core::queue::ResolvedContext) -> Self {
        let bae_core::queue::ResolvedContext {
            source,
            source_title,
            shuffled,
            upcoming,
            upcoming_total,
        } = context;
        crate::types::BridgePlaybackContext {
            kind: crate::types::BridgePlaybackSourceKind::from_core(&source),
            source_title,
            shuffled,
            upcoming: upcoming
                .into_iter()
                .map(crate::types::BridgeQueueEntry::from_core)
                .collect(),
            upcoming_total,
        }
    }
}

impl crate::types::BridgeQueueSnapshot {
    fn from_core(snapshot: bae_core::queue::ResolvedQueueSnapshot) -> Self {
        let bae_core::queue::ResolvedQueueSnapshot {
            manual,
            context,
            has_next,
            has_previous,
            revision,
        } = snapshot;
        crate::types::BridgeQueueSnapshot {
            manual: manual
                .into_iter()
                .map(crate::types::BridgeQueueEntry::from_core)
                .collect(),
            context: context.map(crate::types::BridgePlaybackContext::from_core),
            has_next,
            has_previous,
            revision,
        }
    }
}

impl crate::types::BridgeQueueUpcomingPage {
    fn from_core(page: bae_core::queue::ResolvedQueueUpcomingPage) -> Self {
        let bae_core::queue::ResolvedQueueUpcomingPage { revision, items } = page;
        crate::types::BridgeQueueUpcomingPage {
            revision,
            entries: items
                .into_iter()
                .map(crate::types::BridgeQueueEntry::from_core)
                .collect(),
        }
    }
}

impl crate::types::BridgeSyncStatusSnapshot {
    fn from_core(snapshot: bae_core::library::SyncStatusSnapshot) -> Self {
        let bae_core::library::SyncStatusSnapshot {
            error,
            last_sync_time,
            syncing,
            sync_ready,
        } = snapshot;
        crate::types::BridgeSyncStatusSnapshot {
            error: error.map(crate::types::BridgeError::from_core),
            last_sync_time,
            syncing,
            sync_ready,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeFolderReleaseDecisionKey {
    fn from_core(key: bae_core::import::FolderReleaseDecisionKey) -> Self {
        Self {
            watched_folder_path: key.watched_folder_path,
            relative_folder_path: key.relative_folder_path,
        }
    }

    fn into_core(self) -> bae_core::import::FolderReleaseDecisionKey {
        bae_core::import::FolderReleaseDecisionKey {
            watched_folder_path: self.watched_folder_path,
            relative_folder_path: self.relative_folder_path,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeFolderReleaseDecision {
    fn from_core(decision: bae_core::import::FolderReleaseDecision) -> Self {
        match decision {
            bae_core::import::FolderReleaseDecision::CombineAsOneRelease => {
                Self::CombineAsOneRelease
            }
            bae_core::import::FolderReleaseDecision::KeepAsSeparateReleases => {
                Self::KeepAsSeparateReleases
            }
        }
    }

    fn into_core(self) -> bae_core::import::FolderReleaseDecision {
        match self {
            Self::CombineAsOneRelease => {
                bae_core::import::FolderReleaseDecision::CombineAsOneRelease
            }
            Self::KeepAsSeparateReleases => {
                bae_core::import::FolderReleaseDecision::KeepAsSeparateReleases
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeResolvedFolderReleaseBoundary {
    fn from_core(boundary: bae_core::import::ResolvedFolderReleaseBoundary) -> Self {
        Self {
            key: crate::types::BridgeFolderReleaseDecisionKey::from_core(boundary.key),
            decision: crate::types::BridgeFolderReleaseDecision::from_core(boundary.decision),
            name: boundary.name,
            display_path: boundary.display_path,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeFolderReleaseBoundary {
    fn from_core(boundary: bae_core::import::FolderReleaseBoundary) -> Self {
        Self {
            key: crate::types::BridgeFolderReleaseDecisionKey::from_core(boundary.key),
            name: boundary.name,
            display_path: boundary.display_path,
            shared_file_count: boundary.shared_file_count,
            tree_rows: boundary
                .tree_rows
                .into_iter()
                .map(|row| crate::types::BridgeFolderReleaseTreeRow {
                    name: row.name,
                    display_path: row.display_path,
                    depth: row.depth,
                    kind: match row.kind {
                        bae_core::import::FolderReleaseTreeRowKind::Folder => {
                            crate::types::BridgeFolderReleaseTreeRowKind::Folder
                        }
                        bae_core::import::FolderReleaseTreeRowKind::Candidate { summary } => {
                            crate::types::BridgeFolderReleaseTreeRowKind::Candidate {
                                track_count: summary.track_count,
                                format_label: summary.format_label,
                            }
                        }
                        bae_core::import::FolderReleaseTreeRowKind::Invalid { reason } => {
                            crate::types::BridgeFolderReleaseTreeRowKind::Invalid {
                                reason: crate::types::BridgeInvalidReason::from_core(reason),
                            }
                        }
                    },
                    decision_key: crate::types::BridgeFolderReleaseDecisionKey::from_core(
                        row.decision_key,
                    ),
                    ancestor_decision_keys: row
                        .ancestor_decision_keys
                        .into_iter()
                        .map(crate::types::BridgeFolderReleaseDecisionKey::from_core)
                        .collect(),
                })
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeWatchedFolderScanStatus {
    fn from_core(status: bae_core::import::WatchedFolderScanStatus) -> Self {
        Self {
            watched_folder_path: status.watched_folder_path,
            watched_folder_name: status.watched_folder_name,
            status: match status.status {
                bae_core::import::FolderScanStatus::Scanning => {
                    crate::types::BridgeFolderScanStatus::Scanning
                }
                bae_core::import::FolderScanStatus::Complete => {
                    crate::types::BridgeFolderScanStatus::Complete
                }
                bae_core::import::FolderScanStatus::Failed { error } => {
                    crate::types::BridgeFolderScanStatus::Failed { error }
                }
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeFolderCandidate {
    fn from_core(
        candidate: bae_core::import::FolderCandidate,
        skipped: bool,
        is_added: bool,
    ) -> Self {
        // `track_count()` borrows `&candidate`; compute it before the move.
        let track_count = candidate.track_count();
        let bae_core::import::FolderCandidate {
            path,
            name,
            files,
            watched_folder_path,
            ..
        } = candidate;
        crate::types::BridgeFolderCandidate {
            folder_path: path.to_string_lossy().to_string(),
            source_folder_name: name,
            watched_folder_path,
            files: crate::types::BridgeCandidateFiles::from_core(files),
            track_count,
            skipped,
            is_added,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeInvalidCandidate {
    fn from_core(candidate: bae_core::import::InvalidCandidate) -> Self {
        let bae_core::import::InvalidCandidate {
            path,
            name,
            watched_folder_path,
            display_path,
            resolved_boundaries,
            reason,
        } = candidate;
        crate::types::BridgeInvalidCandidate {
            folder_path: path.to_string_lossy().to_string(),
            source_folder_name: name,
            watched_folder_path,
            display_path,
            resolved_boundaries: resolved_boundaries
                .into_iter()
                .map(crate::types::BridgeResolvedFolderReleaseBoundary::from_core)
                .collect(),
            reason: crate::types::BridgeInvalidReason::from_core(reason),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeImportCandidateSnapshot {
    fn from_core(candidate: bae_core::import::ImportCandidateSnapshot) -> Self {
        match candidate {
            bae_core::import::ImportCandidateSnapshot::Folder {
                candidate,
                runtime,
                actionable,
                skipped,
                is_added,
            } => crate::types::BridgeImportCandidateSnapshot::Folder {
                candidate: crate::types::BridgeFolderCandidate::from_core(
                    candidate, skipped, is_added,
                ),
                runtime_snapshot: crate::types::BridgeCandidateRuntimeSnapshot::from_core(runtime),
                actionable,
            },
            bae_core::import::ImportCandidateSnapshot::Invalid(candidate) => {
                crate::types::BridgeImportCandidateSnapshot::Invalid {
                    candidate: crate::types::BridgeInvalidCandidate::from_core(candidate),
                }
            }
            bae_core::import::ImportCandidateSnapshot::Runtime { key, runtime } => {
                crate::types::BridgeImportCandidateSnapshot::Runtime {
                    key,
                    runtime_snapshot: crate::types::BridgeCandidateRuntimeSnapshot::from_core(
                        runtime,
                    ),
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeCandidateRuntimeSnapshot {
    fn from_core(runtime: bae_core::import::CandidateRuntimeSnapshot) -> Self {
        let bae_core::import::CandidateRuntimeSnapshot {
            identify_state,
            toolbar,
            signals,
            import_status,
        } = runtime;
        crate::types::BridgeCandidateRuntimeSnapshot {
            identify_state: crate::types::BridgeIdentifyState::from_core(identify_state),
            signals_toolbar: crate::types::BridgeSignalsToolbar::from_core(toolbar),
            signals: signals.map(crate::types::BridgeSignals::from_core),
            import_status: import_status.map(crate::types::BridgeCandidateImportStatus::from_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeCandidateImportStatus {
    fn from_core(status: bae_core::import::CandidateImportStatusSnapshot) -> Self {
        match status {
            bae_core::import::CandidateImportStatusSnapshot::Importing {
                progress_percent,
                step,
            } => crate::types::BridgeCandidateImportStatus::Importing {
                progress_percent,
                step: step.map(crate::types::BridgeImportStep::from_core),
            },
            bae_core::import::CandidateImportStatusSnapshot::Complete {
                release_id,
                album_id,
            } => crate::types::BridgeCandidateImportStatus::Complete {
                release_id,
                album_id,
            },
            bae_core::import::CandidateImportStatusSnapshot::Error { error } => {
                crate::types::BridgeCandidateImportStatus::Error {
                    error: crate::types::BridgeError::from_core(bae_core::ui::UiError::import(
                        error,
                    )),
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeFolderImportCandidateSnapshot {
    fn from_core(snapshot: bae_core::import::FolderImportCandidateSnapshot) -> Self {
        let bae_core::import::FolderImportCandidateSnapshot {
            candidate,
            runtime,
            actionable,
            skipped,
            is_added,
        } = snapshot;
        crate::types::BridgeFolderImportCandidateSnapshot {
            candidate: crate::types::BridgeFolderCandidate::from_core(candidate, skipped, is_added),
            runtime: crate::types::BridgeCandidateRuntimeSnapshot::from_core(runtime),
            actionable,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeImportCandidatesSnapshot {
    fn from_core(snapshot: bae_core::import::ImportCandidatesSnapshot) -> Self {
        let bae_core::import::ImportCandidatesSnapshot {
            watched_folders,
            folder_candidates,
            invalid_candidates,
            boundaries,
            folder_scan_statuses,
        } = snapshot;
        crate::types::BridgeImportCandidatesSnapshot {
            watched_folders: watched_folders
                .into_iter()
                .map(crate::types::BridgeWatchedFolder::from_core)
                .collect(),
            folder_candidates: folder_candidates
                .into_iter()
                .map(crate::types::BridgeFolderImportCandidateSnapshot::from_core)
                .collect(),
            invalid_candidates: invalid_candidates
                .into_iter()
                .map(crate::types::BridgeInvalidCandidate::from_core)
                .collect(),
            boundaries: boundaries
                .into_iter()
                .map(crate::types::BridgeFolderReleaseBoundary::from_core)
                .collect(),
            folder_scan_statuses: folder_scan_statuses
                .into_iter()
                .map(crate::types::BridgeWatchedFolderScanStatus::from_core)
                .collect(),
        }
    }
}

// ── Sidebar triage ─────────────────────────────────────────────────────────
//
// A mirror, variant for variant. Every decision behind these values was made in
// `bae_core::import::triage`.

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageQueue {
    pub(crate) fn from_core(queue: bae_core::import::TriageQueue) -> Self {
        let bae_core::import::TriageQueue {
            sections,
            counts,
            folder_scan_statuses,
        } = queue;
        let bae_core::import::TriageTabCounts {
            ready,
            needs_you,
            done,
            skipped,
        } = counts;
        crate::types::BridgeTriageQueue {
            sections: sections
                .into_iter()
                .map(crate::types::BridgeTriageSection::from_core)
                .collect(),
            counts: crate::types::BridgeTriageTabCounts {
                ready,
                needs_you,
                done,
                skipped,
            },
            folder_scan_statuses: folder_scan_statuses
                .into_iter()
                .map(crate::types::BridgeWatchedFolderScanStatus::from_core)
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageRow {
    pub(crate) fn from_core(row: bae_core::import::TriageRow) -> Self {
        let bae_core::import::TriageRow {
            candidate_key,
            folder_name,
            watched_folder_path,
            display_path,
            resolved_boundaries,
            combine_ancestor_key,
            actionable,
            placement,
            matched,
            selectable,
            import_status,
            picked,
        } = row;
        crate::types::BridgeTriageRow {
            candidate_key,
            folder_name,
            watched_folder_path,
            display_path,
            resolved_boundaries: resolved_boundaries
                .into_iter()
                .map(crate::types::BridgeResolvedFolderReleaseBoundary::from_core)
                .collect(),
            combine_ancestor_key: combine_ancestor_key
                .map(crate::types::BridgeFolderReleaseDecisionKey::from_core),
            actionable,
            placement: crate::types::BridgeTriagePlacement::from_core(placement),
            matched: matched.map(crate::types::BridgeMatchedRelease::from_core),
            selectable,
            import_status: import_status.map(crate::types::BridgeCandidateImportStatus::from_core),
            picked: picked.map(crate::types::BridgeIdentityPick::from_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageSection {
    fn from_core(section: bae_core::import::TriageSection) -> Self {
        Self {
            tab: crate::types::BridgeTriageTab::from_core(section.tab),
            watched_folder_path: section.watched_folder_path,
            group: section.group.map(|group| crate::types::BridgeTriageGroup {
                key: crate::types::BridgeFolderReleaseDecisionKey::from_core(group.key),
                name: group.name,
            }),
            entries: section
                .entries
                .into_iter()
                .map(|entry| {
                    let stable_key = entry.stable_key();
                    match entry {
                        bae_core::import::TriageEntry::Candidate(row) => {
                            crate::types::BridgeTriageEntry::Candidate {
                                stable_key,
                                row: crate::types::BridgeTriageRow::from_core(row),
                            }
                        }
                        bae_core::import::TriageEntry::Boundary(boundary) => {
                            crate::types::BridgeTriageEntry::Boundary {
                                stable_key,
                                boundary: crate::types::BridgeFolderReleaseBoundary::from_core(
                                    boundary,
                                ),
                            }
                        }
                        bae_core::import::TriageEntry::Invalid(candidate) => {
                            crate::types::BridgeTriageEntry::Invalid {
                                stable_key,
                                invalid_candidate: crate::types::BridgeInvalidCandidate::from_core(
                                    candidate,
                                ),
                            }
                        }
                    }
                })
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriageTab {
    fn from_core(tab: bae_core::import::TriageTab) -> Self {
        match tab {
            bae_core::import::TriageTab::Ready => Self::Ready,
            bae_core::import::TriageTab::NeedsYou => Self::NeedsYou,
            bae_core::import::TriageTab::Done => Self::Done,
            bae_core::import::TriageTab::Skipped => Self::Skipped,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeTriagePlacement {
    pub(crate) fn from_core(placement: bae_core::import::TriagePlacement) -> Self {
        use bae_core::import::TriagePlacement as P;
        match placement {
            P::Ready => Self::Ready,
            P::NeedsYou { group, reason } => Self::NeedsYou {
                group: crate::types::BridgeNeedsYouGroup::from_core(group),
                reason: crate::types::BridgeNeedsYouReason::from_core(reason),
            },
            P::Done => Self::Done,
            P::Skipped => Self::Skipped,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeNeedsYouGroup {
    pub(crate) fn from_core(group: bae_core::import::NeedsYouGroup) -> Self {
        use bae_core::import::NeedsYouGroup as G;
        match group {
            G::PickAPressing => Self::PickAPressing,
            G::SignalsDisagree => Self::SignalsDisagree,
            G::CountsOrLengthsDisagree => Self::CountsOrLengthsDisagree,
            G::AlreadyInLibrary => Self::AlreadyInLibrary,
            G::NoMatch => Self::NoMatch,
            G::StillIdentifying => Self::StillIdentifying,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeNeedsYouReason {
    pub(crate) fn from_core(reason: bae_core::import::NeedsYouReason) -> Self {
        use bae_core::import::NeedsYouReason as R;
        match reason {
            R::Disagreement(needs_you) => Self::Disagreement {
                disagreement: crate::types::BridgeNeedsYou::from_core(needs_you),
            },
            R::StillIdentifying { phase } => Self::StillIdentifying {
                phase: crate::types::BridgeIdentifyPhase::from_core(phase),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeIdentifyPhase {
    pub(crate) fn from_core(phase: bae_core::import::IdentifyPhase) -> Self {
        use bae_core::import::IdentifyPhase as P;
        match phase {
            P::Queued => Self::Queued,
            P::Running => Self::Running,
            P::NoAnswer => Self::NoAnswer,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeNeedsYou {
    pub(crate) fn from_core(needs_you: bae_core::identify::NeedsYou) -> Self {
        use bae_core::identify::NeedsYou as N;
        match needs_you {
            N::AlreadyInLibrary => Self::AlreadyInLibrary,
            N::SeveralMatches { count } => Self::SeveralMatches { count },
            N::SignalsConflict => Self::SignalsConflict,
            N::NoMatch => Self::NoMatch,
            N::NothingToLookUp => Self::NothingToLookUp,
            N::TrackCountDisagrees { local, source } => Self::TrackCountDisagrees { local, source },
            N::DurationsDisagree {
                probed_ms,
                source_ms,
                tolerance_ms,
            } => Self::DurationsDisagree {
                probed_ms,
                source_ms,
                tolerance_ms,
            },
            N::SourceLengthsUnknown => Self::SourceLengthsUnknown,
            N::LocalDurationUnknown => Self::LocalDurationUnknown,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeMatchedRelease {
    pub(crate) fn from_core(matched: bae_core::import::MatchedRelease) -> Self {
        let bae_core::import::MatchedRelease {
            release_id,
            title,
            artist,
            pressing,
            cover_thumbnail_url,
            evidence,
            claim,
        } = matched;
        let bae_core::import::MatchEvidence { source, signal } = evidence;
        crate::types::BridgeMatchedRelease {
            release_id,
            title,
            artist,
            pressing: pressing.map(|pressing| {
                let bae_core::import::MatchedPressing {
                    year,
                    format,
                    track_count,
                } = pressing;
                crate::types::BridgeMatchedPressing {
                    year,
                    format,
                    track_count,
                }
            }),
            cover_thumbnail_url,
            claim: crate::types::BridgeIdentityChoice::from_core(claim),
            evidence: crate::types::BridgeMatchEvidence {
                source: crate::types::BridgeMetadataSource::from_core(source),
                signal: signal.map(crate::types::BridgeMatchedSignal::from_core),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeMatchedSignal {
    pub(crate) fn from_core(signal: bae_core::import::MatchedSignal) -> Self {
        use bae_core::import::MatchedSignal as S;
        match signal {
            S::DiscId => Self::DiscId,
            S::Barcode => Self::Barcode,
        }
    }
}

impl crate::types::BridgeLoadingTrackInfo {
    fn from_core(t: bae_core::playback::LoadingTrack) -> Self {
        let bae_core::playback::LoadingTrack {
            track_info,
            duration_ms,
        } = t;
        let bae_core::playback::PlaybackTrackInfo {
            track_title,
            artist_names,
            album_id,
            album_title,
            cover_image,
            // The now-playing bar's loading state renders only the fields above;
            // identity/side detail belongs to the full Playing/Paused events.
            track_id: _,
            artist_id: _,
            release_id: _,
            side: _,
        } = track_info;
        crate::types::BridgeLoadingTrackInfo {
            track_title,
            artist_names,
            album_id,
            album_title,
            cover_image: cover_image.map(crate::types::BridgeImageRef::from_core),
            duration_ms,
        }
    }
}

/// Every `UiBusEvent` has a bridge mirror, so this always returns `Some`.
fn convert_ui_event(event: bae_core::ui::UiBusEvent) -> Option<crate::types::BridgeUiEvent> {
    use crate::types::*;
    use bae_core::ui::UiBusEvent;

    match event {
        UiBusEvent::Invalidated(invalidation) => Some(BridgeUiEvent::Invalidated {
            invalidation: BridgeInvalidation::from_core(invalidation),
        }),

        // ── Playback ───────────────────────────────────────────────
        UiBusEvent::PlaybackStopped => Some(BridgeUiEvent::PlaybackStopped),
        UiBusEvent::PlaybackError { reason } => Some(BridgeUiEvent::PlaybackError {
            reason: crate::types::BridgePlaybackErrorReason::from_core(reason),
        }),
        UiBusEvent::PlaybackLoading { track_id, track } => Some(BridgeUiEvent::PlaybackLoading {
            track_id,
            track: track.map(BridgeLoadingTrackInfo::from_core),
        }),
        UiBusEvent::PlaybackPlaying {
            track_id,
            track_title,
            artist_names,
            artist_id,
            album_id,
            album_title,
            cover_image,
            duration_ms,
        } => Some(BridgeUiEvent::PlaybackPlaying {
            track_id,
            track_title,
            artist_names,
            artist_id,
            album_id,
            album_title,
            cover_image: cover_image.map(BridgeImageRef::from_core),
            duration_ms,
        }),
        UiBusEvent::PlaybackPaused {
            track_id,
            track_title,
            artist_names,
            artist_id,
            album_id,
            album_title,
            cover_image,
            duration_ms,
            reason,
        } => Some(BridgeUiEvent::PlaybackPaused {
            track_id,
            track_title,
            artist_names,
            artist_id,
            album_id,
            album_title,
            cover_image: cover_image.map(BridgeImageRef::from_core),
            duration_ms,
            reason: BridgePlaybackPauseReason::from_core(reason),
        }),
        UiBusEvent::PlaybackProgress {
            track_id,
            position_ms,
            duration_ms,
            progress,
        } => Some(BridgeUiEvent::PlaybackProgress {
            track_id,
            position_ms,
            duration_ms,
            progress,
        }),
        UiBusEvent::PlaybackSeeked {
            track_id,
            position_ms,
            duration_ms,
            progress,
        } => Some(BridgeUiEvent::PlaybackSeeked {
            track_id,
            position_ms,
            duration_ms,
            progress,
        }),
        UiBusEvent::VolumeChanged { volume } => Some(BridgeUiEvent::VolumeChanged { volume }),
        UiBusEvent::MuteChanged { is_muted } => Some(BridgeUiEvent::MuteChanged { is_muted }),
        UiBusEvent::RepeatModeChanged { mode } => Some(BridgeUiEvent::RepeatModeChanged {
            mode: BridgeRepeatMode::from_core(mode),
        }),
        UiBusEvent::QueueUpdated(snapshot) => Some(BridgeUiEvent::QueueUpdated {
            snapshot: BridgeQueueSnapshot::from_core(snapshot),
        }),
        UiBusEvent::QueueItemsAdded { count } => Some(BridgeUiEvent::QueueItemsAdded { count }),

        // ── Preview ────────────────────────────────────────────────
        UiBusEvent::PreviewIdle => Some(BridgeUiEvent::PreviewIdle),
        UiBusEvent::PreviewPlaying { path, duration_ms } => {
            Some(BridgeUiEvent::PreviewPlaying { path, duration_ms })
        }
        UiBusEvent::PreviewPaused { path, duration_ms } => {
            Some(BridgeUiEvent::PreviewPaused { path, duration_ms })
        }
        UiBusEvent::PreviewProgress {
            position_ms,
            progress,
        } => Some(BridgeUiEvent::PreviewProgress {
            position_ms,
            progress,
        }),

        // ── Import live progress ───────────────────────────────────
        UiBusEvent::CandidateImportLoudnessProgress {
            key,
            tracks_done,
            tracks_total,
            fraction,
        } => Some(BridgeUiEvent::CandidateImportLoudnessProgress {
            key,
            tracks_done,
            tracks_total,
            fraction,
        }),
        UiBusEvent::ImportQueueIdentifyProgress { identified, total } => {
            Some(BridgeUiEvent::ImportQueueIdentifyProgress { identified, total })
        }

        // ── Release transfer ───────────────────────────────────────
        UiBusEvent::ReleaseTransferProgress { release_id, action } => {
            Some(BridgeUiEvent::ReleaseTransferProgress {
                release_id,
                action: crate::types::BridgeReleaseStorageAction::from_core(action),
            })
        }
        UiBusEvent::ReleaseTransferEnded { release_id } => {
            Some(BridgeUiEvent::ReleaseTransferEnded { release_id })
        }

        // ── Cast ───────────────────────────────────────────────────
        UiBusEvent::CastStatusChanged { device_name } => {
            Some(BridgeUiEvent::CastStatusChanged { device_name })
        }

        // ── Errors ─────────────────────────────────────────────────
        UiBusEvent::Error { error } => Some(BridgeUiEvent::Error {
            error: crate::types::BridgeError::from_core(error),
        }),
    }
}

impl BridgeFile {
    fn from_core(f: bae_core::album_detail::FileDetail) -> Self {
        let bae_core::album_detail::FileDetail {
            id,
            original_filename,
            file_size,
            is_image,
            content_type,
            audio_format,
        } = f;
        BridgeFile {
            id,
            original_filename,
            file_size,
            is_image,
            content_type,
            audio_format: audio_format.map(crate::types::BridgeAudioFormat::from_core),
        }
    }
}

impl BridgeGalleryItem {
    fn from_core(g: bae_core::album_detail::GalleryItem) -> Self {
        let bae_core::album_detail::GalleryItem { id, label, source } = g;
        BridgeGalleryItem {
            id,
            label,
            source: match source {
                bae_core::album_detail::GallerySource::Cover(image) => {
                    crate::types::BridgeGallerySource::Cover {
                        image: crate::types::BridgeImageRef::from_core(image),
                    }
                }
                bae_core::album_detail::GallerySource::ReleaseFile { file_id } => {
                    crate::types::BridgeGallerySource::ReleaseFile { file_id }
                }
            },
        }
    }
}

impl BridgeTrack {
    fn from_core(t: bae_core::album_detail::TrackDetail) -> Self {
        let bae_core::album_detail::TrackDetail {
            id,
            title,
            side,
            track_number,
            duration_ms,
            artist_names,
            display_artist,
            position_text,
            // Structured position drives core-side grouping (`track_groups`); the
            // UI renders `position_text` and the group headers, not this.
            position: _,
        } = t;
        BridgeTrack {
            id,
            title,
            side,
            track_number,
            duration_clock: crate::types::BridgeDurationClock::from_millis(duration_ms),
            duration_ms,
            artist_names,
            display_artist,
            position_text,
        }
    }
}

impl BridgeTrackGroup {
    fn from_core(g: bae_core::album_detail::TrackGroup) -> Self {
        let bae_core::album_detail::TrackGroup { side, tracks } = g;
        let side = crate::types::BridgeTrackSide::from_core(side);
        BridgeTrackGroup {
            header_key: side.header_key().map(str::to_string),
            side,
            tracks: tracks.into_iter().map(BridgeTrack::from_core).collect(),
        }
    }
}

impl BridgeRelease {
    fn from_core(rel: bae_core::album_detail::ReleaseDetail) -> Self {
        let bae_core::album_detail::ReleaseDetail {
            summary,
            display_name,
            year,
            label,
            catalog_number,
            country,
            total_duration_ms,
            tracks,
            track_groups,
            files,
            image_files,
            gallery_items,
        } = rel;
        // Single-source the summary-derived fields through BridgeReleaseSummary,
        // then exhaustively destructure it into BridgeRelease's flat fields so the
        // storage/cover projection lives in one place.
        let BridgeReleaseSummary {
            id,
            album_id,
            format,
            storage_state,
            pinned,
            storage_actions,
            transfer_action,
            file_count,
            total_size,
            cover,
        } = BridgeReleaseSummary::from_core(summary);
        BridgeRelease {
            id,
            album_id,
            display_name,
            year,
            format,
            label,
            catalog_number,
            country,
            storage_state,
            pinned,
            storage_actions,
            transfer_action,
            total_duration: bae_core::util::duration::DurationUnits::from_millis(total_duration_ms)
                .map(crate::types::BridgeDurationUnits::from_core),
            tracks: tracks.into_iter().map(BridgeTrack::from_core).collect(),
            track_groups: track_groups
                .into_iter()
                .map(BridgeTrackGroup::from_core)
                .collect(),
            image_files: image_files.into_iter().map(BridgeFile::from_core).collect(),
            files: files.into_iter().map(BridgeFile::from_core).collect(),
            gallery_items: gallery_items
                .into_iter()
                .map(BridgeGalleryItem::from_core)
                .collect(),
            file_count,
            total_size,
            cover,
        }
    }
}

impl BridgeAlbumSearchResult {
    fn from_core(a: bae_core::album_detail::AlbumSearchResult) -> Self {
        let bae_core::album_detail::AlbumSearchResult {
            id,
            title,
            year,
            artist_name,
            cover,
        } = a;
        BridgeAlbumSearchResult {
            id,
            title,
            year,
            artist_name,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeTrackSearchResult {
    fn from_core(t: bae_core::album_detail::TrackSearchResult) -> Self {
        let bae_core::album_detail::TrackSearchResult {
            id,
            title,
            duration_ms,
            album_id,
            album_title,
            artist_name,
            cover,
        } = t;
        BridgeTrackSearchResult {
            id,
            title,
            duration_clock: crate::types::BridgeDurationClock::from_millis(duration_ms),
            album_id,
            album_title,
            artist_name,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeStoragePage {
    fn from_core(page: bae_core::album_detail::StoragePage) -> Self {
        let bae_core::album_detail::StoragePage { rows, total_count } = page;
        BridgeStoragePage {
            rows: rows.into_iter().map(BridgeStorageRow::from_core).collect(),
            total_count,
        }
    }
}

#[cfg(feature = "desktop")]
impl crate::types::BridgeCandidateSearchResults {
    /// Echoes back the `tab` and `source` the search ran against so the caller
    /// routes results into the matching slot even if the user changed either
    /// during the await.
    fn from_core(
        grouped: bae_core::import::GroupedSearchResults,
        tab: crate::types::BridgeSearchQueryKind,
        source: crate::types::BridgeMetadataSource,
    ) -> Self {
        let bae_core::import::GroupedSearchResults { groups, statuses } = grouped;
        crate::types::BridgeCandidateSearchResults {
            tab,
            source,
            groups: groups
                .into_iter()
                .map(crate::types::BridgeReleaseGroup::from_core)
                .collect(),
            statuses: statuses
                .into_iter()
                .map(crate::types::BridgeLibraryStatus::from_core)
                .collect(),
        }
    }
}

impl BridgeStorageRow {
    fn from_core(raw: bae_core::album_detail::StorageRow) -> Self {
        let bae_core::album_detail::StorageRow { release, album } = raw;
        BridgeStorageRow {
            release: BridgeReleaseSummary::from_core(release),
            album: BridgeAlbum::from_core(album),
        }
    }
}

impl BridgeReleaseSummary {
    fn from_core(s: bae_core::album_detail::ReleaseSummary) -> Self {
        let bae_core::album_detail::ReleaseSummary {
            id,
            album_id,
            format,
            storage_state,
            pinned,
            storage_actions,
            transfer_action,
            file_count,
            total_size,
            cover,
        } = s;
        BridgeReleaseSummary {
            id,
            album_id,
            format,
            storage_state: crate::types::BridgeReleaseStorageState::from_core(storage_state),
            pinned,
            storage_actions: storage_actions
                .into_iter()
                .map(crate::types::BridgeReleaseStorageAction::from_core)
                .collect(),
            transfer_action: transfer_action
                .map(crate::types::BridgeReleaseStorageAction::from_core),
            file_count,
            total_size,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeAlbumDetail {
    fn from_core(detail: bae_core::album_detail::AlbumDetail) -> Self {
        let bae_core::album_detail::AlbumDetail {
            album,
            artist_names,
            releases,
            primary_release_id,
            cover,
        } = detail;
        let release_ids: Vec<String> = releases.iter().map(|r| r.summary.id.clone()).collect();
        let bae_core::db::DbAlbum {
            id,
            title,
            year,
            is_compilation,
            // The album's own artist FK / created_at aren't surfaced, and the
            // primary release comes from the detail-level field below.
            artist_id: _,
            primary_release_id: _,
            created_at: _,
        } = album;
        // Reassemble the album's slim summary from the detail's DbAlbum plus the
        // detail-level artist_names/primary_release_id/cover, then reuse
        // BridgeAlbum::from_core so the BridgeAlbum projection lives in one place.
        let album = BridgeAlbum::from_core(bae_core::album_detail::AlbumSummary {
            id,
            title,
            year,
            is_compilation,
            artist_names,
            release_ids,
            primary_release_id,
            cover,
        });
        BridgeAlbumDetail {
            album,
            releases: releases.into_iter().map(BridgeRelease::from_core).collect(),
        }
    }
}

impl BridgeAlbum {
    fn from_core(a: bae_core::album_detail::AlbumSummary) -> Self {
        let bae_core::album_detail::AlbumSummary {
            id,
            title,
            year,
            is_compilation,
            artist_names,
            release_ids,
            primary_release_id,
            cover,
        } = a;
        BridgeAlbum {
            id,
            title,
            year,
            is_compilation,
            artist_names,
            release_ids,
            primary_release_id,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeComposerSummary {
    fn from_core(s: bae_core::album_detail::ComposerSummary) -> Self {
        let bae_core::album_detail::ComposerSummary { raw, image } = s;
        let bae_core::db::DbComposerSummary {
            artist,
            work_count,
            linked_release_count,
            unlinked_credit_count,
        } = raw;
        let bae_core::db::DbArtist {
            id: artist_id,
            name,
            sort_name,
            // Per-source dedup ids and the row timestamp don't cross to the UI.
            discogs_artist_id: _,
            musicbrainz_artist_id: _,
            created_at: _,
        } = artist;
        BridgeComposerSummary {
            artist_id,
            name,
            sort_name,
            work_count,
            linked_release_count,
            unlinked_credit_count,
            image: image.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeArtistSummary {
    fn from_core(s: bae_core::album_detail::ArtistSummary) -> Self {
        let bae_core::album_detail::ArtistSummary { raw, image } = s;
        let bae_core::db::DbArtistSummary {
            artist,
            album_count,
        } = raw;
        let bae_core::db::DbArtist {
            id: artist_id,
            name,
            // The MB-only sort name, per-source dedup ids, and the row
            // timestamp don't cross to the UI.
            sort_name: _,
            discogs_artist_id: _,
            musicbrainz_artist_id: _,
            created_at: _,
        } = artist;
        BridgeArtistSummary {
            artist_id,
            name,
            album_count,
            image: image.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeArtistDetail {
    fn from_core(d: bae_core::album_detail::ArtistDetail) -> Self {
        let bae_core::album_detail::ArtistDetail { artist, albums } = d;
        BridgeArtistDetail {
            artist: BridgeArtistSummary::from_core(artist),
            albums: albums.into_iter().map(BridgeAlbum::from_core).collect(),
        }
    }
}

impl BridgeWorkSummary {
    fn from_core(s: bae_core::album_detail::WorkSummary) -> Self {
        let bae_core::album_detail::WorkSummary {
            raw,
            representative_cover,
        } = s;
        let bae_core::db::DbWorkSummary {
            work,
            parent_work_id,
            representative_release_id,
            composer_names,
            linked_release_count,
        } = raw;
        let bae_core::db::DbWork {
            id: work_id,
            title,
            disambiguation,
            work_type,
            // The source id the row was deduped on isn't surfaced.
            musicbrainz_work_id: _,
            // Row timestamp isn't surfaced.
            created_at: _,
        } = work;
        BridgeWorkSummary {
            work_id,
            title,
            disambiguation,
            work_type,
            parent_work_id,
            composer_names,
            linked_release_count,
            representative_release_id,
            representative_cover: representative_cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeReleaseRoleSummary {
    fn from_core(s: bae_core::album_detail::ReleaseRoleSummary) -> Self {
        let bae_core::db::DbReleaseRoleSummary { role, album } = s;
        let bae_core::db::DbReleaseArtistRole {
            release_id,
            source,
            source_credit,
            id: _,
            artist_id: _,
            position: _,
            created_at: _,
        } = role;
        let bae_core::db::DbAlbum {
            id: album_id,
            title: album_title,
            artist_id: _,
            year: _,
            primary_release_id: _,
            is_compilation: _,
            created_at: _,
        } = album;
        BridgeReleaseRoleSummary {
            release_id,
            album_id,
            album_title,
            source: BridgeMetadataSource::from_core(source),
            source_credit,
        }
    }
}

impl BridgeTrackRoleSummary {
    fn from_core(s: bae_core::album_detail::TrackRoleSummary) -> Self {
        let bae_core::db::DbTrackRoleSummary {
            role,
            track,
            album,
            artist,
        } = s;
        let bae_core::db::DbTrackArtistRole {
            track_id,
            artist_id,
            source,
            source_credit,
            id: _,
            position: _,
            created_at: _,
        } = role;
        let bae_core::db::DbTrack {
            title: track_title,
            release_id,
            id: _,
            side: _,
            track_number: _,
            duration_ms: _,
            discogs_position: _,
            created_at: _,
        } = track;
        let bae_core::db::DbAlbum {
            id: album_id,
            title: album_title,
            artist_id: _,
            year: _,
            primary_release_id: _,
            is_compilation: _,
            created_at: _,
        } = album;
        let bae_core::db::DbArtist {
            name: artist_name,
            id: _,
            sort_name: _,
            discogs_artist_id: _,
            musicbrainz_artist_id: _,
            created_at: _,
        } = artist;
        BridgeTrackRoleSummary {
            track_id,
            track_title,
            release_id,
            album_id,
            album_title,
            artist_id,
            artist_name,
            source: BridgeMetadataSource::from_core(source),
            source_credit,
        }
    }
}

impl BridgeWorkTrackSummary {
    fn from_core(s: bae_core::album_detail::WorkTrackSummary) -> Self {
        let bae_core::db::DbWorkTrackSummary { link, track, album } = s;
        let bae_core::db::DbTrackWork {
            track_id,
            id: _,
            work_id: _,
            position: _,
            source: _,
            created_at: _,
        } = link;
        let bae_core::db::DbTrack {
            title: track_title,
            release_id,
            id: _,
            side: _,
            track_number: _,
            duration_ms: _,
            discogs_position: _,
            created_at: _,
        } = track;
        let bae_core::db::DbAlbum {
            id: album_id,
            title: album_title,
            artist_id: _,
            year: _,
            primary_release_id: _,
            is_compilation: _,
            created_at: _,
        } = album;
        BridgeWorkTrackSummary {
            track_id,
            track_title,
            release_id,
            album_id,
            album_title,
        }
    }
}

impl BridgeWorkReleaseSummary {
    fn from_core(s: bae_core::album_detail::WorkReleaseSummary) -> Self {
        let bae_core::album_detail::WorkReleaseSummary {
            release_id,
            album_id,
            album_title,
            display_name,
            format,
            cover,
        } = s;
        BridgeWorkReleaseSummary {
            release_id,
            album_id,
            album_title,
            display_name,
            format,
            cover: cover.map(crate::types::BridgeImageRef::from_core),
        }
    }
}

impl BridgeComposerWorkGroup {
    fn from_core(group: bae_core::album_detail::ComposerWorkGroup) -> Self {
        let bae_core::album_detail::ComposerWorkGroup { id, parent, works } = group;
        BridgeComposerWorkGroup {
            id,
            parent: parent.map(BridgeWorkSummary::from_core),
            works: works
                .into_iter()
                .map(BridgeWorkSummary::from_core)
                .collect(),
        }
    }
}

impl BridgeComposerDetail {
    fn from_core(d: bae_core::album_detail::ComposerDetail) -> Self {
        let bae_core::album_detail::ComposerDetail {
            composer,
            work_groups,
            unlinked_release_roles,
            unlinked_track_roles,
            default_work_id,
        } = d;
        BridgeComposerDetail {
            composer: BridgeComposerSummary::from_core(composer),
            work_groups: work_groups
                .into_iter()
                .map(BridgeComposerWorkGroup::from_core)
                .collect(),
            unlinked_release_roles: unlinked_release_roles
                .into_iter()
                .map(BridgeReleaseRoleSummary::from_core)
                .collect(),
            unlinked_track_roles: unlinked_track_roles
                .into_iter()
                .map(BridgeTrackRoleSummary::from_core)
                .collect(),
            default_work_id,
        }
    }
}

impl BridgeWorkDetail {
    fn from_core(d: bae_core::album_detail::WorkDetail) -> Self {
        let bae_core::album_detail::WorkDetail {
            work,
            child_works,
            releases,
            tracks,
        } = d;
        BridgeWorkDetail {
            work: BridgeWorkSummary::from_core(work),
            child_works: child_works
                .into_iter()
                .map(BridgeWorkSummary::from_core)
                .collect(),
            releases: releases
                .into_iter()
                .map(BridgeWorkReleaseSummary::from_core)
                .collect(),
            tracks: tracks
                .into_iter()
                .map(BridgeWorkTrackSummary::from_core)
                .collect(),
        }
    }
}

/// Normalize + validate the editor's raw form into a wire edit. `Valid`
/// carries the savable commit payload; `Invalid` carries the reason the
/// editor disables Save. The editor calls this on every change (to gate Save
/// and show the reason) and on commit (to read the payload it passes to
/// `update_release_metadata_user_edit` / `start_import`). Stateless
/// type-translation wrapper around [`bae_core::import::RawReleaseEdit::shape`].
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn shape_release_edit(
    raw: crate::types::BridgeRawReleaseEdit,
) -> crate::types::BridgeShapeResult {
    let core_raw = raw.into_core();
    match core_raw.shape() {
        Ok(edit) => crate::types::BridgeShapeResult::Valid {
            edit: crate::types::BridgeReleaseUserEdit::from_core(edit),
        },
        Err(e) => crate::types::BridgeShapeResult::Invalid {
            reason: crate::types::BridgeValidationReason::from_core(e),
        },
    }
}

/// Map bae-core's validation error to its bridge mirror. Kept here, not as a
/// `From` in bae-core, so bae-core stays unaware of bridge types.
#[cfg(feature = "desktop")]
impl crate::types::BridgeValidationReason {
    fn from_core(e: bae_core::import::EditValidationError) -> Self {
        use crate::types::BridgeValidationReason as R;
        use bae_core::import::EditValidationError as E;
        match e {
            E::EmptyAlbumTitle => R::EmptyAlbumTitle,
            E::NoAlbumArtist => R::NoAlbumArtist,
            E::InvalidYear => R::InvalidYear,
        }
    }
}

/// The localization key for a validation reason, resolved by the UI against the
/// generated `Core` string table. One exported mapping keeps every platform's
/// keys identical.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_validation_reason_key(reason: crate::types::BridgeValidationReason) -> String {
    reason.loc_key().to_string()
}

/// Seed the editor's raw form from a wire edit — the inverse of
/// `shape_release_edit`: joins artist lists into comma text and renders absent
/// pressing fields as empty. `track_id_prefix` supplies the editor row
/// identities the wire edit lacks. Stateless type translation around
/// [`bae_core::import::RawReleaseEdit::from_user_edit`]. Used by reset-to-source
/// to repopulate the form from the projected edit.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn raw_release_edit_from_user_edit(
    edit: crate::types::BridgeReleaseUserEdit,
    track_id_prefix: String,
) -> crate::types::BridgeRawReleaseEdit {
    let core_edit = edit.into_core();
    let raw = bae_core::import::RawReleaseEdit::from_user_edit(core_edit, &track_id_prefix);
    crate::types::BridgeRawReleaseEdit::from_core(raw)
}

#[cfg(test)]
mod tests {
    use bae_core::album_detail::{
        ComposerDetail, ComposerSummary, ComposerWorkGroup, WorkDetail, WorkReleaseSummary,
        WorkSummary,
    };
    use bae_core::db::{DbArtist, DbComposerSummary, DbWork, DbWorkSummary};
    #[cfg(not(feature = "desktop"))]
    use bae_core::keys::BaeStoreKeysExt;
    use std::sync::{Arc, Mutex};

    /// Records every delivered event so tests can assert on the stream.
    struct CollectingCallback {
        events: Arc<Mutex<Vec<crate::types::BridgeUiEvent>>>,
    }

    impl crate::types::UiEventCallback for CollectingCallback {
        fn on_event(&self, event: crate::types::BridgeUiEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn track_detail(
        position: bae_core::album_detail::TrackPosition,
        duration_ms: Option<i64>,
    ) -> bae_core::album_detail::TrackDetail {
        bae_core::album_detail::TrackDetail {
            id: "track-1".to_string(),
            title: "Track".to_string(),
            side: 1,
            track_number: Some(1),
            duration_ms,
            artist_names: "Artist".to_string(),
            display_artist: None,
            position_text: "1".to_string(),
            position,
        }
    }

    #[test]
    fn track_group_precomputes_header_key_per_side() {
        use bae_core::album_detail::{TrackPosition, TrackSide};

        let cases = [
            (
                TrackSide::Sided {
                    side_letter: "A".to_string(),
                },
                TrackPosition::Sided {
                    side_letter: "A".to_string(),
                    number: 1,
                },
                Some("core.track.side"),
            ),
            (
                TrackSide::Disc { disc: 2 },
                TrackPosition::Disc { disc: 2, number: 1 },
                Some("core.track.disc"),
            ),
            (TrackSide::Flat, TrackPosition::Flat { number: 1 }, None),
        ];

        for (side, position, expected) in cases {
            let group =
                crate::types::BridgeTrackGroup::from_core(bae_core::album_detail::TrackGroup {
                    side,
                    tracks: vec![track_detail(position, Some(187_000))],
                });
            assert_eq!(group.header_key.as_deref(), expected);
        }
    }

    #[test]
    fn track_precomputes_duration_clock() {
        use bae_core::album_detail::TrackPosition;

        // A present duration renders a clock while the raw number is retained.
        let present = crate::types::BridgeTrack::from_core(track_detail(
            TrackPosition::Flat { number: 1 },
            Some(187_000),
        ));
        assert_eq!(present.duration_ms, Some(187_000));
        let clock = present
            .duration_clock
            .expect("clock for a present duration");
        assert_eq!((clock.minutes, clock.seconds), (3, 7));

        // An absent duration has nothing to label.
        let absent = crate::types::BridgeTrack::from_core(track_detail(
            TrackPosition::Flat { number: 1 },
            None,
        ));
        assert_eq!(absent.duration_ms, None);
        assert!(absent.duration_clock.is_none());
    }

    fn track_search_result(duration_ms: Option<i64>) -> bae_core::album_detail::TrackSearchResult {
        bae_core::album_detail::TrackSearchResult {
            id: "track-1".to_string(),
            title: "Track".to_string(),
            duration_ms,
            album_id: "album-1".to_string(),
            album_title: "Album".to_string(),
            artist_name: "Artist".to_string(),
            cover: None,
        }
    }

    #[test]
    fn track_search_result_precomputes_duration_clock() {
        // A present duration renders a clock; the raw number does not cross.
        let present =
            crate::types::BridgeTrackSearchResult::from_core(track_search_result(Some(187_000)));
        let clock = present
            .duration_clock
            .expect("clock for a present duration");
        assert_eq!((clock.minutes, clock.seconds), (3, 7));

        // An absent duration has nothing to label.
        let absent = crate::types::BridgeTrackSearchResult::from_core(track_search_result(None));
        assert!(absent.duration_clock.is_none());
    }

    fn queue_item(duration_ms: Option<i64>) -> bae_core::queue::QueueItem {
        bae_core::queue::QueueItem {
            entry_id: "entry-1".to_string(),
            track_id: "track-1".to_string(),
            title: "Track".to_string(),
            artist_names: "Artist".to_string(),
            duration_ms,
            album_title: "Album".to_string(),
            cover_image: None,
        }
    }

    #[test]
    fn queue_entry_precomputes_duration_clock() {
        // A present duration renders a clock; the raw number does not cross.
        let present = crate::types::BridgeQueueEntry::from_core(queue_item(Some(187_000)));
        let clock = present
            .duration_clock
            .expect("clock for a present duration");
        assert_eq!((clock.minutes, clock.seconds), (3, 7));

        // An absent duration has nothing to label.
        let absent = crate::types::BridgeQueueEntry::from_core(queue_item(None));
        assert!(absent.duration_clock.is_none());
    }

    #[cfg(not(feature = "desktop"))]
    fn fresh_bridge_handle(test_name: &str) -> (super::AppHandle, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("bae-bridge-{test_name}"));
        match std::fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("remove stale test library dir: {error}"),
        }
        let library_dir = coven::StoreDir::new(root.join("library"));
        std::fs::create_dir_all(&*library_dir).expect("create test library dir");
        bae_core::config::install_test_keyring();

        let library_id = format!("test-{test_name}");
        let config = bae_core::config::Config::with_defaults(
            library_id.clone(),
            format!("device-{test_name}"),
            library_dir.clone(),
            "Test Library".to_string(),
        );
        let key_service = bae_core::keys::StoreKeys::bind(library_id.clone());
        key_service
            .set_discogs_key("test-discogs-token")
            .expect("seed test Discogs key");
        // One id source for the whole test app: the database mints the ids it
        // owns (identity rows, copied album artists) from the same provider the
        // manager mints everything else from.
        let ids: coven::IdRef = Arc::new(coven::SequentialIdProvider::new(test_name));
        let database = bae_core::db::Database::open(
            config.to_coven(),
            Arc::new(coven::SystemClock),
            Arc::clone(&ids),
            bae_core::sync::synced_tables(),
            None,
        )
        .expect("open test database");
        let runtime = tokio::runtime::Runtime::new().expect("create test runtime");
        let config_handle = Arc::new(bae_core::config::ConfigHandle::new(config));
        let manager = bae_core::library::LibraryManager::new(
            database,
            config_handle,
            key_service,
            Arc::new(coven::SystemClock),
            ids,
            bae_core::diagnostics::Diagnostics::noop(),
            runtime.handle().clone(),
            bae_core::import::cover_art::CoverArtArchiveClient::hermetic(),
        );
        let playback = bae_core::playback::PlaybackService::start(
            manager.clone(),
            runtime.handle().clone(),
            50,
            true,
        );
        // Gated on the target, not on `desktop`: `AppServices::new`'s arity is
        // itself target-gated in bae-core, so a non-mobile build takes the
        // five-argument form whether or not this crate's `desktop` feature is on.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let services = {
            let import = runtime
                .block_on(bae_core::import::ImportService::start(
                    runtime.handle().clone(),
                    manager.clone(),
                ))
                .expect("start import service");
            let identify = bae_core::identify::IdentifyServiceHandle::new(
                manager.clone(),
                runtime.handle().clone(),
                import.event_sender_for_test(),
            );
            let extraction = bae_core::signals::ExtractionService::start(
                runtime.handle().clone(),
                import.event_sender_for_test(),
                manager.clone(),
            );
            bae_core::library::AppServices::new(manager, playback, import, identify, extraction)
        };
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let services = bae_core::library::AppServices::new(manager, playback);
        let handle = super::AppHandle {
            runtime,
            services,
            ui_event_bus: bae_core::ui::UiEventBus::new(),
        };

        (handle, root)
    }

    #[cfg(not(feature = "desktop"))]
    #[test]
    fn enqueue_export_missing_release_does_not_panic() {
        let (handle, root) = fresh_bridge_handle("enqueue-export-missing-release");
        let result = handle.runtime.block_on(async {
            handle
                .enqueue_export(
                    "missing-release".to_string(),
                    root.join("exports").to_string_lossy().into_owned(),
                )
                .await
        });

        assert!(matches!(
            result,
            Err(crate::types::BridgeError::Diagnostic {
                category: crate::types::BridgeErrorCategory::Export,
                ..
            })
        ));
    }

    /// A consumer that falls behind the broadcast bus gets `Lagged`, not
    /// `Closed`. The dropped events can't be replayed, so the pump synthesizes
    /// every coarse invalidation (the UI re-reads each domain instead of
    /// freezing on the last delivered snapshot) and keeps delivering the
    /// events behind the gap.
    #[tokio::test]
    async fn pump_ui_events_recovers_from_broadcast_lag_by_invalidating() {
        let (tx, rx) = tokio::sync::broadcast::channel(1);

        // Two sends into a capacity-1 channel before the pump runs: the first
        // is overwritten, so the pump's first recv returns Lagged with the
        // second event still queued behind it.
        tx.send(bae_core::ui::UiBusEvent::PlaybackStopped).unwrap();
        tx.send(bae_core::ui::UiBusEvent::MuteChanged { is_muted: true })
            .unwrap();
        drop(tx);

        let events = Arc::new(Mutex::new(Vec::new()));
        let pump = tokio::spawn(super::pump_ui_events(
            rx,
            Box::new(CollectingCallback {
                events: events.clone(),
            }),
        ));
        pump.await.unwrap();

        let events = events.lock().unwrap();
        let (last, recovery) = events.split_last().expect("events delivered");
        assert!(
            matches!(
                last,
                crate::types::BridgeUiEvent::MuteChanged { is_muted: true }
            ),
            "expected the event behind the lag to still be delivered, got: {events:?}",
        );
        // Every recovery event is an invalidation, and the set covers the
        // snapshot-backed domains — outbox included, so a lagged-away final
        // OutboxChanged can't leave the queue pane frozen.
        let invalidations: Vec<_> = recovery
            .iter()
            .map(|event| match event {
                crate::types::BridgeUiEvent::Invalidated { invalidation } => invalidation,
                other => {
                    panic!("expected only invalidations before the queued event, got {other:?}")
                }
            })
            .collect();
        for expected in [
            crate::types::BridgeInvalidation::AlbumList,
            crate::types::BridgeInvalidation::Outbox,
            crate::types::BridgeInvalidation::DownloadQueue,
            crate::types::BridgeInvalidation::OutputQueue,
            crate::types::BridgeInvalidation::SyncStatus,
            crate::types::BridgeInvalidation::Queue,
            crate::types::BridgeInvalidation::Config,
        ] {
            assert!(
                invalidations.iter().any(|i| **i == expected),
                "lag recovery missing {expected:?}: {invalidations:?}",
            );
        }
    }

    /// The per-file converter flattens core's `UploadState` into plain fields:
    /// live bytes ride `Uploading`, `Done` reads as fully transferred, and the
    /// failure message lands in `last_error` under `Retrying`.
    #[test]
    fn upload_file_op_flattens_state_into_fields() {
        use crate::types::BridgeUploadFileState;
        use bae_core::library::{UploadFileOp, UploadState};

        let convert = |state: UploadState| {
            crate::types::BridgeUploadFileOp::from_core(UploadFileOp {
                file_id: "file-1".into(),
                display_name: "01 Track Title.flac".into(),
                bytes_total: 1000,
                state,
            })
        };

        let queued = convert(UploadState::Queued);
        assert_eq!(queued.state, BridgeUploadFileState::Queued);
        assert_eq!((queued.bytes_done, queued.last_error), (0, None));

        let active = convert(UploadState::Active { bytes_done: 400 });
        assert_eq!(active.state, BridgeUploadFileState::Uploading);
        assert_eq!(active.bytes_done, 400);

        let failed = convert(UploadState::Failed {
            last_error: "cloud write failed".into(),
        });
        assert_eq!(failed.state, BridgeUploadFileState::Retrying);
        assert_eq!(failed.last_error.as_deref(), Some("cloud write failed"));

        let done = convert(UploadState::Done);
        assert_eq!(done.state, BridgeUploadFileState::Done);
        assert_eq!(done.bytes_done, 1000);
    }

    #[test]
    fn convert_ui_event_preserves_invalidation_key() {
        let event = super::convert_ui_event(bae_core::ui::UiBusEvent::Invalidated(
            bae_core::ui::Invalidation::Release {
                release_id: "rel-1".to_string(),
            },
        ))
        .expect("invalidation event maps to a bridge event");

        assert!(
            matches!(
                event,
                crate::types::BridgeUiEvent::Invalidated {
                    invalidation: crate::types::BridgeInvalidation::Release { ref release_id },
                } if release_id == "rel-1"
            ),
            "expected release invalidation, got {event:?}",
        );
    }

    #[test]
    fn convert_ui_event_preserves_seeked_position_kind() {
        let event = super::convert_ui_event(bae_core::ui::UiBusEvent::PlaybackSeeked {
            track_id: "track-1".to_string(),
            position_ms: 75_000,
            duration_ms: 100_000,
            progress: 0.75,
        })
        .expect("seeked event maps to a bridge event");

        assert!(
            matches!(
                event,
                crate::types::BridgeUiEvent::PlaybackSeeked {
                    ref track_id,
                    position_ms: 75_000,
                    duration_ms: 100_000,
                    progress,
                } if track_id == "track-1" && progress == 0.75
            ),
            "expected PlaybackSeeked, got {event:?}",
        );
    }

    #[test]
    fn composer_detail_conversion_preserves_work_groups() {
        let created_at = "2026-01-01T00:00:00Z".parse().unwrap();
        let detail = ComposerDetail {
            composer: ComposerSummary {
                raw: DbComposerSummary {
                    artist: DbArtist {
                        id: "artist-composer-a".to_string(),
                        name: "Composer Name A".to_string(),
                        sort_name: Some("Composer Name Sort A".to_string()),
                        discogs_artist_id: None,
                        musicbrainz_artist_id: None,
                        created_at,
                    },
                    work_count: 2,
                    linked_release_count: 1,
                    unlinked_credit_count: 0,
                },
                image: None,
            },
            work_groups: vec![ComposerWorkGroup {
                id: "work-parent-a".to_string(),
                parent: Some(WorkSummary {
                    raw: DbWorkSummary {
                        work: DbWork {
                            id: "work-parent-a".to_string(),
                            title: "Parent Work A".to_string(),
                            disambiguation: None,
                            work_type: Some("work".to_string()),
                            musicbrainz_work_id: "mb-work-parent-a".to_string(),
                            created_at,
                        },
                        parent_work_id: None,
                        composer_names: Some("Composer Name A".to_string()),
                        linked_release_count: 1,
                        representative_release_id: Some("release-a".to_string()),
                    },
                    representative_cover: None,
                }),
                works: vec![WorkSummary {
                    raw: DbWorkSummary {
                        work: DbWork {
                            id: "work-child-a".to_string(),
                            title: "Child Work A".to_string(),
                            disambiguation: None,
                            work_type: Some("part".to_string()),
                            musicbrainz_work_id: "mb-work-child-a".to_string(),
                            created_at,
                        },
                        parent_work_id: Some("work-parent-a".to_string()),
                        composer_names: Some("Composer Name A".to_string()),
                        linked_release_count: 1,
                        representative_release_id: Some("release-a".to_string()),
                    },
                    representative_cover: None,
                }],
            }],
            unlinked_release_roles: Vec::new(),
            unlinked_track_roles: Vec::new(),
            default_work_id: Some("work-parent-a".to_string()),
        };

        let bridge = super::BridgeComposerDetail::from_core(detail);

        assert_eq!(bridge.work_groups.len(), 1);
        assert_eq!(bridge.default_work_id.as_deref(), Some("work-parent-a"));
        let group = &bridge.work_groups[0];
        assert_eq!(group.id, "work-parent-a");
        assert_eq!(
            group.parent.as_ref().map(|work| work.work_id.as_str()),
            Some("work-parent-a")
        );
        assert_eq!(group.works.len(), 1);
        assert_eq!(group.works[0].work_id, "work-child-a");
        assert_eq!(
            group.works[0].parent_work_id.as_deref(),
            Some("work-parent-a")
        );
        assert_eq!(
            group.works[0].representative_release_id.as_deref(),
            Some("release-a")
        );
    }

    #[test]
    fn work_detail_conversion_preserves_work_release_rows() {
        let created_at = "2026-01-01T00:00:00Z".parse().unwrap();
        let work = WorkSummary {
            raw: DbWorkSummary {
                work: DbWork {
                    id: "work-a".to_string(),
                    title: "Work Title A".to_string(),
                    disambiguation: None,
                    work_type: Some("work".to_string()),
                    musicbrainz_work_id: "mb-work-a".to_string(),
                    created_at,
                },
                parent_work_id: None,
                composer_names: Some("Composer Name A".to_string()),
                linked_release_count: 1,
                representative_release_id: Some("release-a".to_string()),
            },
            representative_cover: None,
        };
        let detail = WorkDetail {
            work,
            child_works: Vec::new(),
            releases: vec![WorkReleaseSummary {
                release_id: "release-a".to_string(),
                album_id: "album-a".to_string(),
                album_title: "Album Title A".to_string(),
                display_name: "2026 CD".to_string(),
                format: Some("CD".to_string()),
                cover: None,
            }],
            tracks: Vec::new(),
        };

        let bridge = super::BridgeWorkDetail::from_core(detail);

        assert_eq!(bridge.releases.len(), 1);
        let release = &bridge.releases[0];
        assert_eq!(release.release_id, "release-a");
        assert_eq!(release.album_id, "album-a");
        assert_eq!(release.album_title, "Album Title A");
        assert_eq!(release.display_name, "2026 CD");
        assert_eq!(release.format.as_deref(), Some("CD"));
    }
}
