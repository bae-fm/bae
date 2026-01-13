pub mod cover_art;
mod discogs_matcher;
mod discogs_parser;
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
pub use folder_scanner::{CategorizedFiles, DetectedRelease};
pub use handle::ImportServiceHandle;
#[cfg(feature = "torrent")]
pub use handle::{TorrentFileMetadata, TorrentImportMetadata};
pub use service::ImportService;
#[cfg(feature = "torrent")]
pub use types::TorrentSource;
pub use types::{ImportPhase, ImportProgress, ImportRequest, PrepareStep};
