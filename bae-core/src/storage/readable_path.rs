//! Cloud blob keys for browsable homes.
//!
//! An opaque home keys every remote blob by a hash of its id
//! (`storage/{ab}/{cd}/{id}` for audio, `images/{ab}/{cd}/{id}` for images), so
//! its cloud objects are obscured shard keys. A browsable home stores them
//! unencrypted at stable, structured paths built from the album/release/artist
//! ids, with the file's real name intact:
//!
//! - audio:        `{album_id}/{release_id}/{source_folder}/{filename}` (under the `storage` namespace)
//! - cover image:  `{album_id}/{release_id}/cover.{ext}`   (under the `images` namespace)
//! - artist image: `{artist_id}/artist.{ext}`             (under the `images` namespace)
//!
//! Every key is RELATIVE to its namespace; coven prepends the namespace (`storage`
//! for audio, `images` for art — see [`crate::storage::local::AUDIO_NAMESPACE`]),
//! so the full object key is `storage/{album}/…` for audio and `images/{album}/…`
//! for a cover.
//!
//! `{source_folder}` is the name of the folder the release was imported from
//! (`releases.source_folder_name`), so a browsable bucket mirrors the original
//! release folder. It is omitted when absent (a non-folder import); the
//! `{release_id}` level already makes the key unique either way. Covers/artist
//! art are bae's own (not part of the imported folder), so they carry no
//! `{source_folder}` level.
//!
//! Ids are immutable and unique, so this is collision-free by construction — a
//! release's folder is written once and never absorbs another release's files
//! (two releases of one album get sibling `{release_id}` folders), and no
//! name-sanitizing or disambiguation is needed. Browsable means "not obscured",
//! not "human-named": the real filenames are visible and the tree is navigable
//! by album → release, versus the opaque home's content-hashed shards. Audio
//! lives under the `storage` namespace and art under `images` (coven prepends
//! each), keeping audio clear of coven's reserved root prefixes (`heads/`,
//! `changes/`, `membership/`, `auth/keys/`).
//!
//! The key is computed once when the blob is first destined for the cloud and
//! stored on the synced `cloud_path` column of its row (`release_files` for
//! audio, `library_images` for images); every later upload, read, delete, and
//! pull uses the stored value verbatim. (The key is re-derivable from the
//! immutable ids, but storing it keeps reads uniform with the opaque
//! hashed-by-id path and lets the image `BlobSource`, which has no DB handle at
//! push, read the key straight off the row.)

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

/// Replace path separators in a single path component (a folder name or a
/// filename) so it can't open an unintended sub-level. A real on-disk name has
/// none (the filesystem forbids `/`), so this is purely defensive against an odd
/// source; it does not otherwise alter the name, since the `{release_id}` level
/// already makes every key unique.
fn safe_component(component: &str) -> String {
    component.replace(['/', '\\'], "_")
}

/// The cloud key for a release file on a browsable home, RELATIVE to the
/// `storage` audio namespace coven prepends (see
/// [`crate::storage::local::AUDIO_NAMESPACE`]):
/// `{album_id}/{release_id}/{source_folder}/{filename}`, mirroring the folder the
/// release was imported from. The full object key is `storage/` + this. The
/// readable key is stored RELATIVE on `release_files.cloud_path`, exactly as a
/// cover's key is stored relative to `images` — coven prepends the namespace on
/// every read/write. `source_folder` is `None` for a non-folder import — that
/// level is then omitted; the `{release_id}` level keeps the key unique either
/// way. (`source_folder_name` comes from `Path::file_name`, which is `None` or a
/// non-empty name, so `Some("")` never occurs and isn't guarded here.)
pub fn audio_key(
    album_id: &str,
    release_id: &str,
    source_folder: Option<&str>,
    filename: &str,
) -> String {
    let filename = safe_component(filename);
    match source_folder {
        Some(folder) => format!(
            "{album_id}/{release_id}/{}/{filename}",
            safe_component(folder)
        ),
        None => format!("{album_id}/{release_id}/{filename}"),
    }
}

/// The `cloud_path` for a cover image on a browsable home, RELATIVE to the
/// `images` namespace coven prepends: `{album_id}/{release_id}/cover.{ext}`.
pub fn cover_cloud_path(album_id: &str, release_id: &str, content_type: &ContentType) -> String {
    format!(
        "{album_id}/{release_id}/cover.{}",
        image_extension(content_type)
    )
}

/// The `cloud_path` for an artist image on a browsable home, RELATIVE to the
/// `images` namespace coven prepends: `{artist_id}/artist.{ext}`.
pub fn artist_cloud_path(artist_id: &str, content_type: &ContentType) -> String {
    format!("{artist_id}/artist.{}", image_extension(content_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_key_includes_source_folder_when_present() {
        // Namespace-relative; coven prepends `storage/`.
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
    fn safe_component_strips_path_separators_only() {
        // A stray separator in either the folder or the filename can't open a
        // sub-level; everything else is verbatim.
        assert_eq!(
            audio_key("album-1", "rel-1", Some("a/b"), "sub/dir\\track.flac"),
            "album-1/rel-1/a_b/sub_dir_track.flac"
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
            cover_cloud_path("album-1", "rel-1", &ContentType::Jpeg),
            "album-1/rel-1/cover.jpg"
        );
        assert_eq!(
            artist_cloud_path("artist-1", &ContentType::Png),
            "artist-1/artist.png"
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
