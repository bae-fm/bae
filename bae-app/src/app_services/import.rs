//! The import surface of [`AppServices`]: identification triggers, the import
//! event bus, and the list and per-candidate subscriptions.

use super::*;
use bae_import::{ImportCandidateDetail, ImportListView, LiveImportList};
use bae_library::live_query_events::reconfigurable_live_query_events;

impl AppServices {
    /// The library albums a parsed, non-blank query finds, as the album
    /// picker lists them: title, the album's credit line and year.
    pub async fn search_albums(
        &self,
        query: &bae_views::LibrarySearchQuery,
    ) -> Result<Vec<bae_views::DbAlbumSearchResult>, bae_library::LibraryError> {
        Ok(self
            .inner
            .views
            .search_rows(query.as_str(), bae_views::SEARCH_RESULT_LIMIT)
            .await?
            .albums)
    }

    delegate_async!(import, import_candidate_source_folders => candidate_source_folders(key: &bae_import::CandidateKey) -> Result<Vec<std::path::PathBuf>, bae_import::ImportError>);
    delegate_async!(import, import_combine_candidates => combine_candidates(keys: Vec<bae_import::CandidateKey>) -> Result<bae_import::CandidateKey, bae_import::ImportError>);
    delegate_async!(import, import_combine_folder => combine_folder(folder: bae_import::FolderReleaseDecisionKey) -> Result<bae_import::CandidateKey, bae_import::ImportError>);
    delegate_async!(import, import_separate_candidate => separate_candidate(key: &bae_import::CandidateKey) -> Result<Vec<bae_import::CandidateKey>, bae_import::ImportError>);
    delegate_async!(import, import_add_watched_folder => add_watched_folder(path: std::path::PathBuf) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_cancel => cancel_import(candidate_key: &bae_import::CandidateKey) -> Result<(), bae_import::ImportError>);
    delegate_sync!(import, import_cancel_all_in_background => cancel_all_imports_in_background() -> ());
    delegate_async!(import, import_remove_watched_folder => remove_watched_folder(path: std::path::PathBuf) -> Result<(), bae_import::ImportError>);
    delegate_sync!(import, import_scan_watched_folders => scan_watched_folders() -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_watched_folders => watched_folders() -> Result<Vec<bae_import::WatchedFolder>, bae_import::ImportError>);
    #[cfg(any(test, feature = "test-utils"))]
    delegate_sync!(import, import_emit_event_for_test => emit_event_for_test(event: bae_import::ImportEvent) -> ());
    delegate_async!(import, import_get_candidate => get_candidate(key: &bae_import::CandidateKey) -> Result<Option<bae_import::ImportCandidateSnapshot>, bae_import::ImportError>);
    delegate_async!(import, import_read_watched_folders => read_watched_folders() -> Result<Vec<bae_import::WatchedFolderScanStatus>, bae_import::ImportError>);
    delegate_async!(import, import_set_candidate_skipped => set_candidate_skipped(candidate_key: bae_import::CandidateKey, skipped: bool) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_search_with_status => search_with_status(query: bae_catalogs::SearchQuery, source: bae_catalogs::catalog::LookupCatalog) -> Result<bae_import::GroupedSearchResults, bae_import::ImportError>);
    delegate_async!(import, import_retry_candidate_search => retry_candidate_search(candidate_key: bae_import::CandidateKey) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_start_import => start_import(candidate_key: &bae_import::CandidateKey) -> Result<bae_import::ImportStart, bae_import::ImportError>);
    delegate_async!(import, import_selected => import_selected(candidate_key: &bae_import::CandidateKey) -> Result<bae_import::ImportStart, bae_import::ImportError>);
    delegate_async!(import, import_save_discogs_token => save_discogs_token(token: &str) -> Result<bae_catalogs::providers::DiscogsCheck, bae_import::ImportError>);
    delegate_async!(import, import_revalidate_discogs_token => revalidate_discogs_token() -> Result<(), bae_import::ImportError>);
    delegate_async!(import, get_discogs_token => discogs_token() -> Result<Option<String>, bae_import::ImportError>);
    delegate_async!(import, import_remove_discogs_token => remove_discogs_token() -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_select_candidate_release => select_candidate_release(candidate_key: bae_import::CandidateKey, link: bae_identify::PressingLink, steps: bae_import::CatalogApplySteps) -> Result<u64, bae_import::ImportError>);
    delegate_async!(import, import_select_candidate_file_tags => select_candidate_file_tags(candidate_key: bae_import::CandidateKey) -> Result<u64, bae_import::ImportError>);
    delegate_async!(import, import_clear_candidate_metadata => clear_candidate_metadata(candidate_key: bae_import::CandidateKey) -> Result<u64, bae_import::ImportError>);
    delegate_async!(import, import_refresh_watched_folder => refresh_watched_folder(path: std::path::PathBuf) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_sheet_binding_options => sheet_binding_options(candidate_key: bae_import::CandidateKey, sheet_file_id: String) -> Result<Vec<bae_import::folder_scanner::SheetReferenceOptions>, bae_import::ImportError>);
    delegate_async!(import, import_set_sheet_binding => set_sheet_binding(candidate_key: bae_import::CandidateKey, sheet_file_id: String, file_reference: String, audio_file_id: Option<String>) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_set_sheet_disc => set_sheet_disc(candidate_key: bae_import::CandidateKey, sheet_file_id: String, disc: bae_import::folder_scanner::SheetDisc) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_fetch_remote_covers => fetch_remote_covers(target: bae_import::cover_art::CoverTarget) -> Result<bae_import::cover_art::RemoteCoverGallery, bae_import::ImportError>);
    delegate_async!(import, import_fetch_remote_image_bytes => fetch_remote_image_bytes(image: bae_catalogs::remote_image::RemoteImageSet, pixels: Option<u32>) -> Result<Option<bae_catalogs::remote_image::RemoteImage>, bae_import::ImportError>);
    delegate_async!(import, import_set_candidate_cover => set_candidate_cover(candidate_key: &bae_import::CandidateKey, cover: bae_import::CoverSelection) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_read_candidate_document => read_candidate_document(candidate_key: &bae_import::CandidateKey, file_id: &str) -> Result<String, bae_import::ImportError>);
    delegate_async!(import, import_set_candidate_album => set_candidate_album(candidate_key: &bae_import::CandidateKey, album_id: Option<String>) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_move_candidate_pane => move_candidate_pane(candidate_key: &bae_import::CandidateKey, pane_move: bae_import::PaneMove) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_set_candidate_search_form => set_candidate_search_form(candidate_key: &bae_import::CandidateKey, search: bae_import::SearchForm) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_set_candidate_edit_field => set_candidate_edit_field(candidate_key: &bae_import::CandidateKey, edit: bae_import::DraftFieldEdit) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_set_candidate_album_artists => set_candidate_album_artists(candidate_key: &bae_import::CandidateKey, assignments: Vec<bae_import::Credited>) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_set_candidate_track_edit => set_candidate_track_edit(candidate_key: &bae_import::CandidateKey, track: bae_import::RawTrackEdit) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, import_set_candidate_track_artists => set_candidate_track_artists(candidate_key: &bae_import::CandidateKey, track_ids: Vec<String>, assignments: bae_import::TrackArtistAssignments) -> Result<(), bae_import::ImportError>);
    delegate_async!(import, load_import_candidate => load_candidate(key: &bae_import::CandidateKey) -> Result<Option<ImportCandidateDetail>, bae_import::ImportError>);
    delegate_sync!(import, subscribe_folder_scans => subscribe_folder_scans() -> bae_db::live_query::LiveValues<bae_import::FolderScanProgress>);
    delegate_sync!(import, subscribe_import_counts => subscribe_counts() -> bae_db::live_query::LiveValues<bae_import::ImportCounts>);
    delegate_sync!(import, subscribe_candidate_runtime => subscribe_candidate_runtime(key: &bae_import::CandidateKey) -> bae_db::live_query::LiveValues<Option<bae_import::CandidateRuntimeSnapshot>>);

