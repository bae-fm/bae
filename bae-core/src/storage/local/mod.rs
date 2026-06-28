//! Local blob storage helpers. coven owns the blob store, the content-addressed
//! path layout, the cloud key derivation, and the locality-aware read; bae keeps
//! the pin/unpin transfer queue and deferred cleanup on top. `ReleaseStorageImpl`
//! is bae's name for coven's `BlobStore`.
pub mod cleanup;
pub mod transfer;

pub use coven::BlobStore as ReleaseStorageImpl;
