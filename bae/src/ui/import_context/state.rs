use crate::discogs::client::DiscogsSearchResult;
use crate::discogs::DiscogsClient;
use crate::import::{
    DetectedRelease, FolderMetadata, ImportServiceHandle, MatchCandidate, TorrentImportMetadata,
    TorrentSource,
};
use crate::library::SharedLibraryManager;
use crate::musicbrainz::MbRelease;
use crate::torrent::ffi::TorrentInfo;
use crate::torrent::TorrentManagerHandle;
use crate::ui::components::dialog_context::DialogContext;
use crate::ui::components::import::{
    CategorizedFileInfo, ImportSource, SearchSource, TorrentInputMode,
};
use dioxus::prelude::*;
use dioxus::router::Navigator;
use std::path::PathBuf;
use std::rc::Rc;

use super::types::{ImportPhase, ImportStep};
use super::{detection, import, navigation};

/// Selected cover for import - stores the actual filename, not just an index
#[derive(Debug, Clone, PartialEq)]
pub enum SelectedCover {
    /// Remote cover (MusicBrainz/Discogs) - stores the expected download filename
    Remote {
        url: String,
        /// The filename the cover will be downloaded to (e.g. ".bae/cover-mb.jpg")
        expected_filename: String,
    },
    /// Local artwork file from the album folder
    Local {
        /// Relative path from album folder (e.g. "scans/front.jpg")
        filename: String,
    },
}

/// Compute the expected filename for a remote cover download.
/// Matches the logic in import/cover_art.rs download_cover_art_to_bae_folder.
pub fn compute_expected_cover_filename(url: &str, source: &str) -> String {
    // Extract extension from URL (same logic as cover_art.rs)
    let extension = url
        .rsplit('/')
        .next()
        .and_then(|filename| {
            let ext = filename.rsplit('.').next()?;
            let ext_lower = ext.to_lowercase();
            if ["jpg", "jpeg", "png", "gif", "webp"].contains(&ext_lower.as_str()) {
                Some(ext_lower)
            } else {
                None
            }
        })
        .unwrap_or_else(|| "jpg".to_string());

    let base_name = match source.to_lowercase().as_str() {
        "musicbrainz" | "mb" => "cover-mb",
        "discogs" => "cover-discogs",
        _ => "cover",
    };

    format!(".bae/{}.{}", base_name, extension)
}

/// Which search tab is active
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SearchTab {
    #[default]
    General,
    CatalogNumber,
    Barcode,
}