    /// Register the platform's artwork analyzer, which extraction reads
    /// barcodes and text off a candidate's images with.
    pub fn extraction_register_analyzer(
        &self,
        analyzer: std::sync::Arc<dyn bae_import::extraction::ArtworkAnalyzer>,
    ) {
        self.inner.import.register_artwork_analyzer(analyzer);
    }

    /// Identify an existing library release after the person opens the
    /// re-identify sheet, asking about what this session's choices say —
    /// none made yet on its first run. Extraction resolves the disc ID and
    /// artwork from the library rather than from a scanned folder, so —
    /// unlike [`Self::rerun_identify`] — this does not go through the
    /// identification queue: there is no candidate folder to key a stored
    /// verdict by. Its runs go under the release's re-identify key
    /// ([`bae_import::CandidateKey::reidentify`]).
    pub async fn identify_release_for_lookup(
        &self,
        release_id: String,
    ) -> Result<(), bae_library::LibraryError> {
        let choices = self
            .release_lookup_choices()
            .entry(bae_import::CandidateKey::reidentify(&release_id))
            .or_default()
            .clone();
        self.start_release_identification(release_id, choices).await
    }

    /// Make one change to what a library release's session asks about, and
    /// identify it again from the changed choices.
    pub async fn edit_release_lookup_choices(
        &self,
        release_id: String,
        edit: bae_identify::LookupChoiceEdit,
    ) -> Result<(), bae_library::LibraryError> {
        let choices = {
            let mut held = self.release_lookup_choices();
            let choices =
                held.entry(bae_import::CandidateKey::reidentify(&release_id)).or_default();
            *choices = choices.clone().edited(edit);
            choices.clone()
        };
        self.start_release_identification(release_id, choices).await
    }

