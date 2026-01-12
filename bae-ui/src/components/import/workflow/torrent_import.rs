//! Torrent import workflow view

use super::{
    ConfirmationView, DiscIdLookupErrorView, ExactLookupView, ImportErrorDisplayView,
    ManualSearchPanelView, MetadataDetectionPromptView, SelectedSourceView,
    TorrentFilesDisplayView, TorrentInfoDisplayView, TorrentTrackerDisplayView, TrackerStatus,
};
use crate::display_types::{
    ArtworkFile, FolderMetadata, ImportPhase, MatchCandidate, SearchSource, SearchTab,
    SelectedCover, StorageProfileInfo, TorrentFileInfo, TorrentInfo,
};
use crate::{TorrentInputMode, TorrentInputView};
use dioxus::prelude::*;

/// Props for torrent import workflow view
#[derive(Clone, PartialEq, Props)]
pub struct TorrentImportViewProps {
    // Current phase
    pub phase: ImportPhase,
    // Torrent path (when selected)
    pub torrent_path: String,
    // Torrent info
    pub torrent_info: Option<TorrentInfo>,
    pub tracker_statuses: Vec<TrackerStatus>,
    pub torrent_files: Vec<TorrentFileInfo>,
    // TorrentInput phase
    pub input_mode: TorrentInputMode,
    pub is_dragging: bool,
    pub on_mode_change: EventHandler<TorrentInputMode>,
    pub on_file_select: EventHandler<()>,
    pub on_magnet_submit: EventHandler<String>,
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
    // Metadata detection prompt (for cue files)
    pub show_metadata_detection_prompt: bool,
    pub on_detect_metadata: EventHandler<()>,
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
    // Clear/change torrent
    pub on_clear: EventHandler<()>,
    // Error display
    pub import_error: Option<String>,
    pub duplicate_album_id: Option<String>,
    pub on_view_duplicate: EventHandler<String>,
}

/// Torrent import workflow view
#[component]
pub fn TorrentImportView(props: TorrentImportViewProps) -> Element {
    rsx! {
        div {
            if props.phase == ImportPhase::FolderSelection {
                TorrentInputView {
                    input_mode: props.input_mode,
                    is_dragging: props.is_dragging,
                    on_mode_change: props.on_mode_change,
                    on_select_click: props.on_file_select,
                    on_magnet_submit: props.on_magnet_submit,
                }
            } else {
                div { class: "space-y-6",
                    SelectedSourceView {
                        title: "Selected Torrent".to_string(),
                        path: props.torrent_path.clone(),
                        on_clear: props.on_clear,

                        TorrentTrackerDisplayView { trackers: props.tracker_statuses.clone() }

                        if let Some(ref info) = props.torrent_info {
                            TorrentInfoDisplayView { info: info.clone() }
                        }

                        TorrentFilesDisplayView { files: props.torrent_files.clone() }
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

                        if props.show_metadata_detection_prompt {
                            MetadataDetectionPromptView { on_detect: props.on_detect_metadata }
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
