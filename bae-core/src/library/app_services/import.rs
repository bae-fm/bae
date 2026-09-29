//! The import surface of [`AppServices`]: identification triggers, the import
//! event bus, and the list and per-candidate subscriptions.

use super::*;
use crate::import::{
    CandidateActionBasis, CandidateLiveState, ImportCandidateDetail,
    ImportCandidateDetailProjection, ImportListProjection, ImportListRequest,
    ImportListSubscription, ImportListView, TriageRuntimeFacts,
};

impl AppServices {
    delegate_async!(import, import_candidate_source_folders => candidate_source_folders(key: &str) -> Result<Vec<String>, crate::import::ImportError>);
    delegate_async!(import, import_combine_candidates => combine_candidates(keys: Vec<String>) -> Result<String, crate::import::ImportError>);
    delegate_async!(import, import_combine_folder => combine_folder(folder: crate::import::FolderReleaseDecisionKey) -> Result<String, crate::import::ImportError>);
    delegate_async!(import, import_separate_candidate => separate_candidate(key: &str) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_add_watched_folder => add_watched_folder(path: String) -> Result<(), crate::import::ImportError>);
    delegate_sync!(import, import_cancel => cancel_import(candidate_key: &str) -> Result<(), crate::import::ImportError>);
    delegate_sync!(import, import_cancel_all => cancel_all_imports() -> ());
    delegate_async!(import, import_remove_watched_folder => remove_watched_folder(path: String) -> Result<(), crate::import::ImportError>);
    delegate_sync!(import, import_scan_watched_folders => scan_watched_folders() -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_watched_folders => watched_folders() -> Result<Vec<crate::import::WatchedFolder>, crate::import::ImportError>);
    #[cfg(any(test, feature = "test-utils"))]
    delegate_sync!(import, import_emit_event_for_test => emit_event_for_test(event: crate::import::ImportEvent) -> ());
    delegate_async!(import, import_get_candidate => get_candidate(key: &str) -> Result<Option<crate::import::ImportCandidateSnapshot>, crate::library::LibraryError>);
    delegate_async!(import, import_read_watched_folders => read_watched_folders() -> Result<Vec<crate::import::WatchedFolderScanStatus>, crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_skipped => set_candidate_skipped(path: String, skipped: bool) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_search_with_status => search_with_status(query: crate::import::SearchQuery, source: crate::import::Catalog) -> Result<crate::import::GroupedSearchResults, crate::import::ImportError>);
    delegate_sync!(import, import_retry_candidate_search => retry_candidate_search(candidate_key: String) -> ());
    delegate_sync!(import, import_clear_candidate_search => clear_candidate_search(candidate_key: String) -> ());
    delegate_async!(import, import_start_import => start_import(candidate_key: &str) -> Result<String, crate::import::ImportError>);
    delegate_async!(import, import_selected => import_selected(candidate_key: &str) -> Result<String, crate::import::ImportError>);
    delegate_async!(import, import_save_discogs_token => save_discogs_token(token: &str) -> Result<crate::import::DiscogsSaveOutcome, crate::import::ImportError>);
    delegate_async!(import, import_revalidate_discogs_token => revalidate_discogs_token() -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_remove_discogs_token => remove_discogs_token() -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_select_candidate_release => select_candidate_release(candidate_key: String, link: crate::import::PressingLink) -> Result<u64, crate::import::ImportError>);
    delegate_async!(import, import_select_candidate_file_tags => select_candidate_file_tags(candidate_key: String) -> Result<u64, crate::import::ImportError>);
    delegate_async!(import, import_reset_candidate_setup => reset_candidate_setup(candidate_key: &str) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_clear_candidate_metadata => clear_candidate_metadata(candidate_key: String) -> Result<u64, crate::import::ImportError>);
    delegate_async!(import, import_refresh_watched_folder => refresh_watched_folder(path: String) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_sheet_binding_options => sheet_binding_options(candidate_key: String, sheet_file_id: String) -> Result<Vec<crate::import::folder_scanner::SheetReferenceOptions>, crate::import::ImportError>);
    delegate_async!(import, import_set_sheet_binding => set_sheet_binding(candidate_key: String, sheet_file_id: String, file_reference: String, audio_file_id: Option<String>) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_sheet_disc => set_sheet_disc(candidate_key: String, sheet_file_id: String, disc: crate::import::folder_scanner::SheetDisc) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_fetch_remote_covers => fetch_remote_covers(target: crate::import::cover_art::CoverTarget) -> Result<crate::import::cover_art::RemoteCoverGallery, crate::import::ImportError>);
    delegate_async!(import, import_fetch_remote_image_bytes => fetch_remote_image_bytes(image: crate::import::cover_art::RemoteImageSet, pixels: Option<u32>) -> Result<Option<crate::import::cover_art::RemoteImage>, crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_cover => set_candidate_cover(candidate_key: &str, cover: crate::import::CoverSelection) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_move_candidate_pane => move_candidate_pane(candidate_key: &str, pane_move: crate::import::PaneMove) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_search_form => set_candidate_search_form(candidate_key: &str, search: crate::import::SearchForm) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_edit_field => set_candidate_edit_field(candidate_key: &str, edit: crate::import::DraftFieldEdit) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_album_artists => set_candidate_album_artists(candidate_key: &str, assignments: Vec<crate::import::ArtistAssignment>) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_track_edit => set_candidate_track_edit(candidate_key: &str, track: crate::import::RawTrackEdit) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_track_artists => set_candidate_track_artists(candidate_key: &str, track_ids: Vec<String>, assignments: crate::import::TrackArtistAssignments) -> Result<(), crate::import::ImportError>);

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
    /// verdict by.
    pub fn identify_release_for_lookup(&self, candidate_key: String, release_id: String) {
        let choices = self
            .release_lookup_choices()
            .entry(candidate_key.clone())
            .or_default()
            .clone();
        self.start_release_identification(candidate_key, release_id, choices);
    }