    /// End a library release's re-identify session: stop its identification
    /// and forget what it asked about, so the next session starts afresh.
    pub async fn end_release_identification(
        &self,
        release_id: &str,
    ) -> Result<(), bae_import::ImportError> {
        let candidate_key = bae_import::CandidateKey::reidentify(release_id);
        self.release_lookup_choices().remove(&candidate_key);
        self.inner.identification.cancel(vec![candidate_key]).await
    }

    pub(super) fn release_lookup_choices(
        &self,
    ) -> std::sync::MutexGuard<
        '_,
        std::collections::HashMap<bae_import::CandidateKey, bae_identify::LookupChoices>,
    > {
        self.inner.release_lookup_choices.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn start_release_identification(
        &self,
        release_id: String,
        choices: bae_identify::LookupChoices,
    ) -> Result<(), bae_library::LibraryError> {
        let candidate_key = bae_import::CandidateKey::reidentify(&release_id);
        let run = self.inner.import.new_identification_run();
        // The sheet names a release, not a draft, so no title is read back: a
        // re-identify run searches by words the person typed, else asks its
        // identifiers and stops there.
        let title_search = choices
            .search_words
            .as_ref()
            .and_then(|words| bae_identify::TitleSearch::of(&words.album, &words.artist));
        if !self
            .inner
            .import
            .start_identification(
                run,
                candidate_key.clone(),
                bae_import::extraction::ExtractionSource::Release { release_id },
                bae_catalogs::rate_limiter::CallPriority::Interactive,
                choices,
                title_search,
            )
            .await?
        {
            tracing::warn!("re-identify for {candidate_key} has no source to ask; no run started");
        }
        Ok(())
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
    pub fn rerun_identify(&self, candidate_key: bae_import::CandidateKey) {
        self.inner.identification.rerun_identify(candidate_key);
    }

    /// Stop these candidates' identification, however it was started, and
    /// store nothing.
    pub async fn cancel_identification(
        &self,
        candidate_keys: Vec<bae_import::CandidateKey>,
    ) -> Result<(), bae_import::ImportError> {
        self.inner.identification.cancel(candidate_keys).await
    }

    /// Take every candidate off the identification queue.
    pub async fn cancel_all_identification(&self) -> Result<(), bae_import::ImportError> {
        self.inner.identification.cancel_all().await
    }

    /// Every key with something in flight right now, read from the
    /// standings table and the runtime.
    pub async fn candidate_runtimes(
        &self,
    ) -> Result<
        std::collections::HashMap<bae_import::CandidateKey, bae_import::CandidateRuntimeSnapshot>,
        bae_import::ImportError,
    > {
        self.inner.import.candidate_runtimes().await
    }

    /// What is in flight for one key right now, read from the standings table
    /// and the runtime.
    pub async fn candidate_runtime(
        &self,
        key: &bae_import::CandidateKey,
    ) -> Result<Option<bae_import::CandidateRuntimeSnapshot>, bae_import::ImportError> {
        self.inner.import.candidate_runtime(key).await
    }

    /// Claim a candidate the way committing an import does, for a test with
    /// no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn claim_candidate_for_import_for_test(
        &self,
        candidate_key: &bae_import::CandidateKey,
        import_id: &str,
    ) {
        self.inner.import.claim_candidate_for_import_for_test(candidate_key, import_id).await;
    }

    /// Move an import's claim the way the worker's steps do, for a test with
    /// no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn move_candidate_import_for_test(
        &self,
        candidate_key: &bae_import::CandidateKey,
        import_id: &str,
        step: bae_import::ImportStep,
    ) {
        self.inner.import.move_candidate_import_for_test(candidate_key, import_id, step).await;
    }

