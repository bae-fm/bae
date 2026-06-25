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

/// bae's cloud namespace for release-file (audio) blobs — the root every audio
/// key shards or nests under, mirroring coven's `BlobRef.namespace`. An opaque
/// home shards `storage/{ab}/{cd}/{id}`; a browsable home nests
/// `storage/{cloud_path}` (the readable key stored on `release_files.cloud_path`
/// is RELATIVE to this namespace, exactly as `library_images.cloud_path` is
/// relative to `images` — coven prepends the namespace). Kept clear of coven's
/// reserved root prefixes (`heads/`, `changes/`, `membership/`, `auth/keys/`).
pub const AUDIO_NAMESPACE: &str = "storage";

/// The full cloud object key a managed file's blob lives at: the
/// [`AUDIO_NAMESPACE`] prepended onto the stored readable `cloud_path` (a
/// browsable home), or the hashed-by-id [`storage_path`] default (an opaque home,
/// where `cloud_path` is NULL — already namespace-prefixed). This is the single
/// definition of the "NULL means hashed-by-id" rule — every upload, delete, and
/// cross-device key resolves through here (most via [`crate::db::DbFile::cloud_key`]),
/// so the fallback can never drift between the site that uploaded a blob and the
/// site that later deletes it. It equals `coven`'s `blob_key` for the audio
/// namespace, so a cache read built from `(AUDIO_NAMESPACE, id, cloud_path)`
/// addresses the same object. A NULL `cloud_path` is the documented opaque
/// layout, not a masked error.
pub fn effective_cloud_key(cloud_path: Option<&str>, file_id: &str) -> String {
    match cloud_path {
        Some(rel) => format!("{AUDIO_NAMESPACE}/{rel}"),
        None => storage_path(file_id),
    }
}
