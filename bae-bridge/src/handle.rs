use std::future::Future;

use bae_core::library::AppServices;
#[cfg(feature = "desktop")]
use bae_core::signals::ExtractionSource;
use tracing::info;

use crate::bridge_utils::build_bridge_config;
#[cfg(feature = "desktop")]
use crate::types::BridgeDiscogsSaveOutcome;
#[cfg(feature = "desktop")]
use crate::types::BridgeMetadataSource;
#[cfg(feature = "desktop")]
use crate::types::BridgeRemoteCover;
#[cfg(feature = "oauth-providers")]
use crate::types::{bridge_cloud_provider_to_core, BridgeCloudProvider};
// `BridgeHomeStorage` is named only in the OAuth and CloudKit connect signatures;
// `bridge_home_storage_to_core` is also used by the always-present S3 path below,
// so it is imported unconditionally.
#[cfg(any(feature = "oauth-providers", feature = "cloudkit"))]
use crate::types::BridgeHomeStorage;
use crate::types::{
    bridge_home_storage_to_core, bridge_sort_to_core, BridgeAlbum, BridgeAlbumDetail,
    BridgeAlbumSearchResult, BridgeConfig, BridgeCoverSelection, BridgeError, BridgeFile,
    BridgeGalleryItem, BridgeRelease, BridgeReleaseSummary, BridgeRepeatMode, BridgeSaveSyncConfig,
    BridgeSearchResults, BridgeSortCriterion, BridgeStorageFilter, BridgeStoragePage,
    BridgeStorageRow, BridgeStorageSort, BridgeTrack, BridgeTrackGroup, BridgeTrackSearchResult,
};
#[cfg(feature = "desktop")]
use crate::types::{bridge_storage_mode_to_core, BridgeStorageMode};

#[derive(uniffi::Object)]
pub struct AppHandle {
    pub(crate) runtime: tokio::runtime::Runtime,
    pub(crate) app_services: AppServices,
    pub(crate) ui_event_bus: bae_core::ui::UiEventBus,
}

