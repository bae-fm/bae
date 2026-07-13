use super::manager::LibraryManager;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::identify::IdentifyServiceHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::ImportServiceHandle;
use crate::playback::PlaybackHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::signals::ExtractionServiceHandle;
use std::sync::Arc;

struct AppServicesInner {
    manager: LibraryManager,
    playback: PlaybackHandle,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    import: ImportServiceHandle,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    identify: IdentifyServiceHandle,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    extraction: ExtractionServiceHandle,
}

impl Drop for AppServicesInner {
    /// Stop and join the playback service thread when the last `AppServices` clone
    /// drops — i.e. when the app is torn down. That thread runs on its own OS
    /// thread and, because the service loop holds its own command sender, only
    /// stops on an explicit `Shutdown`; nothing else joins it. Until it exits it
    /// holds a `LibraryManager` clone, and through the shared coven handle that
    /// pins the store's exclusive open lock — so without this teardown the same
    /// library can't be reopened in-process (a `RunningApp` dropped without the
    /// graceful `shutdown` leaks the thread and the lock). A no-op if `shutdown`
    /// already ran: the join handle is taken once.
    fn drop(&mut self) {
        self.playback.stop_and_join();
    }
}

/// The running application: library data layer + all service handles.
#[derive(Clone)]
pub struct AppServices {
    inner: Arc<AppServicesInner>,
}

impl std::fmt::Debug for AppServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppServices")
            .field("manager", &self.inner.manager)
            .finish_non_exhaustive()
    }
}

impl AppServices {
    pub fn new(
        manager: LibraryManager,
        playback: PlaybackHandle,
        #[cfg(not(any(target_os = "ios", target_os = "android")))] import: ImportServiceHandle,
        #[cfg(not(any(target_os = "ios", target_os = "android")))] identify: IdentifyServiceHandle,
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        extraction: ExtractionServiceHandle,
    ) -> Self {
        AppServices {
            inner: Arc::new(AppServicesInner {
                manager,
                playback,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                import,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                identify,
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                extraction,
            }),
        }
    }

    pub fn library_manager(&self) -> &LibraryManager {
        &self.inner.manager
    }

    pub async fn get_queue_snapshot(
        &self,
    ) -> Result<crate::queue::ResolvedQueueSnapshot, crate::library::LibraryError> {
        let projection = self
            .inner
            .playback
            .queue_projection()
            .await
            .map_err(crate::library::LibraryError::Playback)?;
        self.inner
            .manager
            .resolve_queue_projection(projection)
            .await
    }

