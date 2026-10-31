//! The import surface of [`AppServices`]: identification triggers, the import
//! event bus, and the list and per-candidate subscriptions.

use super::*;
use crate::import::{ImportCandidateDetail, ImportListSubscription, ImportListView};

impl AppServices {
    delegate_async!(import, import_candidate_source_folders => candidate_source_folders(key: &crate::import::CandidateKey) -> Result<Vec<std::path::PathBuf>, crate::import::ImportError>);
    delegate_async!(import, import_combine_candidates => combine_candidates(keys: Vec<crate::import::CandidateKey>) -> Result<crate::import::CandidateKey, crate::import::ImportError>);
    delegate_async!(import, import_combine_folder => combine_folder(folder: crate::import::FolderReleaseDecisionKey) -> Result<crate::import::CandidateKey, crate::import::ImportError>);
    delegate_async!(import, import_separate_candidate => separate_candidate(key: &crate::import::CandidateKey) -> Result<Vec<crate::import::CandidateKey>, crate::import::ImportError>);
    delegate_async!(import, import_add_watched_folder => add_watched_folder(path: std::path::PathBuf) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_cancel => cancel_import(candidate_key: &crate::import::CandidateKey) -> Result<(), crate::import::ImportError>);
    delegate_sync!(import, import_cancel_all_in_background => cancel_all_imports_in_background() -> ());
    delegate_async!(import, import_remove_watched_folder => remove_watched_folder(path: std::path::PathBuf) -> Result<(), crate::import::ImportError>);
    delegate_sync!(import, import_scan_watched_folders => scan_watched_folders() -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_watched_folders => watched_folders() -> Result<Vec<crate::import::WatchedFolder>, crate::import::ImportError>);
    #[cfg(any(test, feature = "test-utils"))]
    delegate_sync!(import, import_emit_event_for_test => emit_event_for_test(event: crate::import::ImportEvent) -> ());
    delegate_async!(import, import_get_candidate => get_candidate(key: &crate::import::CandidateKey) -> Result<Option<crate::import::ImportCandidateSnapshot>, crate::library::LibraryError>);
    delegate_async!(import, import_read_watched_folders => read_watched_folders() -> Result<Vec<crate::import::WatchedFolderScanStatus>, crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_skipped => set_candidate_skipped(candidate_key: crate::import::CandidateKey, skipped: bool) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_search_with_status => search_with_status(query: crate::import::SearchQuery, source: bae_catalogs::catalog::Catalog) -> Result<crate::import::GroupedSearchResults, crate::import::ImportError>);
    delegate_sync!(import, import_retry_candidate_search => retry_candidate_search(candidate_key: crate::import::CandidateKey) -> ());
    delegate_sync!(import, import_clear_candidate_search => clear_candidate_search(candidate_key: crate::import::CandidateKey) -> ());
    delegate_async!(import, import_start_import => start_import(candidate_key: &crate::import::CandidateKey) -> Result<crate::import::ImportStart, crate::import::ImportError>);
    delegate_async!(import, import_selected => import_selected(candidate_key: &crate::import::CandidateKey) -> Result<crate::import::ImportStart, crate::import::ImportError>);
    delegate_async!(import, import_save_discogs_token => save_discogs_token(token: &str) -> Result<crate::import::DiscogsSaveOutcome, crate::import::ImportError>);
    delegate_async!(import, import_revalidate_discogs_token => revalidate_discogs_token() -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_remove_discogs_token => remove_discogs_token() -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_select_candidate_release => select_candidate_release(candidate_key: crate::import::CandidateKey, link: crate::import::PressingLink) -> Result<u64, crate::import::ImportError>);
    delegate_async!(import, import_select_candidate_file_tags => select_candidate_file_tags(candidate_key: crate::import::CandidateKey) -> Result<u64, crate::import::ImportError>);
    delegate_async!(import, import_clear_candidate_metadata => clear_candidate_metadata(candidate_key: crate::import::CandidateKey) -> Result<u64, crate::import::ImportError>);
    delegate_async!(import, import_refresh_watched_folder => refresh_watched_folder(path: std::path::PathBuf) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_sheet_binding_options => sheet_binding_options(candidate_key: crate::import::CandidateKey, sheet_file_id: String) -> Result<Vec<crate::import::folder_scanner::SheetReferenceOptions>, crate::import::ImportError>);
    delegate_async!(import, import_set_sheet_binding => set_sheet_binding(candidate_key: crate::import::CandidateKey, sheet_file_id: String, file_reference: String, audio_file_id: Option<String>) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_sheet_disc => set_sheet_disc(candidate_key: crate::import::CandidateKey, sheet_file_id: String, disc: crate::import::folder_scanner::SheetDisc) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_fetch_remote_covers => fetch_remote_covers(target: crate::import::cover_art::CoverTarget) -> Result<crate::import::cover_art::RemoteCoverGallery, crate::import::ImportError>);
    delegate_async!(import, import_fetch_remote_image_bytes => fetch_remote_image_bytes(image: bae_catalogs::remote_image::RemoteImageSet, pixels: Option<u32>) -> Result<Option<bae_catalogs::remote_image::RemoteImage>, crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_cover => set_candidate_cover(candidate_key: &crate::import::CandidateKey, cover: crate::import::CoverSelection) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_album => set_candidate_album(candidate_key: &crate::import::CandidateKey, album_id: Option<String>) -> Result<(), crate::import::ImportError>);
    delegate_async!(manager, search_albums => search_albums(query: &crate::library::LibrarySearchQuery) -> Result<Vec<crate::db::DbAlbumSearchResult>, crate::library::LibraryError>);
    delegate_async!(import, import_move_candidate_pane => move_candidate_pane(candidate_key: &crate::import::CandidateKey, pane_move: crate::import::PaneMove) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_search_form => set_candidate_search_form(candidate_key: &crate::import::CandidateKey, search: crate::import::SearchForm) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_edit_field => set_candidate_edit_field(candidate_key: &crate::import::CandidateKey, edit: crate::import::DraftFieldEdit) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_album_artists => set_candidate_album_artists(candidate_key: &crate::import::CandidateKey, assignments: Vec<crate::import::Credited>) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_track_edit => set_candidate_track_edit(candidate_key: &crate::import::CandidateKey, track: crate::import::RawTrackEdit) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_track_artists => set_candidate_track_artists(candidate_key: &crate::import::CandidateKey, track_ids: Vec<String>, assignments: crate::import::TrackArtistAssignments) -> Result<(), crate::import::ImportError>);