    /// Make one change to what a library release's session asks about, and
    /// identify it again from the changed choices.
    pub fn edit_release_lookup_choices(
        &self,
        candidate_key: String,
        release_id: String,
        edit: crate::import::LookupChoiceEdit,
    ) {
        let choices = {
            let mut held = self.release_lookup_choices();
            let choices = held.entry(candidate_key.clone()).or_default();
            *choices = choices.clone().edited(edit);
            choices.clone()
        };
        self.start_release_identification(candidate_key, release_id, choices);
    }

    /// End a library release's re-identify session: stop its identification
    /// and forget what it asked about, so the next session starts afresh.
    pub async fn end_release_identification(
        &self,
        candidate_key: String,
    ) -> Result<(), crate::library::LibraryError> {
        self.release_lookup_choices().remove(&candidate_key);
        self.inner.identification.cancel(vec![candidate_key]).await
    }

    pub(super) fn release_lookup_choices(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, crate::import::LookupChoices>>
    {
        self.inner
            .release_lookup_choices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn start_release_identification(
        &self,
        candidate_key: String,
        release_id: String,
        choices: crate::import::LookupChoices,
    ) {
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
            crate::util::rate_limiter::CallPriority::Interactive,
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
    pub fn rerun_identify(&self, candidate_key: String) {
        self.inner.identification.rerun_identify(candidate_key);
    }

    /// Stop these candidates' identification, however it was started, and
    /// store nothing.
    pub async fn cancel_identification(
        &self,
        candidate_keys: Vec<String>,
    ) -> Result<(), crate::library::LibraryError> {
        self.inner.identification.cancel(candidate_keys).await
    }

    /// Take every candidate off the identification queue.
    pub async fn cancel_all_identification(&self) -> Result<(), crate::library::LibraryError> {
        self.inner.identification.cancel_all().await
    }

    /// What is running for every candidate the runtime holds, as the rows
    /// read it.
    pub(super) fn runtime_facts(&self) -> std::collections::HashMap<String, TriageRuntimeFacts> {
        self.candidate_runtimes()
            .iter()
            .map(|(key, runtime)| (key.clone(), TriageRuntimeFacts::of(runtime)))
            .collect()
    }

    /// Every key with something in flight right now.
    pub fn candidate_runtimes(
        &self,
    ) -> std::collections::HashMap<String, crate::import::CandidateRuntimeSnapshot> {
        self.inner.import.candidate_runtimes()
    }

    /// What is in flight for one key — the read a view does once when it
    /// appears, after it has subscribed to the changes.
    pub fn candidate_runtime(&self, key: &str) -> Option<crate::import::CandidateRuntimeSnapshot> {
        self.inner.import.candidate_runtime(key)
    }

    /// Claim a candidate the way committing an import does, for a test with
    /// no worker behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn claim_candidate_for_import_for_test(&self, candidate_key: &str, import_id: &str) {
        self.inner
            .import
            .claim_candidate_for_import_for_test(candidate_key, import_id)
            .await;
    }

    /// The signals extraction has found for one key so far — the read a form
    /// does once when it opens, after it has subscribed to the UI events.
    pub fn candidate_signals(&self, key: &str) -> Option<crate::signals::Signals> {
        self.inner.import.candidate_signals(key)
    }

    /// Every key with something in flight, and each key that changes after.
    pub fn watch_candidate_runtimes(&self) -> crate::import::RuntimeSnapshotsWatch {
        self.inner.import.watch_candidate_runtimes()
    }

    /// Every change to a key's runtime from now on, for a test.
    #[cfg(feature = "test-utils")]
    pub fn every_candidate_runtime_change_for_test(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<crate::import::CandidateRuntimeChange> {
        self.inner.import.every_runtime_change_for_test()
    }

    /// The import list, reconfigurable by view and by window.
    ///
    /// The list reads the tables, the upload standing the Done tab is ordered
    /// by, and what is running for the candidates a live filter keeps, which
    /// the subscription keeps current on its own. What is running for each
    /// row is its [`Self::subscribe_candidate_live_state`].
    pub fn subscribe_import_list(
        &self,
        view: ImportListView,
        runtime_handle: &tokio::runtime::Handle,
    ) -> ImportListSubscription {
        let outbox = self.subscribe_outbox_values();
        let runtime_facts = self.inner.import.watch_runtime_facts();
        let request = ImportListRequest {
            live_standings: view.pending_filters.live_standings(runtime_facts.facts()),
            view,
            windows: crate::library::LibraryPageWindows::new(),
            upload_standing: upload_standing_of(&outbox),
        };
        let query = self.inner.manager.subscribe_import_list(request.clone());
        ImportListSubscription::start(
            query,
            self.inner.manager.subscribe_folder_scan_progress(),
            request,
            outbox,
            runtime_facts,
            runtime_handle,
        )
    }

    /// The list request for one read of `view`: the upload standing and what
    /// is running, as they stand now.
    pub(super) fn import_list_request(
        &self,
        view: ImportListView,
        windows: crate::library::LibraryPageWindows,
    ) -> ImportListRequest {
        ImportListRequest {
            live_standings: view.pending_filters.live_standings(&self.runtime_facts()),
            view,
            windows,
            upload_standing: upload_standing_of(&self.subscribe_outbox_values()),
        }
    }

    /// One read of the list, for a caller with no subscription.
    pub async fn load_import_list(
        &self,
        view: ImportListView,
        windows: crate::library::LibraryPageWindows,
    ) -> Result<ImportListProjection, crate::library::LibraryError> {
        self.inner
            .manager
            .load_import_list(self.import_list_request(view, windows))
            .await
    }

    /// The tab, disclosure state and position that reveal one candidate at its
    /// current placement.
    pub async fn locate_import_candidate(
        &self,
        view: ImportListView,
        candidate_key: &str,
    ) -> Result<Option<crate::import::ImportCandidateListLocation>, crate::library::LibraryError>
    {
        self.inner
            .manager
            .locate_import_candidate(
                self.import_list_request(view, crate::library::LibraryPageWindows::new()),
                candidate_key,
            )
            .await
    }

    /// The first candidate the identification count is still waiting on — one
    /// queued, running or having its answer written — in the queue's order
    /// under `view`. `None` when nothing is being identified, or nothing that
    /// is has a row.
    ///
    /// Asked when the person goes to it rather than kept on the list: which
    /// runs are in flight moves no row, and the list does not read it.
    pub async fn first_identifying_candidate(
        &self,
        view: ImportListView,
    ) -> Result<Option<String>, crate::library::LibraryError> {
        let identifying: std::collections::HashSet<String> = self
            .candidate_runtimes()
            .into_iter()
            .filter(|(_, runtime)| TriageRuntimeFacts::of(runtime).identifying())
            .map(|(key, _)| key)
            .collect();
        if identifying.is_empty() {
            return Ok(None);
        }
        self.inner
            .manager
            .first_import_candidate_among(
                self.import_list_request(view, crate::library::LibraryPageWindows::new()),
                identifying,
            )
            .await
    }

    /// One candidate as the pane reads it, once, with its runtime joined.
    pub async fn load_import_candidate(
        &self,
        key: &str,
    ) -> Result<Option<ImportCandidateDetail>, crate::library::LibraryError> {
        let runtime = self.candidate_runtimes().remove(key);
        let facts = runtime
            .as_ref()
            .map(TriageRuntimeFacts::of)
            .unwrap_or_default();
        Ok(self
            .inner
            .manager
            .load_import_candidate(key)
            .await?
            .map(|projection| projection.resolve(&facts)))
    }

    /// What is running for one candidate, and the commands its row offers
    /// with it, now and on every change to either.
    pub fn subscribe_candidate_live_state(
        &self,
        key: String,
        basis: CandidateActionBasis,
    ) -> tokio::sync::mpsc::UnboundedReceiver<CandidateLiveState> {
        self.inner.import.subscribe_candidate_live_state(key, basis)
    }

    /// One candidate as the pane reads it, and every later read of it. `None`
    /// once the key names no scanned folder, which is what clears a selection.
    /// The import pane's candidate as it changes, read for the key the
    /// subscription is set to: its rows, and what is running for it. Another
    /// candidate is a new key on the same read.
    pub fn subscribe_import_candidate(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> crate::library::DetailSubscription<ImportCandidateDetail> {
        let (key_tx, mut keys) = tokio::sync::watch::channel(None::<String>);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = self.inner.manager.clone();
        let query_runtime = runtime_handle.clone();
        let import = self.inner.import.clone();
        let task = runtime_handle.spawn(async move {
            let mut key = keys.borrow_and_update().clone();
            // What is running for the key, watched once there is one.
            let mut watch = key
                .clone()
                .map(|key| import.watch_candidate_facts(key));
            let mut query = reconfigurable_live_query_events(
                &query_runtime,
                manager.subscribe_import_candidate(key.clone()),
            );
            let mut projection: Option<ImportCandidateDetailProjection> = None;
            loop {
                let value = tokio::select! {
                    event = query.recv() => match event {
                        None => return,
                        Some(Ok(read)) => {
                            projection = read;
                            let facts = watch.as_ref().map(|watch| watch.facts().clone()).unwrap_or_default();
                            Ok(projection.clone().map(|read| read.resolve(&facts)))
                        }
                        Some(Err(error)) => Err(error),
                    },
                    facts = async {
                        match watch.as_mut() {
                            Some(watch) => watch.changed().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        let Some(facts) = facts else { return };
                        let Some(read) = projection.clone() else { continue };
                        Ok(Some(read.resolve(&facts)))
                    }
                    changed = keys.changed() => {
                        if changed.is_err() { return; }
                        key = keys.borrow_and_update().clone();
                        projection = None;
                        if let Some(key) = &key {
                            match watch.as_mut() {
                                Some(watch) => watch.set_key(key.clone()),
                                None => watch = Some(import.watch_candidate_facts(key.clone())),
                            }
                        }
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

/// Where every release the outbox still holds work for stands, from the
/// channel's current value. Nothing yet published, or a failed read, means the
/// order starts with everything settled and corrects itself on the next
/// snapshot.
fn upload_standing_of(
    outbox: &tokio::sync::watch::Receiver<Option<Result<crate::library::OutboxSnapshot, String>>>,
) -> std::collections::BTreeMap<String, crate::import::list::UploadStanding> {
    match &*outbox.borrow() {
        Some(Ok(snapshot)) => crate::import::list::UploadStanding::of_outbox(snapshot),
        Some(Err(_)) | None => std::collections::BTreeMap::new(),
    }
}
