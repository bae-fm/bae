//! Local managed blob storage. The blob store and content-addressed path layout
//! live in coven; bae keeps the pin/unpin transfer queue and deferred cleanup on
//! top. `ReleaseStorageImpl` is bae's name for coven's `BlobStore`.
pub mod cleanup;
pub mod transfer;

use coven::library_dir::LibraryDir;
pub use coven::storage::local::BlobStore as ReleaseStorageImpl;
use tracing::warn;

/// A fixed in-library token that no real hashed blob id maps to. A malformed id —
/// only reachable from a peer's synced row, since locally-minted ids are UUIDs —
/// resolves here so a read finds nothing instead of panicking or escaping the
/// library dir.
const MALFORMED_ID_SENTINEL: &str = "__malformed_id__";

/// The hashed-by-id cloud/storage key for a bae blob id. coven validates the id as
/// a single safe path token before hashing it. A locally-minted id (a UUID or a
/// release/file id this device created) always validates. A malformed id can only
/// arrive over sync — a `releases.id`/`release_files.id` from a peer, which bae
/// does not format-validate on apply — so it is hostile input, not a host bug:
/// logged and resolved to a sentinel no real blob occupies, so the caller reads a
/// missing asset instead of crash-looping every device on a durable bad id.
pub fn storage_path(id: &str) -> String {
    coven::storage::local::storage_path(id).unwrap_or_else(|e| {
        warn!(
            "blob id {id:?} is not a valid storage path token ({e}); resolving to a missing asset"
        );
        MALFORMED_ID_SENTINEL.to_string()
    })
}

/// The local plaintext path for an image blob under `library_dir`. Like
/// [`storage_path`], a malformed (peer-supplied) id is logged and resolved to a
/// non-existent in-library path so the read finds no cover rather than panicking.
pub fn image_path(library_dir: &LibraryDir, id: &str) -> std::path::PathBuf {
    library_dir.image_path(id).unwrap_or_else(|e| {
        warn!(
            "image id {id:?} is not a valid storage path token ({e}); resolving to a missing asset"
        );
        library_dir.join(MALFORMED_ID_SENTINEL)
    })
}

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
