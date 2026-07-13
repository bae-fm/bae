//! Readable (browsable-home) cloud-key paths for bae's blobs.
//!
//! An opaque home keys every Remote blob by a hash of its id
//! (`{namespace}/{ab}/{cd}/{id}`), so its cloud objects are obscured shard keys.
//! A browsable home stores them unencrypted at stable, structured paths built
//! from the album/release/artist ids, with the file's real name intact:
//!
//! - release file: `{album_id}/{release_id}/{source_folder}/{filename}` (namespace `release_files`)
//! - cover image:  `{album_id}/{release_id}/cover-{blob_id}.{ext}`      (namespace `covers`)
//! - artist image: `{artist_id}/artist-{blob_id}.{ext}`                 (namespace `artist_images`)
//!
//! Each key here is RELATIVE to its namespace; coven prepends the namespace, so
//! the full object key is `release_files/{album}/…`, `covers/{album}/…`, etc.
//!
//! `{source_folder}` is the name of the folder the release was imported from
//! (`releases.source_folder_name`), so a browsable bucket mirrors the original
//! release folder. It is omitted when absent (a non-folder import); the
//! `{release_id}` level already makes the key unique either way. Covers/artist
//! art are bae's own (not part of the imported folder), so they carry no
//! `{source_folder}` level.
//!
//! Ids are immutable and unique, so this is collision-free by construction — two
//! releases of one album get sibling `{release_id}` folders — and no
//! name-sanitizing or disambiguation is needed. Browsable means "not obscured",
//! not "human-named": the real filenames are visible and the tree is navigable by
//! album → release.
//!
//! # Why an image key carries its blob id
//!
//! An image's key is a pure function of its row, and it includes the row's
//! `blob_id` — so a cover that is changed writes a **new** object rather than
//! overwriting the one it replaces. That is what makes the key immutable: a blob
//! id is minted fresh for every stored image and never reused, so a key is never
//! reused either.
//!
//! Reusing one key for successive covers is what a stable `cover.{ext}` would do,
//! and it breaks two things that no retry can repair. Two devices replacing the
//! same cover would write the same object, and the bucket's last writer need not
//! be the row's last-write-wins winner — leaving readers with bytes whose hash
//! does not match the row that won. And a device applying a changeset written
//! *before* a replacement could never satisfy that changeset's content hash,
//! because the bytes it names were overwritten at the reused key. Distinct keys
//! per blob leave the superseded object in place until its tombstone is
//! collected, so both readers are served.
//!
//! Audio files keep their real names: a release file's bytes never change in
//! place — a new import is a new release id — so its key is already immutable.
//!
//! The key is computed once, at import, and stored on the synced `cloud_path`
//! column of its row (`release_files` / `covers` / `artist_images`); coven keys
//! every later upload, read, delete, and pull off the BlobDecl's
//! `cloud_path_column`, so the path never drifts. (It is re-derivable from the
//! immutable ids, but storing it keeps reads uniform with the opaque
//! hashed-by-id path.)

use crate::util::content_type::ContentType;

/// The file extension for an image blob's name. Covers and artist images are
/// named `cover.{ext}` / `artist.{ext}`, so the extension must be a short,
/// predictable token — not the full content-type extension table (which would
/// emit `bin`/`svg`/etc. for shapes that never reach here). The three formats
/// bae stores map to their familiar extension; anything else falls back to a
/// neutral `img`.
pub fn image_extension(content_type: &ContentType) -> &'static str {
    match content_type {
        ContentType::Jpeg => "jpg",
        ContentType::Png => "png",
        ContentType::Webp => "webp",
        _ => "img",
    }
}

/// The cloud key for a release file on a browsable home, RELATIVE to the
/// `release_files` namespace coven prepends:
/// `{album_id}/{release_id}/{source_folder}/{filename}`, mirroring the folder the
/// release was imported from. `source_folder` is `None` for a non-folder import —
/// that level is then omitted; the `{release_id}` level keeps the key unique
/// either way. (`source_folder_name` comes from `Path::file_name`, which is `None`
/// or a non-empty name, so `Some("")` never occurs and isn't guarded here.)
///
/// `filename` is `release_files.original_filename` — the file's path relative to
/// the release folder, so a disc subfolder (`CD1/track.flac`) is part of it and
/// nests in the key, exactly as it nests on disk when the release is exported or
/// made local. Both fragments were validated as relative paths of ordinary
/// components when their rows were written
/// ([`crate::storage::path_fragment`]), so nothing here can escape the key's
/// namespace or its release prefix.
pub fn audio_key(
    album_id: &str,
    release_id: &str,
    source_folder: Option<&str>,
    filename: &str,
) -> String {
    match source_folder {
        Some(folder) => format!("{album_id}/{release_id}/{folder}/{filename}"),
        None => format!("{album_id}/{release_id}/{filename}"),
    }
}

