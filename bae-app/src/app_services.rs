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

/// A call to one of the library's parts, its failure told as a library error.
macro_rules! delegate_lib {
    ($field:ident, $name:ident => $target:ident($($arg:ident: $ty:ty),* $(,)?) -> $ok:ty) => {
        pub async fn $name(&self, $($arg: $ty),*) -> Result<$ok, bae_library::LibraryError> {
            Ok(self.inner.$field.$target($($arg),*).await?)
        }
    };
}

struct AppServicesInner {
    manager: LibraryManager,
    /// The library's store, which the calls bae adds nothing to go to.
    store: bae_store::Store,
    /// The library's records, which the reads and writes bae adds nothing to
    /// go to.
    records: bae_records::Records,
    /// The library's people, profiles and personal collections, which the
    /// calls bae adds nothing to go to.
    people: bae_people::People,
    /// The library's members and devices, whose administrator changes the
    /// calls bae adds nothing to go to.
    membership: bae_people::Membership,
    /// The library's sync status, whose storage-setup outcomes the calls bae
    /// adds nothing to are dismissed on.
    sync_status: bae_sync_status::SyncStatus,
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
    /// The catalogs import asks, whose Discogs signal writes closing the
    /// library waits for before its store closes.
    #[cfg(desktop)]
    catalogs: bae_import::CatalogSessions,
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
        store: bae_store::Store,
        records: bae_records::Records,
        people: bae_people::People,
        membership: bae_people::Membership,
        sync_status: bae_sync_status::SyncStatus,
        views: bae_views::LibraryViews,
        playback: PlaybackHandle,
        cast_picker: bae_playback::renderer::CastPicker,
        #[cfg(desktop)] import: ImportServiceHandle,
        #[cfg(desktop)] identification: IdentificationHandle,
        #[cfg(desktop)] catalogs: bae_import::CatalogSessions,
    ) -> Self {
        AppServices {
            inner: Arc::new(AppServicesInner {
                manager,
                store,
                records,
                people,
                membership,
                sync_status,
                views,
                playback,
                cast_picker,
                #[cfg(desktop)]
                import,
                #[cfg(desktop)]
                identification,
                #[cfg(desktop)]
                catalogs,
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
            self.inner
                .manager
                .blocking("stop identification and import", move || {
                    // Before the import worker's join, as in the drop: a
                    // cancelled candidate writes no row.
                    identification.stop();
                    import.stop_and_join();
                })
                .await?;
            // The calls that make them have stopped; the store is still open.
            self.inner.catalogs.wait_for_writes().await;
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
        bae_views::LiveAlbumBrowse::new(query, |read, projection| {
            Some(bae_views::album::resolve_album_browse(projection, read.revision, read.cause))
        })
    }

    /// Which of the albums the grid has selected are gone from the library,
    /// as one live query whose ids move in place as the selection changes; it
    /// starts with none.
    pub fn subscribe_album_selection(&self) -> bae_library::LiveAlbumSelection {
        let manager = &self.inner.manager;
        bae_library::LiveAlbumSelection::new(
            manager.until_closed(
                self.inner.views.subscribe_album_selection(std::collections::BTreeSet::new()),
            ),
            |read, gone| {
                Some(bae_library::AlbumSelectionSnapshot { requested: read.request.clone(), gone })
            },
        )
    }

    /// One live library search, pointed at a new query in place as the
    /// person types; it starts with no query.
    pub fn subscribe_library_search(&self) -> bae_views::LiveLibrarySearch {
        let query = self.inner.manager.until_closed(
            self.inner.views.subscribe_library_search(None, bae_views::SEARCH_RESULT_LIMIT),
        );
        bae_views::LiveLibrarySearch::new(query, |read, projection| {
            Some(bae_views::LibrarySearchSnapshot {
                query: read.request.clone(),
                results: bae_views::album::resolve_library_search_projection(projection),
                request_revision: read.revision,
            })
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
            |read, projection| {
                Some(bae_views::artist_search::resolve_artist_search(projection, read.revision))
            },
        )
    }

    /// The library membership of the releases an import pane offers, as one
    /// live query whose checks move in place as the pane's offers change; it
    /// starts with none.
    pub fn subscribe_library_statuses(&self) -> bae_library::LiveLibraryStatus {
        let manager = &self.inner.manager;
        bae_library::LiveLibraryStatus::new(
            manager.until_closed(manager.subscribe_library_statuses(Default::default())),
            |read, statuses| {
                Some(bae_library::LibraryStatusSnapshot {
                    statuses,
                    request_revision: read.revision,
                })
            },
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
        bae_views::LiveArtistBrowse::new(query, |read, projection| {
            Some(bae_views::artist::resolve_artist_browse(projection, read.revision, read.cause))
        })
    }

    pub fn subscribe_composer_browse(
        &self,
        sort: &[bae_views::ComposerSortCriterion],
    ) -> bae_views::LiveComposerBrowse {
        let manager = &self.inner.manager;
        let query = manager.until_closed(
            self.inner.views.subscribe_composer_browse(sort, std::collections::BTreeSet::new()),
        );
        bae_views::LiveComposerBrowse::new(query, |read, projection| {
            Some(bae_views::composer::resolve_composer_browse(
                projection,
                read.revision,
                read.cause,
            ))
        })
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
        bae_views::LiveComposerDetailRows::new(query, |read, projection| {
            Some(bae_views::composer_detail::resolve_composer_detail_rows(
                projection,
                read.revision,
                read.cause,
            ))
        })
    }

    pub fn subscribe_playback_progress(&self) -> bae_playback::PlaybackProgressFeed {
        self.inner.playback.subscribe_progress()
    }

    delegate_async!(manager, preferences => preferences() -> Result<bae_preferences::Preferences, bae_store::DbError>);
    delegate_sync!(manager, library_path => library_path() -> std::path::PathBuf);
    delegate_sync!(store, store_id => store_id() -> coven::StoreId);
    delegate_async!(manager, ensure_mcp_token => ensure_mcp_token() -> Result<String, bae_library::LibraryError>);
    delegate_async!(manager, set_mcp_token => set_mcp_token(token: String) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, set_mcp_config => set_mcp_config(config: bae_preferences::McpConfig) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, get_subsonic_password => get_subsonic_password() -> Result<Option<String>, bae_library::LibraryError>);
    delegate_async!(manager, set_subsonic_password => set_subsonic_password(password: String) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, set_subsonic_enabled => set_subsonic_enabled(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_subsonic_port => set_subsonic_port(port: u32) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_subsonic_username => set_subsonic_username(username: String) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_subsonic_network => set_subsonic_network(network: bae_preferences::SubsonicNetwork) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_cast_enabled => set_cast_enabled(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_cast_transcode_format => set_cast_transcode_format(format: bae_preferences::CastTranscodeFormat) -> Result<(), bae_preferences::PreferencesError>);
    pub fn store_key_state(&self) -> Result<coven::StoreKeyState, bae_library::LibraryError> {
        Ok(self.inner.store.store_key_state()?)
    }
    delegate_async!(manager, set_max_concurrent_uploads => set_max_concurrent_uploads(n: u32) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_max_concurrent_downloads => set_max_concurrent_downloads(n: u32) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_side_pause_countdown => set_side_pause_countdown(countdown: bae_preferences::SidePauseCountdown) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_show_remaining_time => set_show_remaining_time(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_library_full_width => set_library_full_width(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_ask_before_unpinning => set_ask_before_unpinning(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_identify_automatically => set_identify_automatically(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_import_when_identified => set_import_when_identified(enabled: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_import_destination => set_import_destination(destination: bae_preferences::ImportDestination) -> Result<(), bae_preferences::PreferencesError>);

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
    /// Switching Discogs off also clears its key, the last thing done.
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
                // Switching Discogs off is how its key goes: the preference is
                // written (and any refusal made) before the key is cleared, so
                // a refused switch keeps the key, and a clear that fails is
                // told to the person, who switches it off again.
                if source == bae_catalogs::catalog::LookupCatalog::Discogs {
                    self.inner
                        .import
                        .remove_discogs_token()
                        .await
                        .map_err(bae_library::LibraryError::from)?;
                }
            }
        }
        Ok(())
    }

    delegate_async!(manager, set_save_presets => set_save_presets(presets: Vec<bae_preferences::SavePreset>) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_save_embed_cover => set_save_embed_cover(on: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_save_cover_file => set_save_cover_file(on: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_save_loudness_tags => set_save_loudness_tags(on: bool) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_save_folder_pattern => set_save_folder_pattern(pattern: bae_preferences::SaveFolderPattern) -> Result<(), bae_preferences::PreferencesError>);
    delegate_async!(manager, set_save_filename_pattern => set_save_filename_pattern(pattern: bae_preferences::SaveFilenamePattern) -> Result<(), bae_preferences::PreferencesError>);
    pub async fn rename_library(
        &self,
        name: &bae_store::EnteredName,
    ) -> Result<(), bae_library::LibraryError> {
        Ok(self.inner.membership.rename_store(name.as_str()).await?)
    }
    delegate_lib!(store, forget_encryption_key => forget_store_keys() -> ());
    pub async fn unlock_store_key(&self) -> Result<(), bae_library::LibraryError> {
        self.inner.store.unlock_store_key().await?;
        Ok(())
    }
    delegate_sync!(store, trigger_sync => sync_now() -> ());
    delegate_lib!(store, release_memory => release_memory() -> ());
    delegate_async!(manager, retry_now => retry_now() -> Result<impl std::future::Future<Output = bae_sync_status::SyncRetryOutcome> + Send + 'static, bae_library::LibraryError>);
    delegate_async!(manager, reconnect_sync => reconnect_sync() -> Result<(), bae_library::LibraryError>);
    delegate_sync!(store, cancel_storage_setup => cancel_storage_setup() -> coven::SetupCancellation);
    pub async fn dismiss_undone_storage_setup(
        &self,
        operation: coven::OperationId,
    ) -> Result<(), bae_library::LibraryError> {
        Ok(self.inner.sync_status.acknowledge_undone_storage_setup(operation).await?)
    }
    delegate_async!(manager, set_downloads_paused => set_downloads_paused(paused: bool) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, cancel_download => cancel_download(release_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, retry_download => retry_download(release_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, retry_downloads => retry_downloads() -> Result<(), bae_library::LibraryError>);
    delegate_view!(get_artist_count => get_artist_count() -> u64);
    delegate_view!(get_artist_page => get_artist_page(sort: &[bae_views::ArtistSortCriterion], offset: u64, limit: u64) -> Vec<bae_views::DbArtistSummary>);
    delegate_view!(get_artist_detail => get_artist_detail(artist_id: &str) -> Option<bae_views::album_detail::ArtistDetail>);
    delegate_async!(manager, add_artist_credit_name => add_artist_credit_name(artist_id: &str, name: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, rename_artist_credit_name => rename_artist_credit_name(artist_id: &str, key: &str, name: &str) -> Result<(), bae_library::LibraryError>);
    delegate_lib!(records, remove_artist_credit_name => remove_artist_credit_name(artist_id: &str, key: &str) -> ());
    delegate_view!(get_album_index => get_album_index(sort: &[bae_views::AlbumSortCriterion], album_id: &str, group_by_artist: bool) -> Option<u64>);
    delegate_lib!(records, check_releases_in_library => check_releases_in_library(checks: &[bae_records::LibraryCheck]) -> Vec<bae_records::LibraryStatus>);
    delegate_view!(albums_in_browse_order => albums_in_browse_order(sort: &[bae_views::AlbumSortCriterion], album_ids: Vec<String>, group_by_artist: bool) -> Vec<bae_views::album_detail::AlbumSummary>);
    delegate_view!(find_album_detail => find_album_detail(album_id: &str) -> Option<bae_views::album_detail::AlbumDetail>);
    delegate_view!(find_release_detail => find_release_detail(release_id: &str) -> Option<bae_views::album_detail::ReleaseDetail>);
    delegate_view!(get_albums => get_albums(sort: &[bae_views::AlbumSortCriterion]) -> Vec<bae_records::DbAlbum>);
    delegate_lib!(records, get_releases_for_album => get_releases_for_album(album_id: &str) -> Vec<bae_records::DbRelease>);
    delegate_async!(manager, get_release_by_id => get_release_by_id(release_id: &str) -> Result<Option<bae_records::DbRelease>, bae_library::LibraryError>);

    delegate_lib!(records, get_release_records => get_release_records(release_id: &str) -> Vec<bae_catalogs::records::ReleaseRecord>);
    delegate_lib!(records, get_tracks_for_release => get_tracks_for_release(release_id: &str) -> Vec<bae_records::DbTrack>);
    delegate_lib!(records, get_release_mediums => get_release_mediums(release_id: &str) -> bae_records::DbReleaseMediums);
    delegate_async!(manager, get_files_for_release => get_files_for_release(release_id: &str) -> Result<Vec<bae_records::DbFile>, bae_library::LibraryError>);
    delegate_lib!(records, get_file_by_id => find_file_by_id(file_id: &str) -> Option<bae_records::DbFile>);
    delegate_lib!(records, get_library_image => find_library_image(id: &str, image_type: &bae_records::LibraryImageType) -> Option<bae_records::DbLibraryImage>);
    delegate_async!(manager, cover_ref => cover_ref(release_id: &str) -> Result<Option<bae_records::ImageRef>, bae_library::LibraryError>);
    delegate_async!(manager, read_image_bytes => read_image_bytes(image: &bae_records::ImageRef) -> Result<Option<Vec<u8>>, bae_library::LibraryError>);
    delegate_async!(manager, read_library_image_bytes => read_library_image_bytes(id: &str, image_type: &bae_records::LibraryImageType) -> Result<Option<Vec<u8>>, bae_library::LibraryError>);
    delegate_async!(manager, read_gallery_bytes => read_gallery_bytes(release_id: &str, source: &bae_records::GallerySource) -> Result<Vec<u8>, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, read_release_document => read_release_document(release_id: &str, file_id: &str) -> Result<String, bae_library::LibraryError>);
    delegate_async!(manager, read_cover_image_bytes => read_cover_image_bytes(release_id: &str) -> Result<Option<Vec<u8>>, bae_library::LibraryError>);
    delegate_lib!(records, get_shown_artists_for_track => get_shown_artists_for_track(track_id: &str) -> Vec<bae_records::credits::LibraryCredit>);
    delegate_lib!(records, get_all_track_ids => get_all_track_ids() -> Vec<String>);
    delegate_lib!(records, filter_existing_track_ids => filter_existing_track_ids(ids: &[String]) -> Vec<String>);
    pub async fn resolve_track_audio(
        &self,
        track_id: &str,
    ) -> Result<bae_records::ResolvedTrackAudio, bae_library::LibraryError> {
        Ok(bae_records::ResolvedTrackAudio::resolve(&self.inner.records, track_id).await?)
    }
    delegate_async!(manager, resolve_to_track_ids => resolve_to_track_ids(ids: &[String]) -> Result<Vec<String>, bae_library::LibraryError>);
    delegate_async!(manager, get_playback_track_info => get_playback_track_info(track_id: &str) -> Result<bae_playback::PlaybackTrackInfo, bae_library::LibraryError>);
    delegate_async!(manager, get_track_display => get_track_display(track_id: &str) -> Result<bae_playback::TrackDisplay, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, change_cover => change_cover(release_id: &str, selection: bae_library::CoverSelection) -> Result<(), bae_library::LibraryError>);
    delegate_lib!(records, set_album_primary_release => set_album_primary_release(album_id: &str, primary_release_id: &str) -> ());
    delegate_async!(manager, unpin_releases => unpin_releases(release_ids: &[String]) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, clear_cache => clear_cache() -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, set_cache_limit => set_cache_limit(limit: bae_records::CacheLimit) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, delete_data_from_before_reset => delete_data_from_before_reset() -> Result<(), bae_library::LibraryError>);
    delegate_values!(subscribe_device_storage => subscribe_device_storage() -> bae_library::DeviceStorage);
    delegate_async!(manager, upload_releases => upload_releases(release_ids: &[String]) -> Result<bae_library::UploadReleasesOutcome, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, make_releases_local => make_releases_local(release_ids: Vec<String>, folder: &std::path::Path) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, delete_release => delete_release(release_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, delete_album => delete_album(album_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, save_s3_config => save_s3_config(data: bae_library::sync::S3ConfigData, device_name: &str, steps: bae_library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, bae_library::LibraryError>);
    delegate_async!(manager, generate_restore_code => generate_restore_code() -> Result<bae_library::sync::restore_code::RestoreCodeShare, bae_library::LibraryError>);
    delegate_lib!(membership, get_members => members() -> bae_people::membership::Members);
    delegate_lib!(membership, remove_member => remove_member(member: &coven::MemberId) -> ());
    delegate_values!(subscribe_members => subscribe_members() -> bae_people::membership::Members);
    delegate_async!(manager, add_device => add_device() -> Result<bae_library::sync::membership::AddingDevice, bae_library::LibraryError>);
    delegate_values!(subscribe_added_device => subscribe_added_device(watch: bae_library::sync::membership::AddedDeviceWatch) -> Option<bae_people::membership::ActiveDevice>);
    delegate_lib!(membership, remove_device => remove_device(device: coven::DeviceId) -> Option<coven::ProviderSignOut>);
    delegate_async!(manager, holds_store => holds_store() -> Result<bool, bae_library::LibraryError>);
    delegate_async!(manager, confirm_device_cut_off => confirm_device_cut_off(device: coven::DeviceId) -> Result<bae_library::DeviceCutOff, bae_library::LibraryError>);
    delegate_lib!(membership, set_member_role => set_member_role(member: &coven::MemberId, role: coven::MemberRole) -> ());
    delegate_values!(subscribe_admin_changes => subscribe_admin_changes() -> Vec<bae_people::admin_change::AdminChange>);
    delegate_lib!(membership, acknowledge_admin_change => acknowledge_admin_change(operation: coven::OperationId) -> ());
    delegate_lib!(membership, invite_person => invite_person(access: coven::InviteAccess) -> coven::InviteId);
    /// This device's open invites, each with the pending record of its
    /// operation when coven holds one.
    pub fn subscribe_invites(
        &self,
    ) -> bae_store::live_query::LiveValues<Vec<bae_views::InviteEntry>> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_invites())
    }
    delegate_lib!(membership, dismiss_refused_invite => dismiss_refused_invite(operation: coven::OperationId) -> ());
    delegate_lib!(membership, approve_join_request => approve_join_request(request: &coven::JoinRequest) -> ());
    delegate_lib!(store, decline_join_request => decline_join_request(request: &coven::JoinRequest) -> ());
    delegate_async!(manager, cancel_invite => cancel_invite(invite: &coven::InviteId) -> Result<(), bae_library::LibraryError>);

    delegate_async!(manager, retry_outbox_now => retry_outbox_now() -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, cancel_release_transition => cancel_release_transition(release_id: &str) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, cancel_all_release_uploads => cancel_all_release_uploads() -> Result<(), bae_library::LibraryError>);
    delegate_lib!(store, set_sync_paused => set_uploads_paused(paused: bool) -> ());
    delegate_lib!(store, set_unmetered_only => set_unmetered_only(unmetered_only: bool) -> ());
    delegate_async!(manager, enqueue_pins => enqueue_pins(release_ids: Vec<String>) -> Result<(), bae_library::LibraryError>);
    delegate_async!(manager, use_cloudkit => use_cloudkit(device_name: &str, steps: bae_library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, sign_in => sign_in(provider: coven::CloudProvider) -> Result<(), bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, sign_in_again => sign_in_again(device_name: &str, steps: bae_library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_async!(manager, setup_oauth_storage => setup_oauth_storage(storage: coven::StorageConfig, device_name: &str, steps: bae_library::StorageSetupSteps<'_>) -> Result<coven::StorageSetup, bae_library::LibraryError>);
    #[cfg(feature = "oauth-providers")]
    delegate_lib!(store, onedrive_root => onedrive_root() -> coven::StorageFolder);
    #[cfg(feature = "oauth-providers")]
    delegate_lib!(store, onedrive_folders => onedrive_folders(parent: &coven::StorageConfig) -> Vec<coven::StorageFolder>);
    #[cfg(feature = "oauth-providers")]
    delegate_lib!(store, create_onedrive_folder => create_onedrive_folder(parent: &coven::StorageConfig, name: &str) -> coven::StorageFolder);
    #[cfg(feature = "oauth-providers")]
    delegate_lib!(store, dropbox_shared_folders => dropbox_shared_folders() -> Vec<coven::StorageFolder>);
    #[cfg(feature = "oauth-providers")]
    delegate_lib!(store, create_dropbox_shared_folder => create_dropbox_shared_folder(name: &str) -> coven::StorageFolder);
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
    delegate_async!(manager, save_track_suggested_name => save_track_suggested_name(track_id: &str) -> Result<String, bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, apply_release_metadata_user_edit => apply_release_metadata_user_edit(release_id: &str, edit: &bae_import::ReleaseUserEdit) -> Result<(), bae_library::LibraryError>);
    delegate_view!(search_library => search_library(query: &bae_views::LibrarySearchQuery) -> bae_views::album_detail::SearchResults);
    #[cfg(desktop)]
    delegate_async!(manager, set_outputs_paused => set_outputs_paused(paused: bool) -> Result<(), bae_library::LibraryError>);
    #[cfg(desktop)]
    delegate_async!(manager, cancel_output => cancel_output(release_id: &str) -> Result<(), bae_library::LibraryError>);

    #[cfg(any(test, feature = "test-utils"))]
    delegate_lib!(store, syncs => syncs() -> bool);
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
    delegate_sync!(playback, playback_set_volume_live => set_volume_live(volume: f32) -> ());
    delegate_sync!(playback, playback_set_muted => set_muted(muted: bool) -> ());
    delegate_sync!(playback, playback_play_on => play_on(channel: Box<dyn bae_playback::renderer::RendererChannel>, device: bae_playback::renderer::CastReceiver, media_source: bae_playback::renderer::RendererMediaSource) -> ());
    delegate_sync!(playback, playback_play_on_airplay => play_on_airplay(sink: Box<dyn bae_playback::airplay_output::AirPlaySink>, device: bae_playback::renderer::CastReceiver) -> ());
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
    /// commands sent before and after it; an entry no longer queued is
    /// already gone, and the future answers `Ok`.
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
    /// version the move made, `None` when the entry or the one it goes before
    /// is no longer queued there, or why the move failed.
    pub fn playback_reorder_entry(
        &self,
        entry_id: bae_playback::QueueEntryId,
        before: Option<bae_playback::QueueEntryId>,
    ) -> impl std::future::Future<Output = Result<Option<u64>, bae_playback::QueueEditError>>
           + Send
           + 'static {
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
    /// track is loading; an entry no longer queued is skipped to by no one, and
    /// the future answers `Ok`.
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
    delegate_values!(subscribe_sync_status => subscribe_sync_status() -> bae_sync_status::SyncStatusSnapshot);

    delegate_lib!(store, retry_pending_operation => retry_pending_operation(id: coven::OperationId) -> ());

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

    delegate_lib!(store, pending => pending() -> Vec<coven::PendingRecord>);
    delegate_lib!(store, library_name => store_name() -> String);
    pub fn subscribe_library_name(&self) -> bae_store::live_query::LiveValues<String> {
        self.inner.manager.values_until_closed(self.inner.store.subscribe_store_name())
    }

    /// Whether the person paused uploads: the first value at once, then one
    /// after each change.
    pub fn subscribe_uploads_paused(&self) -> bae_store::live_query::LiveValues<bool> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_uploads_paused())
    }

    /// `source`'s values as a surface pulls them: the first at once, then
    /// each the source follows with, until the surface cancels or the
    /// library closes.
    pub fn follow_source<T: Clone + PartialEq + Send + 'static>(
        &self,
        source: impl bae_store::live_query::LiveSource<T> + 'static,
    ) -> bae_store::live_query::LiveValues<T> {
        self.inner.manager.values_until_closed(source)
    }

    /// Whether files move only on unmetered networks: the first value at
    /// once, then one after each change.
    pub fn subscribe_unmetered_only(&self) -> bae_store::live_query::LiveValues<bool> {
        self.inner.manager.values_until_closed(self.inner.views.subscribe_unmetered_only())
    }

    delegate_lib!(store, is_unmetered_only => unmetered_only() -> bool);
    delegate_lib!(store, is_sync_paused => uploads_paused() -> bool);

    delegate_sync!(manager, watch_outbox_transfers => watch_outbox_transfers() -> bae_library::OutboxTransfersWatch);
    delegate_sync!(manager, watch_activity_bars => watch_activity_bars() -> bae_library::ActivityBarsWatch);
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
    delegate_lib!(store, storage_location => storage_config() -> Option<coven::StorageConfig>);
}
