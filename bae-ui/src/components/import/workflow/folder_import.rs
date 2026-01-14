//! Folder import workflow view
//!
//! A 3-step wizard for importing music from a folder:
//!
//! ## Step 1: Select Source
//! User picks a folder. If the folder contains multiple releases (e.g., CD1/CD2),
//! user selects which to import.
//!
//! ## Step 2: Identify
//! System identifies the music via DiscID or metadata parsing. If ambiguous,
//! user picks from candidates. If no match, user searches manually.
//!
//! ## Step 3: Confirm
//! User reviews the match, selects cover art and storage profile, then imports.

use super::{
    ConfirmationView, DetectingMetadataView, DiscIdLookupErrorView, ExactLookupView,
    ImportErrorDisplayView, ManualSearchPanelView, ReleaseSelectorView, SelectedSourceView,
    SmartFileDisplayView,
};
use crate::display_types::{
    ArtworkFile, CategorizedFileInfo, DetectedRelease, FolderMetadata, IdentifyMode,
    MatchCandidate, SearchSource, SearchTab, SelectSourceMode, SelectedCover, StorageProfileInfo,
    WizardStep,
};
use crate::FolderSelectorView;
use dioxus::prelude::*;

// ============================================================================
// Step 1: Select Source
// ============================================================================

/// Props for the SelectSource step
#[derive(Clone, PartialEq, Props)]
pub struct SelectSourceStepProps {
    /// Sub-mode: FolderSelection or ReleaseSelection
    pub mode: SelectSourceMode,
    /// True if user is dragging a folder over the drop zone
    pub is_dragging: bool,
    /// Callback when user clicks to select a folder
    pub on_folder_select_click: EventHandler<()>,
    /// Detected releases (for ReleaseSelection mode)
    pub detected_releases: Vec<DetectedRelease>,
    /// Currently selected release indices
    pub selected_release_indices: Vec<usize>,
    /// Callback when selection changes
    pub on_release_selection_change: EventHandler<Vec<usize>>,
    /// Callback when user confirms release selection
    pub on_releases_import: EventHandler<Vec<usize>>,
}

/// Step 1: Select the source folder (and releases if multi-release)
#[component]
pub fn SelectSourceStep(props: SelectSourceStepProps) -> Element {
    rsx! {
        match props.mode {
            SelectSourceMode::FolderSelection => rsx! {
                FolderSelectorView {
                    is_dragging: props.is_dragging,
                    on_select_click: props.on_folder_select_click,
                }
            },
            SelectSourceMode::ReleaseSelection => rsx! {
                ReleaseSelectorView {
                    releases: props.detected_releases.clone(),
                    selected_indices: props.selected_release_indices.clone(),
                    on_selection_change: props.on_release_selection_change,
                    on_import: props.on_releases_import,
                }
            },
        }
    }
}

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
    /// Image data for gallery (filename, display_url)
    pub image_data: Vec<(String, String)>,
    /// Text file contents keyed by filename
    pub text_file_contents: std::collections::HashMap<String, String>,
    /// Callback to clear/change folder
    pub on_clear: EventHandler<()>,
    // Detecting mode
    /// Callback to skip detection
    pub on_skip_detection: EventHandler<()>,
    // ExactLookup mode
    /// True while loading exact match candidates
    pub is_loading_exact_matches: bool,
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

