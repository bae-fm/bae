//! Local blob storage helpers. coven owns the blob store, the content-addressed
//! path layout, the cloud key derivation, and the locality-aware read; bae keeps
//! the pin/unpin transfer queue and deferred cleanup on top, plus the on-disk
//! resolution of a host-provided image blob (cover / artist image) for the
//! path-consuming UI. `ReleaseStorageImpl` is bae's name for coven's `BlobStore`.
pub mod cleanup;
pub mod transfer;

use coven::library_dir::LibraryDir;
pub use coven::storage::local::BlobStore as ReleaseStorageImpl;
use tracing::warn;

/// The on-disk path a host-provided image blob (a cover or an artist image)
/// lives at on this device, if present — for the path-consuming image UI.
///
/// A host-provided blob lives in coven's local store (`storage/local/<ns>/<id>`)
/// while its subject is Local, and in coven's cache
/// (`storage/pinned/<ns>/…` or `storage/cache/<ns>/…`) while Remote. This checks
/// those three folders in order and returns the first that exists, so the UI can
/// load the cover straight off disk regardless of locality. `None` means the
/// image is not on this device (never produced, or a Remote cover evicted from
/// the cache — the UI shows a placeholder until the next pull re-fetches it). A
/// malformed (peer-supplied) id that can't form a safe path reads as absent.
pub fn image_blob_path(
    library_dir: &LibraryDir,
    namespace: &str,
    id: &str,
) -> Option<std::path::PathBuf> {
    let candidates = [
        library_dir.local_blob_path(namespace, id),
        library_dir.pinned_blob_path(namespace, id),
        library_dir.cache_blob_path(namespace, id),
    ];
    for candidate in candidates {
        match candidate {
            Ok(path) if path.exists() => return Some(path),
            Ok(_) => {}
            Err(e) => {
                warn!("image id {id:?} in {namespace:?} is not a valid path token ({e})");
                return None;
            }
        }
    }
    None
}
