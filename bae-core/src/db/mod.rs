mod client;
mod models;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use client::ImportTriageDbProjection;
pub(crate) use client::QueueCatalogProjection;
pub use client::{
    AlbumDetailProjection, AlbumPageProjection, ArtistDetailProjection, ArtistPageProjection,
    ComposerDetailProjection, ComposerPageProjection, LibraryParentObservationProjection,
    LibrarySearchProjection, ReleaseDetailProjection, StoragePageProjection, WorkDetailProjection,
};
pub use client::{Database, DeleteCleanupPlan, ImportReplacementDelete, ImportReplacementOutcome};
pub use models::*;
