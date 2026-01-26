//! Folder import workflow wrapper - reads context and delegates to FolderImportView

use crate::ui::app_service::use_app;
use crate::ui::import_helpers::{
    confirm_and_start_import, load_selected_release, lookup_discid, search_by_barcode,
    search_by_catalog_number, search_general, DiscIdLookupResult,
};
use bae_ui::components::import::{FolderImportView, ReleaseSidebarView};
use bae_ui::display_types::{SearchSource, SearchTab};
use bae_ui::stores::import::CandidateEvent;
use bae_ui::stores::{AppStateStoreExt, StorageProfilesStateStoreExt};
use bae_ui::ImportSource;
use dioxus::prelude::*;
use tracing::{info, warn};

// ============================================================================
// Sidebar Component
// ============================================================================

/// Folder import sidebar - shows detected releases
#[component]
pub fn FolderImportSidebar() -> Element {
    let app = use_app();
    let import_state = app.state.import();
    let detected_candidates = import_state.read().detected_candidates.clone();

    let on_folder_select = {
        let app = app.clone();
        move |_| {
            let app = app.clone();
            spawn(async move {
                if let Some(path) = rfd::AsyncFileDialog::new().pick_folder().await {
                    let path_str = path.path().to_string_lossy().to_string();
                    let import_handle = app.import_handle.clone();

                    {
                        let mut import_store = app.state.import();
                        if import_store.read().detected_candidates.is_empty() {
                            import_store.write().reset();
                        }
                        import_store.write().is_scanning_candidates = true;
                    }

                    if let Err(e) =
                        import_handle.enqueue_folder_scan(std::path::PathBuf::from(path_str))
                    {
                        warn!("Failed to add folder to scan: {}", e);
                    }
                }
            });
        }
    };

    let on_release_select = {
        let app = app.clone();
        let detected = detected_candidates.clone();
        move |index: usize| {
            let app = app.clone();
            let detected = detected.clone();
            spawn(async move {
                if let Err(e) = load_selected_release(&app, index, &detected).await {
                    warn!("Failed to switch to release: {}", e);
                }
            });
        }
    };

    let on_remove_release = {
        let app = app.clone();
        move |index: usize| {
            app.state.import().write().remove_detected_release(index);
        }
    };

    let on_clear_all_releases = {
        let app = app.clone();
        move |_| {
            let mut store = app.state.import();
            let mut state = store.write();
            state.detected_candidates.clear();
            state.candidate_states.clear();
            state.loading_candidates.clear();
            state.discid_lookup_attempted.clear();
            state.switch_candidate(None);
        }
    };

    rsx! {
        ReleaseSidebarView {
            state: import_state,
            on_select: on_release_select,
            on_add_folder: on_folder_select,
            on_remove: on_remove_release,
            on_clear_all: on_clear_all_releases,
        }
    }
}

// ============================================================================
// Main Content Component
// ============================================================================