pub struct ImportContext {
    // Structured search fields for manual search
    pub(crate) search_artist: Signal<String>,
    pub(crate) search_album: Signal<String>,
    pub(crate) search_year: Signal<String>,
    pub(crate) search_label: Signal<String>,
    pub(crate) search_catalog_number: Signal<String>,
    pub(crate) search_barcode: Signal<String>,
    pub(crate) search_tab: Signal<SearchTab>,
    pub(crate) search_results: Signal<Vec<DiscogsSearchResult>>,
    pub(crate) has_searched: Signal<bool>,
    pub(crate) is_searching_masters: Signal<bool>,
    pub(crate) is_loading_versions: Signal<bool>,
    pub(crate) error_message: Signal<Option<String>>,
    pub(crate) navigation_stack: Signal<Vec<ImportStep>>,
    // MusicBrainz search state
    pub(crate) mb_search_results: Signal<Vec<MbRelease>>,
    pub(crate) is_searching_mb: Signal<bool>,
    pub(crate) mb_error_message: Signal<Option<String>>,
    // Folder detection import state (persists across navigation)
    pub(crate) folder_path: Signal<String>,
    pub(crate) detected_releases: Signal<Vec<DetectedRelease>>,
    pub(crate) selected_release_indices: Signal<Vec<usize>>,
    pub(crate) current_release_index: Signal<usize>,
    pub(crate) detected_metadata: Signal<Option<FolderMetadata>>,
    pub(crate) import_phase: Signal<ImportPhase>,
    pub(crate) exact_match_candidates: Signal<Vec<MatchCandidate>>,
    pub(crate) selected_match_index: Signal<Option<usize>>,
    pub(crate) confirmed_candidate: Signal<Option<MatchCandidate>>,
    pub(crate) is_detecting: Signal<bool>,
    pub(crate) is_looking_up: Signal<bool>,
    pub(crate) is_importing: Signal<bool>,
    pub(crate) import_error_message: Signal<Option<String>>,
    /// Error from DiscID lookup (shown with retry button)
    pub(crate) discid_lookup_error: Signal<Option<String>>,
    pub(crate) duplicate_album_id: Signal<Option<String>>,
    pub(crate) folder_files: Signal<CategorizedFileInfo>,
    /// Selected cover for import - stores actual filename
    pub(crate) selected_cover: Signal<Option<SelectedCover>>,
    // Torrent-specific state
    pub(crate) torrent_source: Signal<Option<TorrentSource>>,
    pub(crate) seed_after_download: Signal<bool>,
    pub(crate) torrent_metadata: Signal<Option<TorrentImportMetadata>>,
    pub(crate) torrent_info_hash: Signal<Option<String>>,
    pub(crate) torrent_info: Signal<Option<TorrentInfo>>,
    pub(crate) torrent_input_mode: Signal<TorrentInputMode>,
    pub(crate) magnet_link: Signal<String>,
    // CD-specific state
    pub(crate) cd_toc_info: Signal<Option<(String, u8, u8)>>, // (disc_id, first_track, last_track)
    // Storage profile selection
    pub(crate) storage_profile_id: Signal<Option<String>>,
    // UI state (persists across navigation)
    pub(crate) selected_import_source: Signal<ImportSource>,
    pub(crate) search_source: Signal<SearchSource>,
    pub(crate) manual_match_candidates: Signal<Vec<MatchCandidate>>,
    pub(crate) dialog: DialogContext,
    pub(crate) discogs_client: DiscogsClient,
    /// Handle to torrent manager service for all torrent operations
    pub(crate) torrent_manager: TorrentManagerHandle,
    /// Handle to library manager for duplicate checking and import operations
    pub(crate) library_manager: SharedLibraryManager,
    /// Handle to import service for submitting import requests
    pub(crate) import_service: ImportServiceHandle,
}

impl ImportContext {
    pub fn new(
        config: &crate::config::Config,
        torrent_manager: TorrentManagerHandle,
        library_manager: SharedLibraryManager,
        import_service: ImportServiceHandle,
        dialog: DialogContext,
    ) -> Self {
        Self {
            search_artist: Signal::new(String::new()),
            search_album: Signal::new(String::new()),
            search_year: Signal::new(String::new()),
            search_label: Signal::new(String::new()),
            search_catalog_number: Signal::new(String::new()),
            search_barcode: Signal::new(String::new()),
            search_tab: Signal::new(SearchTab::default()),
            search_results: Signal::new(Vec::new()),
            has_searched: Signal::new(false),
            is_searching_masters: Signal::new(false),
            is_loading_versions: Signal::new(false),
            error_message: Signal::new(None),
            navigation_stack: Signal::new(vec![ImportStep::FolderIdentification]),
            mb_search_results: Signal::new(Vec::new()),
            is_searching_mb: Signal::new(false),
            mb_error_message: Signal::new(None),
            // Folder detection import state
            folder_path: Signal::new(String::new()),
            detected_releases: Signal::new(Vec::new()),
            selected_release_indices: Signal::new(Vec::new()),
            current_release_index: Signal::new(0),
            detected_metadata: Signal::new(None),
            import_phase: Signal::new(ImportPhase::FolderSelection),
            exact_match_candidates: Signal::new(Vec::new()),
            selected_match_index: Signal::new(None),
            confirmed_candidate: Signal::new(None),
            is_detecting: Signal::new(false),
            is_looking_up: Signal::new(false),
            is_importing: Signal::new(false),
            import_error_message: Signal::new(None),
            discid_lookup_error: Signal::new(None),
            duplicate_album_id: Signal::new(None),
            folder_files: Signal::new(CategorizedFileInfo::default()),
            selected_cover: Signal::new(None),
            torrent_source: Signal::new(None),
            seed_after_download: Signal::new(true),
            torrent_metadata: Signal::new(None),
            torrent_info_hash: Signal::new(None),
            torrent_info: Signal::new(None),
            torrent_input_mode: Signal::new(TorrentInputMode::File),
            magnet_link: Signal::new(String::new()),
            cd_toc_info: Signal::new(None),
            storage_profile_id: Signal::new(None),
            selected_import_source: Signal::new(ImportSource::Folder),
            search_source: Signal::new(SearchSource::MusicBrainz),
            manual_match_candidates: Signal::new(Vec::new()),
            dialog,
            discogs_client: DiscogsClient::new(config.discogs_api_key.clone()),
            torrent_manager,
            library_manager,
            import_service,
        }
    }

