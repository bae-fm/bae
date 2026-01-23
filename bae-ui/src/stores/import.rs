//! Import workflow state store
//!
//! State machine types for the import workflow. These are used by both
//! bae-desktop (real import) and bae-mocks (design tool).

use crate::display_types::{
    CategorizedFileInfo, DetectedCandidate, FolderMetadata, IdentifyMode, MatchCandidate,
    SearchSource, SearchTab, SelectedCover,
};
use dioxus::prelude::*;

// ============================================================================
// State Machine Types
// ============================================================================

/// Per-candidate state. Only constructed after file scan + metadata detection complete.
// We keep this unboxed for store lensing and accept size overhead.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Store)]
pub enum CandidateState {
    /// User picking from auto matches or searching manually (ImportStep::Identify)
    Identifying(IdentifyingState),
    /// User confirming selection before import (ImportStep::Confirm)
    Confirming(Box<ConfirmingState>),
}

/// State for the Identify step
#[derive(Clone, Debug, PartialEq, Store)]
pub struct IdentifyingState {
    /// Files from the candidate folder (required)
    pub files: CategorizedFileInfo,
    /// Detected metadata from tags/filenames (required)
    pub metadata: FolderMetadata,
    /// Current mode within Identify step
    pub mode: IdentifyMode,
    /// Cached auto-match results from DiscID lookup
    pub auto_matches: Vec<MatchCandidate>,
    /// Index of selected match in auto_matches (for MultipleExactMatches mode)
    pub selected_match_index: Option<usize>,
    /// Manual search state (persisted even when in MultipleExactMatches)
    pub search_state: ManualSearchState,
    /// Error from DiscID lookup (shown as banner, user can retry or search manually)
    pub discid_lookup_error: Option<String>,
}

/// State for manual search within Identify step
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct ManualSearchState {
    pub search_source: SearchSource,
    pub search_artist: String,
    pub search_album: String,
    pub search_year: String,
    pub search_label: String,
    pub search_catalog_number: String,
    pub search_barcode: String,
    pub search_tab: SearchTab,
    pub has_searched: bool,
    pub is_searching: bool,
    pub search_results: Vec<MatchCandidate>,
    pub selected_result_index: Option<usize>,
    pub error_message: Option<String>,
}

/// State for the Confirm step
#[derive(Clone, Debug, PartialEq, Store)]
pub struct ConfirmingState {
    /// Files from the candidate folder (required)
    pub files: CategorizedFileInfo,
    /// Detected metadata (required)
    pub metadata: FolderMetadata,
    /// The confirmed release candidate (required)
    pub confirmed_candidate: MatchCandidate,
    /// Selected cover art
    pub selected_cover: Option<SelectedCover>,
    /// Selected storage profile ID
    pub selected_profile_id: Option<String>,
    /// Current phase within Confirm step
    pub phase: ConfirmPhase,
    /// Cached auto-match results (for returning to Identify)
    pub auto_matches: Vec<MatchCandidate>,
    /// Manual search state (for returning to Identify)
    pub search_state: ManualSearchState,
}

/// Phase within the Confirm step
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub enum ConfirmPhase {
    /// User can edit cover/profile and click Confirm
    #[default]
    Ready,
    /// Fetching/preparing after clicking Confirm
    Preparing(String),
    /// Import command sent, controls disabled
    Importing,
    /// Error occurred
    Failed(String),
    /// Import finished successfully
    Completed,
}

// ============================================================================
// Event Types
// ============================================================================

/// Events that can be dispatched to the state machine
#[derive(Clone, Debug)]
pub enum CandidateEvent {
    // --- Identify step events ---
    /// User selects a match from the auto/exact match list
    SelectExactMatch(usize),
    /// User switches from MultipleExactMatches to ManualSearch
    SwitchToManualSearch,
    /// User switches from ManualSearch back to MultipleExactMatches (if auto_matches available)
    SwitchToMultipleExactMatches,
    /// User clicks retry on failed DiscID lookup
    RetryDiscIdLookup,
    /// DiscID lookup completed (from async operation)
    DiscIdLookupComplete {
        matches: Vec<MatchCandidate>,
        error: Option<String>,
    },

