pub mod cover_art;
mod discogs_matcher;
mod discogs_parser;
mod file_validation;
mod folder_metadata_detector;
pub mod folder_scanner;
mod handle;
mod musicbrainz_parser;
mod progress;
mod service;
mod track_to_file_mapper;
mod types;
pub use discogs_matcher::{rank_discogs_matches, rank_mb_matches, MatchCandidate, MatchSource};
pub use folder_metadata_detector::{detect_folder_contents, detect_metadata, FolderMetadata};
pub use folder_scanner::{scan_for_candidates_with_callback, CategorizedFiles, DetectedCandidate};
pub use handle::{ImportServiceHandle, ScanEvent};
#[cfg(feature = "torrent")]
pub use handle::{TorrentFileMetadata, TorrentImportMetadata};
pub use progress::ImportProgressHandle;
pub use service::ImportService;
#[cfg(feature = "torrent")]
pub use types::TorrentSource;
pub use types::{ImportPhase, ImportProgress, ImportRequest, PrepareStep};