    // Getters - return Signal (which can be used as ReadSignal)
    pub fn search_artist(&self) -> Signal<String> {
        self.search_artist
    }

    pub fn search_album(&self) -> Signal<String> {
        self.search_album
    }

    pub fn search_year(&self) -> Signal<String> {
        self.search_year
    }

    pub fn search_label(&self) -> Signal<String> {
        self.search_label
    }

    pub fn search_catalog_number(&self) -> Signal<String> {
        self.search_catalog_number
    }

    pub fn search_barcode(&self) -> Signal<String> {
        self.search_barcode
    }

    pub fn search_tab(&self) -> Signal<SearchTab> {
        self.search_tab
    }

    pub fn has_searched(&self) -> Signal<bool> {
        self.has_searched
    }

    pub fn folder_path(&self) -> Signal<String> {
        self.folder_path
    }

    pub fn detected_releases(&self) -> Signal<Vec<DetectedRelease>> {
        self.detected_releases
    }

    pub fn selected_release_indices(&self) -> Signal<Vec<usize>> {
        self.selected_release_indices
    }

    pub fn current_release_index(&self) -> Signal<usize> {
        self.current_release_index
    }

    pub fn detected_metadata(&self) -> Signal<Option<FolderMetadata>> {
        self.detected_metadata
    }

    pub fn import_phase(&self) -> Signal<ImportPhase> {
        self.import_phase
    }

    pub fn exact_match_candidates(&self) -> Signal<Vec<MatchCandidate>> {
        self.exact_match_candidates
    }

    pub fn selected_match_index(&self) -> Signal<Option<usize>> {
        self.selected_match_index
    }

    pub fn confirmed_candidate(&self) -> Signal<Option<MatchCandidate>> {
        self.confirmed_candidate
    }

    pub fn is_detecting(&self) -> Signal<bool> {
        self.is_detecting
    }

    pub fn is_looking_up(&self) -> Signal<bool> {
        self.is_looking_up
    }

    pub fn is_importing(&self) -> Signal<bool> {
        self.is_importing
    }

    pub fn error_message(&self) -> Signal<Option<String>> {
        self.error_message
    }

    pub fn import_error_message(&self) -> Signal<Option<String>> {
        self.import_error_message
    }

    pub fn duplicate_album_id(&self) -> Signal<Option<String>> {
        self.duplicate_album_id
    }

    pub fn folder_files(&self) -> Signal<CategorizedFileInfo> {
        self.folder_files
    }

    pub fn selected_cover(&self) -> Signal<Option<SelectedCover>> {
        self.selected_cover
    }

    /// Set a remote cover (MusicBrainz/Discogs). Computes the expected download filename from URL.
    pub fn set_remote_cover(&self, url: &str, source: &str) {
        let expected_filename = compute_expected_cover_filename(url, source);
        let mut signal = self.selected_cover;
        signal.set(Some(SelectedCover::Remote {
            url: url.to_string(),
            expected_filename,
        }));
    }

    /// Set a local cover by filename (relative path from album folder)
    pub fn set_local_cover(&self, filename: &str) {
        let mut signal = self.selected_cover;
        signal.set(Some(SelectedCover::Local {
            filename: filename.to_string(),
        }));
    }