impl AppHandle {
    /// Run `fut` on a worker thread of the bridge runtime and resolve to its
    /// output, aborting the work if this call is cancelled.
    ///
    /// Why this exists — the stack: uniffi exports an `async fn` by polling its
    /// future *inline on whatever foreign thread drives it*. Swift runs a
    /// `nonisolated async` call on its cooperative pool, whose threads have
    /// ~0.5 MB stacks. The AWS-SDK S3 future chain these cloud/membership ops
    /// reach descends far deeper than that *synchronously* in one poll — endpoint
    /// and auth-scheme resolution, uncollapsed in debug builds — so polling it on
    /// the foreign thread overflows — the "Thread stack size exceeded" SIGBUS we
    /// hit at startup once sync was configured. `Runtime::block_on` has the same
    /// flaw: it drives the root future on the *caller*, not a worker. `spawn`
    /// moves the future onto a runtime worker instead — those have 16 MB stacks
    /// (sized in `init` for exactly these deep futures) — and the foreign thread
    /// only polls the shallow `JoinHandle`. The deep descent never touches the
    /// Swift stack.
    ///
    /// Why this exists — cancellation: our uniffi fork drops the inflight Rust
    /// future when the Swift `Task` is cancelled. A dropped `JoinHandle` merely
    /// *detaches* its task (tokio runs it to completion), which would leak the
    /// work and skip its teardown, so the guard aborts the task on drop. `coven`'s
    /// own drop guards (AbortOnDrop, connection release) then fire on the worker.
    async fn spawn_on_runtime<T: Send + 'static>(
        &self,
        fut: impl Future<Output = T> + Send + 'static,
    ) -> T {
        let handle = self.runtime.spawn(fut);
        struct AbortOnDrop(tokio::task::AbortHandle);
        impl Drop for AbortOnDrop {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _abort_on_drop = AbortOnDrop(handle.abort_handle());
        handle.await.expect("bridge runtime task panicked")
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    // =========================================================================
    // Library
    // =========================================================================

    pub fn get_album_page(
        &self,
        sort_criteria: Vec<BridgeSortCriterion>,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BridgeAlbum>, BridgeError> {
        // Local SQLite read — shallow and instant, so it polls inline on the
        // caller via block_on. Only the deep, network-bound cloud/membership
        // calls need spawn_on_runtime; these reads can't descend deep enough to
        // overflow the caller stack. Staying synchronous also keeps them usable
        // from the synchronous SwiftUI render path (e.g. image/file-path lookups
        // feeding `NSImage(contentsOfFile:)`), which has no `await` point.
        self.runtime.block_on(async {
            let sort: Vec<bae_core::db::AlbumSortCriterion> =
                sort_criteria.iter().map(bridge_sort_to_core).collect();
            let albums = self
                .app_services
                .library_manager()
                .get_album_page(&sort, offset, limit)
                .await
                .map_err(|e| BridgeError::database(format!("{e}")))?;

            Ok(albums.into_iter().map(convert_album_summary).collect())
        })
    }

    pub fn get_album_count(&self) -> Result<u64, BridgeError> {
        self.runtime.block_on(async {
            self.app_services
                .library_manager()
                .get_album_count()
                .await
                .map_err(|e| BridgeError::database(format!("{e}")))
        })
    }

    /// Cache-bustable identifier for a library image (album cover, artist
    /// photo) if it exists on disk: the file's path with its modification time
    /// appended as `#v=<mtime_secs>`. The version changes when the cover does,
    /// so the view's image cache key changes and it reloads; the loader strips
    /// the `#v=…` suffix before reading the file. Returns `None` when no image
    /// is cached for `image_id`.
    pub fn image_path_if_exists(&self, image_id: String) -> Option<String> {
        self.app_services
            .library_manager()
            .image_path_if_exists(&image_id)
    }

    /// Filesystem path for a library file. Returns `Ok(None)` if the file has
    /// no readable local location (e.g. cloud-only and not cached). Returns
    /// `Err` on DB failures so callers can distinguish a missing file from a
    /// broken library state.
    pub fn file_path(&self, file_id: String) -> Result<Option<String>, BridgeError> {
        self.runtime.block_on(async {
            let path = self
                .app_services
                .library_manager()
                .file_local_path(&file_id)
                .await
                .map_err(|e| BridgeError::database(format!("{e}")))?;
            Ok(path.and_then(|p| p.to_str().map(|s| s.to_string())))
        })
    }

    pub fn get_album_detail(&self, album_id: String) -> Result<BridgeAlbumDetail, BridgeError> {
        self.runtime.block_on(async {
            let detail = self
                .app_services
                .library_manager()
                .find_album_detail(&album_id)
                .await
                .map_err(|e| BridgeError::database(format!("{e}")))?
                .ok_or_else(|| BridgeError::NotFound {
                    entity: crate::types::BridgeEntityKind::Album,
                    id: album_id.to_string(),
                })?;

            Ok(convert_album_detail(detail))
        })
    }

    /// Fat release detail (tracks, files, gallery) for the album-detail
    /// view. Returns `Ok(None)` when the release doesn't exist.
    pub fn find_release_detail(
        &self,
        release_id: String,
    ) -> Result<Option<BridgeRelease>, BridgeError> {
        self.runtime.block_on(async {
            let detail = self
                .app_services
                .library_manager()
                .find_release_detail(&release_id)
                .await
                .map_err(|e| BridgeError::database(format!("{e}")))?;
            Ok(detail.map(convert_release_detail))
        })
    }

    /// One page of the Storage Manager list, pre-sorted and
    /// pre-filtered. Rows carry both halves (release + parent album) so
    /// the UI can populate its normalized slices from a single call.
    pub fn storage_page(
        &self,
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<BridgeStoragePage, BridgeError> {
        self.runtime.block_on(async {
            let core_sort = crate::types::bridge_storage_sort_to_core(&sort);
            let core_filter = crate::types::bridge_storage_filter_to_core(filter);
            let page = self
                .app_services
                .library_manager()
                .get_storage_page(&core_sort, core_filter, offset, limit)
                .await
                .map_err(|e| BridgeError::database(format!("{e}")))?;

            Ok(BridgeStoragePage {
                rows: page.rows.into_iter().map(convert_storage_row).collect(),
                total_count: page.total_count,
            })
        })
    }

    /// Count of storage rows matching `filter`. Matches
    /// `storage_page`'s `total_count` for the same filter.
    pub fn storage_count(&self, filter: BridgeStorageFilter) -> Result<u64, BridgeError> {
        self.runtime.block_on(async {
            let core_filter = crate::types::bridge_storage_filter_to_core(filter);
            self.app_services
                .library_manager()
                .get_storage_count(core_filter)
                .await
                .map_err(|e| BridgeError::database(format!("{e}")))
        })
    }

    pub async fn search_library(&self, query: String) -> Result<BridgeSearchResults, BridgeError> {
        let results = self
            .app_services
            .library_manager()
            .search_library(&query, 50)
            .await
            .map_err(|e| BridgeError::database(format!("{e}")))?;
        Ok(BridgeSearchResults {
            albums: results
                .albums
                .into_iter()
                .map(|a| BridgeAlbumSearchResult {
                    id: a.id,
                    title: a.title,
                    year: a.year,
                    primary_release_id: a.primary_release_id,
                    artist_name: a.artist_name,
                })
                .collect(),
            tracks: results
                .tracks
                .into_iter()
                .map(|t| BridgeTrackSearchResult {
                    id: t.id,
                    title: t.title,
                    duration_ms: t.duration_ms,
                    album_id: t.album_id,
                    album_title: t.album_title,
                    artist_name: t.artist_name,
                })
                .collect(),
        })
    }

    // =========================================================================
    // Playback
    // =========================================================================

    pub fn play_release(&self, release_id: String, start_track_index: Option<u32>, shuffle: bool) {
        self.app_services.playback().play_release(
            release_id,
            start_track_index.map(|i| i as usize),
            shuffle,
        );
    }

    pub fn pause(&self) {
        self.app_services.playback().pause();
    }

    pub fn resume(&self) {
        self.app_services.playback().resume();
    }

    pub fn stop(&self) {
        self.app_services.playback().stop();
    }

    pub fn next_track(&self) {
        self.app_services.playback().next();
    }

    pub fn previous_track(&self) {
        self.app_services.playback().previous();
    }

    pub fn seek_by_ratio(&self, ratio: f64) {
        self.app_services.playback().seek_by_ratio(ratio);
    }

    pub fn set_volume(&self, volume: f32) {
        self.app_services.playback().set_volume(volume);
    }

    pub fn toggle_mute(&self) {
        self.app_services.playback().toggle_mute();
    }

    pub fn preview_play(&self, path: String) {
        self.app_services.playback().preview_play(path);
    }

    pub fn preview_stop(&self) {
        self.app_services.playback().preview_stop();
    }

    pub fn preview_toggle_pause(&self) {
        self.app_services.playback().preview_toggle_pause();
    }

    pub fn preview_seek_by_ratio(&self, ratio: f64) {
        self.app_services.playback().preview_seek_by_ratio(ratio);
    }

    pub fn set_repeat_mode(&self, mode: BridgeRepeatMode) {
        let core_mode = mode.to_core();
        self.app_services.playback().set_repeat_mode(core_mode);
    }

    pub fn cycle_repeat_mode(&self) {
        self.app_services.playback().cycle_repeat_mode();
    }

    pub fn toggle_play_pause(&self) {
        self.app_services.playback().toggle_play_pause();
    }

    /// Graceful shutdown: saves playback state to disk, then stops the playback service.
    pub fn shutdown(&self) {
        self.runtime
            .block_on(self.app_services.playback().shutdown());
    }

    /// Persist the current playback state without stopping playback. Mobile
    /// calls this when the app is backgrounded so the queue, current track, and
    /// position survive a later cold launch — it can't call `shutdown`, which
    /// would stop the background audio.
    pub fn save_playback_state(&self) {
        self.runtime
            .block_on(self.app_services.playback().save_state());
    }

    // =========================================================================
    // Queue
    // =========================================================================

    pub fn add_to_queue(&self, track_ids: Vec<String>) {
        self.app_services.playback().add_to_queue(track_ids);
    }

    pub fn add_next(&self, track_ids: Vec<String>) {
        self.app_services.playback().add_next(track_ids);
    }

    pub fn add_release_to_queue(&self, release_id: String) {
        self.app_services
            .playback()
            .add_release_to_queue(release_id);
    }

    pub fn add_release_next(&self, release_id: String) {
        self.app_services.playback().add_release_next(release_id);
    }

    /// Resolve a list of IDs (album or track) to track IDs.
    /// Album IDs are expanded to the primary release's tracks.
    pub fn resolve_to_track_ids(&self, ids: Vec<String>) -> Result<Vec<String>, BridgeError> {
        self.runtime.block_on(async {
            self.app_services
                .library_manager()
                .resolve_to_track_ids(&ids)
                .await
                .map_err(BridgeError::database)
        })
    }

    pub fn insert_in_queue(&self, track_ids: Vec<String>, index: u32) {
        self.app_services
            .playback()
            .insert_in_queue(track_ids, index as usize);
    }

    pub fn remove_from_queue(&self, index: u32) {
        self.app_services
            .playback()
            .remove_from_queue(index as usize);
    }

    pub fn reorder_queue(&self, from_index: u32, to_index: u32) {
        self.app_services
            .playback()
            .reorder_queue(from_index as usize, to_index as usize);
    }

    pub fn clear_queue(&self) {
        self.app_services.playback().clear_queue();
    }

    pub fn skip_to_queue_index(&self, index: u32) {
        self.app_services.playback().skip_to(index as usize);
    }

    // =========================================================================
    // Settings
    // =========================================================================

    pub fn get_config(&self) -> BridgeConfig {
        build_bridge_config(&self.app_services.library_manager().get_config())
    }

    /// Whether the encryption key is loaded — `init` successfully read it from
    /// the keyring and built the sync manager. Reflects the cached init-time
    /// result, not a fresh keyring read. `false` here for an
    /// `encryption_key_stored` library means the keyring got wiped and the
    /// user needs to enter the key again.
    pub fn has_encryption_key(&self) -> bool {
        self.app_services.library_manager().has_encryption()
    }

    pub fn rename_library(&self, library_id: String, name: String) -> Result<(), BridgeError> {
        self.app_services
            .library_manager()
            .rename_library(&library_id, &name)
            .map_err(BridgeError::config)
    }

    pub fn lock_active_library(&self) -> Result<(), BridgeError> {
        self.app_services
            .library_manager()
            .forget_encryption_key()
            .map_err(BridgeError::internal)
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, BridgeError> {
        self.app_services
            .library_manager()
            .get_discogs_token()
            .map_err(BridgeError::config)
    }

    // Discogs token writes live on the desktop-only import service (see the
    // `feature = "desktop"` impl block); mobile reads status but never writes.

    // =========================================================================
    // Events
    // =========================================================================

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
                source: selection.source.to_core(),
            },
        };

        self.app_services
            .library_manager()
            .change_cover(&album_id, &release_id, core_selection)
            .await
            .map_err(|e| BridgeError::internal(format!("{e}")))
    }

    pub fn set_primary_release(
        &self,
        album_id: String,
        release_id: String,
    ) -> Result<(), BridgeError> {
        self.runtime.block_on(async {
            self.app_services
                .library_manager()
                .set_album_primary_release(&album_id, &release_id)
                .await
                .map_err(|e| BridgeError::internal(format!("{e}")))
        })
    }

    // =========================================================================
    // Storage
    // =========================================================================

    // These descend into the AWS S3 future chain (cloud reads/writes), so they
    // run on a runtime worker via `spawn_on_runtime` rather than `block_on`
    // (which would drive the deep future on the shallow Swift stack — see
    // `spawn_on_runtime`'s doc). Pinning is the exception: it routes through the
    // in-memory download queue (`queue_pin_releases`), which enqueues quickly
    // and downloads on the queue worker.
    pub async fn unpin_release(&self, release_id: String) -> Result<(), BridgeError> {
        let services = self.app_services.clone();
        self.spawn_on_runtime(
            async move { services.library_manager().unpin_release(&release_id).await },
        )
        .await
        .map_err(BridgeError::internal)
    }

    pub async fn manage_release(
        &self,
        release_id: String,
        pin: bool,
        delete_source: bool,
    ) -> Result<(), BridgeError> {
        let services = self.app_services.clone();
        self.spawn_on_runtime(async move {
            services
                .library_manager()
                .manage_release(&release_id, pin, delete_source)
                .await
        })
        .await
        .map_err(BridgeError::internal)
    }

    pub async fn unmanage_release(
        &self,
        release_id: String,
        new_path: String,
    ) -> Result<(), BridgeError> {
        let services = self.app_services.clone();
        self.spawn_on_runtime(async move {
            services
                .library_manager()
                .unmanage_release(&release_id, &new_path)
                .await
        })
        .await
        .map_err(BridgeError::internal)
    }

    pub fn delete_release(&self, release_id: String) {
        let result = self.runtime.block_on(async {
            self.app_services
                .library_manager()
                .delete_release(&release_id)
                .await
        });

        if let Err(e) = result {
            self.app_services
                .library_manager()
                .emit_error(bae_core::ui::UiError::internal(format!(
                    "Delete failed: {e}"
                )));
        }
    }

    // =========================================================================
    // Sync / membership
    // =========================================================================

    pub async fn save_sync_config(
        &self,
        config_data: BridgeSaveSyncConfig,
    ) -> Result<(), BridgeError> {
        use bae_core::sync::sync_manager::S3ConfigData;
        // Probes the bucket (HeadBucket) then starts sync — deep; on a worker.
        let services = self.app_services.clone();
        self.spawn_on_runtime(async move {
            services
                .library_manager()
                .save_s3_config(S3ConfigData {
                    bucket: config_data.bucket,
                    region: config_data.region,
                    endpoint: config_data.endpoint,
                    key_prefix: config_data.key_prefix,
                    access_key: config_data.access_key,
                    secret_key: config_data.secret_key,
                    storage: bridge_home_storage_to_core(config_data.storage),
                })
                .await
        })
        .await
        .map_err(BridgeError::config)
    }

    pub fn disconnect_cloud_provider(&self) -> Result<(), BridgeError> {
        self.app_services
            .library_manager()
            .disconnect_cloud_provider()
            .map_err(BridgeError::config)
    }

    /// Warning text for the disconnect-sync confirmation when releases live
    /// only in the cloud. `None` means no releases are at risk; `Some(msg)`
    /// is a pre-formatted (singular/plural, full sentence) warning the UI
    /// appends to its base "this will stop syncing" message.
    pub fn disconnect_warning_message(&self) -> Result<Option<String>, BridgeError> {
        self.runtime
            .block_on(
                self.app_services
                    .library_manager()
                    .disconnect_warning_message(),
            )
            .map_err(BridgeError::internal)
    }

    pub fn generate_restore_code(&self) -> Result<String, BridgeError> {
        self.app_services
            .library_manager()
            .generate_restore_code()
            .map_err(BridgeError::config)
    }

    /// Forget the active local library on this device: delete its key, clear the
    /// active pointer, and remove its data directory (the owner's cloud copy is
    /// untouched). The caller must drop this handle right after — the database
    /// lives in the removed directory — and re-open / onboard from scratch.
    pub fn forget_library(&self) -> Result<(), BridgeError> {
        self.app_services
            .library_manager()
            .forget_library()
            .map_err(BridgeError::config)?;

        info!("Forgot local library");
        Ok(())
    }

    pub fn trigger_sync(&self) {
        self.app_services.library_manager().trigger_sync();
    }

    pub fn is_sync_ready(&self) -> bool {
        self.app_services.library_manager().is_sync_ready()
    }

    /// The current cloud outbox processing snapshot. Seeds the Storage Manager
    /// panel before the first `OutboxChanged` event arrives.
    pub fn get_outbox_snapshot(&self) -> Result<crate::types::BridgeOutboxSnapshot, BridgeError> {
        let snapshot = self
            .runtime
            .block_on(self.app_services.library_manager().outbox_snapshot())
            .map_err(BridgeError::internal)?;
        Ok(convert_outbox_snapshot(snapshot))
    }

    /// Retry failed uploads now (clears their backoff and kicks the sync loop).
    pub fn retry_outbox(&self) -> Result<(), BridgeError> {
        self.runtime
            .block_on(self.app_services.library_manager().retry_outbox_now())
            .map_err(BridgeError::internal)
    }

    /// Cancel one queued outbox entry by id (dequeues it; the local file stays).
    pub fn cancel_outbox_item(&self, id: i64) -> Result<(), BridgeError> {
        self.runtime
            .block_on(self.app_services.library_manager().cancel_outbox_item(id))
            .map_err(BridgeError::internal)
    }

    /// Cancel whatever transition a release is mid-flight — a pin (download), a
    /// managed upload, or an unmanage — leaving it in its prior state. The UI
    /// calls this from the storage row and the queue pane without knowing which
    /// is running; a no-op if nothing is in progress.
    pub fn cancel_release_transition(&self, release_id: String) -> Result<(), BridgeError> {
        self.runtime
            .block_on(
                self.app_services
                    .library_manager()
                    .cancel_release_transition(&release_id),
            )
            .map_err(BridgeError::internal)
    }

    /// Pause or resume the cloud-upload pipeline. While paused, new enqueues
    /// still land in the outbox but the sync cycle won't drain them; the
    /// snapshot's `paused` field flips so the UI can render the toggle.
    pub fn set_sync_paused(&self, paused: bool) {
        self.runtime
            .block_on(self.app_services.library_manager().set_sync_paused(paused));
    }

    // ── Download (pin) queue ─────────────────────────────────────────

    /// The current download-queue snapshot. Seeds the Downloads pane before the
    /// first `DownloadQueueChanged` event arrives.
    pub fn get_download_snapshot(&self) -> crate::types::BridgeDownloadSnapshot {
        convert_download_snapshot(self.app_services.library_manager().download_snapshot())
    }

    /// Enqueue releases to pin for offline. They join the in-memory serial
    /// download queue; the worker drains them one at a time. The DB lookups
    /// (resolving each release's title/size for its pane row) are shallow, so
    /// this polls inline via block_on — the deep cloud download runs on the
    /// queue worker, not here.
    pub fn queue_pin_releases(&self, release_ids: Vec<String>) {
        self.runtime.block_on(
            self.app_services
                .library_manager()
                .enqueue_pins(release_ids),
        );
    }

    /// Pause or resume the download queue. In-flight downloads finish; the queue
    /// stops starting new ones until resumed.
    pub fn set_downloads_paused(&self, paused: bool) {
        self.app_services
            .library_manager()
            .set_downloads_paused(paused);
    }

    /// Cancel a release's download — drops a queued/failed entry or aborts the
    /// in-flight one (a partial download never lands, so the release stays
    /// cloud-only).
    pub fn cancel_download(&self, release_id: String) {
        self.app_services
            .library_manager()
            .cancel_download(&release_id);
    }

    /// Retry every failed download now (flips them back to queued and wakes the
    /// worker).
    pub fn retry_downloads(&self) {
        self.app_services.library_manager().retry_downloads();
    }
}

// =========================================================================
// Release gallery (all platforms)
// =========================================================================

#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    /// Bytes of a release's gallery image, fetched from the release's cloud home
    /// (and decrypted) when it isn't on disk here. The lightbox calls this for
    /// gallery items whose `local_path` is `None` — the release's cloud-only
    /// image files. `file_id` is the gallery item's `id`.
    pub async fn fetch_gallery_image(
        &self,
        release_id: String,
        file_id: String,
    ) -> Result<Vec<u8>, BridgeError> {
        self.app_services
            .library_manager()
            .load_gallery_image(&release_id, &file_id)
            .await
            .map_err(BridgeError::import)
    }
}

