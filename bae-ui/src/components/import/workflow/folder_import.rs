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

use super::release_sidebar::{DEFAULT_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH};
use super::{
    ConfirmationView, DetectingMetadataView, DiscIdLookupErrorView, ImportErrorDisplayView,
    ManualSearchPanelView, MultipleMatchesView, ReleaseSidebarView, SmartFileDisplayView,
};
use crate::components::icons::{FolderIcon, LoaderIcon};
use crate::components::{ResizablePanel, ResizeDirection};
use crate::display_types::{
    CategorizedFileInfo, DetectedCandidate, FileInfo, FolderMetadata, IdentifyMode, ImportStep,
    MatchCandidate, SearchSource, SearchTab, SelectedCover, StorageProfileInfo,
};
use dioxus::prelude::*;

// ============================================================================
// Step 2: Identify
// ============================================================================

/// Props for the Identify step
#[derive(Clone, PartialEq, Props)]
pub struct IdentifyStepProps {
    /// Current mode within the Identify step
    pub mode: IdentifyMode,
    // Source display (shown in all modes)
    /// Path to the selected folder
    pub folder_path: String,
    /// Files in the folder
    pub folder_files: CategorizedFileInfo,
    /// Callback to clear/change folder
    pub on_clear: EventHandler<()>,
    /// Callback to reveal folder in file browser
    pub on_reveal: EventHandler<()>,
    // Detecting mode
    /// Callback to skip detection
    pub on_skip_detection: EventHandler<()>,
    // MultipleExactMatches mode
    /// Exact match candidates from DiscID lookup
    pub exact_match_candidates: Vec<MatchCandidate>,
    /// Currently selected match index
    pub selected_match_index: Option<usize>,
    /// Callback when user selects a match
    pub on_exact_match_select: EventHandler<usize>,
    // ManualSearch mode
    /// Detected metadata for search form pre-fill
    pub detected_metadata: Option<FolderMetadata>,
    /// Current search source
    pub search_source: SearchSource,
    /// Callback when search source changes
    pub on_search_source_change: EventHandler<SearchSource>,
    /// Current search tab
    pub search_tab: SearchTab,
    /// Callback when search tab changes
    pub on_search_tab_change: EventHandler<SearchTab>,
    /// Search form fields
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
    /// True while searching
    pub is_searching: bool,
    /// Search error message
    pub search_error: Option<String>,
    /// True if user has searched at least once
    pub has_searched: bool,
    /// Search result candidates
    pub manual_match_candidates: Vec<MatchCandidate>,
    /// Callback when user selects a search result
    pub on_manual_match_select: EventHandler<usize>,
    /// Callback to trigger search
    pub on_search: EventHandler<()>,
    /// Callback when user confirms a manual match
    pub on_manual_confirm: EventHandler<MatchCandidate>,
    // DiscID error (shown in ManualSearch mode)
    /// DiscID lookup error message
    pub discid_lookup_error: Option<String>,
    /// True while retrying DiscID lookup
    pub is_retrying_discid_lookup: bool,
    /// Callback to retry DiscID lookup
    pub on_retry_discid_lookup: EventHandler<()>,
}

/// Step 2: Identify the music (search/matching UI only, no file display)
#[component]
pub fn IdentifyStep(props: IdentifyStepProps) -> Element {
    rsx! {
        div { class: "space-y-6",
            // Mode-specific content
            match props.mode {
                IdentifyMode::Created | IdentifyMode::DiscIdLookup => rsx! {
                    DetectingMetadataView {
                        message: "Looking up release...".to_string(),
                        on_skip: props.on_skip_detection,
                    }
                },
                IdentifyMode::MultipleExactMatches => rsx! {
                    MultipleMatchesView {
                        candidates: props.exact_match_candidates.clone(),
                        selected_index: props.selected_match_index,
                        on_select: props.on_exact_match_select,
                    }
                },
                IdentifyMode::ManualSearch => rsx! {
                    // DiscID error banner (if applicable)
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
                },
            }
        }
    }
}

// ============================================================================
// Step 3: Confirm
// ============================================================================