    /// Clear cover selection
    pub fn clear_cover(&self) {
        let mut signal = self.selected_cover;
        signal.set(None);
    }

    pub fn torrent_source(&self) -> Signal<Option<TorrentSource>> {
        self.torrent_source
    }

    pub fn seed_after_download(&self) -> Signal<bool> {
        self.seed_after_download
    }

    pub fn torrent_metadata(&self) -> Signal<Option<TorrentImportMetadata>> {
        self.torrent_metadata
    }

    pub fn storage_profile_id(&self) -> Signal<Option<String>> {
        self.storage_profile_id
    }

    pub fn set_storage_profile_id(&self, value: Option<String>) {
        let mut signal = self.storage_profile_id;
        signal.set(value);
    }

    pub fn set_search_artist(&self, value: String) {
        let mut signal = self.search_artist;
        signal.set(value);
    }

    pub fn set_search_album(&self, value: String) {
        let mut signal = self.search_album;
        signal.set(value);
    }

    pub fn set_search_year(&self, value: String) {
        let mut signal = self.search_year;
        signal.set(value);
    }

    pub fn set_search_catalog_number(&self, value: String) {
        let mut signal = self.search_catalog_number;
        signal.set(value);
    }

    pub fn set_search_barcode(&self, value: String) {
        let mut signal = self.search_barcode;
        signal.set(value);
    }

    pub fn set_search_label(&self, value: String) {
        let mut signal = self.search_label;
        signal.set(value);
    }

    pub fn set_search_tab(&self, value: SearchTab) {
        let mut signal = self.search_tab;
        signal.set(value);
    }

    pub fn set_search_results(&self, value: Vec<DiscogsSearchResult>) {
        let mut signal = self.search_results;
        signal.set(value);
    }

    pub fn set_has_searched(&self, value: bool) {
        let mut signal = self.has_searched;
        signal.set(value);
    }

    pub fn set_is_searching_masters(&self, value: bool) {
        let mut signal = self.is_searching_masters;
        signal.set(value);
    }

    pub fn set_is_loading_versions(&self, value: bool) {
        let mut signal = self.is_loading_versions;
        signal.set(value);
    }

    pub fn set_error_message(&self, value: Option<String>) {
        let mut signal = self.error_message;
        signal.set(value);
    }

    pub fn set_navigation_stack(&self, value: Vec<ImportStep>) {
        let mut signal = self.navigation_stack;
        signal.set(value);
    }

    pub fn set_mb_search_results(&self, value: Vec<MbRelease>) {
        let mut signal = self.mb_search_results;
        signal.set(value);
    }

    pub fn set_is_searching_mb(&self, value: bool) {
        let mut signal = self.is_searching_mb;
        signal.set(value);
    }

    pub fn set_mb_error_message(&self, value: Option<String>) {
        let mut signal = self.mb_error_message;
        signal.set(value);
    }

    pub fn set_folder_path(&self, value: String) {
        let mut signal = self.folder_path;
        signal.set(value);
    }

    pub fn set_detected_releases(&self, value: Vec<DetectedRelease>) {
        let mut signal = self.detected_releases;
        signal.set(value);
    }

    pub fn set_selected_release_indices(&self, value: Vec<usize>) {
        let mut signal = self.selected_release_indices;
        signal.set(value);
    }

    pub fn set_current_release_index(&self, value: usize) {
        let mut signal = self.current_release_index;
        signal.set(value);
    }

    pub fn set_detected_metadata(&self, value: Option<FolderMetadata>) {
        let mut signal = self.detected_metadata;
        signal.set(value);
    }

    pub fn set_import_phase(&self, value: ImportPhase) {
        let mut signal = self.import_phase;
        signal.set(value);
    }

    pub fn set_exact_match_candidates(&self, value: Vec<MatchCandidate>) {
        let mut signal = self.exact_match_candidates;
        signal.set(value);
    }

