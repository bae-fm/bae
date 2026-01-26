//! Torrent import workflow view
//!
//! A 2-step flow for importing music from a torrent:
//!
//! ## Step 1: Identify
//! User provides a .torrent file or magnet link, then the system identifies the music
//! via metadata. If ambiguous, user picks from candidates. If no match, user searches manually.
//!
//! ## Step 2: Confirm
//! User reviews the match, selects cover art and storage profile, then imports.
//!
//! ## Reactive State Pattern
//! Pass `ReadStore<ImportState>` down to children. Use lenses where possible.

use super::{
    ConfirmationView, DiscIdLookupErrorView, ImportErrorDisplayView, ManualSearchPanelView,
    MetadataDetectionPromptView, MultipleExactMatchesView, SelectedSourceView,
    TorrentFilesDisplayView, TorrentInfoDisplayView, TorrentTrackerDisplayView, TrackerStatus,
};
use crate::components::StorageProfile;
use crate::display_types::{
    IdentifyMode, ImportStep, MatchCandidate, SearchSource, SearchTab, TorrentFileInfo, TorrentInfo,
};
use crate::stores::import::{CandidateState, ConfirmPhase, ImportState};
use crate::{TorrentInputMode, TorrentInputView};
use dioxus::prelude::*;

/// Props for torrent import workflow view
#[derive(Clone, PartialEq, Props)]
pub struct TorrentImportViewProps {
    /// Import state store (enables lensing into fields)
    pub state: ReadStore<ImportState>,

    // === Torrent-specific state (not in ImportState) ===
    /// Torrent info (parsed from .torrent file)
    pub torrent_info: Option<TorrentInfo>,
    /// Tracker connection statuses
    pub tracker_statuses: Vec<TrackerStatus>,
    /// Files in the torrent
    pub torrent_files: Vec<TorrentFileInfo>,
    /// Current input mode (file or magnet)
    pub input_mode: TorrentInputMode,
    /// True if dragging over drop zone
    pub is_dragging: bool,
    /// Callback when input mode changes
    pub on_mode_change: EventHandler<TorrentInputMode>,
    /// Callback when user clicks to select file
    pub on_file_select: EventHandler<()>,
    /// Callback when user submits magnet link
    pub on_magnet_submit: EventHandler<String>,

    // === External data ===
    /// Storage profiles (from app context)
    pub storage_profiles: ReadSignal<Vec<StorageProfile>>,

    // === Callbacks ===
    pub on_exact_match_select: EventHandler<usize>,
    pub on_confirm_exact_match: EventHandler<()>,
    pub on_search_source_change: EventHandler<SearchSource>,
    pub on_search_tab_change: EventHandler<SearchTab>,
    pub on_artist_change: EventHandler<String>,
    pub on_album_change: EventHandler<String>,
    pub on_year_change: EventHandler<String>,
    pub on_label_change: EventHandler<String>,
    pub on_catalog_number_change: EventHandler<String>,
    pub on_barcode_change: EventHandler<String>,
    pub on_manual_match_select: EventHandler<usize>,
    pub on_search: EventHandler<()>,
    pub on_manual_confirm: EventHandler<MatchCandidate>,
    pub on_retry_discid_lookup: EventHandler<()>,
    pub on_detect_metadata: EventHandler<()>,
    pub on_select_remote_cover: EventHandler<String>,
    pub on_select_local_cover: EventHandler<String>,
    pub on_storage_profile_change: EventHandler<Option<String>>,
    pub on_edit: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
    pub on_configure_storage: EventHandler<()>,
    pub on_clear: EventHandler<()>,
    pub on_view_duplicate: EventHandler<String>,
}

