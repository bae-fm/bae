//! A library a unit test opens as the app opens one, and the parts a test
//! starts over it: the whole of the library's services through the app's own
//! step, or one service alone for a test that drives its internals.

use crate::db::Database;
use crate::import::{CatalogSessions, ImportServiceHandle};
use crate::library::{AppServices, CancellationToken, LibraryManager};
use bae_catalogs::remote_image::RemoteImageCache;
use bae_diagnostics::Diagnostics;
use coven::{ClockRef, IdSourceRef};
use std::sync::Arc;

/// An open test library: its manager, which it reads as, and what the app
/// would hand the library's services beside it.
pub(crate) struct TestLibrary {
    manager: LibraryManager,
    database: Database,
    catalogs: CatalogSessions,
    closing: CancellationToken,
    clock: ClockRef,
    ids: IdSourceRef,
    diagnostics: Diagnostics,
    remote_images: RemoteImageCache,
}

impl std::ops::Deref for TestLibrary {
    type Target = LibraryManager;
    fn deref(&self) -> &LibraryManager {
        &self.manager
    }
}

impl TestLibrary {
    /// Open the library over `database` on the real clock and ids, asking
    /// `providers`, reporting nowhere.
    pub(crate) async fn open(
        database: Database,
        app_dir: crate::app_dir::AppDir,
        providers: bae_catalogs::providers::Providers,
    ) -> Self {
        Self::open_with(
            database,
            app_dir,
            providers,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidIds),
            Diagnostics::noop(),
        )
        .await
    }

    /// Open the library over `database` through the app's opening step, on
    /// `clock` and `ids`, reporting to `diagnostics`.
    pub(crate) async fn open_with(
        database: Database,
        app_dir: crate::app_dir::AppDir,
        providers: bae_catalogs::providers::Providers,
        clock: ClockRef,
        ids: IdSourceRef,
        diagnostics: Diagnostics,
    ) -> Self {
        let remote_images = RemoteImageCache::for_test(providers.http().clone());
        let catalogs = CatalogSessions::new(
            providers,
            database.clone(),
            diagnostics.clone(),
            tokio::runtime::Handle::current(),
        );
        let closing = CancellationToken::new();
        let manager = super::open_library_manager(
            database.clone(),
            catalogs.clone(),
            closing.clone(),
            app_dir,
            clock.clone(),
            ids.clone(),
            diagnostics.clone(),
            tokio::runtime::Handle::current(),
            remote_images.clone(),
        )
        .await
        .expect("open the test library");
        Self { manager, database, catalogs, closing, clock, ids, diagnostics, remote_images }
    }

    /// The library's store.
    pub(crate) fn database(&self) -> &Database {
        &self.database
    }

    /// The library's views over its store.
    pub(crate) fn views(&self) -> crate::library::views::LibraryViews {
        crate::library::views::LibraryViews::new(self.database.clone())
    }

    /// The catalog sessions the library asks through.
    pub(crate) fn catalogs(&self) -> &CatalogSessions {
        &self.catalogs
    }

    /// The library's services, started through the app's own step, with
    /// playback on a device that plays nothing and no stored queue restored.
    pub(crate) async fn start_services(&self) -> Result<AppServices, super::ServicesStartError> {
        super::start_library_services(
            self.manager.clone(),
            self.database.clone(),
            self.catalogs.clone(),
            self.closing.clone(),
            self.clock.clone(),
            self.ids.clone(),
            self.diagnostics.clone(),
            tokio::runtime::Handle::current(),
            self.remote_images.clone(),
            50,
            false,
            Arc::new(crate::playback::WallPlaybackClock::new(self.clock.clone())),
            super::PlaybackOutput::Device(Box::new(
                crate::playback::audio_output::FailingAudioDevice,
            )),
        )
        .await
    }

    /// The import service alone, over the library's store, ending as the
    /// library closes.
    pub(crate) async fn start_import(
        &self,
    ) -> Result<ImportServiceHandle, crate::import::ImportError> {
        crate::import::ImportService::start(
            tokio::runtime::Handle::current(),
            self.database.clone(),
            self.catalogs.clone(),
            self.remote_images.clone(),
            self.diagnostics.clone(),
            crate::import::CandidatePreparations::new(self.database.clone()),
            self.clock.clone(),
            self.ids.clone(),
            self.closing.child_token(),
        )
        .await
    }

    /// [`Self::start_import`] reading folders' tags through `file_tags`.
    pub(crate) fn start_import_reading_tags_with(
        &self,
        file_tags: Arc<dyn crate::import::file_tag_snapshot::FileTagReader>,
    ) -> ImportServiceHandle {
        crate::import::ImportService::start_reading_tags_with(
            tokio::runtime::Handle::current(),
            self.database.clone(),
            self.catalogs.clone(),
            self.remote_images.clone(),
            self.diagnostics.clone(),
            crate::import::CandidatePreparations::new(self.database.clone()),
            self.clock.clone(),
            self.ids.clone(),
            self.closing.child_token(),
            file_tags,
        )
    }

    /// The playback service alone, on a device that plays nothing, with no
    /// stored queue restored.
    pub(crate) async fn start_playback(&self) -> crate::playback::PlaybackHandle {
        self.start_playback_with(
            Box::new(crate::playback::audio_output::FailingAudioDevice),
            50,
            false,
        )
        .await
    }

    /// The playback service alone, playing on `device`, reporting its
    /// position every `position_update_interval_ms`, and resuming the stored
    /// queue when `restore_playback`.
    pub(crate) async fn start_playback_with(
        &self,
        device: Box<dyn crate::playback::AudioOutputDevice>,
        position_update_interval_ms: u32,
        restore_playback: bool,
    ) -> crate::playback::PlaybackHandle {
        self.manager
            .settle_playback_queue(restore_playback)
            .await
            .expect("settle the playback queue");
        crate::playback::PlaybackService::start_with_audio_device(
            self.database.clone(),
            self.ids.clone(),
            crate::playback::tables::PlaybackQueueTables::new(self.database.clone()),
            crate::playback::tables::PlaybackNowRow::new(self.database.clone()),
            crate::playback::tables::PlaybackTimelineRow::new(self.database.clone()),
            self.manager.subscribe_track_deletions(),
            self.diagnostics.clone(),
            tokio::runtime::Handle::current(),
            position_update_interval_ms,
            restore_playback,
            device,
            Arc::new(crate::playback::WallPlaybackClock::new(self.clock.clone())),
        )
    }
}