    // --- Manual search events ---
    /// User updates a search field
    UpdateSearchField { field: SearchField, value: String },
    /// User changes the active search tab
    SetSearchTab(SearchTab),
    /// User changes the search source (MusicBrainz/Discogs)
    SetSearchSource(SearchSource),
    /// User initiates a search
    StartSearch,
    /// Search completed (from async operation)
    SearchComplete {
        results: Vec<MatchCandidate>,
        error: Option<String>,
    },
    /// User selects a result from manual search
    SelectSearchResult(usize),
    /// User confirms the selected search result
    ConfirmSearchResult,

    // --- Confirm step events ---
    /// User clicks "Edit" to go back to Identify
    GoBackToIdentify,
    /// User selects cover art
    SelectCover(Option<SelectedCover>),
    /// User selects storage profile
    SelectStorageProfile(Option<String>),
    /// User clicks "Import" button
    StartImport,
    /// Import is preparing (from async operation)
    ImportPreparing(String),
    /// Import command sent successfully
    ImportStarted,
    /// Import failed (from async operation)
    ImportFailed(String),
    /// Import completed successfully
    ImportComplete,
}

/// Which search field is being updated
#[derive(Clone, Debug, PartialEq)]
pub enum SearchField {
    Artist,
    Album,
    Year,
    Label,
    CatalogNumber,
    Barcode,
}

// ============================================================================
// State Machine Implementation
// ============================================================================

impl CandidateState {
    /// Get the files from any state
    pub fn files(&self) -> &CategorizedFileInfo {
        match self {
            CandidateState::Identifying(s) => &s.files,
            CandidateState::Confirming(s) => &s.files,
        }
    }

    /// Get the metadata from any state
    pub fn metadata(&self) -> &FolderMetadata {
        match self {
            CandidateState::Identifying(s) => &s.metadata,
            CandidateState::Confirming(s) => &s.metadata,
        }
    }

    /// Check if this candidate is currently importing
    pub fn is_importing(&self) -> bool {
        matches!(
            self,
            CandidateState::Confirming(s) if matches!(s.phase, ConfirmPhase::Importing)
        )
    }

    /// Check if import is in progress (preparing or importing)
    pub fn is_import_in_progress(&self) -> bool {
        matches!(
            self,
            CandidateState::Confirming(s) if matches!(s.phase, ConfirmPhase::Importing | ConfirmPhase::Preparing(_))
        )
    }

    /// Check if this candidate has been imported (completed)
    pub fn is_imported(&self) -> bool {
        matches!(
            self,
            CandidateState::Confirming(s) if matches!(s.phase, ConfirmPhase::Completed)
        )
    }

    /// Apply an event and return the new state.
    /// This is the core state machine transition function.
    pub fn transition(self, event: CandidateEvent) -> CandidateState {
        match self {
            CandidateState::Identifying(s) => s.on_event(event),
            CandidateState::Confirming(s) => s.on_event(event),
        }
    }
}