// =========================================================================
// Apple-only: CloudKit
// =========================================================================

#[cfg(feature = "cloudkit")]
#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    pub async fn use_cloudkit(&self, storage: BridgeHomeStorage) -> Result<(), BridgeError> {
        // Starts sync against CloudKit — deep; on a worker.
        let services = self.app_services.clone();
        let storage = bridge_home_storage_to_core(storage);
        self.spawn_on_runtime(async move { services.library_manager().use_cloudkit(storage).await })
            .await
            .map_err(BridgeError::config)
    }
}

// =========================================================================
// OAuth-only: consumer-cloud sign-in (Google Drive, Dropbox, OneDrive)
// =========================================================================

#[cfg(feature = "oauth-providers")]
#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    pub async fn sign_in_cloud_provider(
        &self,
        provider: BridgeCloudProvider,
        storage: BridgeHomeStorage,
    ) -> Result<(), BridgeError> {
        // OAuth + cloud-folder setup then starts sync — network; on a worker.
        // Cancellation tears down the OAuth listener via coven's own drop guard.
        let services = self.app_services.clone();
        let core_provider = bridge_cloud_provider_to_core(provider);
        let storage = bridge_home_storage_to_core(storage);
        self.spawn_on_runtime(async move {
            services
                .library_manager()
                .sign_in_cloud_provider(core_provider, storage)
                .await
        })
        .await
        .map_err(BridgeError::config)
    }
}

