//! CD import workflow wrapper - reads context and delegates to CdImportView

use crate::ui::components::import::workflow::shared::confirmation::to_display_candidate;
use crate::ui::components::import::ImportSource;
use crate::ui::components::import::SearchSource as BaeSearchSource;
use crate::ui::import_context::{detection, ImportContext, ImportPhase, SearchTab as BaeSearchTab};
use bae_core::cd::CdDrive;
use bae_ui::components::import::{CdImportView, CdTocInfo};
use bae_ui::display_types::{CdDriveInfo, SearchSource, SearchTab};
use dioxus::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;
use tracing::{info, warn};

fn to_display_phase(phase: &ImportPhase) -> bae_ui::display_types::ImportPhase {
    match phase {
        ImportPhase::FolderSelection => bae_ui::display_types::ImportPhase::FolderSelection,
        ImportPhase::ReleaseSelection => bae_ui::display_types::ImportPhase::ReleaseSelection,
        ImportPhase::MetadataDetection => bae_ui::display_types::ImportPhase::MetadataDetection,
        ImportPhase::ExactLookup => bae_ui::display_types::ImportPhase::ExactLookup,
        ImportPhase::ManualSearch => bae_ui::display_types::ImportPhase::ManualSearch,
        ImportPhase::Confirmation => bae_ui::display_types::ImportPhase::Confirmation,
    }
}

fn to_display_search_source(source: &BaeSearchSource) -> SearchSource {
    match source {
        BaeSearchSource::MusicBrainz => SearchSource::MusicBrainz,
        BaeSearchSource::Discogs => SearchSource::Discogs,
    }
}

fn to_display_search_tab(tab: &BaeSearchTab) -> SearchTab {
    match tab {
        BaeSearchTab::General => SearchTab::General,
        BaeSearchTab::CatalogNumber => SearchTab::CatalogNumber,
        BaeSearchTab::Barcode => SearchTab::Barcode,
    }
}

