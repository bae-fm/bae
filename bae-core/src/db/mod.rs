mod client;
pub mod identity;
mod models;
pub use client::{
    AlbumBrowseProjection, AlbumDetailProjection, AlbumSelectionProjection, ArtistBrowseProjection,
    ArtistDetailProjection, ComposerBrowseProjection, ComposerDetailProjection,
    LibrarySearchProjection, ReleaseDetailProjection, StorageBrowseProjection,
    StorageBrowseRequest, WorkDetailProjection,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::{
    CandidateListSource, CandidateStateListRow, ImportQueueRows, ScanCandidateKind,
    ScanCandidateListRow, ScanItemWrite,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::{
    CandidateLookupUpdate, CandidatePaneWrite, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, CandidateScanExpectation, FolderReadingCommit, FolderReadingStamp,
    FolderReadingWrite, ImportRows, RemoteImport, ScanItemToWrite, ScannedCandidateKey,
};
pub use client::{Database, ImportReplacementOutcome, ReleaseDeletion};
pub(crate) use client::{
    OutboxDisplayContext, OutboxDisplayRequest, QueueCatalogProjection, QueueCatalogRequest,
};
pub use models::*;
