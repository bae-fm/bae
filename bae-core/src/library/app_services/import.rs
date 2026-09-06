//! The import surface of [`AppServices`]: identification triggers, the import
//! event bus, and the list and per-candidate subscriptions.

use super::*;
use crate::import::{
    ImportCandidateDetail, ImportCandidateDetailProjection, ImportListProjection,
    ImportListRequest, ImportListSubscription, ImportListView, TriageRuntimeFacts,
};

impl AppServices {
    delegate_async!(import, import_candidate_source_folders => candidate_source_folders(key: &str) -> Result<Vec<String>, crate::import::ImportError>);
    delegate_async!(import, import_review_combination => review_candidate_combination(keys: Vec<String>) -> Result<crate::import::combination::CombinationReview, crate::import::ImportError>);
    delegate_async!(import, import_combine_reviewed_candidates => combine_reviewed_candidates(review: &crate::import::combination::CombinationReview, keys: Vec<String>, order: crate::import::combination::CombinationTrackOrder, name: String) -> Result<String, crate::import::ImportError>);
    delegate_async!(import, import_separate_combined_candidate => separate_combined_candidate(key: &str) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_add_watched_folder => add_watched_folder(path: String) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_remove_watched_folder => remove_watched_folder(path: String) -> Result<(), crate::import::ImportError>);
    delegate_sync!(import, import_scan_watched_folders => scan_watched_folders() -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_watched_folders => watched_folders() -> Result<Vec<crate::import::WatchedFolder>, crate::import::ImportError>);
    #[cfg(any(test, feature = "test-utils"))]
    delegate_sync!(import, import_emit_event_for_test => emit_event_for_test(event: crate::import::ImportEvent) -> ());
    delegate_async!(import, import_get_candidate => get_candidate(key: &str) -> Result<Option<crate::import::ImportCandidateSnapshot>, crate::library::LibraryError>);
    delegate_sync!(import, import_subscribe_folder_scan_events => subscribe_folder_scan_events() -> tokio::sync::mpsc::UnboundedReceiver<crate::import::ScanEvent>);
    delegate_async!(import, import_set_candidate_skipped => set_candidate_skipped(path: String, skipped: bool) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_search_with_status => search_with_status(query: crate::import::SearchQuery, source: crate::import::MetadataSource) -> Result<crate::import::GroupedSearchResults, crate::import::ImportError>);
    delegate_sync!(import, import_start_candidate_search => start_candidate_search(candidate_key: String, query: crate::import::SearchQuery) -> ());
    delegate_sync!(import, import_retry_candidate_search => retry_candidate_search(candidate_key: String) -> ());
    delegate_sync!(import, import_clear_candidate_search => clear_candidate_search(candidate_key: String) -> ());
    delegate_async!(import, import_preview_file_tags_for_folder => preview_file_tags_for_folder(candidate_key: String) -> Result<crate::import::ReleaseUserEdit, crate::import::ImportError>);
    delegate_async!(import, import_start_import => start_import(candidate_key: &str, storage_mode: crate::import::StorageMode, pin: bool) -> Result<String, crate::import::ImportError>);
    delegate_async!(import, import_merge_candidate_artist_identity_conflict => merge_candidate_artist_identity_conflict(candidate_key: &str, surviving_artist_id: &str) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_save_discogs_token => save_discogs_token(token: &str) -> Result<crate::import::DiscogsSaveOutcome, crate::import::ImportError>);
    delegate_async!(import, import_revalidate_discogs_token => revalidate_discogs_token() -> Result<(), crate::import::ImportError>);
    delegate_sync!(import, import_remove_discogs_token => remove_discogs_token() -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_select_candidate_metadata_provenance => select_candidate_metadata_provenance(candidate_key: String, provenance: crate::import::MetadataProvenance) -> Result<u64, crate::import::ImportError>);
    delegate_async!(import, import_clear_candidate_metadata => clear_candidate_metadata(candidate_key: String) -> Result<u64, crate::import::ImportError>);
    delegate_async!(import, import_refresh_watched_folder => refresh_watched_folder(path: String) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_folder_release_decision => set_folder_release_decision(key: crate::import::FolderReleaseDecisionKey, decision: crate::import::FolderReleaseDecision) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_sheet_binding_options => sheet_binding_options(candidate_key: String, sheet_file_id: String) -> Result<Vec<crate::import::folder_scanner::SheetBindingOption>, crate::import::ImportError>);
    delegate_async!(import, import_set_sheet_binding => set_sheet_binding(candidate_key: String, sheet_file_id: String, audio_file_id: Option<String>) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_sheet_disc => set_sheet_disc(candidate_key: String, sheet_file_id: String, disc: crate::import::folder_scanner::SheetDisc) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_file_role => set_file_role(candidate_key: String, file_id: String, choice: crate::import::folder_scanner::FileRoleChoice) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_fetch_remote_covers => fetch_remote_covers(target: crate::import::cover_art::CoverTarget) -> Result<crate::import::cover_art::RemoteCoverGallery, crate::import::ImportError>);
    delegate_async!(import, import_fetch_remote_image_bytes => fetch_remote_image_bytes(url: String) -> Result<Option<crate::import::cover_art::RemoteImage>, crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_cover => set_candidate_cover(candidate_key: &str, cover: crate::import::CoverSelection) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_presentation => set_candidate_presentation(candidate_key: &str, presentation: crate::import::MetadataPresentation) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_search_form => set_candidate_search_form(candidate_key: &str, search: crate::import::SearchForm) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_pane_error => set_candidate_pane_error(candidate_key: &str, error: Option<String>) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_edit_field => set_candidate_edit_field(candidate_key: &str, field: crate::import::CandidateEditField, value: String) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_album_artists => set_candidate_album_artists(candidate_key: &str, assignments: Vec<crate::import::ArtistAssignment>) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_track_edit => set_candidate_track_edit(candidate_key: &str, track: crate::import::RawTrackEdit) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_set_candidate_track_artists => set_candidate_track_artists(candidate_key: &str, track_ids: Vec<String>, assignments: crate::import::TrackArtistAssignments) -> Result<(), crate::import::ImportError>);
    delegate_async!(import, import_drop_candidate_track => drop_candidate_track(candidate_key: &str, track_id: String) -> Result<(), crate::import::ImportError>);
    delegate_sync!(identify, identify_new_run => new_run() -> crate::identify::IdentifyRunId);
    delegate_sync!(identify, identify_start => start(run: crate::identify::IdentifyRunId, key: String, priority: crate::util::rate_limiter::CallPriority) -> ());
    delegate_sync!(identify, identify_cancel => cancel(key: &str) -> ());
    delegate_sync!(identify, identify_toggle_signal => toggle_signal(key: &str, signal: crate::identify::SignalToggle) -> ());
    delegate_sync!(identify, identify_rerun => rerun(key: &str) -> ());
    delegate_sync!(extraction, extraction_register_analyzer => register_analyzer(analyzer: std::sync::Arc<dyn crate::signals::ArtworkAnalyzer>) -> ());
    delegate_sync!(extraction, extraction_start => start(key: String, source: crate::signals::ExtractionSource, priority: crate::util::rate_limiter::CallPriority) -> ());
    delegate_sync!(extraction, extraction_cancel => cancel(key: &str) -> ());

