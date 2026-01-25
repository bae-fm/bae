//! Folder import workflow view
//!
//! Multi-pane layout for importing music from folders:
//!
//! ## Initial State
//! User picks a folder (full-width folder selector).
//!
//! ## Working State (after folder selected)
//! Two-pane layout:
//! - Left sidebar: List of detected releases with status
//! - Right main area:
//!   - Top: Identify/Confirm workflow for selected release
//!   - Bottom (fixed): SmartFileDisplay showing folder contents
//!
//! ## Reactive State Pattern
//! Pass `ReadStore<ImportState>` down through the tree. Use lenses to read
//! individual fields. Only call `.read()` at the leaf level where you
//! actually render values.

use super::{
    ConfirmationView, DetectingMetadataView, DiscIdLookupErrorView, ImportErrorDisplayView,
    ManualSearchPanelView, MultipleMatchesView, SmartFileDisplayView,
};
use crate::components::icons::{FolderIcon, LoaderIcon};
use crate::components::StorageProfile;
use crate::components::{Button, ResizablePanel, ResizeDirection};
use crate::display_types::{IdentifyMode, ImportStep, MatchCandidate, SearchSource, SearchTab};
use crate::stores::import::{CandidateState, ConfirmPhase, ImportState, ImportStateStoreExt};
use dioxus::prelude::*;

// ============================================================================
// Main Folder Import View
// ============================================================================

/// Props for the folder import workflow view (main content area only)
///
/// The sidebar (ReleaseSidebarView) is rendered separately by the parent
/// and passed to ImportView.
#[derive(Clone, PartialEq, Props)]
pub struct FolderImportViewProps {
    /// Import state store (enables lensing into fields)
    pub state: ReadStore<ImportState>,

    // === UI-only state (not in ImportState) ===
    /// Currently viewed text file name
    #[props(default)]
    pub selected_text_file: Option<String>,
    /// Loaded text file content (for selected file)
    #[props(default)]
    pub text_file_content: Option<String>,

    // === External data (not in ImportState) ===
    /// Storage profiles (from app context)
    pub storage_profiles: ReadSignal<Vec<StorageProfile>>,

    // === Callbacks ===
    pub on_folder_select_click: EventHandler<()>,
    pub on_text_file_select: EventHandler<String>,
    pub on_text_file_close: EventHandler<()>,
    pub on_skip_detection: EventHandler<()>,
    pub on_exact_match_select: EventHandler<usize>,
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
    pub on_select_remote_cover: EventHandler<String>,
    pub on_select_local_cover: EventHandler<String>,
    pub on_storage_profile_change: EventHandler<Option<String>>,
    pub on_edit: EventHandler<()>,
    pub on_confirm: EventHandler<()>,
    pub on_configure_storage: EventHandler<()>,
    pub on_view_duplicate: EventHandler<String>,
}

/// Folder import workflow view - main content area only
///
/// The sidebar (ReleaseSidebarView) is rendered by the parent via ImportView.
/// This component renders the workflow steps and files dock.
#[component]
pub fn FolderImportView(props: FolderImportViewProps) -> Element {
    let state = props.state;

    // Use lenses for routing decisions
    let is_empty = !state.detected_candidates().read().is_empty();
    let is_scanning = *state.is_scanning_candidates().read();
    // get_import_step() is a computed value, so we read just for that
    let step = state.read().get_import_step();

    rsx! {
        div { class: "flex-1 flex flex-col min-w-0",
            if !is_empty {
                EmptyView {
                    is_scanning,
                    on_folder_select: props.on_folder_select_click,
                }
            } else {
                WorkflowContent {
                    state,
                    step,
                    storage_profiles: props.storage_profiles,
                    on_skip_detection: props.on_skip_detection,
                    on_exact_match_select: props.on_exact_match_select,
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
                    on_select_remote_cover: props.on_select_remote_cover,
                    on_select_local_cover: props.on_select_local_cover,
                    on_storage_profile_change: props.on_storage_profile_change,
                    on_edit: props.on_edit,
                    on_confirm: props.on_confirm,
                    on_configure_storage: props.on_configure_storage,
                    on_view_duplicate: props.on_view_duplicate,
                }
                FilesDock {
                    state,
                    selected_text_file: props.selected_text_file.clone(),
                    text_file_content: props.text_file_content.clone(),
                    on_text_file_select: props.on_text_file_select,
                    on_text_file_close: props.on_text_file_close,
                }
            }
        }
    }
}

// ============================================================================
// Empty State
// ============================================================================

/// Empty state shown when no candidates are detected yet
#[component]
fn EmptyView(is_scanning: bool, on_folder_select: EventHandler<()>) -> Element {
    rsx! {
        div { class: "flex-1 flex items-center justify-center px-6 py-4",
            div { class: "w-full max-w-3xl text-center space-y-3",
                if is_scanning {
                    LoaderIcon { class: "w-5 h-5 text-gray-400 animate-spin mx-auto" }
                    p { class: "text-sm text-gray-400", "Scanning folder for releases..." }
                } else {
                    Button { onclick: move |_| on_folder_select.call(()),
                        FolderIcon { class: "w-4 h-4" }
                        "Select folder"
                    }
                    p { class: "text-sm text-gray-400",
                        "We'll scan this folder for releases to import"
                    }
                }
            }
        }
    }
}

