//! Cache-bustable identifier for a library image that lives at a stable,
//! content-addressed path.
//!
//! A cover (or any library image) is stored at a fixed path derived from the
//! release id, so when the user changes a cover the file is overwritten in
//! place and the path never changes. Image caches on every platform key on that
//! path — SwiftUI's `.task(id:)`, WinUI's URI-keyed `BitmapImage` cache — so a
//! cover change wouldn't reach the screen until the cache was discarded
//! (app restart).
//!
//! The fix threads the file's modification time into the identifier the UI
//! addresses the image by. The path stays the same, but the identifier changes
//! whenever the bytes change, so the cache key changes and the view reloads.
//!
//! The identifier is `<path>#v=<mtime_secs>`. The version is appended as a
//! fragment so each platform's loader strips it back to the bare path before
//! opening the file:
//! - macOS `ImageLoader.load` splits on [`VERSION_SEPARATOR`] and opens the
//!   path part.
//! - Windows `CoverImage.Load` splits on `#v=`, opens the path part as a file
//!   stream, and decodes via `SetSourceAsync` (the stream path bypasses WinUI's
//!   URI cache entirely, so the current bytes are always read).

use std::path::Path;
use std::time::UNIX_EPOCH;

use tracing::warn;

/// Separates the on-disk path from its cache-busting version in the identifier
/// the UI addresses an image by. Mirrored in the macOS `ImageLoader` and the
/// Windows `CoverImage` loaders, which strip it back to the bare path.
pub const VERSION_SEPARATOR: &str = "#v=";

/// Build the cache-bustable identifier for an image file: its path with the
/// file's modification time appended as `#v=<mtime_secs>`. Returns the bare
/// path (no version) when the modification time can't be read — a stamped
/// identifier is the optimization, the path is the correctness floor.
pub fn versioned_image_identifier(path: &Path, metadata: &std::fs::Metadata) -> Option<String> {
    let path = path.to_str()?;
    let version = match metadata.modified() {
        Ok(mtime) => mtime.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()),
        Err(e) => {
            warn!("cover mtime unavailable for {path}: {e}");
            None
        }
    };
    Some(match version {
        Some(secs) => format!("{path}{VERSION_SEPARATOR}{secs}"),
        None => path.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn appends_the_modification_time_as_a_version_fragment() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("cover");
        std::fs::write(&path, b"cover").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
            .unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        let identifier = versioned_image_identifier(&path, &metadata).unwrap();

        assert_eq!(
            identifier,
            format!("{}#v=1700000000", path.to_str().unwrap())
        );
        // The loaders split on the separator to recover the bare path.
        let (bare, version) = identifier.split_once(VERSION_SEPARATOR).unwrap();
        assert_eq!(bare, path.to_str().unwrap());
        assert_eq!(version, "1700000000");
    }
}
