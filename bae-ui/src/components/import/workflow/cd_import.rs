//! CD import workflow view
//!
//! A 2-step flow for importing music from a CD:
//!
//! ## Step 1: Identify
//! User selects a CD drive, then the system identifies the CD via DiscID.
//! If ambiguous, user picks from candidates. If no match, user searches manually.
//!
//! ## Step 2: Confirm
//! User reviews the match, selects cover art and storage profile, then imports.
//!
//! ## Reactive State Pattern
//! Pass `ReadStore<ImportState>` down to children. Use lenses where possible.

use super::{
    CdRipperView, CdTocDisplayView, ConfirmationView, DiscIdLookupErrorView,
    ImportErrorDisplayView, ManualSearchPanelView, MultipleExactMatchesView, SelectedSourceView,
};
use crate::components::StorageProfile;
use crate::display_types::{
    CdDriveInfo, IdentifyMode, ImportStep, MatchCandidate, SearchSource, SearchTab,
};
use crate::stores::import::{CandidateState, ConfirmPhase, ImportState};
use dioxus::prelude::*;

/// Props for CD import workflow view
#[derive(Clone, PartialEq, Props)]
pub struct CdImportViewProps {
    /// Import state store (enables lensing into fields)
    pub state: ReadStore<ImportState>,

    // === CD-specific state (not in ImportState) ===
    /// True while scanning for CD drives
    pub is_scanning: bool,
    /// Available CD drives
    pub drives: Vec<CdDriveInfo>,
    /// Currently selected drive path
    pub selected_drive: Option<String>,
    /// Callback when user selects a drive
    pub on_drive_select: EventHandler<String>,

    // === External data ===
    /// Storage profiles (from app context)
    pub storage_profiles: ReadSignal<Vec<StorageProfile>>,

    // === Callbacks ===
    pub on_exact_match_select: EventHandler<usize>,
    pub on_confirm_exact_match: EventHandler<()>,
    pub on_switch_to_manual_search: EventHandler<()>,
    pub on_switch_to_exact_matches: EventHandler<String>,
    pub on_search_source_change: EventHandler<SearchSource>,
    pub on_search_tab_change: EventHandler<SearchTab>,
    pub on_artist_change: EventHandler<String>,
    pub on_album_change: EventHandler<String>,
    pub on_catalog_number_change: EventHandler<String>,
    pub on_barcode_change: EventHandler<String>,
    pub on_manual_match_select: EventHandler<usize>,
    pub on_search: EventHandler<()>,
    pub on_manual_confirm: EventHandler<MatchCandidate>,
    pub on_retry_discid_lookup: EventHandler<()>,
    pub on_select_remote_cover: EventHandler<String>,
    pub on_select_local_cover: EventHandler<String>,
    pub on_storage_profile_change: EventHandler<Option<String>>,
    pub on_edit: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
    pub on_configure_storage: EventHandler<()>,
    pub on_clear: EventHandler<()>,
    pub on_view_duplicate: EventHandler<String>,
}

