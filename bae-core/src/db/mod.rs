mod client;
mod error;
pub use error::DbError;
pub mod identity;
mod models;
// Only an import names its own artist credits from outside `db`.
#[cfg(test)]
pub(crate) use client::resolved_in_an_empty_library;
pub(crate) use client::AbandonedRow;
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::{AlbumFields, ReleaseEditWrite};
pub use client::{ArtistWriteError, Database, ReleaseDeletion};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::{
    CandidateListGrouping, CandidateStateListRow, FinishedScan, GroupingChanges, ImportQueueRows,
    ScanCandidateKind, ScanCandidateListRow, ScanItemWrite,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::{
    CandidatePaneWrite, CandidateSaveExpectation, CandidateSaveExtras, CandidateSaved,
    FolderReadingCommit, FolderReadingStamp, FolderReadingWrite, GroupingFacts, ImportClaim,
    ImportRows, NewArtistImages, RunVerdictSaved, ScanItemSeed, ScanItemToWrite,
    ScannedCandidateKey,
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

#[cfg(test)]
mod tests {
    /// `.cargo/config.toml` builds SQLite without its process-wide memory
    /// statistics, which would serialize every allocation through one mutex.
    #[test]
    fn sqlite_is_built_without_memory_statistics() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        let built_without: bool = connection
            .query_row(
                "SELECT sqlite_compileoption_used('DEFAULT_MEMSTATUS=0')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(built_without, "SQLite was built with memory statistics on");
    }
}
