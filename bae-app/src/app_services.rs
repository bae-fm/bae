#[cfg(desktop)]
use bae_import::IdentificationHandle;
#[cfg(desktop)]
use bae_import::ImportServiceHandle;
use bae_library::LibraryManager;
use bae_playback::PlaybackHandle;
use std::sync::Arc;

/// Why switching a metadata source on or off failed: the preference was
/// refused or could not be written, or the searches asking the source could
/// not be closed after it was.
#[derive(Debug, thiserror::Error)]
pub enum MetadataSourceSwitchError {
    #[error(transparent)]
    Preference(#[from] bae_preferences::PreferencesError),
    #[error(transparent)]
    Searches(#[from] bae_library::LibraryError),
}

macro_rules! delegate_sync {
    ($field:ident, $name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty) => {
        pub fn $name(&self, $($arg: $ty),*) -> $ret {
            self.inner.$field.$target($($arg),*)
        }
    };
}

/// A live query of the manager's, as a surface pulls its values: the first
/// at once, then one after each write that changes it, until the surface
/// cancels or the library closes.
macro_rules! delegate_values {
    ($name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $value:ty) => {
        pub fn $name(&self, $($arg: $ty),*) -> bae_store::live_query::LiveValues<$value> {
            let manager = &self.inner.manager;
            manager.values_until_closed(manager.$target($($arg),*))
        }
    };
}

macro_rules! delegate_async {
    ($field:ident, $name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty) => {
        pub async fn $name(&self, $($arg: $ty),*) -> $ret {
            self.inner.$field.$target($($arg),*).await
        }
    };
}

/// A read the library's views answer as it is, its failure told as a
/// library error.
macro_rules! delegate_view {
    ($name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $ok:ty) => {
        pub async fn $name(&self, $($arg: $ty),*) -> Result<$ok, bae_library::LibraryError> {
            Ok(self.inner.views.$target($($arg),*).await?)
        }
    };
}

struct AppServicesInner {
    manager: LibraryManager,
    /// The library's views, for the reads a screen shows as they are.
    views: bae_views::LibraryViews,
    playback: PlaybackHandle,
    /// The cast picker's list, which casting writes and the picker reads.
    cast_picker: bae_playback::renderer::CastPicker,
    /// The import service, which also owns the identify driver and the
    /// extraction feeding it: one of each per library, reached through it.
    #[cfg(desktop)]
    import: ImportServiceHandle,
    /// Queue-wide identification, started as the library's services start:
    /// the queue runs whether or not anyone has the Import section open, and
    /// opening a view is not what starts it.
    #[cfg(desktop)]
    identification: IdentificationHandle,
    /// What each library release being identified again asks about, by its
    /// session's candidate key. A release has no candidate row to store them
    /// on, so they are held here from the session's first run to its end.
    #[cfg(desktop)]
    release_lookup_choices: std::sync::Mutex<
        std::collections::HashMap<bae_import::CandidateKey, bae_identify::LookupChoices>,
    >,
}

impl Drop for AppServicesInner {
    /// Stop and join the playback and import worker threads when the last
    /// `AppServices` clone drops — i.e. when the app is torn down. Each runs on
    /// its own OS thread and, because a live command sender sits in this very
    /// struct, only stops on an explicit `Shutdown`; nothing else joins them.
    /// Until a thread exits it holds a `LibraryManager` clone, and through the
    /// shared coven handle that pins the store's exclusive open lock — so
    /// without joining *every* such thread the same library can't be reopened
    /// in-process. No-ops if [`AppServices::close`] or a playback `shutdown`
    /// already ran: each join handle is taken once.
    fn drop(&mut self) {
        self.playback.stop_and_join();
        #[cfg(desktop)]
        {
            // Before the import worker's join: the queue's in-flight
            // candidates are cancelled here, and a cancelled candidate writes
            // no row.
            self.identification.stop();
            self.import.stop_and_join();
        }
    }
}

/// The running application: library data layer + all service handles.
#[derive(Clone)]
pub struct AppServices {
    inner: Arc<AppServicesInner>,
}

impl std::fmt::Debug for AppServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppServices").field("manager", &self.inner.manager).finish_non_exhaustive()
    }
}

impl AppServices {
    pub fn new(
        manager: LibraryManager,
        views: bae_views::LibraryViews,
        playback: PlaybackHandle,
        cast_picker: bae_playback::renderer::CastPicker,
        #[cfg(desktop)] import: ImportServiceHandle,
        #[cfg(desktop)] identification: IdentificationHandle,
    ) -> Self {
        AppServices {
            inner: Arc::new(AppServicesInner {
                manager,
                views,
                playback,
                cast_picker,
                #[cfg(desktop)]
                import,
                #[cfg(desktop)]
                identification,
                #[cfg(desktop)]
                release_lookup_choices: Default::default(),
            }),
        }
    }