    pub(crate) fn subscribe_import_events(
        &self,
    ) -> tokio::sync::broadcast::Receiver<crate::import::ImportEvent> {
        self.inner.import.subscribe_events()
    }

    /// Identify a folder candidate after the person explicitly enters Lookup:
    /// the run goes out at `Interactive`, and its verdict is persisted like
    /// the sweep's own.
    ///
    /// Re-identifying a library release is deliberately *not* routed through
    /// here: it has no candidate folder, so there is nothing to key a stored
    /// verdict by.
    pub fn identify_folder_for_lookup(&self, candidate_key: String) {
        self.inner.sweep.identify_for_explicit_lookup(candidate_key);
    }

    /// Re-run a candidate's identification from the toolbar. Dispatches on
    /// where the run lives: a live driver re-combines from its retained
    /// signals; a candidate showing a resumed verdict has no driver, so a
    /// fresh interactive run replaces the stored answer. Re-identify keys
    /// always have a live driver while their sheet is open, so they take the
    /// first arm.
    pub fn rerun_identify(&self, candidate_key: String) {
        if self.inner.identify.is_running(&candidate_key) {
            self.inner.identify.rerun(&candidate_key);
        } else {
            self.inner.sweep.rerun_for_explicit_lookup(candidate_key);
        }
    }

