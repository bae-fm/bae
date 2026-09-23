mod client;
mod models;
pub(crate) use client::QueueCatalogProjection;
pub use client::{
    AlbumBrowseProjection, AlbumDetailProjection, AlbumPageProjection, ArtistDetailProjection,
    ArtistPageProjection, ComposerBrowseProjection, ComposerDetailProjection,
    ComposerPageProjection, LibrarySearchProjection, ReleaseDetailProjection,
    StoragePageProjection, WorkDetailProjection,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use client::{
    CandidateListSource, CandidateStateListRow, ImportQueueRows, ListedVerdict, ScanCandidateKind,
    ScanCandidateListRow, ScanItemWrite,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::{
    CandidateLookupUpdate, CandidateResultWrite, CandidateSaveExpectation, CandidateSaveExtras,
    CandidateSaved, CandidateScanExpectation, ImportRows, ScannedCandidateKey,
};
pub use client::{Database, DeleteCleanupPlan, ImportReplacementDelete, ImportReplacementOutcome};
pub use models::*;