    /// Register the platform's artwork analyzer, which extraction reads
    /// barcodes and text off a candidate's images with.
    pub fn extraction_register_analyzer(
        &self,
        analyzer: std::sync::Arc<dyn crate::signals::ArtworkAnalyzer>,
    ) {
        self.inner.import.register_artwork_analyzer(analyzer);
    }

    pub(crate) fn watch_import_runtime_values(
        &self,
    ) -> crate::import::candidate_runtime::RuntimeValuesWatch {
        self.inner.import.watch_runtime_values()
    }

    /// Identify an existing library release after the person opens the
    /// re-identify sheet, asking about what this session's choices say —
    /// none made yet on its first run. Extraction resolves the disc ID and
    /// artwork from the library rather than from a scanned folder, so —
    /// unlike [`Self::rerun_identify`] — this does not go through the
    /// identification queue: there is no candidate folder to key a stored
    /// verdict by. Its runs go under the release's re-identify key
    /// ([`crate::import::CandidateKey::reidentify`]).
    pub fn identify_release_for_lookup(&self, release_id: String) {
        let choices = self
            .release_lookup_choices()
            .entry(crate::import::CandidateKey::reidentify(&release_id))
            .or_default()
            .clone();
        self.start_release_identification(release_id, choices);
    }

    /// Make one change to what a library release's session asks about, and
    /// identify it again from the changed choices.
    pub fn edit_release_lookup_choices(
        &self,
        release_id: String,
        edit: crate::import::LookupChoiceEdit,
    ) {
        let choices = {
            let mut held = self.release_lookup_choices();
            let choices = held
                .entry(crate::import::CandidateKey::reidentify(&release_id))
                .or_default();
            *choices = choices.clone().edited(edit);
            choices.clone()
        };
        self.start_release_identification(release_id, choices);
    }

    /// End a library release's re-identify session: stop its identification
    /// and forget what it asked about, so the next session starts afresh.
    pub async fn end_release_identification(
        &self,
        release_id: &str,
    ) -> Result<(), crate::library::LibraryError> {
        let candidate_key = crate::import::CandidateKey::reidentify(release_id);
        self.release_lookup_choices().remove(&candidate_key);
        self.inner.identification.cancel(vec![candidate_key]).await
    }