    pub fn set_selected_match_index(&self, value: Option<usize>) {
        let mut signal = self.selected_match_index;
        signal.set(value);
    }

    pub fn set_confirmed_candidate(&self, value: Option<MatchCandidate>) {
        // Initialize selected_cover when confirming a candidate with cover art
        if let Some(ref candidate) = value {
            if let Some(ref url) = candidate.cover_art_url {
                let source = match &candidate.source {
                    crate::import::MatchSource::MusicBrainz(_) => "musicbrainz",
                    crate::import::MatchSource::Discogs(_) => "discogs",
                };
                self.set_remote_cover(url, source);
            }
        } else {
            // Clear selection when clearing candidate
            let mut signal = self.selected_cover;
            signal.set(None);
        }

        let mut signal = self.confirmed_candidate;
        signal.set(value);
    }

    pub fn set_is_detecting(&self, value: bool) {
        let mut signal = self.is_detecting;
        signal.set(value);
    }

    pub fn set_is_looking_up(&self, value: bool) {
        let mut signal = self.is_looking_up;
        signal.set(value);
    }

    pub fn set_is_importing(&self, value: bool) {
        let mut signal = self.is_importing;
        signal.set(value);
    }

    pub fn set_import_error_message(&self, value: Option<String>) {
        let mut signal = self.import_error_message;
        signal.set(value);
    }

    pub fn discid_lookup_error(&self) -> Signal<Option<String>> {
        self.discid_lookup_error
    }

    pub fn set_discid_lookup_error(&self, value: Option<String>) {
        let mut signal = self.discid_lookup_error;
        signal.set(value);
    }

    pub fn set_duplicate_album_id(&self, value: Option<String>) {
        let mut signal = self.duplicate_album_id;
        signal.set(value);
    }

    pub fn set_folder_files(&self, value: CategorizedFileInfo) {
        let mut signal = self.folder_files;
        signal.set(value);
    }

    pub fn set_torrent_source(&self, value: Option<TorrentSource>) {
        let mut signal = self.torrent_source;
        signal.set(value);
    }

    pub fn set_seed_after_download(&self, value: bool) {
        let mut signal = self.seed_after_download;
        signal.set(value);
    }

    pub fn set_torrent_metadata(&self, value: Option<TorrentImportMetadata>) {
        let mut signal = self.torrent_metadata;
        signal.set(value);
    }

    pub fn set_torrent_info_hash(&self, value: Option<String>) {
        let mut signal = self.torrent_info_hash;
        signal.set(value);
    }

    pub fn torrent_info(&self) -> Signal<Option<TorrentInfo>> {
        self.torrent_info
    }

    pub fn set_torrent_info(&self, value: Option<TorrentInfo>) {
        let mut signal = self.torrent_info;
        signal.set(value);
    }

    pub fn torrent_input_mode(&self) -> Signal<TorrentInputMode> {
        self.torrent_input_mode
    }

    pub fn set_torrent_input_mode(&self, value: TorrentInputMode) {
        let mut signal = self.torrent_input_mode;
        signal.set(value);
    }

    pub fn magnet_link(&self) -> Signal<String> {
        self.magnet_link
    }

    pub fn set_magnet_link(&self, value: String) {
        let mut signal = self.magnet_link;
        signal.set(value);
    }

    pub fn cd_toc_info(&self) -> Signal<Option<(String, u8, u8)>> {
        self.cd_toc_info
    }

    pub fn set_cd_toc_info(&self, value: Option<(String, u8, u8)>) {
        let mut signal = self.cd_toc_info;
        signal.set(value);
    }

    pub fn selected_import_source(&self) -> Signal<ImportSource> {
        self.selected_import_source
    }

    pub fn set_selected_import_source(&self, value: ImportSource) {
        let mut signal = self.selected_import_source;
        signal.set(value);
    }

    pub fn search_source(&self) -> Signal<SearchSource> {
        self.search_source
    }

    pub fn set_search_source(&self, value: SearchSource) {
        let mut signal = self.search_source;
        signal.set(value);
    }