/// Torrent import workflow view
///
/// Passes state signal down to children - reads only at leaf level.
#[component]
pub fn TorrentImportView(props: TorrentImportViewProps) -> Element {
    let state = props.state;

    // Read only what's needed for routing decisions
    let st = state.read();
    let step = st.get_import_step();
    let torrent_path = st.current_candidate_key.clone().unwrap_or_default();
    drop(st);

    rsx! {
        div {
            match step {
                ImportStep::Identify => rsx! {
                    if torrent_path.is_empty() {
                        TorrentInputView {
                            input_mode: props.input_mode,
                            is_dragging: props.is_dragging,
                            on_mode_change: props.on_mode_change,
                            on_select_click: props.on_file_select,
                            on_magnet_submit: props.on_magnet_submit,
                        }
                    } else {
                        TorrentIdentifyContent {
                            state,
                            torrent_path,
                            torrent_info: props.torrent_info.clone(),
                            tracker_statuses: props.tracker_statuses.clone(),
                            torrent_files: props.torrent_files.clone(),
                            on_clear: props.on_clear,
                            on_exact_match_select: props.on_exact_match_select,
                            on_confirm_exact_match: props.on_confirm_exact_match,
                            on_search_source_change: props.on_search_source_change,
                            on_search_tab_change: props.on_search_tab_change,
                            on_artist_change: props.on_artist_change,
                            on_album_change: props.on_album_change,
                            on_year_change: props.on_year_change,
                            on_label_change: props.on_label_change,
                            on_catalog_number_change: props.on_catalog_number_change,
                            on_barcode_change: props.on_barcode_change,
                            on_manual_match_select: props.on_manual_match_select,
                            on_search: props.on_search,
                            on_manual_confirm: props.on_manual_confirm,
                            on_retry_discid_lookup: props.on_retry_discid_lookup,
                            on_detect_metadata: props.on_detect_metadata,
                        }
                    }
                },
                ImportStep::Confirm => rsx! {
                    TorrentConfirmContent {
                        state,
                        torrent_path,
                        torrent_info: props.torrent_info.clone(),
                        tracker_statuses: props.tracker_statuses.clone(),
                        torrent_files: props.torrent_files.clone(),
                        storage_profiles: props.storage_profiles,
                        on_clear: props.on_clear,
                        on_select_remote_cover: props.on_select_remote_cover,
                        on_select_local_cover: props.on_select_local_cover,
                        on_storage_profile_change: props.on_storage_profile_change,
                        on_edit: props.on_edit,
                        on_confirm: props.on_confirm,
                        on_configure_storage: props.on_configure_storage,
                        on_view_duplicate: props.on_view_duplicate,
                    }
                },
            }
        }
    }
}

/// Torrent Identify content - reads state at leaf level
#[component]
fn TorrentIdentifyContent(
    state: ReadStore<ImportState>,
    torrent_path: String,
    torrent_info: Option<TorrentInfo>,
    tracker_statuses: Vec<TrackerStatus>,
    torrent_files: Vec<TorrentFileInfo>,
    on_clear: EventHandler<()>,
    on_exact_match_select: EventHandler<usize>,
    on_confirm_exact_match: EventHandler<()>,
    on_search_source_change: EventHandler<SearchSource>,
    on_search_tab_change: EventHandler<SearchTab>,
    on_artist_change: EventHandler<String>,
    on_album_change: EventHandler<String>,
    on_year_change: EventHandler<String>,
    on_label_change: EventHandler<String>,
    on_catalog_number_change: EventHandler<String>,
    on_barcode_change: EventHandler<String>,
    on_manual_match_select: EventHandler<usize>,
    on_search: EventHandler<()>,
    on_manual_confirm: EventHandler<MatchCandidate>,
    on_retry_discid_lookup: EventHandler<()>,
    on_detect_metadata: EventHandler<()>,
) -> Element {
    // Read state at leaf level
    let st = state.read();
    let identify_mode = st.get_identify_mode();
    let is_looking_up = st.is_looking_up;
    let discid_lookup_error = st.get_discid_lookup_error();
    let detected_metadata = st.get_metadata();
    let has_searched = st
        .get_search_state()
        .map(|s| s.has_searched)
        .unwrap_or(false);
    drop(st);

    let show_metadata_detection_prompt =
        identify_mode == IdentifyMode::ManualSearch && detected_metadata.is_none() && !has_searched;

    rsx! {
        div { class: "space-y-6",
            SelectedSourceView {
                title: "Selected Torrent".to_string(),
                path: torrent_path,
                on_clear,
                on_reveal: |_| {},
                TorrentTrackerDisplayView { trackers: tracker_statuses }
                if let Some(ref info) = torrent_info {
                    TorrentInfoDisplayView { info: info.clone() }
                }
                TorrentFilesDisplayView { files: torrent_files }
            }
            match identify_mode {
                IdentifyMode::Created | IdentifyMode::DiscIdLookup(_) => rsx! {},
                IdentifyMode::MultipleExactMatches(_) => rsx! {
                    MultipleExactMatchesView {
                        state,
                        on_select: on_exact_match_select,
                        on_confirm: on_confirm_exact_match,
                    }
                },
                IdentifyMode::ManualSearch => rsx! {
                    if discid_lookup_error.is_some() {
                        DiscIdLookupErrorView {
                            error_message: discid_lookup_error,
                            is_retrying: is_looking_up,
                            on_retry: on_retry_discid_lookup,
                        }
                    }
                    if show_metadata_detection_prompt {
                        MetadataDetectionPromptView { on_detect: on_detect_metadata }
                    }
                    ManualSearchPanelView {
                        state,
                        on_search_source_change,
                        on_tab_change: on_search_tab_change,
                        on_artist_change,
                        on_album_change,
                        on_year_change,
                        on_label_change,
                        on_catalog_number_change,
                        on_barcode_change,
                        on_match_select: on_manual_match_select,
                        on_search,
                        on_confirm: on_manual_confirm,
                    }
                },
            }
        }
    }
}