    /// Close the library: stop playback, saving where it was, then the
    /// identification queue and the import service, joining each worker, and
    /// close the store last. When this returns no service holds a file of the
    /// library, the track it was playing included, or the store's lock. The
    /// order is the one dropping the last handle to these services keeps;
    /// whichever runs first stops each worker, and the other finds it stopped.
    /// A step that panics is reported as a bug and fails the close, and the
    /// steps after it still run, the store's close among them.
    pub async fn close(&self) -> Result<(), bae_library::LibraryError> {
        self.close_reporting(&|_| ()).await
    }

    /// [`Self::close`], telling `steps` as each step starts.
    pub async fn close_reporting(
        &self,
        steps: &(dyn Fn(bae_library::ClosingStep) + Send + Sync),
    ) -> Result<(), bae_library::LibraryError> {
        let stopped = self.stop_workers(steps).await;
        let closed = self.inner.manager.close_reporting(steps).await;
        match (stopped, closed) {
            (Ok(()), closed) => closed,
            (Err(stopped), Ok(())) => Err(stopped),
            (Err(stopped), Err(closed)) => Err(bae_library::LibraryError::Cleanup {
                operation: Box::new(stopped),
                cleanup: Box::new(closed),
            }),
        }
    }

    /// Report a bug found in work on these services that has no caller to
    /// fail, such as closing a library whose handle was released, to the log
    /// and the platform's crash reporter.
    pub fn report_bug(&self, operation: &str, error: &dyn std::fmt::Display) {
        self.inner.manager.report_bug(operation, error);
    }

    /// Run `task` as the library's own work until it ends, `stop` is
    /// cancelled, or the library closes. Closing or removing the library ends
    /// it with the manager's other tasks, before the store closes, so it never
    /// reads a closed store. A panic in it is reported as a bug. A library
    /// that has closed starts nothing and says so.
    pub fn spawn_library_task(
        &self,
        task_name: &'static str,
        stop: bae_library::CancellationToken,
        task: impl std::future::Future<Output = ()> + Send + 'static,
    ) -> Result<(), bae_library::LibraryError> {
        self.inner.manager.spawn_stoppable_task(task_name, stop, task, |_| {})
    }

    /// Stop playback, then identification and import, telling `steps` as
    /// each starts.
    async fn stop_workers(
        &self,
        steps: &(dyn Fn(bae_library::ClosingStep) + Send + Sync),
    ) -> Result<(), bae_library::LibraryError> {
        steps(bae_library::ClosingStep::StoppingPlayback);
        self.inner.playback.shutdown().await;
        #[cfg(desktop)]
        {
            steps(bae_library::ClosingStep::StoppingImports);
            let identification = self.inner.identification.clone();
            let import = self.inner.import.clone();
            // Both joins block until their threads exit, so they run off the
            // runtime rather than stall one of its threads.
            bae_util::task_failure::blocking("stop identification and import", move || {
                // Before the import worker's join, as in the drop: a cancelled
                // candidate writes no row.
                identification.stop();
                import.stop_and_join();
            })
            .await
            .map_err(|failed| self.inner.manager.bug(failed))?;
        }
        Ok(())
    }

    /// The library's preferences as the settings screens read them: the
    /// first value at once, then one after each write that changes them.
    pub fn subscribe_config(&self) -> bae_store::live_query::LiveValues<bae_preferences::Config> {
        self.inner.manager.subscribe_config()
    }