/// CD import workflow view
///
/// Passes state signal down to children - reads only at leaf level.
#[component]
pub fn CdImportView(props: CdImportViewProps) -> Element {
    let state = props.state;

    // Read only what's needed for routing decisions
    let st = state.read();
    let step = st.get_import_step();
    let cd_path = st.current_candidate_key.clone().unwrap_or_default();
    let identify_mode = st.get_identify_mode();
    drop(st);

    rsx! {
        div { class: "space-y-6",
            match step {
                ImportStep::Identify => rsx! {
                    if cd_path.is_empty() {
                        CdRipperView {
                            is_scanning: props.is_scanning,
                            drives: props.drives.clone(),
                            selected_drive: props.selected_drive.clone(),
                            on_drive_select: props.on_drive_select,
                        }
                    } else {
                        CdIdentifyContent {
                            state,
                            cd_path,
                            identify_mode,
                            on_clear: props.on_clear,
                            on_exact_match_select: props.on_exact_match_select,
                            on_confirm_exact_match: props.on_confirm_exact_match,
                            on_switch_to_manual_search: props.on_switch_to_manual_search,
                            on_switch_to_exact_matches: props.on_switch_to_exact_matches,
                            on_search_source_change: props.on_search_source_change,
                            on_search_tab_change: props.on_search_tab_change,
                            on_artist_change: props.on_artist_change,
                            on_album_change: props.on_album_change,
                            on_catalog_number_change: props.on_catalog_number_change,
                            on_barcode_change: props.on_barcode_change,
                            on_manual_match_select: props.on_manual_match_select,
                            on_search: props.on_search,
                            on_manual_confirm: props.on_manual_confirm,
                            on_retry_discid_lookup: props.on_retry_discid_lookup,
                        }
                    }
                },
                ImportStep::Confirm => rsx! {
                    CdConfirmContent {
                        state,
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

/// CD Identify content - reads state at leaf level
#[component]
fn CdIdentifyContent(
    state: ReadStore<ImportState>,
    cd_path: String,
    identify_mode: IdentifyMode,
    on_clear: EventHandler<()>,
    on_exact_match_select: EventHandler<usize>,
    on_confirm_exact_match: EventHandler<()>,
    on_switch_to_manual_search: EventHandler<()>,
    on_switch_to_exact_matches: EventHandler<String>,
    on_search_source_change: EventHandler<SearchSource>,
    on_search_tab_change: EventHandler<SearchTab>,
    on_artist_change: EventHandler<String>,
    on_album_change: EventHandler<String>,
    on_catalog_number_change: EventHandler<String>,
    on_barcode_change: EventHandler<String>,
    on_manual_match_select: EventHandler<usize>,
    on_search: EventHandler<()>,
    on_manual_confirm: EventHandler<MatchCandidate>,
    on_retry_discid_lookup: EventHandler<()>,
) -> Element {
    // Read TOC info at leaf level
    let st = state.read();
    let toc_info = st
        .cd_toc_info
        .as_ref()
        .map(|(disc_id, first, last)| super::CdTocInfo {
            disc_id: disc_id.clone(),
            first_track: *first,
            last_track: *last,
        });
    let is_looking_up = st.is_looking_up;
    let discid_lookup_error = st.get_discid_lookup_error();
    drop(st);

    rsx! {
        div { class: "space-y-6",
            SelectedSourceView {
                title: "Selected CD".to_string(),
                path: cd_path,
                on_clear,
                on_reveal: |_| {},
                CdTocDisplayView { toc: toc_info, is_reading: is_looking_up }
            }
            match identify_mode {
                IdentifyMode::Created | IdentifyMode::DiscIdLookup(_) => rsx! {},
                IdentifyMode::MultipleExactMatches(_) => rsx! {
                    MultipleExactMatchesView {
                        state,
                        on_select: on_exact_match_select,
                        on_confirm: on_confirm_exact_match,
                        on_switch_to_manual_search,
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
                    ManualSearchPanelView {
                        state,
                        on_search_source_change,
                        on_tab_change: on_search_tab_change,
                        on_artist_change,
                        on_album_change,
                        on_catalog_number_change,
                        on_barcode_change,
                        on_match_select: on_manual_match_select,
                        on_search,
                        on_confirm: on_manual_confirm,
                        on_switch_to_exact_matches,
                    }
                },
            }
        }
    }
}

/// CD Confirm content - reads state at leaf level
#[component]
fn CdConfirmContent(
    state: ReadStore<ImportState>,
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
    let cd_path = st.current_candidate_key.clone().unwrap_or_default();
    let toc_info = st
        .cd_toc_info
        .as_ref()
        .map(|(disc_id, first, last)| super::CdTocInfo {
            disc_id: disc_id.clone(),
            first_track: *first,
            last_track: *last,
        });
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
                title: "Selected CD".to_string(),
                path: cd_path,
                on_clear,
                on_reveal: |_| {},
                CdTocDisplayView { toc: toc_info, is_reading: false }
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