// =========================================================================
// Desktop-only: Import, Cover fetching
// =========================================================================

#[cfg(feature = "desktop")]
#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    /// Validate then persist a Discogs API token, returning what happened so the
    /// UI can react (keep the draft on `Rejected`, show the optimistic-save note
    /// on `Unvalidated`). Lives on the import service, which only runs on desktop
    /// (identification). Mobile reads token status via `get_config` but never
    /// writes.
    pub async fn save_discogs_token(
        &self,
        token: String,
    ) -> Result<BridgeDiscogsSaveOutcome, BridgeError> {
        self.app_services
            .import()
            .save_discogs_token(&token)
            .await
            .map(BridgeDiscogsSaveOutcome::from)
            .map_err(BridgeError::config)
    }

    /// Re-check a stored `Unvalidated` key against Discogs. No-op when no key is
    /// stored or it's already settled. Called at app launch and settings-tab
    /// open for the offline-saved case.
    pub async fn revalidate_discogs_token(&self) -> Result<(), BridgeError> {
        self.app_services
            .import()
            .revalidate_discogs_token()
            .await
            .map_err(BridgeError::config)
    }

    pub fn remove_discogs_token(&self) -> Result<(), BridgeError> {
        self.app_services
            .import()
            .remove_discogs_token()
            .map_err(BridgeError::config)
    }

    /// Register the platform artwork analyzer. Called once at app boot
    /// (e.g. from `BaeApp`'s startup path). The same adapter backs both the
    /// identify pipeline's barcode phase and the candidate text-scan
    /// service, so Swift supplies a single Vision implementation. Without a
    /// registered analyzer both paths fall back to no-ops.
    pub fn register_artwork_analyzer(
        &self,
        analyzer: Box<dyn crate::types::ArtworkAnalyzerCallback>,
    ) {
        let adapter = std::sync::Arc::new(crate::identify::ArtworkAnalyzerAdapter::new(analyzer));
        self.app_services
            .identify()
            .register_analyzer(adapter.clone());
        self.app_services.extraction().register_analyzer(adapter);
    }

    /// Start identifying a folder candidate. Identify subscribes first, then
    /// extraction streams the candidate's `Signals` (disc ID, barcodes,
    /// classified text) that identify looks up and the UI surfaces. Events
    /// flow through the unified import event channel → bus → reducer → store.
    pub fn auto_identify_folder(&self, candidate_key: String, folder_path: String) {
        let folder: std::path::PathBuf = folder_path.into();
        self.app_services.identify().start(candidate_key.clone());
        self.app_services
            .extraction()
            .start(candidate_key, ExtractionSource::Folder(folder));
    }

    /// Start re-identifying an existing library release. Extraction resolves
    /// the release's disc ID and artwork from the library. Events stream
    /// through the same identify channel — the UI consumes them by candidate
    /// key the same way it does for folder imports.
    pub fn auto_identify_release(&self, candidate_key: String, release_id: String) {
        self.app_services.identify().start(candidate_key.clone());
        self.app_services
            .extraction()
            .start(candidate_key, ExtractionSource::Release { release_id });
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
        self.app_services
            .identify()
            .toggle_signal(&candidate_key, signal.to_core());
    }

    /// Re-run a candidate's lookups from the toolbar. The identify driver
    /// resets to triangulating and re-dispatches the disc-ID / barcode
    /// lookups from the retained signals, preserving exclusions. A no-op when
    /// the candidate isn't running.
    pub fn rerun_identify_for_candidate(&self, candidate_key: String) {
        self.app_services.identify().rerun(&candidate_key);
    }

    /// Re-identify commit. Translates the user's `IdentityChoice` from the
    /// re-identify result list into a fully cross-linked identity vec +
    /// metadata pointer, then writes via `set_identity`. Mirrors what the
    /// import commit does for a folder import — the outcome is
    /// indistinguishable from re-importing the release with the same choice.
    ///
    /// Returns the album id the release lives on after the commit. May
    /// have changed if the new identity vec didn't fit the source album
    /// (`set_identity` move semantics).
    ///
    /// Caller decides whether to also reseed metadata via
    /// `reset_metadata_to_source` + `update_release_metadata_user_edit`.
    pub async fn re_identify_release(
        &self,
        release_id: String,
        identity_choice: crate::types::BridgeIdentityChoice,
    ) -> Result<String, BridgeError> {
        let core_choice = identity_choice.to_core();
        let library_manager = self.app_services.library_manager();
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
        self.app_services
            .import()
            .watched_folders()
            .into_iter()
            .map(crate::types::BridgeWatchedFolder::from_core)
            .collect()
    }

    pub fn add_watched_folder(&self, path: String) -> Result<(), BridgeError> {
        self.app_services
            .import()
            .add_watched_folder(path)
            .map_err(BridgeError::import)
    }

    pub fn remove_watched_folder(&self, path: String) -> Result<(), BridgeError> {
        self.app_services
            .import()
            .remove_watched_folder(path)
            .map_err(BridgeError::import)
    }

    /// Mark the candidate at `path` skipped or unskipped. Persists the change and
    /// broadcasts a `CandidateSkipChanged` event so the import view re-tabs the
    /// row.
    pub fn set_candidate_skipped(&self, path: String, skipped: bool) -> Result<(), BridgeError> {
        self.app_services
            .import()
            .set_candidate_skipped(path, skipped)
            .map_err(BridgeError::import)
    }

    /// Scan every watched folder, streaming results back as
    /// `FolderCandidateAdded` events. The UI calls this when the import view
    /// appears to populate the candidate list.
    pub fn scan_watched_folders(&self) -> Result<(), BridgeError> {
        self.app_services
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
                    source: source.to_core(),
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
                    source: source.to_core(),
                },
                crate::types::BridgeSearchQueryKind::CatalogNumber,
                source,
            ),
            crate::types::BridgeSearchQuery::Barcode { barcode, source } => (
                SearchQuery::Barcode {
                    barcode,
                    source: source.to_core(),
                },
                crate::types::BridgeSearchQueryKind::Barcode,
                source,
            ),
        };

        let grouped = self
            .app_services
            .import()
            .search_with_status(core_query)
            .await
            .map_err(BridgeError::import)?;

        Ok(crate::types::BridgeCandidateSearchResults {
            tab,
            source: bridge_source,
            groups: grouped
                .groups
                .into_iter()
                .map(crate::types::release_group_to_bridge)
                .collect(),
            statuses: grouped
                .statuses
                .into_iter()
                .map(crate::types::library_status_to_bridge)
                .collect(),
        })
    }

    pub fn is_source_folder_name_imported(&self, name: String) -> Result<bool, BridgeError> {
        self.runtime.block_on(async {
            self.app_services
                .library_manager()
                .is_source_folder_name_imported(&name)
                .await
                .map_err(|e| BridgeError::database(format!("{e}")))
        })
    }

    // The bridge boundary surface is wide on purpose — uniffi flattens
    // primitives across the FFI per Swift idiom, not per Rust idiom.
    #[allow(clippy::too_many_arguments)]
    pub fn start_import(
        &self,
        candidate_key: String,
        folder_path: String,
        selected_cover: Option<BridgeCoverSelection>,
        storage_mode: BridgeStorageMode,
        identity_choice: crate::types::BridgeIdentityChoice,
        user_edit: Option<crate::types::BridgeReleaseUserEdit>,
    ) -> Result<(), BridgeError> {
        let cover = selected_cover.map(crate::types::bridge_cover_to_import);

        let user_edit = user_edit.map(crate::types::release_user_edit_from_bridge);

        self.app_services
            .import()
            .start_import(
                &candidate_key,
                std::path::PathBuf::from(&folder_path),
                cover,
                bridge_storage_mode_to_core(storage_mode),
                identity_choice.to_core(),
                user_edit,
            )
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
        folder_path: String,
    ) -> Result<crate::types::BridgeReleaseUserEdit, BridgeError> {
        let edit = self
            .app_services
            .import()
            .preview_file_tags_for_folder(std::path::PathBuf::from(&folder_path))
            .await
            .map_err(BridgeError::import)?;
        Ok(crate::types::release_user_edit_to_bridge(edit))
    }

    /// Apply a user-supplied metadata edit (from the edit-metadata sheet) to a
    /// release. Writes the user's edited values directly without touching
    /// identity, `metadata_source`, or cached source payloads.
    pub async fn update_release_metadata_user_edit(
        &self,
        release_id: String,
        edit: crate::types::BridgeReleaseUserEdit,
    ) -> Result<(), BridgeError> {
        let core_edit = crate::types::release_user_edit_from_bridge(edit);
        self.app_services
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
            .app_services
            .library_manager()
            .release_edit_seed(&release_id)
            .await
            .map_err(BridgeError::import)?;
        Ok(crate::types::raw_release_edit_to_bridge(raw))
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
            .app_services
            .library_manager()
            .reset_metadata_to_source(&release_id)
            .await
            .map_err(BridgeError::import)?;
        Ok(crate::types::release_user_edit_to_bridge(edit))
    }

    pub async fn prefetch_release(
        &self,
        release_id: String,
        source: BridgeMetadataSource,
        local_track_count: Option<u32>,
    ) -> Result<crate::types::BridgeReleaseDetail, BridgeError> {
        let detail = self
            .app_services
            .import()
            .prefetch_release(&release_id, source.to_core())
            .await
            .map_err(BridgeError::import)?;
        Ok(crate::types::release_detail_to_bridge(
            detail,
            local_track_count,
        ))
    }

    pub async fn fetch_remote_covers(
        &self,
        release_id: String,
    ) -> Result<Vec<BridgeRemoteCover>, BridgeError> {
        let covers = self
            .app_services
            .import()
            .fetch_remote_covers(&release_id)
            .await
            .map_err(BridgeError::import)?;
        Ok(covers
            .into_iter()
            .map(crate::types::remote_cover_data_to_bridge)
            .collect())
    }

    /// Fetch raw bytes for a remote cover-art URL. The UI caches the
    /// decoded `NSImage` in front of this; when the user confirms a
    /// remote cover, the bytes are passed back via `start_import`. The
    /// commit worker never fetches on its own.
    pub async fn fetch_cover_bytes(&self, url: String) -> Result<Vec<u8>, BridgeError> {
        self.app_services
            .import()
            .fetch_cover_bytes(url)
            .await
            .map_err(BridgeError::import)
    }
}