/// The `cloud_path` for a cover image on a browsable home, RELATIVE to the
/// `covers` namespace coven prepends: `{album_id}/{release_id}/cover-{blob_id}.{ext}`.
///
/// `blob_id` is the `covers` row's blob id, so changing a release's cover writes a
/// new object instead of overwriting the old one — see the module docs.
pub fn cover_cloud_path(
    album_id: &str,
    release_id: &str,
    blob_id: &str,
    content_type: &ContentType,
) -> String {
    format!(
        "{album_id}/{release_id}/cover-{blob_id}.{}",
        image_extension(content_type)
    )
}

/// The `cloud_path` for an artist image on a browsable home, RELATIVE to the
/// `artist_images` namespace coven prepends: `{artist_id}/artist-{blob_id}.{ext}`.
///
/// Carries the image's blob id for the same reason a cover's key does.
pub fn artist_cloud_path(artist_id: &str, blob_id: &str, content_type: &ContentType) -> String {
    format!(
        "{artist_id}/artist-{blob_id}.{}",
        image_extension(content_type)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_key_includes_source_folder_when_present() {
        // Namespace-relative; coven prepends `release_files/`.
        assert_eq!(
            audio_key(
                "album-1",
                "rel-1",
                Some("Album Folder [FLAC]"),
                "01 Track Title.flac"
            ),
            "album-1/rel-1/Album Folder [FLAC]/01 Track Title.flac"
        );
    }

    #[test]
    fn audio_key_omits_source_folder_when_absent() {
        // A non-folder import has no source folder; the release-id level still
        // makes the key unique.
        assert_eq!(
            audio_key("album-1", "rel-1", None, "01 Track Title.flac"),
            "album-1/rel-1/01 Track Title.flac"
        );
    }

    #[test]
    fn audio_key_nests_a_subfoldered_filename() {
        // `original_filename` carries the file's path within the release folder,
        // so a disc subfolder nests in the key — the browsable bucket mirrors the
        // folder the release was imported from, and matches what an export writes
        // to disk.
        assert_eq!(
            audio_key(
                "album-1",
                "rel-1",
                Some("Album [FLAC]"),
                "CD1/01 Track.flac"
            ),
            "album-1/rel-1/Album [FLAC]/CD1/01 Track.flac"
        );
        // Spaces, punctuation, unicode all pass through untouched — the
        // {release_id} level already guarantees uniqueness.
        assert_eq!(
            audio_key("album-1", "rel-1", None, "01 — Track (Live) 音.flac"),
            "album-1/rel-1/01 — Track (Live) 音.flac"
        );
    }

    #[test]
    fn cover_and_artist_paths_have_expected_shape() {
        assert_eq!(
            cover_cloud_path("album-1", "rel-1", "blob-1", &ContentType::Jpeg),
            "album-1/rel-1/cover-blob-1.jpg"
        );
        assert_eq!(
            artist_cloud_path("artist-1", "blob-1", &ContentType::Png),
            "artist-1/artist-blob-1.png"
        );
    }

    /// An image key is a pure function of its row and carries the row's blob id, so
    /// two successive covers of one release never name the same object. The
    /// replacement path depends on this: it tombstones the object the old blob
    /// occupied, which must not be the one the new blob just wrote.
    #[test]
    fn a_new_cover_blob_never_reuses_the_previous_cover_key() {
        let first = cover_cloud_path("album-1", "rel-1", "blob-1", &ContentType::Jpeg);
        let second = cover_cloud_path("album-1", "rel-1", "blob-2", &ContentType::Jpeg);
        assert_ne!(first, second);
        assert_ne!(
            artist_cloud_path("artist-1", "blob-1", &ContentType::Jpeg),
            artist_cloud_path("artist-1", "blob-2", &ContentType::Jpeg)
        );
    }

    #[test]
    fn image_extension_maps_known_and_falls_back() {
        assert_eq!(image_extension(&ContentType::Jpeg), "jpg");
        assert_eq!(image_extension(&ContentType::Png), "png");
        assert_eq!(image_extension(&ContentType::Webp), "webp");
        assert_eq!(image_extension(&ContentType::Gif), "img");
        assert_eq!(image_extension(&ContentType::OctetStream), "img");
    }
}