    /// Fetch one page of the context's not-yet-played tail, past the window
    /// `get_queue_snapshot` already resolved. Offset 0 is the first
    /// not-yet-played entry after the current track — the same coordinate
    /// space as the snapshot's `context.upcoming` window. Clamped to the
    /// tail's bounds; empty when nothing is playing from a context or the
    /// offset is past its end. The page is stamped with the live
    /// `PlaybackQueue` revision so the caller can drop it if a `QueueUpdated`
    /// for a newer revision has since arrived.
    pub async fn get_queue_upcoming_page(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<crate::queue::ResolvedQueueUpcomingPage, crate::library::LibraryError> {
        let projection = self
            .inner
            .playback
            .queue_projection()
            .await
            .map_err(crate::library::LibraryError::Playback)?;
        let tail: &[crate::playback::QueueEntry] = projection
            .context
            .as_ref()
            .map(|c| c.upcoming.as_slice())
            .unwrap_or(&[]);
        let page = crate::queue::clamp_upcoming_page(tail, offset, limit);
        let items = self.inner.manager.get_queue_items(page).await?;
        Ok(crate::queue::ResolvedQueueUpcomingPage {
            revision: projection.revision,
            items,
        })
    }

    pub fn get_sync_status(&self) -> crate::library::SyncStatusSnapshot {
        self.inner.manager.get_sync_status()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn get_import_candidates(&self) -> crate::import::ImportCandidatesSnapshot {
        self.inner.import.get_import_candidates()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn get_candidate(&self, key: &str) -> Option<crate::import::ImportCandidateSnapshot> {
        self.inner.import.get_candidate(key)
    }

    pub fn playback(&self) -> &PlaybackHandle {
        &self.inner.playback
    }

    /// Set whether playback pauses between vinyl/cassette sides. Turning it on
    /// must take effect at the boundary already staged for gapless playback,
    /// not just the next one: `preload_next_track` decides staging once, at
    /// preload time, so writing the config alone leaves an already-staged
    /// track to cross gaplessly. Turning it off needs no follow-up — the
    /// drain-time gate (`side_pause_for_queue_front`) already re-reads the
    /// config before every boundary.
    pub fn set_pause_between_sides(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.inner.manager.set_pause_between_sides(enabled)?;
        if enabled {
            self.inner.playback.reevaluate_side_pause_staging();
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn import(&self) -> &ImportServiceHandle {
        &self.inner.import
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn identify(&self) -> &IdentifyServiceHandle {
        &self.inner.identify
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn extraction(&self) -> &ExtractionServiceHandle {
        &self.inner.extraction
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigHandle};
    use crate::db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack};
    use crate::keys::StoreKeys;
    use coven::StoreDir;
    use tempfile::TempDir;

    /// Build a real `AppServices` — library manager, actor-backed playback,
    /// and (natively) the import/identify/extraction trio — wired up exactly
    /// as `bootstrap` does for desktop, seeded with one release of
    /// `track_count` tracks. Starts playing the release from its first track,
    /// so by the time this returns the queue's context is a real,
    /// actor-resolved tail — not a hand-built `PlaybackQueueProjection` — for
    /// `get_queue_upcoming_page` to page through. The seeded tracks have no
    /// backing audio file, so preparing the first track for playback fails
    /// fast (a DB lookup, no I/O); that failure only stops playback, it
    /// doesn't touch the queue the `PlayRelease` command already set.
    async fn playing_app_services(track_count: usize) -> (AppServices, Vec<String>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let database = Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
            .await
            .unwrap();

        let artist = DbArtist {
            id: "test-artist-id".to_string(),
            name: "Test Artist".to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: chrono::Utc::now(),
        };
        database.insert_artist(&artist).await.unwrap();
        let album = DbAlbum::new_test("Album Title", &artist.id);
        let release = DbRelease::new_test(&album.id, "release-under-test");
        database.insert_album(&album).await.unwrap();
        database.insert_release(&release).await.unwrap();
        let mut track_ids = Vec::with_capacity(track_count);
        for i in 0..track_count {
            let track_id = format!("track-{i}");
            let track = DbTrack::new_test(
                &release.id,
                &track_id,
                &format!("Track {i}"),
                Some(i as i32),
            );
            database.insert_track(&track).await.unwrap();
            track_ids.push(track_id);
        }

        let library_id = format!("app-services-test-{}", uuid::Uuid::new_v4());
        let config = Config::with_defaults(
            library_id.clone(),
            "test-device".to_string(),
            StoreDir::new(temp_dir.path().to_path_buf()),
            "Test Library".to_string(),
        );
        let config_handle = Arc::new(ConfigHandle::new(config));
        crate::config::install_test_keyring();
        let key_service = StoreKeys::new(library_id);
        let manager = LibraryManager::new(
            database,
            config_handle,
            key_service,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            crate::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
        );

        let playback = crate::playback::PlaybackService::start(
            manager.clone(),
            tokio::runtime::Handle::current(),
            50,
            false,
        );

        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let services = {
            let cover_art = crate::import::cover_art::CoverArtArchiveClient::hermetic();
            let import = crate::import::ImportService::start(
                tokio::runtime::Handle::current(),
                manager.clone(),
                cover_art.clone(),
            );
            let identify = crate::identify::IdentifyServiceHandle::new(
                manager.clone(),
                tokio::runtime::Handle::current(),
                import.event_sender_for_test(),
                cover_art,
            );
            let extraction = crate::signals::ExtractionService::start(
                tokio::runtime::Handle::current(),
                import.event_sender_for_test(),
                Arc::new(coven::SystemClock),
                manager.clone(),
            );
            AppServices::new(manager, playback, import, identify, extraction)
        };
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let services = AppServices::new(manager, playback);

        // Dispatched here and only awaited by this function's callers'
        // subsequent round-tripping queries on the same command channel —
        // the actor processes commands FIFO, so `PlayRelease` (which sets
        // the queue's context before it attempts to play the first track)
        // is guaranteed to have run before any later `queue_projection`
        // request this test makes.
        services
            .playback()
            .play_release(release.id.clone(), Some(0), false);

        (services, track_ids, temp_dir)
    }

    /// `get_queue_upcoming_page` slices a real, actor-resolved context tail —
    /// not a hand-built projection — in order, and stamps the page with the
    /// same revision the live queue's own snapshot reports.
    #[tokio::test]
    async fn get_queue_upcoming_page_slices_and_orders_a_live_context_tail() {
        let (services, track_ids, _temp_dir) = playing_app_services(12).await;

        let snapshot = services.get_queue_snapshot().await.unwrap();
        let page = services.get_queue_upcoming_page(2, 5).await.unwrap();

        assert_eq!(
            page.revision, snapshot.revision,
            "the page is stamped with the same revision as the live queue's own snapshot"
        );
        let page_track_ids: Vec<&str> = page.items.iter().map(|i| i.track_id.as_str()).collect();
        // track_ids[0] is the currently playing track, so the context tail
        // is track_ids[1..]; offset 2 into that tail lands on track_ids[3].
        let expected: Vec<&str> = track_ids[3..8].iter().map(String::as_str).collect();
        assert_eq!(
            page_track_ids, expected,
            "the slice preserves the tail's order"
        );
    }

    /// A limit reaching past the live tail's end clamps to what remains, and
    /// an offset past the end returns no items rather than erroring — both
    /// through the real `AppServices` -> `PlaybackHandle` -> `PlaybackQueue`
    /// chain, not `clamp_upcoming_page` called directly.
    #[tokio::test]
    async fn get_queue_upcoming_page_clamps_to_the_live_tails_end() {
        let (services, track_ids, _temp_dir) = playing_app_services(12).await;

        // The tail has 11 entries (track_ids[1..12]); offset 9 has only 2
        // left, so a limit of 100 clamps down to those 2.
        let page = services.get_queue_upcoming_page(9, 100).await.unwrap();
        let page_track_ids: Vec<&str> = page.items.iter().map(|i| i.track_id.as_str()).collect();
        assert_eq!(
            page_track_ids,
            vec![track_ids[10].as_str(), track_ids[11].as_str()],
            "the limit clamps to what remains instead of erroring"
        );

        let empty_page = services.get_queue_upcoming_page(50, 10).await.unwrap();
        assert!(
            empty_page.items.is_empty(),
            "an offset past the tail's end yields no items"
        );
        assert_eq!(
            empty_page.revision, page.revision,
            "revision is stable across reads that don't mutate the queue"
        );
    }
}