// =========================================================================
// Export (desktop-only)
// =========================================================================

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[uniffi::export(async_runtime = "tokio")]
impl AppHandle {
    pub async fn export_track(
        &self,
        track_id: String,
        output_path: String,
        format: crate::types::BridgeExportFormat,
    ) -> Result<(), BridgeError> {
        use crate::types::BridgeExportFormat;

        let core_format = match format {
            BridgeExportFormat::Flac => bae_core::library::ExportFormat::Flac,
            BridgeExportFormat::Mp3 => bae_core::library::ExportFormat::Mp3 {
                bitrate: bae_core::library::MP3_EXPORT_BITRATE,
            },
        };

        self.app_services
            .library_manager()
            .export_track(&track_id, std::path::Path::new(&output_path), core_format)
            .await
            .map_err(|e| BridgeError::export(format!("{e}")))
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
/// Falling behind the bus (`Lagged`) drops the lagged events but must not kill
/// the subscription — this loop is the UI's only event feed for the whole
/// library session, and the callback is a synchronous FFI call, so a slow
/// consumer during an event burst is exactly when lag happens.
async fn pump_ui_events(
    mut rx: tokio::sync::broadcast::Receiver<bae_core::ui::UiBusEvent>,
    callback: Box<dyn crate::types::UiEventCallback>,
) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                if let Some(bridge_event) = convert_ui_event(event) {
                    callback.on_event(bridge_event);
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("UI event subscription lagged; dropped {n} events");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Translate bae-core's outbox snapshot to its bridge mirror. `last_error` is
/// `last_error` is lifted out of the `Failed` variant into a flat field
/// beside the three-state enum so the UI doesn't switch on associated data.
///
/// Ungated (unlike `convert_ui_event`): `get_outbox_snapshot` lives in the
/// ungated `AppHandle` impl block, so this helper must exist on every target it
/// can be called from. It references only cross-platform types.
fn convert_outbox_snapshot(
    snapshot: bae_core::library::OutboxSnapshot,
) -> crate::types::BridgeOutboxSnapshot {
    use crate::types::{BridgeDeleteOp, BridgeOutboxSnapshot, BridgeUploadReleaseGroup};

    let upload_groups = snapshot
        .upload_groups
        .into_iter()
        .map(|g| BridgeUploadReleaseGroup {
            release_id: g.release_id,
            display_title: g.display_title,
            file_count: g.file_count,
            progress: convert_upload_progress(g.progress),
        })
        .collect();

    let deletes = snapshot
        .deletes
        .into_iter()
        .map(|op| BridgeDeleteOp {
            id: op.id,
            file_id: op.file_id,
            cloud_key: op.cloud_key,
            created_at: op.created_at,
        })
        .collect();

    let per_release = snapshot
        .per_release
        .into_iter()
        .map(|(k, v)| (k, convert_upload_progress(v)))
        .collect();

    BridgeOutboxSnapshot {
        upload_groups,
        deletes,
        per_release,
        total: convert_upload_progress(snapshot.total),
        active_bytes_total: snapshot.active_bytes_total,
        pending_deletes: snapshot.pending_deletes,
        paused: snapshot.paused,
        throughput_bps: snapshot.throughput_bps,
        eta_seconds: snapshot.eta_seconds,
    }
}

fn convert_upload_progress(
    p: bae_core::library::UploadProgress,
) -> crate::types::BridgeUploadProgress {
    crate::types::BridgeUploadProgress {
        queued: p.queued,
        active: p.active,
        failed: p.failed,
        bytes_done: p.bytes_done,
        bytes_total: p.bytes_total,
        activity: p.activity().map(convert_upload_activity),
    }
}

fn convert_upload_activity(
    a: bae_core::library::UploadActivity,
) -> crate::types::BridgeUploadActivity {
    use crate::types::BridgeUploadActivity;
    use bae_core::library::UploadActivity;
    match a {
        UploadActivity::Uploading => BridgeUploadActivity::Uploading,
        UploadActivity::Retrying => BridgeUploadActivity::Retrying,
        UploadActivity::Queued => BridgeUploadActivity::Queued,
    }
}

fn convert_download_snapshot(
    snapshot: bae_core::library::DownloadSnapshot,
) -> crate::types::BridgeDownloadSnapshot {
    use crate::types::{BridgeDownloadOp, BridgeDownloadSnapshot, BridgeDownloadState};
    use bae_core::library::DownloadState;

    let downloads = snapshot
        .downloads
        .into_iter()
        .map(|op| {
            let (state, percent, error) = match op.state {
                DownloadState::Queued => (BridgeDownloadState::Queued, 0, None),
                DownloadState::Active { percent } => (BridgeDownloadState::Active, percent, None),
                DownloadState::Failed { error } => (BridgeDownloadState::Failed, 0, Some(error)),
            };
            BridgeDownloadOp {
                release_id: op.release_id,
                title: op.title,
                file_count: op.file_count,
                total_size: op.total_size,
                created_at: op.created_at,
                state,
                percent,
                error,
            }
        })
        .collect();

    BridgeDownloadSnapshot {
        downloads,
        total: convert_download_progress(snapshot.total),
        paused: snapshot.paused,
    }
}

fn convert_download_progress(
    p: bae_core::library::DownloadProgress,
) -> crate::types::BridgeDownloadProgress {
    crate::types::BridgeDownloadProgress {
        queued: p.queued,
        active: p.active,
        failed: p.failed,
    }
}

/// Convert a core UiBusEvent to a bridge BridgeUiEvent.
/// Returns None for events we don't need to forward (or can't convert yet).
fn convert_ui_event(event: bae_core::ui::UiBusEvent) -> Option<crate::types::BridgeUiEvent> {
    use crate::types::*;
    use bae_core::ui::UiBusEvent;

    match event {
        // ── Playback ───────────────────────────────────────────────
        UiBusEvent::PlaybackStopped => Some(BridgeUiEvent::PlaybackStopped),
        UiBusEvent::PlaybackError { reason } => Some(BridgeUiEvent::PlaybackError {
            reason: crate::types::bridge_playback_error_reason(reason),
        }),
        UiBusEvent::PlaybackLoading { track_id, track } => Some(BridgeUiEvent::PlaybackLoading {
            track_id,
            track: track.map(|t| BridgeLoadingTrackInfo {
                track_title: t.track_info.track_title,
                artist_names: t.track_info.artist_names,
                album_id: t.track_info.album_id,
                album_title: t.track_info.album_title,
                cover_image_id: t.track_info.cover_image_id,
                duration_ms: t.duration_ms,
            }),
        }),
        UiBusEvent::PlaybackPlaying {
            track_id,
            track_title,
            artist_names,
            artist_id,
            album_id,
            album_title,
            cover_image_id,
            duration_ms,
        } => Some(BridgeUiEvent::PlaybackPlaying {
            track_id,
            track_title,
            artist_names,
            artist_id,
            album_id,
            album_title,
            cover_image_id,
            duration_ms,
        }),
        UiBusEvent::PlaybackPaused {
            track_id,
            track_title,
            artist_names,
            artist_id,
            album_id,
            album_title,
            cover_image_id,
            duration_ms,
        } => Some(BridgeUiEvent::PlaybackPaused {
            track_id,
            track_title,
            artist_names,
            artist_id,
            album_id,
            album_title,
            cover_image_id,
            duration_ms,
        }),
        UiBusEvent::PlaybackProgress {
            position_ms,
            duration_ms,
            progress,
        } => Some(BridgeUiEvent::PlaybackProgress {
            position_ms,
            duration_ms,
            progress,
        }),
        UiBusEvent::VolumeChanged { volume } => Some(BridgeUiEvent::VolumeChanged { volume }),
        UiBusEvent::MuteChanged { is_muted } => Some(BridgeUiEvent::MuteChanged { is_muted }),
        UiBusEvent::RepeatModeChanged { mode } => Some(BridgeUiEvent::RepeatModeChanged {
            mode: BridgeRepeatMode::from_core(mode),
        }),
        UiBusEvent::QueueUpdated {
            items,
            has_next,
            has_previous,
        } => Some(BridgeUiEvent::QueueUpdated {
            items: items
                .into_iter()
                .map(|i| BridgeQueueItem {
                    track_id: i.track_id,
                    title: i.title,
                    artist_names: i.artist_names,
                    duration_ms: i.duration_ms,
                    album_title: i.album_title,
                    cover_image_id: i.cover_image_id,
                })
                .collect(),
            has_next,
            has_previous,
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

        // ── Candidate-scoped ───────────────────────────────────────
        // These two carry identify-pipeline payloads, which are desktop-only.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        UiBusEvent::CandidateIdentifyStateChanged {
            key,
            state,
            toolbar,
        } => Some(BridgeUiEvent::CandidateIdentifyStateChanged {
            key,
            state: crate::types::identify_state_to_bridge(state),
            toolbar: crate::types::toolbar_to_bridge(toolbar),
        }),
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        UiBusEvent::CandidateSignalsUpdated { key, signals } => {
            Some(BridgeUiEvent::CandidateSignalsUpdated {
                key,
                signals: crate::types::signals_to_bridge(signals),
            })
        }
        UiBusEvent::CandidateImportImporting {
            key,
            progress_percent,
            step,
        } => Some(BridgeUiEvent::CandidateImportImporting {
            key,
            progress_percent,
            step: step.map(crate::types::import_step_to_bridge),
        }),
        UiBusEvent::CandidateImportComplete {
            key,
            release_id,
            album_id,
        } => Some(BridgeUiEvent::CandidateImportComplete {
            key,
            release_id,
            album_id,
        }),
        UiBusEvent::CandidateImportError { key, error } => {
            Some(BridgeUiEvent::CandidateImportError {
                key,
                error: crate::types::bridge_error(error),
            })
        }

        // ── Scan ───────────────────────────────────────────────────
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        UiBusEvent::WatchedFoldersChanged { folders } => {
            Some(BridgeUiEvent::WatchedFoldersChanged {
                folders: folders
                    .into_iter()
                    .map(crate::types::BridgeWatchedFolder::from_core)
                    .collect(),
            })
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        UiBusEvent::FolderCandidateAdded { candidate } => {
            let track_count = crate::bridge_utils::extract_track_count(&candidate.files)
                .expect("folder candidate must have a known track count");
            Some(BridgeUiEvent::FolderCandidateAdded {
                candidate: BridgeFolderCandidate {
                    folder_path: candidate.path.to_string_lossy().to_string(),
                    source_folder_name: candidate.name,
                    watched_folder_path: candidate.watched_folder_path,
                    files: categorized_files_to_bridge(candidate.files),
                    track_count,
                    skipped: candidate.skipped,
                    is_added: candidate.is_added,
                },
            })
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        UiBusEvent::InvalidCandidate { candidate } => Some(BridgeUiEvent::InvalidCandidate {
            candidate: BridgeInvalidCandidate {
                folder_path: candidate.path.to_string_lossy().to_string(),
                source_folder_name: candidate.name,
                watched_folder_path: candidate.watched_folder_path,
                reason: crate::types::invalid_reason_to_bridge(candidate.reason),
            },
        }),
        UiBusEvent::ScanCandidateRemoved { key } => {
            Some(BridgeUiEvent::ScanCandidateRemoved { key })
        }
        UiBusEvent::CandidateSkipChanged { key, skipped } => {
            Some(BridgeUiEvent::CandidateSkipChanged { key, skipped })
        }
        UiBusEvent::ScanFinished => Some(BridgeUiEvent::ScanFinished),

        // ── Library ────────────────────────────────────────────────
        UiBusEvent::AlbumAdded { album } => Some(BridgeUiEvent::AlbumAdded {
            album: convert_album_detail(album),
        }),
        UiBusEvent::AlbumUpdated { album } => Some(BridgeUiEvent::AlbumUpdated {
            album: convert_album_detail(album),
        }),
        UiBusEvent::AlbumRemoved {
            album_id,
            release_ids,
        } => Some(BridgeUiEvent::AlbumRemoved {
            album_id,
            release_ids,
        }),
        UiBusEvent::ReleaseAdded { album, release } => Some(BridgeUiEvent::ReleaseAdded {
            album: convert_album_summary(album),
            release: convert_release_detail(release),
        }),
        UiBusEvent::ReleaseUpdated { album_id, release } => Some(BridgeUiEvent::ReleaseUpdated {
            album_id,
            release: convert_release_detail(release),
        }),
        UiBusEvent::ReleaseRemoved {
            album_id,
            release_id,
            album,
        } => Some(BridgeUiEvent::ReleaseRemoved {
            album_id,
            release_id,
            album: album.map(convert_album_summary),
        }),
        UiBusEvent::ConfigChanged { config, sync_ready } => Some(BridgeUiEvent::ConfigChanged {
            config: build_bridge_config(&config),
            sync_ready,
        }),
        UiBusEvent::SyncError { error } => Some(BridgeUiEvent::SyncError {
            error: error.map(crate::types::bridge_error),
        }),
        UiBusEvent::SyncTimeChanged { time } => Some(BridgeUiEvent::SyncTimeChanged { time }),
        UiBusEvent::SyncingChanged { syncing } => Some(BridgeUiEvent::SyncingChanged { syncing }),
        UiBusEvent::OutboxChanged { snapshot } => Some(BridgeUiEvent::OutboxChanged {
            snapshot: convert_outbox_snapshot(snapshot),
        }),
        UiBusEvent::ReleaseTransferProgress {
            release_id,
            action,
            file_no,
            total,
            percent,
        } => Some(BridgeUiEvent::ReleaseTransferProgress {
            release_id,
            action: crate::types::BridgeReleaseStorageAction::from_core(action),
            file_no,
            total,
            percent,
        }),
        UiBusEvent::ReleaseTransferEnded { release_id } => {
            Some(BridgeUiEvent::ReleaseTransferEnded { release_id })
        }
        UiBusEvent::DownloadQueueChanged { snapshot } => {
            Some(BridgeUiEvent::DownloadQueueChanged {
                snapshot: convert_download_snapshot(snapshot),
            })
        }

        // ── Errors ─────────────────────────────────────────────────
        UiBusEvent::Error { error } => Some(BridgeUiEvent::Error {
            error: crate::types::bridge_error(error),
        }),
        UiBusEvent::ErrorCleared => Some(BridgeUiEvent::ErrorCleared),
    }
}

fn convert_release_detail(rel: bae_core::album_detail::ReleaseDetail) -> BridgeRelease {
    let convert_file = |f: bae_core::album_detail::FileDetail| BridgeFile {
        id: f.id,
        original_filename: f.original_filename,
        file_size: f.file_size,
        is_image: f.is_image,
        content_type: f.content_type,
        audio_format: f.audio_format.map(crate::types::audio_format_to_bridge),
    };
    let convert_gallery_item = |g: bae_core::album_detail::GalleryItem| BridgeGalleryItem {
        id: g.id,
        label: g.label,
        local_path: g.local_path,
    };
    let convert_track = |t: bae_core::album_detail::TrackDetail| BridgeTrack {
        id: t.id,
        title: t.title,
        side: t.side,
        track_number: t.track_number,
        duration_ms: t.duration_ms,
        artist_names: t.artist_names,
        position: crate::types::BridgeTrackPosition::from_core(t.position),
    };
    let summary = rel.summary;
    BridgeRelease {
        id: summary.id,
        album_id: summary.album_id,
        display_name: rel.display_name,
        release_name: rel.release_name,
        year: rel.year,
        format: summary.format,
        label: rel.label,
        catalog_number: rel.catalog_number,
        country: rel.country,
        storage_state: crate::types::BridgeReleaseStorageState::from_core(summary.storage_state),
        storage_actions: summary
            .storage_actions
            .into_iter()
            .map(crate::types::BridgeReleaseStorageAction::from_core)
            .collect(),
        total_duration_ms: rel.total_duration_ms,
        tracks: rel.tracks.into_iter().map(convert_track).collect(),
        track_groups: rel
            .track_groups
            .into_iter()
            .map(|g| BridgeTrackGroup {
                side: crate::types::BridgeTrackSide::from_core(g.side),
                tracks: g.tracks.into_iter().map(convert_track).collect(),
            })
            .collect(),
        image_files: rel.image_files.into_iter().map(convert_file).collect(),
        files: rel.files.into_iter().map(convert_file).collect(),
        gallery_items: rel
            .gallery_items
            .into_iter()
            .map(convert_gallery_item)
            .collect(),
        file_count: summary.file_count,
        total_size: summary.total_size,
        cover_path: summary.cover_path,
    }
}

fn convert_storage_row(raw: bae_core::album_detail::StorageRow) -> BridgeStorageRow {
    BridgeStorageRow {
        release: convert_release_summary(raw.release),
        album: convert_album_summary(raw.album),
    }
}

fn convert_release_summary(s: bae_core::album_detail::ReleaseSummary) -> BridgeReleaseSummary {
    BridgeReleaseSummary {
        id: s.id,
        album_id: s.album_id,
        format: s.format,
        storage_state: crate::types::BridgeReleaseStorageState::from_core(s.storage_state),
        storage_actions: s
            .storage_actions
            .into_iter()
            .map(crate::types::BridgeReleaseStorageAction::from_core)
            .collect(),
        file_count: s.file_count,
        total_size: s.total_size,
        cover_path: s.cover_path,
    }
}

fn convert_album_detail(detail: bae_core::album_detail::AlbumDetail) -> BridgeAlbumDetail {
    let release_ids: Vec<String> = detail
        .releases
        .iter()
        .map(|r| r.summary.id.clone())
        .collect();
    let releases: Vec<BridgeRelease> = detail
        .releases
        .into_iter()
        .map(convert_release_detail)
        .collect();

    BridgeAlbumDetail {
        album: BridgeAlbum {
            id: detail.album.id,
            title: detail.album.title,
            year: detail.album.year,
            is_compilation: detail.album.is_compilation,
            artist_names: detail.artist_names,
            primary_release_id: detail.primary_release_id,
            release_ids,
            cover_path: detail.cover_path,
        },
        releases,
    }
}

fn convert_album_summary(a: bae_core::album_detail::AlbumSummary) -> BridgeAlbum {
    BridgeAlbum {
        id: a.id,
        title: a.title,
        year: a.year,
        is_compilation: a.is_compilation,
        artist_names: a.artist_names,
        release_ids: a.release_ids,
        primary_release_id: a.primary_release_id,
        cover_path: a.cover_path,
    }
}

fn bridge_album_to_summary(a: BridgeAlbum) -> bae_core::album_detail::AlbumSummary {
    bae_core::album_detail::AlbumSummary {
        id: a.id,
        title: a.title,
        year: a.year,
        is_compilation: a.is_compilation,
        artist_names: a.artist_names,
        release_ids: a.release_ids,
        primary_release_id: a.primary_release_id,
        cover_path: a.cover_path,
    }
}

/// Order an in-memory album list using the same comparator the live SQL
/// query applies. The live grid sorts via the database; previews and
/// tests build rows by hand and call this so their ordering matches
/// what users see. See [`bae_core::db::sort_albums`] for the rule set.
#[uniffi::export]
pub fn sort_albums(
    albums: Vec<BridgeAlbum>,
    criteria: Vec<BridgeSortCriterion>,
) -> Vec<BridgeAlbum> {
    let mut summaries: Vec<bae_core::album_detail::AlbumSummary> =
        albums.into_iter().map(bridge_album_to_summary).collect();
    let core_criteria: Vec<bae_core::db::AlbumSortCriterion> =
        criteria.iter().map(bridge_sort_to_core).collect();
    bae_core::db::sort_albums(&mut summaries, &core_criteria);
    summaries.into_iter().map(convert_album_summary).collect()
}

/// Project a prefetched release detail into the editor's user-edit shape,
/// honoring the user's identity choice. Exact keeps pressing fields;
/// Approximate / Unknown nil them. Per-track artist overrides are emptied
/// when they equal the album artist. Stateless type-translation wrapper
/// around [`bae_core::import::shape_user_edit_from_search_detail`]: the
/// caller (Swift) must not branch on `IdentityChoice` itself.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn shape_user_edit_from_release_detail(
    detail: crate::types::BridgeReleaseDetail,
    choice: crate::types::BridgeIdentityChoice,
) -> crate::types::BridgeReleaseUserEdit {
    let core_detail = crate::types::release_detail_from_bridge(detail);
    let core_choice = choice.to_core();
    let edit = bae_core::import::shape_user_edit_from_search_detail(&core_detail, &core_choice);
    crate::types::release_user_edit_to_bridge(edit)
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
    let core_raw = crate::types::raw_release_edit_from_bridge(raw);
    match core_raw.shape() {
        Ok(edit) => crate::types::BridgeShapeResult::Valid {
            edit: crate::types::release_user_edit_to_bridge(edit),
        },
        Err(e) => crate::types::BridgeShapeResult::Invalid {
            reason: validation_reason_from_core(e),
        },
    }
}

/// Map bae-core's validation error to its bridge mirror. Kept here, not as a
/// `From` in bae-core, so bae-core stays unaware of bridge types.
#[cfg(feature = "desktop")]
fn validation_reason_from_core(
    e: bae_core::import::EditValidationError,
) -> crate::types::BridgeValidationReason {
    use crate::types::BridgeValidationReason as R;
    use bae_core::import::EditValidationError as E;
    match e {
        E::EmptyAlbumTitle => R::EmptyAlbumTitle,
        E::NoAlbumArtist => R::NoAlbumArtist,
        E::InvalidYear => R::InvalidYear,
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
    let core_edit = crate::types::release_user_edit_from_bridge(edit);
    let raw = bae_core::import::RawReleaseEdit::from_user_edit(core_edit, &track_id_prefix);
    crate::types::raw_release_edit_to_bridge(raw)
}

#[cfg(test)]
mod tests {
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

    /// A consumer that falls behind the broadcast bus gets `Lagged`, not
    /// `Closed`. The pump must keep delivering events after the gap — a lag
    /// during an event burst must not freeze the UI for the rest of the
    /// session.
    #[tokio::test]
    async fn pump_ui_events_survives_broadcast_lag() {
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
        assert!(
            matches!(
                events.as_slice(),
                [crate::types::BridgeUiEvent::MuteChanged { is_muted: true }]
            ),
            "expected the event behind the lag to still be delivered, got: {events:?}",
        );
    }
}