    pub(super) fn release_lookup_choices(
        &self,
    ) -> std::sync::MutexGuard<
        '_,
        std::collections::HashMap<crate::import::CandidateKey, crate::import::LookupChoices>,
    > {
        self.inner
            .release_lookup_choices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn start_release_identification(
        &self,
        release_id: String,
        choices: crate::import::LookupChoices,
    ) {
        let candidate_key = crate::import::CandidateKey::reidentify(&release_id);
        let run = self.inner.import.new_identification_run();
        // The sheet names a release, not a draft, and reading its title back
        // is a database round trip this synchronous command does not take: a
        // re-identify run searches by words the person typed, else asks its
        // identifiers and stops there.
        let title_search = choices
            .search_words
            .as_ref()
            .and_then(|words| crate::identify::TitleSearch::of(&words.album, &words.artist));
        if !self.inner.import.start_identification(
            run,
            candidate_key.clone(),
            crate::signals::ExtractionSource::Release { release_id },
            bae_catalogs::rate_limiter::CallPriority::Interactive,
            choices,
            title_search,
        ) {
            tracing::warn!("re-identify for {candidate_key} has no source to ask; no run started");
        }
    }

    /// Identify a folder candidate again, reading what the candidate says its
    /// lookup asks about and the sources the library asks now. A run takes its
    /// inputs once, at its start, so asking for it again is asking for a new
    /// run: whatever is going for this candidate is cancelled and a fresh
    /// interactive run replaces it, along with any stored answer.
    ///
    /// This is also what re-asking a failed provider is. Every lookup goes out
    /// again; the response cache answers the ones that had already succeeded,
    /// so what is bought is exactly what failed.
    pub fn rerun_identify(&self, candidate_key: crate::import::CandidateKey) {
        self.inner.identification.rerun_identify(candidate_key);
    }

    /// Stop these candidates' identification, however it was started, and
    /// store nothing.
    pub async fn cancel_identification(
        &self,
        candidate_keys: Vec<crate::import::CandidateKey>,
    ) -> Result<(), crate::library::LibraryError> {
        self.inner.identification.cancel(candidate_keys).await
    }

    /// Take every candidate off the identification queue.
    pub async fn cancel_all_identification(&self) -> Result<(), crate::library::LibraryError> {
        self.inner.identification.cancel_all().await
    }

    /// Every key with something in flight right now, read from the
    /// standings table and the runtime.
    pub async fn candidate_runtimes(
        &self,
    ) -> Result<
        std::collections::HashMap<
            crate::import::CandidateKey,
            crate::import::CandidateRuntimeSnapshot,
        >,
        crate::library::LibraryError,
    > {
        self.inner.import.candidate_runtimes().await
    }

    /// What is in flight for one key right now, read from the standings table
    /// and the runtime.
    pub async fn candidate_runtime(
        &self,
        key: &crate::import::CandidateKey,
    ) -> Result<Option<crate::import::CandidateRuntimeSnapshot>, crate::library::LibraryError> {
        self.inner.import.candidate_runtime(key).await
    }

    /// Claim a candidate the way committing an import does, for a test with
    /// no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn claim_candidate_for_import_for_test(
        &self,
        candidate_key: &crate::import::CandidateKey,
        import_id: &str,
    ) {
        self.inner
            .import
            .claim_candidate_for_import_for_test(candidate_key, import_id)
            .await;
    }