    pub fn manual_match_candidates(&self) -> Signal<Vec<MatchCandidate>> {
        self.manual_match_candidates
    }

    pub fn set_manual_match_candidates(&self, value: Vec<MatchCandidate>) {
        let mut signal = self.manual_match_candidates;
        signal.set(value);
    }

    pub fn reset(&self) {
        self.set_search_artist(String::new());
        self.set_search_album(String::new());
        self.set_search_year(String::new());
        self.set_search_label(String::new());
        self.set_search_catalog_number(String::new());
        self.set_search_barcode(String::new());
        self.set_search_tab(SearchTab::default());
        self.set_search_results(Vec::new());
        self.set_has_searched(false);
        self.set_is_searching_masters(false);
        self.set_is_loading_versions(false);
        self.set_error_message(None);
        self.set_mb_search_results(Vec::new());
        self.set_is_searching_mb(false);
        self.set_mb_error_message(None);
        self.set_navigation_stack(vec![ImportStep::FolderIdentification]);

        // Also reset folder detection import state
        self.set_folder_path(String::new());
        self.set_detected_metadata(None);
        self.set_import_phase(ImportPhase::FolderSelection);
        self.set_exact_match_candidates(Vec::new());
        self.set_selected_match_index(None);
        self.set_confirmed_candidate(None);
        self.set_is_detecting(false);
        self.set_is_looking_up(false);
        self.set_is_importing(false);
        self.set_import_error_message(None);
        self.set_discid_lookup_error(None);
        self.set_duplicate_album_id(None);
        self.set_folder_files(CategorizedFileInfo::default());
        self.set_torrent_source(None);
        self.set_seed_after_download(true);
        self.set_torrent_metadata(None);
        self.set_torrent_info_hash(None);
        self.set_torrent_info(None);
        self.set_torrent_input_mode(TorrentInputMode::File);
        self.set_magnet_link(String::new());
        self.set_cd_toc_info(None);
        self.set_manual_match_candidates(Vec::new());
        // Note: selected_import_source and search_source are NOT reset - they persist across navigation
    }

    /// Reset detection state and return to folder selection phase
    pub fn reset_to_folder_selection(&self) {
        self.set_is_detecting(false);
        self.set_import_phase(ImportPhase::FolderSelection);
    }

    /// Clear search fields when entering manual search phase.
    /// Users fill fields by clicking pills extracted from metadata.
    #[allow(unused_variables)]
    pub fn init_search_query_from_metadata(&self, metadata: &FolderMetadata) {
        // Clear all search fields - users click pills to fill them
        self.set_search_artist(String::new());
        self.set_search_album(String::new());
        self.set_search_year(String::new());
        self.set_search_label(String::new());
        self.set_search_catalog_number(String::new());
        self.set_search_barcode(String::new());
        self.set_search_tab(SearchTab::default());
    }

    /// Reset state for a new torrent selection
    pub fn select_torrent_file(
        &self,
        path: String,
        torrent_source: TorrentSource,
        seed_after_download: bool,
    ) {
        // Store torrent source and seed flag
        self.set_torrent_source(Some(torrent_source));
        self.set_seed_after_download(seed_after_download);

        // Reset state for new selection
        self.set_folder_path(path);
        self.set_detected_metadata(None);
        self.set_exact_match_candidates(Vec::new());
        self.set_selected_match_index(None);
        self.set_confirmed_candidate(None);
        self.set_import_error_message(None);
        self.set_duplicate_album_id(None);
        self.set_import_phase(ImportPhase::MetadataDetection);
        self.set_is_detecting(true);
    }

    // Multi-release workflow helpers

    /// Check if there are more releases to import in the current batch
    pub fn has_more_releases(&self) -> bool {
        let current_idx = *self.current_release_index.read();
        let selected_indices = self.selected_release_indices.read();
        current_idx + 1 < selected_indices.len()
    }

    /// Advance to the next release in the batch
    pub fn advance_to_next_release(&self) {
        if self.has_more_releases() {
            let current_idx = *self.current_release_index.read();
            self.set_current_release_index(current_idx + 1);
        }
    }