#[component]
pub fn FolderImport() -> Element {
    let app = use_app();
    let navigator = use_navigator();

    // Get lenses for reactive props - pass directly for granular reactivity
    let import_state = app.state.import();
    let storage_profiles = app.state.storage_profiles().profiles();

    // Extract values needed by handlers (handlers need current values, not lenses)
    let current_candidate_key = import_state.read().current_candidate_key.clone();

    // Handlers
    let on_folder_select = {
        let app = app.clone();
        move |_| {
            let app = app.clone();
            spawn(async move {
                if let Some(path) = rfd::AsyncFileDialog::new().pick_folder().await {
                    let path_str = path.path().to_string_lossy().to_string();
                    let import_handle = app.import_handle.clone();

                    // Clear existing candidates if this is the first folder
                    {
                        let mut import_store = app.state.import();
                        if import_store.read().detected_candidates.is_empty() {
                            import_store.write().reset();
                        }
                        import_store.write().is_scanning_candidates = true;
                    }

                    if let Err(e) =
                        import_handle.enqueue_folder_scan(std::path::PathBuf::from(path_str))
                    {
                        warn!("Failed to add folder to scan: {}", e);
                    }
                }
            });
        }
    };

    let on_exact_match_select = {
        let app = app.clone();
        move |index: usize| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SelectExactMatch(index));
        }
    };

    let on_confirm_exact_match = {
        let app = app.clone();
        move |_| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::ConfirmExactMatch);
        }
    };

    // Manual search handler
    let perform_search = {
        let app = app.clone();
        move || {
            let app = app.clone();
            spawn(async move {
                let mut import_store = app.state.import();
                let search_state = import_store.read().get_search_state();
                let metadata = import_store.read().get_metadata();

                let Some(search_state) = search_state else {
                    return;
                };

                let tab = search_state.search_tab;
                let source = search_state.search_source;

                match tab {
                    bae_ui::display_types::SearchTab::General => {
                        let artist = search_state.search_artist.clone();
                        let album = search_state.search_album.clone();
                        let year = search_state.search_year.clone();
                        let label = search_state.search_label.clone();

                        if artist.trim().is_empty()
                            && album.trim().is_empty()
                            && year.trim().is_empty()
                            && label.trim().is_empty()
                        {
                            import_store
                                .write()
                                .dispatch(CandidateEvent::SearchComplete {
                                    results: vec![],
                                    error: Some("Please fill in at least one field".to_string()),
                                });
                            return;
                        }

                        import_store.write().dispatch(CandidateEvent::StartSearch);

                        let result =
                            search_general(metadata, source, artist, album, year, label).await;
                        match result {
                            Ok(candidates) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: candidates,
                                        error: None,
                                    });
                            }
                            Err(e) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: vec![],
                                        error: Some(format!("Search failed: {}", e)),
                                    });
                            }
                        }
                    }
                    bae_ui::display_types::SearchTab::CatalogNumber => {
                        let catno = search_state.search_catalog_number.clone();
                        if catno.trim().is_empty() {
                            import_store
                                .write()
                                .dispatch(CandidateEvent::SearchComplete {
                                    results: vec![],
                                    error: Some("Please enter a catalog number".to_string()),
                                });
                            return;
                        }

                        import_store.write().dispatch(CandidateEvent::StartSearch);

                        let result = search_by_catalog_number(metadata, source, catno).await;
                        match result {
                            Ok(candidates) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: candidates,
                                        error: None,
                                    });
                            }
                            Err(e) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: vec![],
                                        error: Some(format!("Search failed: {}", e)),
                                    });
                            }
                        }
                    }
                    bae_ui::display_types::SearchTab::Barcode => {
                        let barcode = search_state.search_barcode.clone();
                        if barcode.trim().is_empty() {
                            import_store
                                .write()
                                .dispatch(CandidateEvent::SearchComplete {
                                    results: vec![],
                                    error: Some("Please enter a barcode".to_string()),
                                });
                            return;
                        }

                        import_store.write().dispatch(CandidateEvent::StartSearch);

                        let result = search_by_barcode(metadata, source, barcode).await;
                        match result {
                            Ok(candidates) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: candidates,
                                        error: None,
                                    });
                            }
                            Err(e) => {
                                import_store
                                    .write()
                                    .dispatch(CandidateEvent::SearchComplete {
                                        results: vec![],
                                        error: Some(format!("Search failed: {}", e)),
                                    });
                            }
                        }
                    }
                }
            });
        }
    };

    let on_manual_match_select = {
        let app = app.clone();
        move |index: usize| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SelectSearchResult(index));
        }
    };

    let on_manual_confirm = {
        let app = app.clone();
        move |_candidate: bae_ui::display_types::MatchCandidate| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::ConfirmSearchResult);
        }
    };

    let on_retry_discid_lookup = {
        let app = app.clone();
        move |_| {
            let app = app.clone();
            spawn(async move {
                let mut import_store = app.state.import();

                let mb_discid = import_store
                    .read()
                    .get_metadata()
                    .and_then(|m| m.mb_discid.clone());

                if let Some(mb_discid) = mb_discid {
                    import_store
                        .write()
                        .dispatch(CandidateEvent::StartDiscIdLookup(mb_discid.clone()));
                    import_store.write().is_looking_up = true;

                    info!("Retrying DiscID lookup...");
                    match lookup_discid(&mb_discid).await {
                        Ok(result) => {
                            let matches = match result {
                                DiscIdLookupResult::NoMatches => vec![],
                                DiscIdLookupResult::SingleMatch(c) => vec![*c],
                                DiscIdLookupResult::MultipleMatches(cs) => cs,
                            };
                            import_store.write().is_looking_up = false;
                            import_store
                                .write()
                                .dispatch(CandidateEvent::DiscIdLookupComplete {
                                    matches,
                                    error: None,
                                });
                        }
                        Err(e) => {
                            import_store.write().is_looking_up = false;
                            import_store
                                .write()
                                .dispatch(CandidateEvent::DiscIdLookupComplete {
                                    matches: vec![],
                                    error: Some(e),
                                });
                        }
                    }
                } else {
                    import_store.write().is_looking_up = false;
                }
            });
        }
    };

    // Confirmation handlers
    let on_edit = {
        let app = app.clone();
        move |_| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::GoBackToIdentify);
        }
    };

    let on_confirm = {
        let app = app.clone();
        move |_| {
            let app = app.clone();
            let navigator = navigator;
            spawn(async move {
                let confirmed = app.state.import().read().get_confirmed_candidate();
                if let Some(candidate) = confirmed {
                    if let Err(e) =
                        confirm_and_start_import(&app, candidate, ImportSource::Folder, navigator)
                            .await
                    {
                        warn!("Failed to confirm and start import: {}", e);
                    }
                }
            });
        }
    };

    // Search field change handlers
    let on_search_source_change = {
        let app = app.clone();
        move |source: SearchSource| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SetSearchSource(source));
        }
    };

    let on_search_tab_change = {
        let app = app.clone();
        move |tab: SearchTab| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::SetSearchTab(tab));
        }
    };

    let on_artist_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Artist,
                    value,
                });
        }
    };

    let on_album_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Album,
                    value,
                });
        }
    };

    let on_year_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Year,
                    value,
                });
        }
    };

    let on_label_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Label,
                    value,
                });
        }
    };

    let on_catalog_number_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::CatalogNumber,
                    value,
                });
        }
    };

    let on_barcode_change = {
        let app = app.clone();
        move |value: String| {
            app.state
                .import()
                .write()
                .dispatch(CandidateEvent::UpdateSearchField {
                    field: bae_ui::stores::import::SearchField::Barcode,
                    value,
                });
        }
    };

    // Text file viewing
    let mut selected_text_file = use_signal(|| None::<String>);

    // Load text file contents when selected
    let folder_path = current_candidate_key.clone();
    let text_file_contents_resource = use_resource(move || {
        let folder = folder_path.clone().unwrap_or_default();
        let selected = selected_text_file.read().clone();
        async move {
            let name = selected?;
            let path = std::path::Path::new(&folder).join(&name);
            std::fs::read_to_string(&path).ok()
        }
    });

    let text_file_content = text_file_contents_resource.read().clone().unwrap_or(None);

    rsx! {
        FolderImportView {
            state: import_state,
            selected_text_file: selected_text_file.read().clone(),
            text_file_content,
            storage_profiles,
            on_folder_select_click: on_folder_select,
            on_text_file_select: move |name| selected_text_file.set(Some(name)),
            on_text_file_close: move |_| selected_text_file.set(None),
            on_skip_detection: |_| {},
            on_exact_match_select,
            on_confirm_exact_match,
            on_search_source_change,
            on_search_tab_change,
            on_artist_change,
            on_album_change,
            on_year_change,
            on_label_change,
            on_catalog_number_change,
            on_barcode_change,
            on_manual_match_select,
            on_search: move |_| perform_search(),
            on_manual_confirm,
            on_retry_discid_lookup,
            on_select_remote_cover: |_| {},
            on_select_local_cover: |_| {},
            on_storage_profile_change: |_| {},
            on_edit,
            on_confirm,
            on_configure_storage: |_| {},
            on_view_duplicate: |_| {},
        }
    }
}