/// Props for the Confirm step
#[derive(Clone, PartialEq, Props)]
pub struct ConfirmStepProps {
    // Source display
    /// Path to the selected folder
    pub folder_path: String,
    /// Files in the folder
    pub folder_files: CategorizedFileInfo,
    /// Callback to clear/change folder
    pub on_clear: EventHandler<()>,
    /// Callback to reveal folder in file browser
    pub on_reveal: EventHandler<()>,
    // Confirmation
    /// The confirmed match candidate
    pub confirmed_candidate: MatchCandidate,
    /// Currently selected cover
    pub selected_cover: Option<SelectedCover>,
    /// URL to display the selected cover
    pub display_cover_url: Option<String>,
    /// Artwork files with resolved display URLs
    pub artwork_files: Vec<FileInfo>,
    /// Available storage profiles
    pub storage_profiles: Vec<StorageProfileInfo>,
    /// Currently selected storage profile ID
    pub selected_profile_id: Option<String>,
    /// True while importing
    pub is_importing: bool,
    /// Current preparation step text (e.g., "Encoding tracks...")
    pub preparing_step_text: Option<String>,
    /// Callback when user selects remote cover
    pub on_select_remote_cover: EventHandler<String>,
    /// Callback when user selects local cover
    pub on_select_local_cover: EventHandler<String>,
    /// Callback when storage profile changes
    pub on_storage_profile_change: EventHandler<Option<String>>,
    /// Callback to go back and edit
    pub on_edit: EventHandler<()>,
    /// Callback to confirm and import
    pub on_confirm: EventHandler<()>,
    /// Callback to configure storage
    pub on_configure_storage: EventHandler<()>,
    // Error display
    /// Import error message
    pub import_error: Option<String>,
    /// ID of duplicate album (if import failed due to duplicate)
    pub duplicate_album_id: Option<String>,
    /// Callback to view duplicate album
    pub on_view_duplicate: EventHandler<String>,
}