#[component]
pub fn CdImport() -> Element {
    let import_context = use_context::<Rc<ImportContext>>();
    let navigator = use_navigator();

    // CD drive scanning state
    let is_scanning = use_signal(|| true);
    let drives = use_signal(Vec::<CdDriveInfo>::new);
    let mut selected_drive = use_signal(|| Option::<String>::None);
    let mut is_searching = use_signal(|| false);

    // Scan for drives on mount
    use_effect({
        let mut is_scanning = is_scanning;
        let mut drives = drives;
        move || {
            spawn(async move {
                is_scanning.set(true);
                match CdDrive::detect_drives() {
                    Ok(drive_list) => {
                        let display_drives: Vec<CdDriveInfo> = drive_list
                            .iter()
                            .map(|d| CdDriveInfo {
                                device_path: d.device_path.to_string_lossy().to_string(),
                                name: d.name.clone(),
                            })
                            .collect();
                        drives.set(display_drives);
                    }
                    Err(e) => {
                        warn!("Failed to list CD drives: {}", e);
                    }
                }
                is_scanning.set(false);
            });
        }
    });

    // Read context state
    let folder_path = import_context.folder_path();
    let _detected_metadata = import_context.detected_metadata();
    let import_phase = import_context.import_phase();
    let exact_match_candidates = import_context.exact_match_candidates();
    let selected_match_index = import_context.selected_match_index();
    let confirmed_candidate = import_context.confirmed_candidate();
    let is_looking_up = import_context.is_looking_up();
    let import_error_message = import_context.import_error_message();
    let duplicate_album_id = import_context.duplicate_album_id();
    let cd_toc_info = import_context.cd_toc_info();

    // Manual search state from context
    let search_artist = import_context.search_artist();
    let search_album = import_context.search_album();
    let search_year = import_context.search_year();
    let search_label = import_context.search_label();
    let search_catalog_number = import_context.search_catalog_number();
    let search_barcode = import_context.search_barcode();
    let active_tab = import_context.search_tab();
    let search_source = import_context.search_source();
    let match_candidates = import_context.manual_match_candidates();
    let error_message = import_context.error_message();
    let has_searched = import_context.has_searched();

    // Prepare TOC info for view
    let toc_info = cd_toc_info
        .read()
        .as_ref()
        .map(|(disc_id, first_track, last_track)| CdTocInfo {
            disc_id: disc_id.clone(),
            first_track: *first_track,
            last_track: *last_track,
        });

    // Convert candidates to display types
    let display_exact_candidates: Vec<bae_ui::display_types::MatchCandidate> =
        exact_match_candidates
            .read()
            .iter()
            .map(to_display_candidate)
            .collect();

    let display_manual_candidates: Vec<bae_ui::display_types::MatchCandidate> = match_candidates
        .read()
        .iter()
        .map(to_display_candidate)
        .collect();

    let display_confirmed: Option<bae_ui::display_types::MatchCandidate> = confirmed_candidate
        .read()
        .as_ref()
        .map(to_display_candidate);

    // Handlers
    let on_drive_select = {
        let import_context = import_context.clone();
        move |drive_path_str: String| {
            selected_drive.set(Some(drive_path_str.clone()));
            let import_context = import_context.clone();
            let drive_path = PathBuf::from(&drive_path_str);
            spawn(async move {
                use bae_core::cd::CdDrive;
                let drive = CdDrive {
                    device_path: drive_path.clone(),
                    name: drive_path_str.clone(),
                };
                match drive.read_toc() {
                    Ok(toc) => {
                        import_context.set_cd_toc_info(Some((
                            toc.disc_id.clone(),
                            toc.first_track,
                            toc.last_track,
                        )));
                        if let Err(e) = import_context
                            .load_cd_for_import(drive_path_str, toc.disc_id)
                            .await
                        {
                            warn!("Failed to load CD: {}", e);
                        }
                    }
                    Err(e) => {
                        import_context.set_is_looking_up(false);
                        import_context.set_import_error_message(Some(format!(
                            "Failed to read CD TOC: {}",
                            e
                        )));
                    }
                }
            });
        }
    };

    let on_clear = {
        let import_context = import_context.clone();
        move |_| {
            import_context.reset();
        }
    };

    let on_exact_match_select = {
        let import_context = import_context.clone();
        move |index: usize| {
            import_context.select_exact_match(index);
        }
    };

    // Manual search handlers
    let mut perform_search = {
        let import_context = import_context.clone();
        move || {
            let tab = *active_tab.read();
            let source = *search_source.read();

            match tab {
                BaeSearchTab::General => {
                    let artist = search_artist.read().clone();
                    let album = search_album.read().clone();
                    let year = search_year.read().clone();
                    let label = search_label.read().clone();

                    if artist.trim().is_empty()
                        && album.trim().is_empty()
                        && year.trim().is_empty()
                        && label.trim().is_empty()
                    {
                        import_context.set_error_message(Some(
                            "Please fill in at least one field".to_string(),
                        ));
                        return;
                    }

                    is_searching.set(true);
                    import_context.set_error_message(None);
                    import_context.set_manual_match_candidates(Vec::new());

                    let ctx = import_context.clone();
                    let mut is_searching = is_searching;
                    spawn(async move {
                        match ctx.search_general(source, artist, album, year, label).await {
                            Ok(candidates) => ctx.set_manual_match_candidates(candidates),
                            Err(e) => ctx.set_error_message(Some(format!("Search failed: {}", e))),
                        }
                        ctx.set_has_searched(true);
                        is_searching.set(false);
                    });
                }
                BaeSearchTab::CatalogNumber => {
                    let catno = search_catalog_number.read().clone();
                    if catno.trim().is_empty() {
                        import_context
                            .set_error_message(Some("Please enter a catalog number".to_string()));
                        return;
                    }

                    is_searching.set(true);
                    import_context.set_error_message(None);
                    import_context.set_manual_match_candidates(Vec::new());

                    let ctx = import_context.clone();
                    let mut is_searching = is_searching;
                    spawn(async move {
                        match ctx.search_by_catalog_number(source, catno).await {
                            Ok(candidates) => ctx.set_manual_match_candidates(candidates),
                            Err(e) => ctx.set_error_message(Some(format!("Search failed: {}", e))),
                        }
                        ctx.set_has_searched(true);
                        is_searching.set(false);
                    });
                }
                BaeSearchTab::Barcode => {
                    let barcode = search_barcode.read().clone();
                    if barcode.trim().is_empty() {
                        import_context
                            .set_error_message(Some("Please enter a barcode".to_string()));
                        return;
                    }

                    is_searching.set(true);
                    import_context.set_error_message(None);
                    import_context.set_manual_match_candidates(Vec::new());

                    let ctx = import_context.clone();
                    let mut is_searching = is_searching;
                    spawn(async move {
                        match ctx.search_by_barcode(source, barcode).await {
                            Ok(candidates) => ctx.set_manual_match_candidates(candidates),
                            Err(e) => ctx.set_error_message(Some(format!("Search failed: {}", e))),
                        }
                        ctx.set_has_searched(true);
                        is_searching.set(false);
                    });
                }
            }
        }
    };

    let on_manual_match_select = {
        let import_context = import_context.clone();
        move |index: usize| {
            import_context.set_selected_match_index(Some(index));
        }
    };

    let on_manual_confirm = {
        let import_context = import_context.clone();
        move |candidate: bae_ui::display_types::MatchCandidate| {
            if let Some(bae_candidate) = match_candidates
                .read()
                .iter()
                .find(|c| c.title() == candidate.title)
            {
                import_context.confirm_candidate(bae_candidate.clone());
            }
        }
    };

    let on_retry_discid_lookup = {
        let import_context = import_context.clone();
        move |_| {
            let import_context = import_context.clone();
            spawn(async move {
                info!("Retrying DiscID lookup...");
                detection::retry_discid_lookup(&import_context).await;
            });
        }
    };

    // Confirmation handlers
    let on_edit = {
        let import_context = import_context.clone();
        move |_| {
            import_context.reject_confirmation();
        }
    };

    let on_confirm = {
        let import_context = import_context.clone();
        move |_| {
            if let Some(candidate) = confirmed_candidate.read().as_ref().cloned() {
                let import_context = import_context.clone();
                let navigator = navigator;
                spawn(async move {
                    if let Err(e) = import_context
                        .confirm_and_start_import(candidate, ImportSource::Cd, navigator)
                        .await
                    {
                        warn!("Failed to confirm and start import: {}", e);
                    }
                });
            }
        }
    };

    rsx! {
        CdImportView {
            phase: to_display_phase(&import_phase.read()),
            cd_path: folder_path.read().clone(),
            toc_info,
            is_scanning: *is_scanning.read(),
            drives: drives.read().clone(),
            selected_drive: selected_drive.read().clone(),
            on_drive_select,
            is_loading_exact_matches: *is_looking_up.read(),
            exact_match_candidates: display_exact_candidates,
            selected_match_index: *selected_match_index.read(),
            on_exact_match_select,
            detected_metadata: None, // CD imports don't use folder metadata
            search_source: to_display_search_source(&search_source.read()),
            on_search_source_change: {
                let import_context = import_context.clone();
                move |source: SearchSource| {
                    let bae_source = match source {
                        SearchSource::MusicBrainz => BaeSearchSource::MusicBrainz,
                        SearchSource::Discogs => BaeSearchSource::Discogs,
                    };
                    import_context.set_search_source(bae_source);
                    import_context.set_manual_match_candidates(Vec::new());
                    import_context.set_error_message(None);
                }
            },
            search_tab: to_display_search_tab(&active_tab.read()),
            on_search_tab_change: {
                let import_context = import_context.clone();
                move |tab: SearchTab| {
                    let bae_tab = match tab {
                        SearchTab::General => BaeSearchTab::General,
                        SearchTab::CatalogNumber => BaeSearchTab::CatalogNumber,
                        SearchTab::Barcode => BaeSearchTab::Barcode,
                    };
                    import_context.set_search_tab(bae_tab);
                }
            },
            search_artist: search_artist.read().clone(),
            on_artist_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_artist(value)
            },
            search_album: search_album.read().clone(),
            on_album_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_album(value)
            },
            search_year: search_year.read().clone(),
            on_year_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_year(value)
            },
            search_label: search_label.read().clone(),
            on_label_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_label(value)
            },
            search_catalog_number: search_catalog_number.read().clone(),
            on_catalog_number_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_catalog_number(value)
            },
            search_barcode: search_barcode.read().clone(),
            on_barcode_change: {
                let import_context = import_context.clone();
                move |value: String| import_context.set_search_barcode(value)
            },
            is_searching: *is_searching.read(),
            search_error: error_message.read().clone(),
            has_searched: *has_searched.read(),
            manual_match_candidates: display_manual_candidates,
            on_manual_match_select,
            on_search: move |_| perform_search(),
            on_manual_confirm,
            discid_lookup_error: import_context.discid_lookup_error().read().clone(),
            is_retrying_discid_lookup: *is_looking_up.read(),
            on_retry_discid_lookup,
            confirmed_candidate: display_confirmed,
            selected_cover: None,
            display_cover_url: None,
            artwork_files: Vec::new(),
            storage_profiles: Vec::new(),
            selected_profile_id: None,
            is_importing: false,
            preparing_step_text: None,
            on_select_remote_cover: |_| {},
            on_select_local_cover: |_| {},
            on_storage_profile_change: |_| {},
            on_edit,
            on_confirm,
            on_configure_storage: |_| {},
            on_clear,
            import_error: import_error_message.read().clone(),
            duplicate_album_id: duplicate_album_id.read().clone(),
            on_view_duplicate: |_| {},
        }
    }
}