    /// End an import's claim on a candidate the way the import's own ending
    /// does, for a test with no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn end_candidate_import_for_test(
        &self,
        candidate_key: &bae_import::CandidateKey,
        import_id: &str,
    ) {
        self.inner.import.end_candidate_import_for_test(candidate_key, import_id).await;
    }

    /// Each running import's percent, by candidate key, as its import last
    /// reported.
    pub fn import_percents(&self) -> std::collections::HashMap<bae_import::CandidateKey, u32> {
        self.inner.import.import_percents()
    }

    /// Each running import's percent, by candidate key, now and as it moves.
    pub fn watch_import_percents(&self) -> bae_import::ImportPercentsWatch {
        self.inner.import.watch_import_percents()
    }

    /// Every key's runtime as each write of its standing leaves it, in the
    /// order they land, from now on, for a test.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_candidate_runtime_change_for_test(&self) -> bae_import::CandidateRuntimeChanges {
        self.inner.import.every_runtime_change_for_test()
    }

    /// Every scan event from now on, none dropped, for a test.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_scan_event_for_test(&self) -> bae_import::ScanEvents {
        self.inner.import.every_scan_event_for_test()
    }

    /// Open an import list showing `view`, reconfigurable by view and by
    /// window, with nothing folded and nothing selected.
    pub async fn subscribe_import_list(
        &self,
        view: ImportListView,
    ) -> Result<LiveImportList, bae_library::LibraryError> {
        Ok(self.inner.import.open_list(view, bae_db::LibraryPageWindows::new()).await?)
    }

    /// One read of a list showing `view` through `windows`, for a caller
    /// with no subscription: a list opened for the read and closed after it.
    pub async fn load_import_list(
        &self,
        view: ImportListView,
        windows: bae_db::LibraryPageWindows,
    ) -> Result<bae_import::ImportListProjection, bae_library::LibraryError> {
        Ok(self.inner.import.load_list(view, windows).await?)
    }

    /// The import pane's candidate as it changes, read for the key the
    /// subscription is set to: its rows and where it stands among them.
    /// `None` once the key names no scanned folder, which is what clears a
    /// selection. Another candidate is a new key on the same read.
    pub fn subscribe_import_candidate(
        &self,
    ) -> bae_library::LiveDetail<ImportCandidateDetail, bae_import::CandidateKey> {
        self.read_import_candidate(self.inner.import.subscribe_candidate(None))
    }

    /// The live query [`Self::subscribe_import_candidate`] reads through,
    /// with no key, for a test that counts its runs as it reads it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_import_candidate_query_for_test(
        &self,
    ) -> coven::ReconfigurableLiveQuery<
        Option<bae_import::CandidateKey>,
        Option<bae_import::ImportCandidateDetailProjection>,
    > {
        self.inner.import.subscribe_candidate(None)
    }

    /// The pane's read through `query`, which starts with no key.
    fn read_import_candidate(
        &self,
        query: coven::ReconfigurableLiveQuery<
            Option<bae_import::CandidateKey>,
            Option<bae_import::ImportCandidateDetailProjection>,
        >,
    ) -> bae_library::LiveDetail<ImportCandidateDetail, bae_import::CandidateKey> {
        let (key_tx, mut keys) = tokio::sync::watch::channel(None::<bae_import::CandidateKey>);
        let (tx, rx) = bae_library::live_read_channel();
        let manager = self.inner.manager.clone();
        let task = bae_library::CancellationToken::new();
        self.inner.manager.spawn_stoppable_subscription_task(
            "import candidate",
            task.clone(),
            tx.clone(),
            async move {
                let mut query = reconfigurable_live_query_events(&manager, query, tx.clone());
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
                                Ok(read.map(bae_import::ImportCandidateDetailProjection::resolve))
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
                    let value =
                        value.map(|value| bae_library::DetailSnapshot { id: key.clone(), value });
                    if tx.send(value).is_err() {
                        return;
                    }
                }
            },
        );
        bae_library::LiveDetail::new(key_tx, rx, task)
    }
}
