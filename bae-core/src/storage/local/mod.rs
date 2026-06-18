//! Local managed blob storage. The blob store and content-addressed path layout
//! live in coven; bae keeps the pin/unpin transfer queue and deferred cleanup on
//! top. `ReleaseStorageImpl` is bae's name for coven's `BlobStore`.
pub mod cleanup;
pub mod transfer;

pub use coven::storage::local::{storage_path, BlobStore as ReleaseStorageImpl};

/// The cloud object key a managed file's blob lives at: the stored readable
/// `cloud_path` (a browsable home), or the hashed-by-id [`storage_path`] default
/// (an opaque home, where `cloud_path` is NULL). This is the single definition
/// of the "NULL means hashed-by-id" rule — every upload, read, delete, and
/// cross-device pull resolves a file's key through here (most via
/// [`crate::db::DbFile::cloud_key`]), so the fallback can never drift between
/// the site that uploaded a blob and the site that later reads or deletes it.
/// A NULL `cloud_path` is the documented opaque layout, not a masked error.
pub fn effective_cloud_key(cloud_path: Option<&str>, file_id: &str) -> String {
    cloud_path
        .map(str::to_string)
        .unwrap_or_else(|| storage_path(file_id))
}