    /// Get the currently selected release
    pub fn get_current_release(&self) -> Option<DetectedRelease> {
        let current_idx = *self.current_release_index.read();
        let selected_indices = self.selected_release_indices.read();
        let releases = self.detected_releases.read();

        selected_indices
            .get(current_idx)
            .and_then(|&release_idx| releases.get(release_idx).cloned())
    }

    // Facade methods delegating to submodules

    pub async fn load_torrent_for_import(
        &self,
        path: PathBuf,
        seed_flag: bool,
    ) -> Result<(), String> {
        detection::load_torrent_for_import(self, path, seed_flag).await
    }

    pub async fn retry_torrent_metadata_detection(&self) -> Result<(), String> {
        detection::retry_torrent_metadata_detection(self).await
    }

    pub async fn confirm_and_start_import(
        &self,
        candidate: MatchCandidate,
        import_source: ImportSource,
        navigator: Navigator,
    ) -> Result<(), String> {
        import::confirm_and_start_import(self, candidate, import_source, navigator).await
    }

    pub async fn load_folder_for_import(&self, path: String) -> Result<(), String> {
        // Reset state for new folder selection
        self.set_folder_path(path.clone());
        self.set_detected_metadata(None);
        self.set_exact_match_candidates(Vec::new());
        self.set_selected_match_index(None);
        self.set_confirmed_candidate(None);
        self.set_import_error_message(None);
        self.set_duplicate_album_id(None);
        self.set_import_phase(ImportPhase::MetadataDetection);

        let result = detection::load_folder_for_import(self, path).await?;

        // If we're in ReleaseSelection phase, stop here (multiple releases detected)
        if *self.import_phase.read() == ImportPhase::ReleaseSelection {
            return Ok(());
        }

        // Files and metadata are already set inside load_folder_for_import
        // so they appear immediately in the UI before the MusicBrainz lookup

        match result.discid_result {
            None | Some(detection::DiscIdLookupResult::NoMatches) => {
                self.init_search_query_from_metadata(&result.metadata);
                self.set_import_phase(ImportPhase::ManualSearch);
            }
            Some(detection::DiscIdLookupResult::SingleMatch(candidate)) => {
                self.set_confirmed_candidate(Some(*candidate));
                self.set_import_phase(ImportPhase::Confirmation);
            }
            Some(detection::DiscIdLookupResult::MultipleMatches(candidates)) => {
                self.set_exact_match_candidates(candidates);
                self.set_import_phase(ImportPhase::ExactLookup);
            }
        }

        Ok(())
    }

    pub async fn load_selected_release(&self, release_index: usize) -> Result<(), String> {
        // Reset state for new release
        self.set_detected_metadata(None);
        self.set_exact_match_candidates(Vec::new());
        self.set_selected_match_index(None);
        self.set_confirmed_candidate(None);
        self.set_import_error_message(None);
        self.set_duplicate_album_id(None);
        self.set_import_phase(ImportPhase::MetadataDetection);

        let result = detection::load_selected_release(self, release_index).await?;

        // Files and metadata are already set inside load_selected_release

        match result.discid_result {
            None | Some(detection::DiscIdLookupResult::NoMatches) => {
                self.init_search_query_from_metadata(&result.metadata);
                self.set_import_phase(ImportPhase::ManualSearch);
            }
            Some(detection::DiscIdLookupResult::SingleMatch(candidate)) => {
                self.set_confirmed_candidate(Some(*candidate));
                self.set_import_phase(ImportPhase::Confirmation);
            }
            Some(detection::DiscIdLookupResult::MultipleMatches(candidates)) => {
                self.set_exact_match_candidates(candidates);
                self.set_import_phase(ImportPhase::ExactLookup);
            }
        }

        Ok(())
    }