/// Main workflow content area with step routing
#[component]
fn WorkflowContent(
    state: ReadStore<ImportState>,
    step: ImportStep,
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
    on_skip_detection: EventHandler<()>,
    on_exact_match_select: EventHandler<usize>,
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
    on_select_remote_cover: EventHandler<String>,
    on_select_local_cover: EventHandler<String>,
    on_storage_profile_change: EventHandler<Option<String>>,
    on_edit: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_configure_storage: EventHandler<()>,
    on_view_duplicate: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "flex-1 flex justify-center items-center",
            match step {
                ImportStep::Identify => rsx! {
                    IdentifyStep {
                        state,
                        on_skip_detection,
                        on_exact_match_select,
                        on_search_source_change,
                        on_search_tab_change,
                        on_artist_change,
                        on_album_change,
                        on_year_change,
                        on_label_change,
                        on_catalog_number_change,
                        on_barcode_change,
                        on_manual_match_select,
                        on_search,
                        on_manual_confirm,
                        on_retry_discid_lookup,
                    }
                },
                ImportStep::Confirm => rsx! {
                    ConfirmStep {
                        state,
                        storage_profiles,
                        on_select_remote_cover,
                        on_select_local_cover,
                        on_storage_profile_change,
                        on_edit,
                        on_confirm,
                        on_configure_storage,
                        on_view_duplicate,
                    }
                },
            }
        }
    }
}

// ============================================================================
// Step 2: Identify
// ============================================================================

/// Identify step - passes state down, reads at leaf level
#[component]
fn IdentifyStep(
    state: ReadStore<ImportState>,
    on_skip_detection: EventHandler<()>,
    on_exact_match_select: EventHandler<usize>,
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
) -> Element {
    // Read to determine mode - this is routing
    let mode = state.read().get_identify_mode();

    rsx! {
        div { class: "space-y-6",
            match mode {
                IdentifyMode::Created | IdentifyMode::DiscIdLookup => rsx! {
                    DetectingMetadataView { message: "Looking up release...".to_string(), on_skip: on_skip_detection }
                },
                IdentifyMode::MultipleExactMatches => rsx! {
                    MultipleMatchesView { state, on_select: on_exact_match_select }
                },
                IdentifyMode::ManualSearch => rsx! {
                    DiscIdErrorBanner { state, on_retry: on_retry_discid_lookup }
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

/// DiscID error banner - uses lenses where possible
#[component]
fn DiscIdErrorBanner(state: ReadStore<ImportState>, on_retry: EventHandler<()>) -> Element {
    // get_discid_lookup_error() is computed, need full read for that
    let error = state.read().get_discid_lookup_error();
    let is_retrying = *state.is_looking_up().read();

    if error.is_some() {
        rsx! {
            DiscIdLookupErrorView { error_message: error, is_retrying, on_retry }
        }
    } else {
        rsx! {}
    }
}

// ============================================================================
// Step 3: Confirm
// ============================================================================

/// Confirm step - reads state at leaf level for display
#[component]
fn ConfirmStep(
    state: ReadStore<ImportState>,
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
    on_select_remote_cover: EventHandler<String>,
    on_select_local_cover: EventHandler<String>,
    on_storage_profile_change: EventHandler<Option<String>>,
    on_edit: EventHandler<()>,
    on_confirm: EventHandler<()>,
    on_configure_storage: EventHandler<()>,
    on_view_duplicate: EventHandler<String>,
) -> Element {
    // Read state at this level to get confirm-specific data
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

    let Some(candidate) = confirmed_candidate else {
        return rsx! {};
    };

    rsx! {
        div { class: "space-y-6",
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

// ============================================================================
// Files Dock
// ============================================================================

/// Resizable bottom dock showing files
#[component]
fn FilesDock(
    state: ReadStore<ImportState>,
    selected_text_file: Option<String>,
    text_file_content: Option<String>,
    on_text_file_select: EventHandler<String>,
    on_text_file_close: EventHandler<()>,
) -> Element {
    // Read files at this level
    let files = state
        .read()
        .current_candidate_state()
        .map(|s| s.files().clone())
        .unwrap_or_default();

    rsx! {
        ResizablePanel {
            storage_key: "import-files-dock-height",
            min_size: 156.0,
            max_size: 250.0,
            default_size: 156.0,
            grabber_span_ratio: 0.95,
            direction: ResizeDirection::Vertical,
            DockCard { title: "Files", class: "w-fit max-w-3xl",
                SmartFileDisplayView {
                    files,
                    selected_text_file,
                    text_file_content,
                    on_text_file_select,
                    on_text_file_close,
                }
            }
        }
    }
}

/// Centered card container with title header
#[component]
fn DockCard(
    title: &'static str,
    #[props(default = "")] class: &'static str,
    children: Element,
) -> Element {
    rsx! {
        div { class: "p-3 flex justify-center h-full",
            div { class: "h-full bg-surface-raised rounded-2xl shadow-lg shadow-black/10 px-4 py-3 overflow-y-auto {class}",
                div { class: "text-xs font-medium text-gray-300 mb-2", "{title}" }
                {children}
            }
        }
    }
}
