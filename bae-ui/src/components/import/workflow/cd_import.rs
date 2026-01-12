//! CD import workflow view

use super::{
    CdRipperView, CdTocDisplayView, CdTocInfo, ConfirmationView, DiscIdLookupErrorView,
    ExactLookupView, ImportErrorDisplayView, ManualSearchPanelView, SelectedSourceView,
};
use crate::display_types::{
    ArtworkFile, CdDriveInfo, FolderMetadata, ImportPhase, MatchCandidate, SearchSource, SearchTab,
    SelectedCover, StorageProfileInfo,
};
use dioxus::prelude::*;

/// Props for CD import workflow view
#[derive(Clone, PartialEq, Props)]
pub struct CdImportViewProps {
    // Current phase
    pub phase: ImportPhase,
    // CD path (when selected)
    pub cd_path: String,
    // TOC info
    pub toc_info: Option<CdTocInfo>,
    // CdRipper phase
    pub is_scanning: bool,
    pub drives: Vec<CdDriveInfo>,
    pub selected_drive: Option<String>,
    pub on_drive_select: EventHandler<String>,
    // ExactLookup phase
    /// True while fetching exact match candidates from MusicBrainz/Discogs
    pub is_loading_exact_matches: bool,
    pub exact_match_candidates: Vec<MatchCandidate>,
    pub selected_match_index: Option<usize>,
    pub on_exact_match_select: EventHandler<usize>,
    // ManualSearch phase
    pub detected_metadata: Option<FolderMetadata>,
    pub search_source: SearchSource,
    pub on_search_source_change: EventHandler<SearchSource>,
    pub search_tab: SearchTab,
    pub on_search_tab_change: EventHandler<SearchTab>,
    pub search_artist: String,
    pub on_artist_change: EventHandler<String>,
    pub search_album: String,
    pub on_album_change: EventHandler<String>,
    pub search_year: String,
    pub on_year_change: EventHandler<String>,
    pub search_label: String,
    pub on_label_change: EventHandler<String>,
    pub search_catalog_number: String,
    pub on_catalog_number_change: EventHandler<String>,
    pub search_barcode: String,
    pub on_barcode_change: EventHandler<String>,
    pub is_searching: bool,
    pub search_error: Option<String>,
    pub has_searched: bool,
    pub manual_match_candidates: Vec<MatchCandidate>,
    pub on_manual_match_select: EventHandler<usize>,
    pub on_search: EventHandler<()>,
    pub on_manual_confirm: EventHandler<MatchCandidate>,
    // DiscID lookup error
    pub discid_lookup_error: Option<String>,
    /// True while retrying a failed DiscID lookup
    pub is_retrying_discid_lookup: bool,
    pub on_retry_discid_lookup: EventHandler<()>,
    // Confirmation phase
    pub confirmed_candidate: Option<MatchCandidate>,
    pub selected_cover: Option<SelectedCover>,
    pub display_cover_url: Option<String>,
    pub artwork_files: Vec<ArtworkFile>,
    pub storage_profiles: Vec<StorageProfileInfo>,
    pub selected_profile_id: Option<String>,
    pub is_importing: bool,
    pub preparing_step_text: Option<String>,
    pub on_select_remote_cover: EventHandler<String>,
    pub on_select_local_cover: EventHandler<String>,
    pub on_storage_profile_change: EventHandler<Option<String>>,
    pub on_edit: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
    pub on_configure_storage: EventHandler<()>,
    // Clear/change CD
    pub on_clear: EventHandler<()>,
    // Error display
    pub import_error: Option<String>,
    pub duplicate_album_id: Option<String>,
    pub on_view_duplicate: EventHandler<String>,
}

/// CD import workflow view
#[component]
pub fn CdImportView(props: CdImportViewProps) -> Element {
    rsx! {
        div { class: "space-y-6",
            if props.phase == ImportPhase::FolderSelection {
                CdRipperView {
                    is_scanning: props.is_scanning,
                    drives: props.drives.clone(),
                    selected_drive: props.selected_drive.clone(),
                    on_drive_select: props.on_drive_select,
                }
            } else {
                div { class: "space-y-6",
                    SelectedSourceView {
                        title: "Selected CD".to_string(),
                        path: props.cd_path.clone(),
                        on_clear: props.on_clear,
                        CdTocDisplayView { toc: props.toc_info.clone(), is_reading: props.is_loading_exact_matches }
                    }

                    if props.phase == ImportPhase::ExactLookup {
                        ExactLookupView {
                            is_loading: props.is_loading_exact_matches,
                            exact_match_candidates: props.exact_match_candidates.clone(),
                            selected_match_index: props.selected_match_index,
                            on_select: props.on_exact_match_select,
                        }
                    }

                    if props.phase == ImportPhase::ManualSearch {
                        if props.discid_lookup_error.is_some() {
                            DiscIdLookupErrorView {
                                error_message: props.discid_lookup_error.clone(),
                                is_retrying: props.is_retrying_discid_lookup,
                                on_retry: props.on_retry_discid_lookup,
                            }
                        }
                        ManualSearchPanelView {
                            search_source: props.search_source,
                            on_search_source_change: props.on_search_source_change,
                            active_tab: props.search_tab,
                            on_tab_change: props.on_search_tab_change,
                            search_artist: props.search_artist.clone(),
                            on_artist_change: props.on_artist_change,
                            search_album: props.search_album.clone(),
                            on_album_change: props.on_album_change,
                            search_year: props.search_year.clone(),
                            on_year_change: props.on_year_change,
                            search_label: props.search_label.clone(),
                            on_label_change: props.on_label_change,
                            search_catalog_number: props.search_catalog_number.clone(),
                            on_catalog_number_change: props.on_catalog_number_change,
                            search_barcode: props.search_barcode.clone(),
                            on_barcode_change: props.on_barcode_change,
                            search_tokens: props
                                .detected_metadata
                                .as_ref()
                                .map(|m| m.folder_tokens.clone())
                                .unwrap_or_default(),
                            is_searching: props.is_searching,
                            error_message: props.search_error.clone(),
                            has_searched: props.has_searched,
                            match_candidates: props.manual_match_candidates.clone(),
                            selected_index: props.selected_match_index,
                            on_match_select: props.on_manual_match_select,
                            on_search: props.on_search,
                            on_confirm: props.on_manual_confirm,
                        }
                    }

                    if props.phase == ImportPhase::Confirmation {
                        if let Some(ref candidate) = props.confirmed_candidate {
                            ConfirmationView {
                                candidate: candidate.clone(),
                                selected_cover: props.selected_cover.clone(),
                                display_cover_url: props.display_cover_url.clone(),
                                artwork_files: props.artwork_files.clone(),
                                remote_cover_url: candidate.cover_url.clone(),
                                storage_profiles: props.storage_profiles.clone(),
                                selected_profile_id: props.selected_profile_id.clone(),
                                is_importing: props.is_importing,
                                preparing_step_text: props.preparing_step_text.clone(),
                                on_select_remote_cover: props.on_select_remote_cover,
                                on_select_local_cover: props.on_select_local_cover,
                                on_storage_profile_change: props.on_storage_profile_change,
                                on_edit: props.on_edit,
                                on_confirm: props.on_confirm,
                                on_configure_storage: props.on_configure_storage,
                            }
                        }
                    }

                    ImportErrorDisplayView {
                        error_message: props.import_error.clone(),
                        duplicate_album_id: props.duplicate_album_id.clone(),
                        on_view_duplicate: props.on_view_duplicate,
                    }
                }
            }
        }
    }
}