    pub async fn load_cd_for_import(
        &self,
        drive_path: String,
        disc_id: String,
    ) -> Result<(), String> {
        // Reset state for new CD selection
        self.set_folder_path(drive_path.clone());
        self.set_detected_metadata(None);
        self.set_exact_match_candidates(Vec::new());
        self.set_selected_match_index(None);
        self.set_confirmed_candidate(None);
        self.set_import_error_message(None);
        self.set_duplicate_album_id(None);
        self.set_import_phase(ImportPhase::MetadataDetection);

        let result = detection::load_cd_for_import(self, disc_id).await?;

        match result {
            detection::DiscIdLookupResult::NoMatches => {
                // For CD import, we'll populate search fields after metadata detection
                self.set_search_artist(String::new());
                self.set_search_album(String::new());
                self.set_import_phase(ImportPhase::ManualSearch);
            }
            detection::DiscIdLookupResult::SingleMatch(candidate) => {
                self.set_confirmed_candidate(Some(*candidate));
                self.set_import_phase(ImportPhase::Confirmation);
            }
            detection::DiscIdLookupResult::MultipleMatches(candidates) => {
                self.set_exact_match_candidates(candidates);
                self.set_import_phase(ImportPhase::ExactLookup);
            }
        }

        Ok(())
    }

    pub fn try_switch_import_source(self: &Rc<Self>, source: ImportSource) {
        navigation::try_switch_import_source(self, source)
    }

    pub fn try_switch_torrent_input_mode(self: &Rc<Self>, mode: TorrentInputMode) {
        navigation::try_switch_torrent_input_mode(self, mode)
    }

    pub fn select_exact_match(&self, index: usize) {
        navigation::select_exact_match(self, index)
    }

    pub fn confirm_candidate(&self, candidate: MatchCandidate) {
        navigation::confirm_candidate(self, candidate)
    }

    pub fn reject_confirmation(&self) {
        navigation::reject_confirmation(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_expected_cover_filename_musicbrainz_jpg() {
        let filename = compute_expected_cover_filename(
            "https://coverartarchive.org/release/abc/123.jpg",
            "musicbrainz",
        );
        assert_eq!(filename, ".bae/cover-mb.jpg");
    }

    #[test]
    fn test_compute_expected_cover_filename_musicbrainz_jpeg() {
        let filename = compute_expected_cover_filename(
            "https://coverartarchive.org/release/abc/456.jpeg",
            "musicbrainz",
        );
        assert_eq!(filename, ".bae/cover-mb.jpeg");
    }

    #[test]
    fn test_compute_expected_cover_filename_musicbrainz_png() {
        let filename = compute_expected_cover_filename(
            "https://coverartarchive.org/release/abc/789.png",
            "mb",
        );
        assert_eq!(filename, ".bae/cover-mb.png");
    }

    #[test]
    fn test_compute_expected_cover_filename_discogs() {
        let filename =
            compute_expected_cover_filename("https://i.discogs.com/abc/cover.jpg", "discogs");
        assert_eq!(filename, ".bae/cover-discogs.jpg");
    }

    #[test]
    fn test_compute_expected_cover_filename_no_extension_defaults_to_jpg() {
        let filename = compute_expected_cover_filename("https://example.com/image", "musicbrainz");
        assert_eq!(filename, ".bae/cover-mb.jpg");
    }

    #[test]
    fn test_selected_cover_local_stores_filename() {
        let cover = SelectedCover::Local {
            filename: "scans/front.jpg".to_string(),
        };
        if let SelectedCover::Local { filename } = cover {
            assert_eq!(filename, "scans/front.jpg");
        } else {
            panic!("Expected Local variant");
        }
    }

    #[test]
    fn test_selected_cover_remote_stores_expected_filename() {
        let cover = SelectedCover::Remote {
            url: "https://example.com/cover.jpeg".to_string(),
            expected_filename: ".bae/cover-mb.jpeg".to_string(),
        };
        if let SelectedCover::Remote {
            url,
            expected_filename,
        } = cover
        {
            assert_eq!(url, "https://example.com/cover.jpeg");
            assert_eq!(expected_filename, ".bae/cover-mb.jpeg");
        } else {
            panic!("Expected Remote variant");
        }
    }
}
