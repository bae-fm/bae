mod client;
pub mod identity;
mod models;
pub(crate) use client::ArtistCredits;
pub use client::{
    AlbumBrowseProjection, AlbumDetailProjection, AlbumSelectionProjection, ArtistBrowseProjection,
    ArtistDetailProjection, ComposerBrowseProjection, ComposerDetailProjection,
    LibrarySearchProjection, ReleaseDetailProjection, StorageBrowseProjection,
    StorageBrowseRequest, WorkDetailProjection,
};
pub use client::{ArtistWriteError, Database, ImportReplacementOutcome, ReleaseDeletion};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::{
    CandidateListSource, CandidateStateListRow, ImportQueueRows, ScanCandidateKind,
    ScanCandidateListRow, ScanItemWrite,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::{
    CandidateLookupUpdate, CandidatePaneWrite, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, CandidateScanExpectation, FolderReadingCommit, FolderReadingStamp,
    FolderReadingWrite, ImportRows, NewArtistImages, RemoteImport, ScanItemToWrite,
    ScannedCandidateKey,
};
pub(crate) use client::{
    OutboxDisplayContext, OutboxDisplayRequest, QueueCatalogProjection, QueueCatalogRequest,
};
pub use models::*;