    /// The part of the preferences `part` takes: the first value at once,
    /// then one after each write that changes it.
    pub fn follow_preferences<T: Clone + PartialEq + Send + 'static>(
        &self,
        part: impl Fn(bae_preferences::Preferences) -> T + Send + Sync + 'static,
    ) -> bae_store::live_query::LiveValues<T> {
        self.inner.manager.follow_preferences(part)
    }

    pub fn subscribe_album_browse(
        &self,
        sort: &[bae_views::AlbumSortCriterion],
        group_by_artist: bool,
    ) -> bae_views::LiveAlbumBrowse {
        let manager = &self.inner.manager;
        let query = manager.until_closed(self.inner.views.subscribe_album_browse(
            sort,
            group_by_artist,
            std::collections::BTreeSet::new(),
        ));
        bae_views::LiveLibraryBrowse::new(query, bae_views::album::resolve_album_browse)
    }

    /// Which of the albums the grid has selected are gone from the library,
    /// as one live query whose ids move in place as the selection changes; it
    /// starts with none.
    pub fn subscribe_album_selection(&self) -> bae_library::LiveAlbumSelection {
        let manager = &self.inner.manager;
        bae_library::LiveAlbumSelection::new(manager.until_closed(
            self.inner.views.subscribe_album_selection(std::collections::BTreeSet::new()),
        ))
    }

    /// One live library search, pointed at a new query in place as the
    /// person types; it starts with no query.
    pub fn subscribe_library_search(&self) -> bae_views::LiveLibrarySearch {
        let manager = self.inner.manager.clone();
        let query = manager.until_closed(
            self.inner.views.subscribe_library_search(None, bae_views::SEARCH_RESULT_LIMIT),
        );
        bae_views::LiveLibrarySearch::new(query, move |projection| {
            bae_views::album::resolve_library_search_projection(projection)
        })
    }

    /// One live search of the library's artists for an artist picker,
    /// pointed at a new text and new windows in place as the person types and
    /// scrolls; it starts with no text and no windows.
    pub fn subscribe_artist_search(&self) -> bae_views::LiveArtistSearch {
        let manager = &self.inner.manager;
        bae_views::LiveArtistSearch::new(
            manager.until_closed(self.inner.views.subscribe_artist_search(
                bae_views::ArtistSearchView {
                    query: None,
                    windows: bae_store::LibraryPageWindows::new(),
                },
            )),
            bae_views::artist_search::resolve_artist_search,
        )
    }

    /// The library membership of the releases an import pane offers, as one
    /// live query whose checks move in place as the pane's offers change; it
    /// starts with none.
    pub fn subscribe_library_statuses(&self) -> bae_library::LiveLibraryStatus {
        let manager = &self.inner.manager;
        bae_library::LiveLibraryStatus::new(
            manager.until_closed(
                manager.subscribe_library_statuses(std::collections::BTreeSet::new()),
            ),
        )
    }

    pub fn subscribe_artist_browse(
        &self,
        sort: &[bae_views::ArtistSortCriterion],
    ) -> bae_views::LiveArtistBrowse {
        let manager = &self.inner.manager;
        let query = manager.until_closed(
            self.inner.views.subscribe_artist_browse(sort, std::collections::BTreeSet::new()),
        );
        bae_views::LiveLibraryBrowse::new(query, bae_views::artist::resolve_artist_browse)
    }

    pub fn subscribe_composer_browse(
        &self,
        sort: &[bae_views::ComposerSortCriterion],
    ) -> bae_views::LiveComposerBrowse {
        let manager = &self.inner.manager;
        let query = manager.until_closed(
            self.inner.views.subscribe_composer_browse(sort, std::collections::BTreeSet::new()),
        );
        bae_views::LiveLibraryBrowse::new(query, bae_views::composer::resolve_composer_browse)
    }

    /// The rows of `artist_id`'s composer page, reading no windows until
    /// they are set.
    pub fn subscribe_composer_detail_rows(
        &self,
        artist_id: &str,
    ) -> bae_views::LiveComposerDetailRows {
        let manager = &self.inner.manager;
        let query = manager.until_closed(
            self.inner
                .views
                .subscribe_composer_detail_rows(artist_id, std::collections::BTreeSet::new()),
        );
        bae_views::LiveLibraryBrowse::new(
            query,
            bae_views::composer_detail::resolve_composer_detail_rows,
        )
    }

    pub fn subscribe_playback_progress(&self) -> bae_playback::PlaybackProgressFeed {
        self.inner.playback.subscribe_progress()
    }

    delegate_async!(manager, preferences => preferences() -> Result<bae_preferences::Preferences, bae_store::DbError>);
    delegate_sync!(manager, library_path => library_path() -> std::path::PathBuf);
    delegate_sync!(manager, store_id => store_id() -> coven::StoreId);
    delegate_async!(manager, ensure_mcp_token => ensure_mcp_token() -> Result<String, bae_library::LibraryError>);
    delegate_async!(manager, set_mcp_token => set_mcp_token(token: String) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, set_mcp_config => set_mcp_config(config: bae_preferences::McpConfig) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, get_subsonic_password => get_subsonic_password() -> Result<Option<String>, bae_library::LibraryError>);
    delegate_async!(manager, set_subsonic_password => set_subsonic_password(password: String) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, set_subsonic_config => set_subsonic_config(config: bae_preferences::SubsonicConfig) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_cast_enabled => set_cast_enabled(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_cast_transcode_format => set_cast_transcode_format(format: bae_preferences::CastTranscodeFormat) -> Result<(), bae_preferences::PreferencesError>);
    delegate_sync!(manager, store_key_state => store_key_state() -> Result<coven::StoreKeyState, bae_library::LibraryError>);
    delegate_async!(manager, set_max_concurrent_uploads => set_max_concurrent_uploads(n: u32) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_max_concurrent_downloads => set_max_concurrent_downloads(n: u32) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_side_pause_countdown => set_side_pause_countdown(countdown: bae_preferences::SidePauseCountdown) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_show_remaining_time => set_show_remaining_time(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_library_full_width => set_library_full_width(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_identify_automatically => set_identify_automatically(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_import_when_identified => set_import_when_identified(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_import_to_cloud => set_import_to_cloud(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_import_pinned => set_import_pinned(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);

    /// Ask, or stop asking, one metadata source — the switch on the Find online
    /// header and in Settings, which are two views of this one preference.
    ///
    /// Written here rather than delegated, because switching a source off is
    /// not only a preference: everything already asking it has to stop. The
    /// live searches close that source's part, dropping the results it had
    /// found and the answer it still has out; every live identify run is
    /// replaced by one that reads the list as it now stands, so no run is left
    /// waiting on the source nobody is asking. This is the one object that
    /// holds the config, the searches, and the runs, which is why the order
    /// lives here.
    ///
    /// Switching a source *on* re-runs nothing: the next run or search asks it.
    /// Re-dispatching a settled run because a source became available would
    /// throw away an answer the person is reading.
    pub async fn set_metadata_source_enabled(
        &self,
        source: bae_catalogs::catalog::LookupCatalog,
        enabled: bool,
    ) -> Result<(), MetadataSourceSwitchError> {
        self.inner.manager.set_metadata_source_enabled(source, enabled).await?;
        #[cfg(desktop)]
        {
            if !enabled {
                // Searches first: a landing that races the switch then finds a
                // part that has stopped looking, and is dropped.
                self.inner
                    .import
                    .stop_asking_source(source)
                    .await
                    .map_err(bae_library::LibraryError::from)?;
                // A run reads the provider list once, at its start, so every
                // run in flight is answering the list as it was. Each is
                // superseded by a run that reads the list as it now is — every
                // one of them, not only the candidate whose pane the switch was
                // flicked on, because a background run waiting on a source
                // nobody asks is exactly as wrong as the open one. A candidate
                // that already settled has no run to supersede; its next run
                // reads the list as it stands then.
                for key in self.inner.import.identifying_keys() {
                    self.inner.identification.rerun_identify(key);
                }
            }
        }
        Ok(())
    }

    delegate_async!(manager, set_save_presets => set_save_presets(presets: Vec<bae_preferences::SavePreset>) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_default_track_save_preset => set_default_track_save_preset(preset_id: String) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_default_release_save_preset => set_default_release_save_preset(preset_id: String) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, rename_library => rename_library(name: &bae_store::EnteredName) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, forget_encryption_key => forget_encryption_key() -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, unlock_store_key => unlock_store_key() -> Result<(), bae_library::LibraryError>);
    delegate_sync!(manager, trigger_sync => trigger_sync() -> ());
    delegate_async!(manager, release_memory => release_memory() -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, retry_now => retry_now() -> Result<impl std::future::Future<Output = bae_db::SyncRetryOutcome> + Send + 'static, bae_library::LibraryError>);
    delegate_async!(manager, reconnect_sync => reconnect_sync() -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, set_downloads_paused => set_downloads_paused(paused: bool) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, cancel_download => cancel_download(release_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, retry_downloads => retry_downloads() -> Result<(), bae_library::LibraryError>);
    delegate_view!(get_artist_count => get_artist_count() -> u64);
    delegate_view!(get_artist_page => get_artist_page(sort: &[bae_views::ArtistSortCriterion], offset: u64, limit: u64) -> Vec<bae_views::album_detail::ArtistSummary>);
    delegate_view!(get_artist_detail => get_artist_detail(artist_id: &str) -> Option<bae_views::album_detail::ArtistDetail>);
    delegate_async!(manager, add_artist_credit_name => add_artist_credit_name(artist_id: &str, name: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, rename_artist_credit_name => rename_artist_credit_name(artist_id: &str, key: &str, name: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, remove_artist_credit_name => remove_artist_credit_name(artist_id: &str, key: &str) -> Result<(), bae_library::LibraryError>);
    delegate_view!(get_album_index => get_album_index(sort: &[bae_views::AlbumSortCriterion], album_id: &str, group_by_artist: bool) -> Option<u64>);
    delegate_async!(manager, check_releases_in_library => check_releases_in_library(checks: &[bae_records::LibraryCheck]) -> Result<Vec<bae_records::LibraryStatus>, bae_library::LibraryError>);
    delegate_view!(albums_in_browse_order => albums_in_browse_order(sort: &[bae_views::AlbumSortCriterion], album_ids: Vec<String>, group_by_artist: bool) -> Vec<bae_views::album_detail::AlbumSummary>);
    delegate_view!(find_album_detail => find_album_detail(album_id: &str) -> Option<bae_views::album_detail::AlbumDetail>);
    delegate_view!(find_release_detail => find_release_detail(release_id: &str) -> Option<bae_views::album_detail::ReleaseDetail>);
    delegate_view!(get_albums => get_albums(sort: &[bae_views::AlbumSortCriterion]) -> Vec<bae_records::DbAlbum>);
    delegate_async!(manager, get_releases_for_album => get_releases_for_album(album_id: &str) -> Result<Vec<bae_records::DbRelease>, bae_library::LibraryError>);
    delegate_async!(manager, get_release_by_id => get_release_by_id(release_id: &str) -> Result<Option<bae_records::DbRelease>, bae_library::LibraryError>);

    delegate_async!(manager, get_release_records => get_release_records(release_id: &str) -> Result<Vec<bae_catalogs::records::ReleaseRecord>, bae_library::LibraryError>);
    delegate_async!(manager, get_tracks_for_release => get_tracks_for_release(release_id: &str) -> Result<Vec<bae_records::DbTrack>, bae_library::LibraryError>);
    delegate_async!(manager, get_release_mediums => get_release_mediums(release_id: &str) -> Result<bae_records::DbReleaseMediums, bae_library::LibraryError>);
    delegate_async!(manager, get_files_for_release => get_files_for_release(release_id: &str) -> Result<Vec<bae_records::DbFile>, bae_library::LibraryError>);
    delegate_async!(manager, get_file_by_id => get_file_by_id(file_id: &str) -> Result<Option<bae_records::DbFile>, bae_library::LibraryError>);
    delegate_async!(manager, get_library_image => get_library_image(id: &str, image_type: &bae_records::LibraryImageType) -> Result<Option<bae_records::DbLibraryImage>, bae_library::LibraryError>);
    delegate_async!(manager, cover_ref => cover_ref(release_id: &str) -> Result<Option<bae_records::ImageRef>, bae_library::LibraryError>);
    delegate_async!(manager, read_image_bytes => read_image_bytes(image: &bae_records::ImageRef) -> Result<Option<Vec<u8>>, bae_library::LibraryError>);
    delegate_async!(manager, read_library_image_bytes => read_library_image_bytes(id: &str, image_type: &bae_records::LibraryImageType) -> Result<Option<Vec<u8>>, bae_library::LibraryError>);
    delegate_async!(manager, read_gallery_bytes => read_gallery_bytes(release_id: &str, source: &bae_records::GallerySource) -> Result<Vec<u8>, bae_library::LibraryError>);
    delegate_async!(manager, read_cover_image_bytes => read_cover_image_bytes(release_id: &str) -> Result<Option<Vec<u8>>, bae_library::LibraryError>);
    delegate_async!(manager, get_shown_artists_for_track => get_shown_artists_for_track(track_id: &str) -> Result<Vec<bae_records::credits::LibraryCredit>, bae_library::LibraryError>);
    delegate_async!(manager, get_all_track_ids => get_all_track_ids() -> Result<Vec<String>, bae_library::LibraryError>);
    delegate_async!(manager, filter_existing_track_ids => filter_existing_track_ids(ids: &[String]) -> Result<Vec<String>, bae_library::LibraryError>);
    delegate_async!(manager, resolve_track_audio => resolve_track_audio(track_id: &str) -> Result<bae_records::ResolvedTrackAudio, bae_library::LibraryError>);
    delegate_async!(manager, resolve_to_track_ids => resolve_to_track_ids(ids: &[String]) -> Result<Vec<String>, bae_library::LibraryError>);
    delegate_async!(manager, get_playback_track_info => get_playback_track_info(track_id: &str) -> Result<bae_playback::PlaybackTrackInfo, bae_library::LibraryError>);
    delegate_async!(manager, get_track_display => get_track_display(track_id: &str) -> Result<bae_playback::TrackDisplay, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, change_cover => change_cover(release_id: &str, selection: bae_library::CoverSelection) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, set_album_primary_release => set_album_primary_release(album_id: &str, primary_release_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, unpin_release => unpin_release(release_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, upload_releases => upload_releases(release_ids: &[String]) -> Result<bae_library::UploadReleasesOutcome, bae_library::LibraryError>);
    delegate_async!(manager, detach_release_audio => detach_release_audio(release_id: &str, new_path: &std::path::Path) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, delete_release => delete_release(release_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, delete_album => delete_album(album_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, save_s3_config => save_s3_config(data: bae_library::sync::S3ConfigData, device_name: &str, steps: bae_library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, bae_library::LibraryError>);
    delegate_async!(manager, generate_restore_code => generate_restore_code() -> Result<bae_library::sync::restore_code::RestoreCodeShare, bae_library::LibraryError>);
    delegate_async!(manager, get_members => get_members() -> Result<bae_db::membership::Membership, bae_library::LibraryError>);
    delegate_async!(manager, remove_member => remove_member(member: &coven::MemberId) -> Result<(), bae_library::LibraryError>);
    delegate_values!(subscribe_members => subscribe_members() -> bae_db::membership::Membership);
    delegate_async!(manager, add_device => add_device() -> Result<bae_library::sync::membership::AddingDevice, bae_library::LibraryError>);
    delegate_values!(subscribe_added_device => subscribe_added_device(watch: bae_library::sync::membership::AddedDeviceWatch) -> Option<bae_db::membership::ActiveDevice>);
    delegate_async!(manager, remove_device => remove_device(device: coven::DeviceId) -> Result<Option<coven::ProviderSignOut>, bae_library::LibraryError>);
    delegate_async!(manager, set_member_role => set_member_role(member: &coven::MemberId, role: coven::MemberRole) -> Result<(), bae_library::LibraryError>);
    delegate_values!(subscribe_admin_changes => subscribe_admin_changes() -> Vec<bae_db::admin_change::AdminChange>);
    delegate_async!(manager, acknowledge_admin_change => acknowledge_admin_change(operation: coven::OperationId) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, invite_person => invite_person(role: coven::MemberRole, name: String, access: coven::InviteAccess, delivery: coven::InviteDelivery) -> Result<coven::InviteId, bae_library::LibraryError>);
    /// This device's open invites, each with the pending record of its
    /// operation when coven holds one.
    pub fn subscribe_invites(
        &self,
    ) -> bae_store::live_query::LiveValues<Vec<bae_views::OpenInvite>> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_invites())
    }
    delegate_async!(manager, approve_join_request => approve_join_request(request: &coven::JoinRequest) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, decline_join_request => decline_join_request(request: &coven::JoinRequest) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, cancel_invite => cancel_invite(invite: &coven::InviteId) -> Result<(), bae_library::LibraryError>);

    delegate_async!(manager, retry_outbox_now => retry_outbox_now() -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, cancel_release_transition => cancel_release_transition(release_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, cancel_all_release_uploads => cancel_all_release_uploads() -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, set_sync_paused => set_sync_paused(paused: bool) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, enqueue_pins => enqueue_pins(release_ids: Vec<String>) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, use_cloudkit => use_cloudkit(device_name: &str, steps: bae_library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, sign_in => sign_in(provider: coven::CloudProvider) -> Result<(), bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, sign_in_again => sign_in_again(device_name: &str, steps: bae_library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, setup_oauth_storage => setup_oauth_storage(storage: coven::StorageConfig, device_name: &str, steps: bae_library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, onedrive_root => onedrive_root() -> Result<coven::StorageFolder, bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, onedrive_folders => onedrive_folders(parent: &coven::StorageConfig) -> Result<Vec<coven::StorageFolder>, bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, create_onedrive_folder => create_onedrive_folder(parent: &coven::StorageConfig, name: &str) -> Result<coven::StorageFolder, bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, dropbox_shared_folders => dropbox_shared_folders() -> Result<Vec<coven::StorageFolder>, bae_library::LibraryError>);
    delegate_async!(manager, get_album_id_for_release => get_album_id_for_release(release_id: &str) -> Result<String, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, open_release_edit => open_release_edit(release_id: &str) -> Result<bae_library::ReleaseEdit, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, apply_release_edit_catalog => apply_release_edit_catalog(session: &bae_library::ReleaseEdit, link: &bae_identify::PressingLink, steps: &bae_import::CatalogApplySteps) -> Result<bae_import::ReleaseEditDraft, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, save_release_edit => save_release_edit(session: &bae_library::ReleaseEdit) -> Result<(), bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, set_release_edit_album => set_release_edit_album(session: &bae_library::ReleaseEdit, album_id: Option<String>) -> Result<bae_import::ReleaseEditDraft, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, release_edit_draft => release_edit_draft(session: &bae_library::ReleaseEdit) -> Result<bae_import::ReleaseEditDraft, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, edit_release_edit_field => edit_release_edit_field(session: &bae_library::ReleaseEdit, edit: bae_import::DraftFieldEdit) -> Result<bae_import::ReleaseEditDraft, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, set_release_edit_album_artists => set_release_edit_album_artists(session: &bae_library::ReleaseEdit, assignments: Vec<bae_import::Credited>) -> Result<bae_import::ReleaseEditDraft, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, set_release_edit_track => set_release_edit_track(session: &bae_library::ReleaseEdit, track: bae_import::RawTrackEdit) -> Result<bae_import::ReleaseEditDraft, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, enqueue_export => enqueue_export(release_id: &str, target_dir: std::path::PathBuf) -> Result<(), bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, enqueue_release_save => enqueue_release_save(release_id: &str, target_dir: std::path::PathBuf, preset_id: &str) -> Result<(), bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, save_track => save_track(track_id: &str, output_path: &std::path::Path, preset_id: &str) -> Result<(), bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, save_track_suggested_name => save_track_suggested_name(track_id: &str, preset_id: &str) -> Result<String, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, apply_release_metadata_user_edit => apply_release_metadata_user_edit(release_id: &str, edit: &bae_import::ReleaseUserEdit) -> Result<(), bae_library::LibraryError>);
    delegate_view!(search_library => search_library(query: &bae_views::LibrarySearchQuery) -> bae_views::album_detail::SearchResults);
    #[cfg(desktop)]
    delegate_async!(manager, set_outputs_paused => set_outputs_paused(paused: bool) -> Result<(), bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, cancel_output => cancel_output(release_id: &str) -> Result<(), bae_library::LibraryError>);

    #[cfg(any(test, feature = "test-utils"))]
    delegate_async!(manager, has_cloud_home => has_cloud_home() -> Result<bool, bae_library::LibraryError>);
    #[cfg(any(test, feature = "test-utils"))]
    delegate_async!(manager, is_sync_configured => is_sync_configured() -> Result<bool, bae_library::LibraryError>);

    delegate_async!(playback, playback_play_release => play_release(release_id: String, start_track_id: Option<String>, shuffle: bool) -> Result<bae_playback::PlayOutcome, bae_playback::PlayError>);
    delegate_async!(playback, playback_play_releases => play_releases(release_ids: Vec<String>) -> Result<bae_playback::PlayOutcome, bae_playback::PlayError>);
    delegate_async!(playback, playback_play_tracks => play_tracks(track_ids: Vec<String>, start_index: usize) -> Result<bae_playback::PlayOutcome, bae_playback::PlayError>);
    delegate_async!(playback, playback_play_library_shuffled => play_library_shuffled() -> Result<bae_playback::PlayOutcome, bae_playback::PlayError>);
    delegate_sync!(playback, playback_pause => pause() -> ());
    delegate_sync!(playback, playback_resume => resume() -> ());
    delegate_sync!(playback, playback_cancel_side_pause_countdown => cancel_side_pause_countdown() -> ());
    delegate_sync!(playback, playback_stop => stop() -> ());
    delegate_sync!(playback, playback_next => next() -> ());
    delegate_sync!(playback, playback_previous => previous() -> ());
    delegate_sync!(playback, playback_seek_by_ratio => seek_by_ratio(ratio: f64) -> ());
    delegate_sync!(playback, playback_set_volume => set_volume(volume: f32) -> ());
    delegate_sync!(playback, playback_set_muted => set_muted(muted: bool) -> ());
    delegate_sync!(playback, playback_play_on => play_on(channel: Box<dyn bae_playback::renderer::RendererChannel>, device: bae_playback::RemoteDevice, media_source: bae_playback::renderer::RendererMediaSource) -> ());
    delegate_sync!(playback, playback_play_on_airplay => play_on_airplay(sink: Box<dyn bae_playback::airplay_output::AirPlaySink>, device: bae_playback::RemoteDevice) -> ());
    delegate_sync!(playback, playback_stop_remote => stop_remote() -> ());
    delegate_sync!(playback, playback_preview_play => preview_play(target: bae_track_audio::PreviewTarget) -> ());
    delegate_sync!(playback, playback_preview_stop => preview_stop() -> ());
    delegate_sync!(playback, playback_preview_toggle_pause => preview_toggle_pause() -> ());
    delegate_sync!(playback, playback_preview_seek_by_ratio => preview_seek_by_ratio(ratio: f64) -> ());
    delegate_sync!(playback, playback_set_repeat_mode => set_repeat_mode(mode: bae_playback::RepeatMode) -> ());
    delegate_sync!(playback, playback_set_shuffle => set_shuffle(on: bool) -> ());
    delegate_sync!(playback, playback_add_to_queue => add_to_queue(track_ids: Vec<String>) -> ());
    delegate_sync!(playback, playback_add_next => add_next(track_ids: Vec<String>) -> ());
    delegate_sync!(playback, playback_add_release_to_queue => add_release_to_queue(release_id: String) -> ());
    delegate_sync!(playback, playback_add_release_next => add_release_next(release_id: String) -> ());
    delegate_sync!(playback, playback_insert_in_queue => insert_in_queue(track_ids: Vec<String>, index: usize) -> ());

    /// Remove a queue entry, sent at this call, in order with the playback
    /// commands sent before and after it; the future answers that the queue
    /// changed when the entry is no longer queued.
    pub fn playback_remove_entry(
        &self,
        entry_id: bae_playback::QueueEntryId,
    ) -> impl std::future::Future<Output = Result<(), bae_playback::QueueEditError>> + Send + 'static
    {
        self.inner.playback.remove_entry(entry_id)
    }

    /// Empty Up Next, sent at this call; the future answers once it is empty
    /// or why it isn't.
    pub fn playback_clear_up_next(
        &self,
    ) -> impl std::future::Future<Output = Result<(), bae_playback::QueueEditError>> + Send + 'static
    {
        self.inner.playback.clear_up_next()
    }

    /// Move a queue entry, sent at this call, in order with the playback
    /// commands sent before and after it; the future answers the lane's
    /// version the move made, or why nothing moved.
    pub fn playback_reorder_entry(
        &self,
        entry_id: bae_playback::QueueEntryId,
        before: Option<bae_playback::QueueEntryId>,
    ) -> impl std::future::Future<Output = Result<u64, bae_playback::QueueEditError>> + Send + 'static
    {
        self.inner.playback.reorder_entry(entry_id, before)
    }

    /// Drop the context lane, sent at this call; the future answers once it
    /// is dropped or why it isn't.
    pub fn playback_clear_playing_from(
        &self,
    ) -> impl std::future::Future<Output = Result<(), bae_playback::QueueEditError>> + Send + 'static
    {
        self.inner.playback.clear_playing_from()
    }

    /// Skip to a queue entry, sent at this call; the future answers once its
    /// track is loading, or that the queue changed when the entry is no longer
    /// queued.
    pub fn playback_skip_to_entry(
        &self,
        entry_id: bae_playback::QueueEntryId,
    ) -> impl std::future::Future<Output = Result<(), bae_playback::QueueEditError>> + Send + 'static
    {
        self.inner.playback.skip_to_entry(entry_id)
    }

    delegate_async!(playback, playback_save_state => save_state() -> ());

    /// `file_id`'s bytes, `size` long, read into a buffer from the start;
    /// see [`LibraryManager::open_release_file_stream`].
    pub fn open_release_file_stream(
        &self,
        file_id: &str,
        size: u64,
    ) -> bae_audio::sparse_buffer::SharedSparseBuffer {
        self.inner.manager.open_release_file_stream(file_id, size)
    }

    /// `audio`'s stream, open for one decode start to finish; see
    /// [`LibraryManager::open_track_decode`].
    pub fn open_track_decode(
        &self,
        audio: &bae_records::ResolvedTrackAudio,
    ) -> bae_library::TrackDecode {
        self.inner.manager.open_track_decode(audio)
    }

    // The sync status, each value read from its tables in one go.
    delegate_values!(subscribe_sync_status => subscribe_sync_status() -> bae_db::SyncStatusSnapshot);

    pub async fn retry_pending_operation(
        &self,
        id: coven::OperationId,
    ) -> Result<(), bae_library::LibraryError> {
        self.inner.manager.retry_pending_operation(id).await
    }

    /// End a release's failed move to the cloud, `id`, as its pending
    /// record offers: the release stays on this device only, as it was.
    pub async fn cancel_failed_release_move(
        &self,
        id: coven::MoveId,
    ) -> Result<(), bae_library::LibraryError> {
        self.inner.manager.cancel_failed_release_move(id).await
    }

    // Everything sync is doing, for the activity indicator: each value read
    // in one go.
    pub fn subscribe_sync_activity(
        &self,
    ) -> bae_store::live_query::LiveValues<bae_views::SyncActivity> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_sync_activity())
    }

    delegate_async!(manager, pending => pending() -> Result<Vec<coven::PendingRecord>, bae_library::LibraryError>);
    delegate_async!(manager, library_name => library_name() -> Result<String, bae_library::LibraryError>);
    delegate_values!(subscribe_library_name => subscribe_library_name() -> String);

    /// Whether the person paused uploads: the first value at once, then one
    /// after each change.
    pub fn subscribe_uploads_paused(&self) -> bae_store::live_query::LiveValues<bool> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_uploads_paused())
    }

    delegate_async!(manager, is_sync_paused => is_sync_paused() -> Result<bool, bae_library::LibraryError>);

    delegate_sync!(manager, watch_outbox_transfers => watch_outbox_transfers() -> bae_library::OutboxTransfersWatch);
    delegate_sync!(manager, watch_outbox_total_transfer => watch_outbox_total_transfer() -> bae_library::OutboxTotalTransferWatch);
    delegate_sync!(manager, watch_upload_file_transfers => watch_upload_file_transfers() -> bae_library::FileTransfersWatch);
    delegate_sync!(manager, watch_pin_file_transfers => watch_pin_file_transfers() -> bae_library::FileTransfersWatch);

    /// Set whether playback pauses between vinyl/cassette sides and CD discs. Turning it on
    /// must take effect at the boundary already staged for gapless playback,
    /// not just the next one: `preload_next_track` decides staging once, at
    /// preload time, so writing the config alone leaves an already-staged
    /// track to cross gaplessly. Turning it off needs no follow-up — the
    /// drain-time gate (`side_pause_for_queue_front`) already re-reads the
    /// config before every boundary.
    pub async fn set_pause_between_sides(
        &self,
        enabled: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.inner.manager.set_pause_between_sides(enabled).await?;
        if enabled {
            self.inner.playback.reevaluate_side_pause_staging();
        }
        Ok(())
    }
}

mod detail_reads;
#[cfg(desktop)]
mod import;
#[cfg(desktop)]
mod pane;
mod playback_reads;
mod queue_reads;
#[cfg(desktop)]
mod release_files_reads;
mod release_queue_reads;
#[cfg(desktop)]
mod selection;
#[cfg(desktop)]
mod storage_inspector_reads;
mod storage_reads;
#[cfg(test)]
#[path = "app_services_tests.rs"]
mod tests;

mod personal;

mod removal;

impl AppServices {
    delegate_async!(manager, detect_s3_settings => detect_s3_settings(storage:coven::StorageConfig, credentials:coven::S3Credentials) -> Result<coven::S3Detection, bae_library::LibraryError>);
    delegate_async!(manager, storage_location => storage_location() -> Result<Option<coven::StorageConfig>, bae_library::LibraryError>);
}