    /// Move an import's claim the way the worker's steps do, for a test with
    /// no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn move_candidate_import_for_test(
        &self,
        candidate_key: &crate::import::CandidateKey,
        import_id: &str,
        standing: crate::import::ImportStanding,
    ) {
        self.inner
            .import
            .move_candidate_import_for_test(candidate_key, import_id, standing)
            .await;
    }

    /// End an import's claim on a candidate the way the import's own ending
    /// does, for a test with no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn end_candidate_import_for_test(
        &self,
        candidate_key: &crate::import::CandidateKey,
        import_id: &str,
    ) {
        self.inner
            .import
            .end_candidate_import_for_test(candidate_key, import_id)
            .await;
    }

    /// The signals extraction has found for one key so far — the read a form
    /// does once when it opens, after it has subscribed to the UI events.
    pub fn candidate_signals(
        &self,
        key: &crate::import::CandidateKey,
    ) -> Option<crate::signals::Signals> {
        self.inner.import.candidate_signals(key)
    }

    /// Every key with something in flight, and each key that changes after.
    pub fn watch_candidate_runtimes(&self) -> crate::import::RuntimeSnapshotsWatch {
        self.inner.import.watch_candidate_runtimes()
    }

    /// Each running import's percent, by candidate key, as it stands now.
    pub fn import_percents(&self) -> std::collections::HashMap<crate::import::CandidateKey, u32> {
        self.inner.import.watch_import_percents().borrow().clone()
    }

    /// Each running import's percent, by candidate key, now and as it moves.
    pub fn watch_import_percents(
        &self,
    ) -> tokio::sync::watch::Receiver<std::collections::HashMap<crate::import::CandidateKey, u32>>
    {
        self.inner.import.watch_import_percents()
    }

    /// Every key's runtime as each write of its standing leaves it, in the
    /// order they land, from now on, for a test.
    #[cfg(feature = "test-utils")]
    pub fn every_candidate_runtime_change_for_test(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<crate::import::CandidateRuntimeChange> {
        self.inner.import.every_runtime_change_for_test()
    }

    /// Every scan event from now on, none dropped, for a test.
    #[cfg(feature = "test-utils")]
    pub fn every_scan_event_for_test(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<crate::import::ScanEvent> {
        self.inner.import.every_scan_event_for_test()
    }

    /// Open an import list showing `view`, reconfigurable by view and by
    /// window, with nothing folded and nothing selected.
    pub async fn subscribe_import_list(
        &self,
        view: ImportListView,
    ) -> Result<ImportListSubscription, crate::library::LibraryError> {
        ImportListSubscription::open(
            self.inner.manager.clone(),
            view,
            crate::library::LibraryPageWindows::new(),
        )
        .await
    }

    /// Where each watched folder's scan stands: the first value at once, then
    /// a value only when the scans changed, which the live query tests on the
    /// processed progress.
    pub fn subscribe_folder_scans(&self) -> coven::LiveQuery<crate::import::FolderScanProgress> {
        self.inner.manager.subscribe_folder_scan_progress()
    }

    /// One read of a list showing `view` through `windows`, for a caller
    /// with no subscription: a list opened for the read and closed after it.
    pub async fn load_import_list(
        &self,
        view: ImportListView,
        windows: crate::library::LibraryPageWindows,
    ) -> Result<crate::import::ImportListProjection, crate::library::LibraryError> {
        self.inner
            .manager
            .load_import_list_once(view, windows)
            .await
    }

    /// One candidate as the pane reads it, once, with its standing.
    pub async fn load_import_candidate(
        &self,
        key: &crate::import::CandidateKey,
    ) -> Result<Option<ImportCandidateDetail>, crate::library::LibraryError> {
        Ok(self
            .inner
            .manager
            .load_import_candidate(key)
            .await?
            .map(crate::import::ImportCandidateDetailProjection::resolve))
    }

    /// The import pane's candidate as it changes, read for the key the
    /// subscription is set to: its rows and where it stands among them.
    /// `None` once the key names no scanned folder, which is what clears a
    /// selection. Another candidate is a new key on the same read.
    pub fn subscribe_import_candidate(
        &self,
    ) -> crate::library::DetailSubscription<ImportCandidateDetail, crate::import::CandidateKey>
    {
        self.read_import_candidate(self.inner.manager.subscribe_import_candidate(None))
    }

    /// [`Self::subscribe_import_candidate`], with a handle that counts its
    /// live query's runs, for a test that measures which writes rerun it.
    #[cfg(feature = "test-utils")]
    pub fn subscribe_import_candidate_with_runs_for_test(
        &self,
    ) -> (
        crate::library::DetailSubscription<ImportCandidateDetail, crate::import::CandidateKey>,
        coven::LiveQueryRuns,
    ) {
        let query = self.inner.manager.subscribe_import_candidate(None);
        let runs = query.runs();
        (self.read_import_candidate(query), runs)
    }

    /// The pane's read through `query`, which starts with no key.
    fn read_import_candidate(
        &self,
        query: coven::ReconfigurableLiveQuery<
            Option<crate::import::CandidateKey>,
            Option<crate::import::ImportCandidateDetailProjection>,
        >,
    ) -> crate::library::DetailSubscription<ImportCandidateDetail, crate::import::CandidateKey>
    {
        let (key_tx, mut keys) = tokio::sync::watch::channel(None::<crate::import::CandidateKey>);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = self.inner.manager.clone();
        let task = crate::library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task("import candidate", task.clone(), async move {
            let mut query = reconfigurable_live_query_events(&manager, query);
            // A key set before this task runs is the first one read.
            let mut key = keys.borrow_and_update().clone();
            if key.is_some() {
                query.set(key.clone());
            }
            loop {
                let value = tokio::select! {
                    event = query.recv() => match event {
                        None => return,
                        Some(Ok(read)) => {
                            Ok(read.map(crate::import::ImportCandidateDetailProjection::resolve))
                        }
                        Some(Err(error)) => Err(error),
                    },
                    changed = keys.changed() => {
                        if changed.is_err() { return; }
                        key = keys.borrow_and_update().clone();
                        query.set(key.clone());
                        continue;
                    }
                };
                let value = value.map(|value| crate::library::DetailSnapshot {
                    id: key.clone(),
                    value,
                });
                if tx.send(value).is_err() {
                    return;
                }
            }
        });
        crate::library::DetailSubscription::new(key_tx, rx, task)
    }
}