impl IdentifyingState {
    fn on_event(self, event: CandidateEvent) -> CandidateState {
        match event {
            CandidateEvent::SelectExactMatch(idx) => {
                if let Some(candidate) = self.auto_matches.get(idx).cloned() {
                    CandidateState::Confirming(Box::new(ConfirmingState {
                        files: self.files,
                        metadata: self.metadata,
                        confirmed_candidate: candidate,
                        selected_cover: None,
                        selected_profile_id: None,
                        phase: ConfirmPhase::Ready,
                        auto_matches: self.auto_matches,
                        search_state: self.search_state,
                    }))
                } else {
                    CandidateState::Identifying(self)
                }
            }
            CandidateEvent::SwitchToManualSearch => {
                let mut state = self;
                state.mode = IdentifyMode::ManualSearch;
                CandidateState::Identifying(state)
            }
            CandidateEvent::SwitchToMultipleExactMatches => {
                let mut state = self;
                if !state.auto_matches.is_empty() {
                    state.mode = IdentifyMode::MultipleExactMatches;
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::RetryDiscIdLookup => {
                let mut state = self;
                state.mode = IdentifyMode::DiscIdLookup;
                state.discid_lookup_error = None;
                CandidateState::Identifying(state)
            }
            CandidateEvent::DiscIdLookupComplete { matches, error } => {
                let mut state = self;
                state.auto_matches = matches.clone();
                state.discid_lookup_error = error;
                state.mode = if matches.is_empty() {
                    IdentifyMode::ManualSearch
                } else if matches.len() == 1 {
                    return CandidateState::Confirming(Box::new(ConfirmingState {
                        files: state.files,
                        metadata: state.metadata,
                        confirmed_candidate: matches.into_iter().next().unwrap(),
                        selected_cover: None,
                        selected_profile_id: None,
                        phase: ConfirmPhase::Ready,
                        auto_matches: vec![],
                        search_state: state.search_state,
                    }));
                } else {
                    IdentifyMode::MultipleExactMatches
                };
                CandidateState::Identifying(state)
            }
            CandidateEvent::UpdateSearchField { field, value } => {
                let mut state = self;
                match field {
                    SearchField::Artist => state.search_state.search_artist = value,
                    SearchField::Album => state.search_state.search_album = value,
                    SearchField::Year => state.search_state.search_year = value,
                    SearchField::Label => state.search_state.search_label = value,
                    SearchField::CatalogNumber => state.search_state.search_catalog_number = value,
                    SearchField::Barcode => state.search_state.search_barcode = value,
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::SetSearchTab(tab) => {
                let mut state = self;
                state.search_state.search_tab = tab;
                CandidateState::Identifying(state)
            }
            CandidateEvent::SetSearchSource(source) => {
                let mut state = self;
                state.search_state.search_source = source;
                CandidateState::Identifying(state)
            }
            CandidateEvent::StartSearch => {
                let mut state = self;
                state.search_state.is_searching = true;
                state.search_state.error_message = None;
                CandidateState::Identifying(state)
            }
            CandidateEvent::SearchComplete { results, error } => {
                let mut state = self;
                state.search_state.is_searching = false;
                state.search_state.has_searched = true;
                state.search_state.search_results = results;
                state.search_state.error_message = error;
                state.search_state.selected_result_index = None;
                CandidateState::Identifying(state)
            }
            CandidateEvent::SelectSearchResult(idx) => {
                let mut state = self;
                if idx < state.search_state.search_results.len() {
                    state.search_state.selected_result_index = Some(idx);
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::ConfirmSearchResult => {
                let state = self;
                if let Some(idx) = state.search_state.selected_result_index {
                    if let Some(candidate) = state.search_state.search_results.get(idx).cloned() {
                        return CandidateState::Confirming(Box::new(ConfirmingState {
                            files: state.files,
                            metadata: state.metadata,
                            confirmed_candidate: candidate,
                            selected_cover: None,
                            selected_profile_id: None,
                            phase: ConfirmPhase::Ready,
                            auto_matches: state.auto_matches,
                            search_state: state.search_state,
                        }));
                    }
                }
                CandidateState::Identifying(state)
            }
            CandidateEvent::GoBackToIdentify
            | CandidateEvent::SelectCover(_)
            | CandidateEvent::SelectStorageProfile(_)
            | CandidateEvent::StartImport
            | CandidateEvent::ImportPreparing(_)
            | CandidateEvent::ImportStarted
            | CandidateEvent::ImportFailed(_)
            | CandidateEvent::ImportComplete => CandidateState::Identifying(self),
        }
    }
}

impl ConfirmingState {
    fn on_event(self, event: CandidateEvent) -> CandidateState {
        match event {
            CandidateEvent::GoBackToIdentify => {
                let mode = if self.auto_matches.is_empty() {
                    IdentifyMode::ManualSearch
                } else {
                    IdentifyMode::MultipleExactMatches
                };
                CandidateState::Identifying(IdentifyingState {
                    files: self.files,
                    metadata: self.metadata,
                    mode,
                    auto_matches: self.auto_matches,
                    selected_match_index: None,
                    search_state: self.search_state,
                    discid_lookup_error: None,
                })
            }
            CandidateEvent::SelectCover(cover) => {
                let mut state = self;
                state.selected_cover = cover;
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::SelectStorageProfile(profile) => {
                let mut state = self;
                state.selected_profile_id = profile;
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::StartImport => {
                let mut state = self;
                state.phase = ConfirmPhase::Preparing("Starting...".to_string());
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::ImportPreparing(step) => {
                let mut state = self;
                state.phase = ConfirmPhase::Preparing(step);
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::ImportStarted => {
                let mut state = self;
                state.phase = ConfirmPhase::Importing;
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::ImportFailed(error) => {
                let mut state = self;
                state.phase = ConfirmPhase::Failed(error);
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::ImportComplete => {
                let mut state = self;
                state.phase = ConfirmPhase::Completed;
                CandidateState::Confirming(Box::new(state))
            }
            CandidateEvent::SelectExactMatch(_)
            | CandidateEvent::SwitchToManualSearch
            | CandidateEvent::SwitchToMultipleExactMatches
            | CandidateEvent::RetryDiscIdLookup
            | CandidateEvent::DiscIdLookupComplete { .. }
            | CandidateEvent::UpdateSearchField { .. }
            | CandidateEvent::SetSearchTab(_)
            | CandidateEvent::SetSearchSource(_)
            | CandidateEvent::StartSearch
            | CandidateEvent::SearchComplete { .. }
            | CandidateEvent::SelectSearchResult(_)
            | CandidateEvent::ConfirmSearchResult => CandidateState::Confirming(Box::new(self)),
        }
    }
}

// ============================================================================
// Global Import State
// ============================================================================

/// Global import workflow state (not per-candidate)
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct ImportState {
    /// List of all detected candidates from the scan
    pub detected_candidates: Vec<DetectedCandidate>,
    /// Key of the currently selected candidate (release path)
    pub current_candidate_key: Option<String>,
    /// Per-candidate state machines
    pub candidate_states: std::collections::HashMap<String, CandidateState>,
    /// Loading state for candidates that haven't completed detection yet
    pub loading_candidates: std::collections::HashMap<String, bool>,
    /// Whether DiscID lookup is in progress
    pub is_looking_up: bool,
    /// ID of duplicate album if found during import
    pub duplicate_album_id: Option<String>,
    /// Error message from import process
    pub import_error_message: Option<String>,
    /// Files in current folder (for UI reactivity)
    pub folder_files: CategorizedFileInfo,
    /// True while scanning a folder for release candidates
    pub is_scanning_candidates: bool,
    /// Track which candidates have already attempted DiscID lookup
    pub discid_lookup_attempted: std::collections::HashSet<String>,
    /// Which releases are selected for batch import
    pub selected_release_indices: Vec<usize>,
    /// Which release in the batch we're currently on
    pub current_release_index: usize,
    /// Which import source type (Folder, Torrent, CD)
    pub selected_import_source: crate::ImportSource,
    /// CD TOC info: (disc_id, first_track, last_track)
    pub cd_toc_info: Option<(String, u8, u8)>,
}

impl ImportState {
    /// Reset the import state to initial values
    pub fn reset(&mut self) {
        self.detected_candidates = Vec::new();
        self.current_candidate_key = None;
        self.candidate_states.clear();
        self.loading_candidates.clear();
        self.is_looking_up = false;
        self.duplicate_album_id = None;
        self.import_error_message = None;
        self.folder_files = CategorizedFileInfo::default();
        self.is_scanning_candidates = false;
        self.discid_lookup_attempted.clear();
        self.selected_release_indices = Vec::new();
        self.current_release_index = 0;
    }

    /// Get the current candidate's state (if any)
    pub fn current_candidate_state(&self) -> Option<&CandidateState> {
        let key = self.current_candidate_key.as_ref()?;
        self.candidate_states.get(key)
    }

    /// Switch to a different candidate by key
    pub fn switch_candidate(&mut self, new_key: Option<String>) {
        self.current_candidate_key = new_key.clone();
        if let Some(key) = new_key {
            if let Some(state) = self.candidate_states.get(&key) {
                self.folder_files = state.files().clone();
            }
        } else {
            self.folder_files = CategorizedFileInfo::default();
        }
    }

    /// Dispatch an event to the current candidate's state machine
    pub fn dispatch(&mut self, event: CandidateEvent) {
        let Some(key) = self.current_candidate_key.clone() else {
            return;
        };
        self.dispatch_to_candidate(&key, event);
    }

    /// Dispatch an event to a specific candidate by key
    pub fn dispatch_to_candidate(&mut self, key: &str, event: CandidateEvent) {
        if let Some(current_state) = self.candidate_states.remove(key) {
            let new_state = current_state.transition(event);
            self.candidate_states.insert(key.to_string(), new_state);
        }
    }

    /// Initialize state machine for a candidate after detection completes
    pub fn init_state_machine(
        &mut self,
        key: &str,
        files: CategorizedFileInfo,
        metadata: FolderMetadata,
    ) {
        // Sync folder_files only when this candidate is selected
        if self.current_candidate_key.as_deref() == Some(key) {
            self.folder_files = files.clone();
        }

        let initial_state = CandidateState::Identifying(IdentifyingState {
            files,
            metadata,
            mode: IdentifyMode::Created,
            auto_matches: vec![],
            selected_match_index: None,
            search_state: ManualSearchState::default(),
            discid_lookup_error: None,
        });
        self.candidate_states.insert(key.to_string(), initial_state);
        self.loading_candidates.remove(key);
    }

    /// Find the next release index that is not importing or imported
    pub fn find_next_available_release(&self) -> Option<usize> {
        for batch_idx in (self.current_release_index + 1)..self.selected_release_indices.len() {
            if let Some(&release_idx) = self.selected_release_indices.get(batch_idx) {
                if let Some(candidate) = self.detected_candidates.get(release_idx) {
                    let key = candidate.path.clone();
                    let skip = self
                        .candidate_states
                        .get(&key)
                        .map(|s| s.is_importing() || s.is_imported())
                        .unwrap_or(false);
                    if !skip {
                        return Some(batch_idx);
                    }
                }
            }
        }
        None
    }

    /// Check if there are more releases to import in the current batch
    pub fn has_more_releases(&self) -> bool {
        self.find_next_available_release().is_some()
    }

    /// Advance to the next release in the batch
    pub fn advance_to_next_release(&mut self) {
        if let Some(next_idx) = self.find_next_available_release() {
            self.current_release_index = next_idx;
        }
    }

    /// Get import step from current candidate state
    pub fn get_import_step(&self) -> crate::display_types::ImportStep {
        self.current_candidate_state()
            .map(|s| match s {
                CandidateState::Identifying(_) => crate::display_types::ImportStep::Identify,
                CandidateState::Confirming(_) => crate::display_types::ImportStep::Confirm,
            })
            .unwrap_or(crate::display_types::ImportStep::Identify)
    }

    /// Get identify mode from current candidate state
    pub fn get_identify_mode(&self) -> IdentifyMode {
        self.current_candidate_state()
            .and_then(|s| match s {
                CandidateState::Identifying(is) => Some(is.mode),
                _ => None,
            })
            .unwrap_or(IdentifyMode::Created)
    }

    /// Get metadata from current candidate state
    pub fn get_metadata(&self) -> Option<FolderMetadata> {
        self.current_candidate_state().map(|s| s.metadata().clone())
    }

    /// Get exact match candidates from current candidate state
    pub fn get_exact_match_candidates(&self) -> Vec<MatchCandidate> {
        self.current_candidate_state()
            .map(|s| match s {
                CandidateState::Identifying(is) => is.auto_matches.clone(),
                CandidateState::Confirming(cs) => cs.auto_matches.clone(),
            })
            .unwrap_or_default()
    }

    /// Get confirmed candidate from current candidate state
    pub fn get_confirmed_candidate(&self) -> Option<MatchCandidate> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Confirming(cs) => Some(cs.confirmed_candidate.clone()),
            _ => None,
        })
    }

    /// Get discid lookup error from current candidate state
    pub fn get_discid_lookup_error(&self) -> Option<String> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Identifying(is) => is.discid_lookup_error.clone(),
            _ => None,
        })
    }

    /// Get selected match index from current candidate state
    pub fn get_selected_match_index(&self) -> Option<usize> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Identifying(is) => is.selected_match_index,
            _ => None,
        })
    }

    /// Get manual search state from current candidate
    pub fn get_search_state(&self) -> Option<ManualSearchState> {
        self.current_candidate_state().map(|s| match s {
            CandidateState::Identifying(is) => is.search_state.clone(),
            CandidateState::Confirming(cs) => cs.search_state.clone(),
        })
    }

    /// Get selected cover from current candidate state
    pub fn get_selected_cover(&self) -> Option<SelectedCover> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Confirming(cs) => cs.selected_cover.clone(),
            _ => None,
        })
    }

    /// Get selected storage profile ID from current candidate state
    pub fn get_storage_profile_id(&self) -> Option<String> {
        self.current_candidate_state().and_then(|s| match s {
            CandidateState::Confirming(cs) => cs.selected_profile_id.clone(),
            _ => None,
        })
    }

    /// Remove a detected release by index
    pub fn remove_detected_release(&mut self, index: usize) {
        if index < self.detected_candidates.len() {
            let release_path = self.detected_candidates[index].path.clone();
            self.detected_candidates.remove(index);
            self.candidate_states.remove(&release_path);
            self.loading_candidates.remove(&release_path);
            self.discid_lookup_attempted.remove(&release_path);

            if self.current_candidate_key.as_deref() == Some(&release_path) {
                if let Some(first) = self.detected_candidates.first() {
                    let new_key = first.path.clone();
                    self.switch_candidate(Some(new_key));
                } else {
                    self.switch_candidate(None);
                }
            }
        }
    }
}