    /// Re-ask only the lookups that failed, keeping every answer that landed.
    /// A live driver retries its failed providers in place; a candidate
    /// showing a resumed verdict has no driver and no per-provider answers
    /// to keep, so a fresh interactive run replaces the stored answer.
    pub fn retry_failed_identify(&self, candidate_key: String) {
        if self.inner.identify.is_running(&candidate_key) {
            self.inner.identify.retry_failed(&candidate_key);
        } else {
            self.inner.sweep.rerun_for_explicit_lookup(candidate_key);
        }
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
    pub async fn claim_candidate_for_import_for_test(&self, candidate_key: &str) {
        self.inner
            .import
            .claim_candidate_for_import_for_test(candidate_key)
            .await;
    }

    /// The signals extraction has found for one key so far — the read a form
    /// does once when it opens, after it has subscribed to the UI bus.
    pub fn candidate_signals(&self, key: &str) -> Option<crate::signals::Signals> {
        self.inner.import.candidate_signals(key)
    }

    /// Every key with something in flight right now, and one change per key as
    /// runs advance.
    pub fn subscribe_candidate_runtime(
        &self,
    ) -> (
        std::collections::HashMap<String, crate::import::CandidateRuntimeSnapshot>,
        tokio::sync::broadcast::Receiver<crate::import::CandidateRuntimeChange>,
    ) {
        self.inner.import.subscribe_candidate_runtime()
    }

    /// The import list, reconfigurable by view and by window.
    ///
    /// The runtime stream is taken before the first read, so no change lands
    /// between the two; the subscription then keeps the request's runtime facts
    /// and upload standing current on its own.
    pub fn subscribe_import_list(
        &self,
        view: ImportListView,
        runtime_handle: &tokio::runtime::Handle,
    ) -> ImportListSubscription {
        let (initial_runtime, changes) = self.subscribe_candidate_runtime();
        let outbox = self.subscribe_outbox_values();
        let request = ImportListRequest {
            view,
            windows: crate::library::LibraryPageWindows::new(),
            runtime_facts: crate::import::list::facts_of(&initial_runtime),
            upload_standing: upload_standing_of(&outbox),
        };
        let query = self.inner.manager.subscribe_import_list(request.clone());
        let import = self.inner.import.clone();
        ImportListSubscription::start(
            query,
            request,
            changes,
            move || import.candidate_runtimes(),
            outbox,
            runtime_handle,
        )
    }

    /// One read of the list, for a caller with no subscription.
    pub async fn load_import_list(
        &self,
        view: ImportListView,
        windows: crate::library::LibraryPageWindows,
    ) -> Result<ImportListProjection, crate::library::LibraryError> {
        self.inner
            .manager
            .load_import_list(ImportListRequest {
                view,
                windows,
                runtime_facts: crate::import::list::facts_of(&self.candidate_runtimes()),
                upload_standing: upload_standing_of(&self.subscribe_outbox_values()),
            })
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
                ImportListRequest {
                    view,
                    windows: crate::library::LibraryPageWindows::new(),
                    runtime_facts: crate::import::list::facts_of(&self.candidate_runtimes()),
                    upload_standing: upload_standing_of(&self.subscribe_outbox_values()),
                },
                candidate_key,
            )
            .await
    }

    /// One candidate as the pane reads it, once, with its runtime folded in.
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

    /// One candidate as the pane reads it, and every later read of it. `None`
    /// once the key names no scanned folder, which is what clears a selection.
    pub fn subscribe_import_candidate_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        key: String,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<Option<ImportCandidateDetail>, crate::library::LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (initial_runtime, mut changes) = self.subscribe_candidate_runtime();
        let mut query = self.inner.manager.subscribe_import_candidate(&key);
        let import = self.inner.import.clone();
        runtime_handle.spawn(async move {
            let initial = initial_runtime.get(&key);
            let mut facts = initial.map(TriageRuntimeFacts::of).unwrap_or_default();
            let mut projection: Option<ImportCandidateDetailProjection> = None;
            let deliver = |projection: &Option<ImportCandidateDetailProjection>,
                           facts: &TriageRuntimeFacts| {
                projection
                    .clone()
                    .map(|projection| projection.resolve(facts))
            };
            loop {
                tokio::select! {
                    value = query.next() => match value {
                        Ok(value) => {
                            projection = value;
                            if tx
                                .send(Ok(deliver(&projection, &facts)))
                                .is_err()
                            {
                                return;
                            }
                        }
                        Err(error) => {
                            let error = match error {
                                coven::CovenError::Database(error) => *error,
                                other => coven::DbError::Message(other.to_string()),
                            };
                            if tx
                                .send(Err(crate::library::LibraryError::Database(error)))
                                .is_err()
                            {
                                return;
                            }
                        }
                    },
                    change = changes.recv() => {
                        let next = match change {
                            Ok(crate::import::CandidateRuntimeChange::Updated {
                                key: changed,
                                runtime,
                            }) => {
                                if changed != key {
                                    continue;
                                }
                                Some(runtime)
                            }
                            Ok(crate::import::CandidateRuntimeChange::Removed { key: changed }) => {
                                if changed != key {
                                    continue;
                                }
                                None
                            }
                            Ok(crate::import::CandidateRuntimeChange::Reset { mut runtimes }) => {
                                runtimes.remove(&key)
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                                tracing::warn!(
                                    "the selected candidate dropped {count} runtime changes; \
                                     re-reading its runtime"
                                );
                                import.candidate_runtimes().remove(&key)
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                        };
                        let next_facts = next
                            .as_ref()
                            .map(TriageRuntimeFacts::of)
                            .unwrap_or_default();
                        if next_facts == facts {
                            continue;
                        }
                        facts = next_facts;
                        if projection.is_some()
                            && tx
                                .send(Ok(deliver(&projection, &facts)))
                                .is_err()
                        {
                            return;
                        }
                    },
                }
            }
        });
        rx
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