/// Step 3: Confirm and import (confirmation UI only, no file display)
#[component]
pub fn ConfirmStep(props: ConfirmStepProps) -> Element {
    rsx! {
        div { class: "space-y-6",
            // Confirmation view
            ConfirmationView {
                candidate: props.confirmed_candidate.clone(),
                selected_cover: props.selected_cover.clone(),
                display_cover_url: props.display_cover_url.clone(),
                artwork_files: props.artwork_files.clone(),
                remote_cover_url: props.confirmed_candidate.cover_url.clone(),
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

            // Error display
            ImportErrorDisplayView {
                error_message: props.import_error.clone(),
                duplicate_album_id: props.duplicate_album_id.clone(),
                on_view_duplicate: props.on_view_duplicate,
            }
        }
    }
}

// ============================================================================
// Main Folder Import View
// ============================================================================

/// Props for the folder import workflow view
#[derive(Clone, PartialEq, Props)]
pub struct FolderImportViewProps {
    /// Current import step
    pub step: ImportStep,
    // Folder selection
    /// True if dragging over drop zone
    #[props(default)]
    pub is_dragging: bool,
    /// Callback when user clicks to select folder
    pub on_folder_select_click: EventHandler<()>,
    /// True while scanning a folder for release candidates
    #[props(default)]
    pub is_scanning_candidates: bool,
    // Release sidebar
    /// Detected release candidates
    #[props(default)]
    pub detected_candidates: Vec<DetectedCandidate>,
    /// Currently selected candidate index in sidebar
    #[props(default)]
    pub selected_candidate_index: Option<usize>,
    /// Callback when a candidate is selected in sidebar
    pub on_release_select: EventHandler<usize>,
    // Step 2: Identify props
    /// Mode within Identify step
    pub identify_mode: IdentifyMode,
    /// Path to selected folder
    #[props(default)]
    pub folder_path: String,
    /// Files in the folder
    pub folder_files: CategorizedFileInfo,
    /// Currently viewed text file name
    #[props(default)]
    pub selected_text_file: Option<String>,
    /// Loaded text file content (for selected file)
    #[props(default)]
    pub text_file_content: Option<String>,
    /// Callback when user selects a text file to view
    pub on_text_file_select: EventHandler<String>,
    /// Callback when user closes text file modal
    pub on_text_file_close: EventHandler<()>,
    /// Callback to clear folder
    pub on_clear: EventHandler<()>,
    /// Callback to reveal folder in file browser
    pub on_reveal: EventHandler<()>,
    /// Callback to remove a release from the list
    pub on_remove_release: EventHandler<usize>,
    /// Callback to clear all releases
    pub on_clear_all_releases: EventHandler<()>,
    /// Callback to skip detection
    pub on_skip_detection: EventHandler<()>,
    /// Exact match candidates
    #[props(default)]
    pub exact_match_candidates: Vec<MatchCandidate>,
    /// Selected match index
    #[props(default)]
    pub selected_match_index: Option<usize>,
    /// Callback when user selects exact match
    pub on_exact_match_select: EventHandler<usize>,
    /// Detected metadata
    #[props(default)]
    pub detected_metadata: Option<FolderMetadata>,
    /// Search source
    pub search_source: SearchSource,
    /// Callback when search source changes
    pub on_search_source_change: EventHandler<SearchSource>,
    /// Search tab
    pub search_tab: SearchTab,
    /// Callback when search tab changes
    pub on_search_tab_change: EventHandler<SearchTab>,
    /// Search form fields
    #[props(default)]
    pub search_artist: String,
    pub on_artist_change: EventHandler<String>,
    #[props(default)]
    pub search_album: String,
    pub on_album_change: EventHandler<String>,
    #[props(default)]
    pub search_year: String,
    pub on_year_change: EventHandler<String>,
    #[props(default)]
    pub search_label: String,
    pub on_label_change: EventHandler<String>,
    #[props(default)]
    pub search_catalog_number: String,
    pub on_catalog_number_change: EventHandler<String>,
    #[props(default)]
    pub search_barcode: String,
    pub on_barcode_change: EventHandler<String>,
    /// True while searching
    #[props(default)]
    pub is_searching: bool,
    /// Search error
    #[props(default)]
    pub search_error: Option<String>,
    /// True if user has searched
    #[props(default)]
    pub has_searched: bool,
    /// Manual search results
    #[props(default)]
    pub manual_match_candidates: Vec<MatchCandidate>,
    /// Callback when user selects manual match
    pub on_manual_match_select: EventHandler<usize>,
    /// Callback to search
    pub on_search: EventHandler<()>,
    /// Callback when user confirms manual match
    pub on_manual_confirm: EventHandler<MatchCandidate>,
    /// DiscID lookup error
    #[props(default)]
    pub discid_lookup_error: Option<String>,
    /// True while retrying DiscID lookup
    #[props(default)]
    pub is_retrying_discid_lookup: bool,
    /// Callback to retry DiscID lookup
    pub on_retry_discid_lookup: EventHandler<()>,
    // Step 3: Confirm props
    /// Confirmed match candidate
    #[props(default)]
    pub confirmed_candidate: Option<MatchCandidate>,
    /// Selected cover
    #[props(default)]
    pub selected_cover: Option<SelectedCover>,
    /// Display URL for selected cover
    #[props(default)]
    pub display_cover_url: Option<String>,
    /// Artwork files
    #[props(default)]
    pub artwork_files: Vec<FileInfo>,
    /// Storage profiles
    #[props(default)]
    pub storage_profiles: Vec<StorageProfileInfo>,
    /// Selected storage profile ID
    #[props(default)]
    pub selected_profile_id: Option<String>,
    /// True while importing
    #[props(default)]
    pub is_importing: bool,
    /// Preparation step text
    #[props(default)]
    pub preparing_step_text: Option<String>,
    /// Callback when user selects remote cover
    pub on_select_remote_cover: EventHandler<String>,
    /// Callback when user selects local cover
    pub on_select_local_cover: EventHandler<String>,
    /// Callback when storage profile changes
    pub on_storage_profile_change: EventHandler<Option<String>>,
    /// Callback to edit
    pub on_edit: EventHandler<()>,
    /// Callback to confirm import
    pub on_confirm: EventHandler<()>,
    /// Callback to configure storage
    pub on_configure_storage: EventHandler<()>,
    /// Import error
    #[props(default)]
    pub import_error: Option<String>,
    /// Duplicate album ID
    #[props(default)]
    pub duplicate_album_id: Option<String>,
    /// Callback to view duplicate
    pub on_view_duplicate: EventHandler<String>,
}

/// Folder import workflow view - two-pane layout with sidebar
#[component]
pub fn FolderImportView(props: FolderImportViewProps) -> Element {
    // Two-pane layout: sidebar + main content
    rsx! {
        div { class: "flex flex-grow h-full bg-surface-base",
            // Left sidebar - floating panel (resizable)
            ResizablePanel {
                storage_key: "import-sidebar-width",
                min_size: MIN_SIDEBAR_WIDTH,
                max_size: MAX_SIDEBAR_WIDTH,
                default_size: DEFAULT_SIDEBAR_WIDTH,
                grabber_span_ratio: 0.95,
                direction: ResizeDirection::Horizontal,
                ReleaseSidebarView {
                    candidates: props.detected_candidates.clone(),
                    selected_index: props.selected_candidate_index,
                    on_select: props.on_release_select,
                    on_add_folder: props.on_folder_select_click,
                    on_remove: props.on_remove_release,
                    on_clear_all: props.on_clear_all_releases,
                    is_scanning: props.is_scanning_candidates,
                }
            }

            // Right main area - flex column with scrollable top and fixed bottom
            div { class: "flex-1 flex flex-col min-w-0",
                if props.detected_candidates.is_empty() {
                    if props.is_scanning_candidates {
                        div { class: "flex-1 flex items-center justify-center px-6 py-4",
                            div { class: "w-full max-w-3xl text-center space-y-3",
                                LoaderIcon { class: "w-5 h-5 text-gray-400 animate-spin mx-auto" }
                                p { class: "text-sm text-gray-400", "Scanning folder for releases..." }
                            }
                        }
                    } else {
                        div { class: "flex-1 flex items-center justify-center px-6 py-4",
                            div { class: "w-full max-w-3xl text-center space-y-4",
                                button {
                                    class: "px-4 py-2 text-sm font-medium text-gray-200 bg-white/5 hover:bg-white/10 rounded-md transition-colors inline-flex items-center gap-2",
                                    onclick: move |_| props.on_folder_select_click.call(()),
                                    FolderIcon { class: "w-4 h-4" }
                                    "Select a folder"
                                }
                                p { class: "text-sm text-gray-400",
                                    "We'll scan the folder for possible releases to import"
                                }
                            }
                        }
                    }
                } else {
                    // Top section: Workflow content (scrollable)
                    div { class: "flex-1 overflow-y-auto px-6 py-4",
                        div { class: "mx-auto w-full max-w-5xl",
                            match props.step {
                                ImportStep::Identify => rsx! {
                                    IdentifyStep {
                                        mode: props.identify_mode,
                                        folder_path: props.folder_path.clone(),
                                        folder_files: props.folder_files.clone(),
                                        on_clear: props.on_clear,
                                        on_reveal: props.on_reveal,
                                        on_skip_detection: props.on_skip_detection,
                                        exact_match_candidates: props.exact_match_candidates.clone(),
                                        selected_match_index: props.selected_match_index,
                                        on_exact_match_select: props.on_exact_match_select,
                                        detected_metadata: props.detected_metadata.clone(),
                                        search_source: props.search_source,
                                        on_search_source_change: props.on_search_source_change,
                                        search_tab: props.search_tab,
                                        on_search_tab_change: props.on_search_tab_change,
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
                                        is_searching: props.is_searching,
                                        search_error: props.search_error.clone(),
                                        has_searched: props.has_searched,
                                        manual_match_candidates: props.manual_match_candidates.clone(),
                                        on_manual_match_select: props.on_manual_match_select,
                                        on_search: props.on_search,
                                        on_manual_confirm: props.on_manual_confirm,
                                        discid_lookup_error: props.discid_lookup_error.clone(),
                                        is_retrying_discid_lookup: props.is_retrying_discid_lookup,
                                        on_retry_discid_lookup: props.on_retry_discid_lookup,
                                    }
                                },
                                ImportStep::Confirm => rsx! {
                                    if let Some(ref candidate) = props.confirmed_candidate {
                                        ConfirmStep {
                                            folder_path: props.folder_path.clone(),
                                            folder_files: props.folder_files.clone(),
                                            on_clear: props.on_clear,
                                            on_reveal: props.on_reveal,
                                            confirmed_candidate: candidate.clone(),
                                            selected_cover: props.selected_cover.clone(),
                                            display_cover_url: props.display_cover_url.clone(),
                                            artwork_files: props.artwork_files.clone(),
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
                                            import_error: props.import_error.clone(),
                                            duplicate_album_id: props.duplicate_album_id.clone(),
                                            on_view_duplicate: props.on_view_duplicate,
                                        }
                                    }
                                },
                            }
                        }
                    }
                    // Bottom section: File display dock
                    // Files are guaranteed to be present - state machine requires them by construction
                    FilesDock {
                        files: props.folder_files.clone(),
                        selected_text_file: props.selected_text_file.clone(),
                        text_file_content: props.text_file_content.clone(),
                        on_text_file_select: props.on_text_file_select,
                        on_text_file_close: props.on_text_file_close,
                    }
                }
            }
        }
    }
}

/// Resizable bottom dock showing files from the folder being imported
#[component]
fn FilesDock(
    files: CategorizedFileInfo,
    selected_text_file: Option<String>,
    text_file_content: Option<String>,
    on_text_file_select: EventHandler<String>,
    on_text_file_close: EventHandler<()>,
) -> Element {
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