/// Torrent Confirm content - reads state at leaf level
#[component]
fn TorrentConfirmContent(
    state: ReadStore<ImportState>,
    torrent_path: String,
    torrent_info: Option<TorrentInfo>,
    tracker_statuses: Vec<TrackerStatus>,
    torrent_files: Vec<TorrentFileInfo>,
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
    on_clear: EventHandler<()>,
    on_select_remote_cover: EventHandler<String>,
    on_select_local_cover: EventHandler<String>,
    on_storage_profile_change: EventHandler<Option<String>>,
    on_edit: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_configure_storage: EventHandler<()>,
    on_view_duplicate: EventHandler<String>,
) -> Element {
    // Read state at leaf level
    let st = state.read();
    let confirmed_candidate = st.get_confirmed_candidate();
    let selected_cover = st.get_selected_cover();
    let display_cover_url = st.get_display_cover_url();
    let artwork_files = st
        .current_candidate_state()
        .map(|s| s.files().artwork.clone())
        .unwrap_or_default();
    let selected_profile_id = st.get_storage_profile_id();

    let (is_importing, preparing_step_text, import_error) = st
        .current_candidate_state()
        .and_then(|s| match s {
            CandidateState::Confirming(cs) => Some(&cs.phase),
            _ => None,
        })
        .map(|phase| match phase {
            ConfirmPhase::Ready => (false, None, None),
            ConfirmPhase::Preparing(msg) => (false, Some(msg.clone()), None),
            ConfirmPhase::Importing => (true, None, None),
            ConfirmPhase::Failed(err) => (false, None, Some(err.clone())),
            ConfirmPhase::Completed => (false, None, None),
        })
        .unwrap_or((false, None, None));

    let import_error = import_error.or_else(|| st.import_error_message.clone());
    let duplicate_album_id = st.duplicate_album_id.clone();
    drop(st);

    let Some(candidate) = confirmed_candidate else {
        return rsx! {};
    };

    rsx! {
        div { class: "space-y-6",
            SelectedSourceView {
                title: "Selected Torrent".to_string(),
                path: torrent_path,
                on_clear,
                on_reveal: |_| {},
                TorrentTrackerDisplayView { trackers: tracker_statuses }
                if let Some(ref info) = torrent_info {
                    TorrentInfoDisplayView { info: info.clone() }
                }
                TorrentFilesDisplayView { files: torrent_files }
            }
            ConfirmationView {
                candidate: candidate.clone(),
                selected_cover,
                display_cover_url,
                artwork_files,
                remote_cover_url: candidate.cover_url.clone(),
                storage_profiles,
                selected_profile_id,
                is_importing,
                preparing_step_text,
                on_select_remote_cover,
                on_select_local_cover,
                on_storage_profile_change,
                on_edit,
                on_confirm,
                on_configure_storage,
            }
            ImportErrorDisplayView {
                error_message: import_error,
                duplicate_album_id,
                on_view_duplicate,
            }
        }
    }
}