/// Step 2: Identify the music
#[component]
pub fn IdentifyStep(props: IdentifyStepProps) -> Element {
    rsx! {
        div { class: "space-y-6",
            // Selected source display (always shown)
            SelectedSourceView {
                title: "Selected Folder".to_string(),
                path: props.folder_path.clone(),
                on_clear: props.on_clear,
                if !props.folder_files.is_empty() {
                    div { class: "mt-4",
                        SmartFileDisplayView {
                            files: props.folder_files.clone(),
                            image_data: props.image_data.clone(),
                            text_file_contents: props.text_file_contents.clone(),
                        }
                    }
                }
            }

            // Mode-specific content
            match props.mode {
                IdentifyMode::Detecting => rsx! {
                    DetectingMetadataView {
                        message: "Looking up release...".to_string(),
                        on_skip: props.on_skip_detection,
                    }
                },
                IdentifyMode::ExactLookup => rsx! {
                    ExactLookupView {
                        is_loading: props.is_loading_exact_matches,
                        exact_match_candidates: props.exact_match_candidates.clone(),
                        selected_match_index: props.selected_match_index,
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
    /// Image data for gallery (filename, display_url)
    pub image_data: Vec<(String, String)>,
    /// Text file contents keyed by filename
    pub text_file_contents: std::collections::HashMap<String, String>,
    /// Callback to clear/change folder
    pub on_clear: EventHandler<()>,
    // Confirmation
    /// The confirmed match candidate
    pub confirmed_candidate: MatchCandidate,
    /// Currently selected cover
    pub selected_cover: Option<SelectedCover>,
    /// URL to display the selected cover
    pub display_cover_url: Option<String>,
    /// Artwork files with resolved display URLs
    pub artwork_files: Vec<ArtworkFile>,
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

/// Step 3: Confirm and import
#[component]
pub fn ConfirmStep(props: ConfirmStepProps) -> Element {
    rsx! {
        div { class: "space-y-6",
            // Selected source display
            SelectedSourceView {
                title: "Selected Folder".to_string(),
                path: props.folder_path.clone(),
                on_clear: props.on_clear,
                if !props.folder_files.is_empty() {
                    div { class: "mt-4",
                        SmartFileDisplayView {
                            files: props.folder_files.clone(),
                            image_data: props.image_data.clone(),
                            text_file_contents: props.text_file_contents.clone(),
                        }
                    }
                }
            }

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
    /// Current wizard step
    pub step: WizardStep,
    // Step 1: SelectSource props
    /// Sub-mode within SelectSource step
    #[props(default)]
    pub select_source_mode: SelectSourceMode,
    /// True if dragging over drop zone
    #[props(default)]
    pub is_dragging: bool,
    /// Callback when user clicks to select folder
    pub on_folder_select_click: EventHandler<()>,
    /// Detected releases for multi-release folders
    #[props(default)]
    pub detected_releases: Vec<DetectedRelease>,
    /// Selected release indices
    #[props(default)]
    pub selected_release_indices: Vec<usize>,
    /// Callback when release selection changes
    pub on_release_selection_change: EventHandler<Vec<usize>>,
    /// Callback when user confirms release selection
    pub on_releases_import: EventHandler<Vec<usize>>,
    // Step 2: Identify props
    /// Mode within Identify step
    #[props(default)]
    pub identify_mode: IdentifyMode,
    /// Path to selected folder
    #[props(default)]
    pub folder_path: String,
    /// Files in the folder
    #[props(default)]
    pub folder_files: CategorizedFileInfo,
    /// Image data for gallery
    #[props(default)]
    pub image_data: Vec<(String, String)>,
    /// Text file contents
    #[props(default)]
    pub text_file_contents: std::collections::HashMap<String, String>,
    /// Callback to clear folder
    pub on_clear: EventHandler<()>,
    /// Callback to skip detection
    pub on_skip_detection: EventHandler<()>,
    /// True while loading exact matches
    #[props(default)]
    pub is_loading_exact_matches: bool,
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
    #[props(default)]
    pub search_source: SearchSource,
    /// Callback when search source changes
    pub on_search_source_change: EventHandler<SearchSource>,
    /// Search tab
    #[props(default)]
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
    pub artwork_files: Vec<ArtworkFile>,
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

/// Folder import workflow view - routes between wizard steps
#[component]
pub fn FolderImportView(props: FolderImportViewProps) -> Element {
    rsx! {
        div { class: "space-y-6",
            match props.step {
                WizardStep::SelectSource => rsx! {
                    SelectSourceStep {
                        mode: props.select_source_mode,
                        is_dragging: props.is_dragging,
                        on_folder_select_click: props.on_folder_select_click,
                        detected_releases: props.detected_releases.clone(),
                        selected_release_indices: props.selected_release_indices.clone(),
                        on_release_selection_change: props.on_release_selection_change,
                        on_releases_import: props.on_releases_import,
                    }
                },
                WizardStep::Identify => rsx! {
                    IdentifyStep {
                        mode: props.identify_mode,
                        folder_path: props.folder_path.clone(),
                        folder_files: props.folder_files.clone(),
                        image_data: props.image_data.clone(),
                        text_file_contents: props.text_file_contents.clone(),
                        on_clear: props.on_clear,
                        on_skip_detection: props.on_skip_detection,
                        is_loading_exact_matches: props.is_loading_exact_matches,
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
                WizardStep::Confirm => rsx! {
                    if let Some(ref candidate) = props.confirmed_candidate {
                        ConfirmStep {
                            folder_path: props.folder_path.clone(),
                            folder_files: props.folder_files.clone(),
                            image_data: props.image_data.clone(),
                            text_file_contents: props.text_file_contents.clone(),
                            on_clear: props.on_clear,
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
}
