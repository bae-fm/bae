mod client;
mod error;
pub use error::DbError;
pub mod identity;
mod models;
// Only an import names its own artist credits from outside `db`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::ArtistCredits;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::RecordsChange;
pub(crate) use client::ReleaseFileStream;
pub(crate) use client::ReleaseMetadataRows;
pub use client::{
    AlbumBrowseProjection, AlbumDetailProjection, AlbumSelectionProjection, ArtistBrowseProjection,
    ArtistDetailProjection, ComposerBrowseProjection, ComposerDetailProjection,
    LibrarySearchProjection, ReleaseDetailProjection, StorageBrowseProjection,
    StorageBrowseRequest, WorkDetailProjection,
};
pub use client::{ArtistWriteError, Database, ReleaseDeletion};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::{
    CandidateListGrouping, CandidateStateListRow, FinishedScan, GroupingChanges, ImportQueueRows,
    ScanCandidateKind, ScanCandidateListRow, ScanItemWrite,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::{
    CandidatePaneWrite, CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved,
    CandidateScanExpectation, FolderReadingCommit, FolderReadingStamp, FolderReadingWrite,
    GroupingFacts, ImportRows, NewArtistImages, ScanItemToWrite, ScannedCandidateKey,
};
pub(crate) use client::{
    OutboxDisplayContext, OutboxDisplayRequest, QueueCatalogProjection, QueueCatalogRequest,
};
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
pub(crate) use client::{SeededAlbum, SeededAudio};
pub use models::*;
