//! Cloud blob keys for browsable homes.
//!
//! An opaque home keys every managed blob by a hash of its id
//! (`storage/{ab}/{cd}/{id}` for audio, `images/{ab}/{cd}/{id}` for images), so
//! its cloud objects are obscured shard keys. A browsable home stores them
//! unencrypted at stable, structured paths built from the album/release/artist
//! ids, with the file's real name intact:
//!
//! - audio:        `storage/{album_id}/{release_id}/{filename}`
//! - cover image:  `{album_id}/{release_id}/cover.{ext}`   (under the `images` namespace)
//! - artist image: `{artist_id}/artist.{ext}`             (under the `images` namespace)
//!
//! Ids are immutable and unique, so this is collision-free by construction — a
//! release's folder is written once and never absorbs another release's files
//! (two releases of one album get sibling `{release_id}` folders), and no
//! name-sanitizing or disambiguation is needed. Browsable means "not obscured",
//! not "human-named": the real filenames are visible and the tree is navigable
//! by album → release, versus the opaque home's content-hashed shards. Audio
//! keys carry the `storage/` prefix (bae's audio namespace, the same root an
//! opaque home shards under), keeping them clear of coven's reserved root
//! prefixes (`heads/`, `changes/`, `membership/`, `auth/keys/`); image keys stay
//! under coven's `images/` namespace (coven prepends it).
//!
//! The key is computed once when the blob is first destined for the cloud and
//! stored on the synced `cloud_path` column of its row (`release_files` for
//! audio, `library_images` for images); every later upload, read, delete, and
//! pull uses the stored value verbatim. (The key is re-derivable from the
//! immutable ids, but storing it keeps reads uniform with the opaque
//! hashed-by-id path and lets the image `BlobPlan`, which has no DB handle at
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

/// Replace path separators in a filename so it stays a single component under
/// its `{release_id}` folder. A real on-disk filename has none (the filesystem
/// forbids `/`), so this only guards against a stray separator from an odd
/// source creating an unintended sub-level; it does not otherwise alter the
/// name, since the `{release_id}` folder already makes every key unique.
fn safe_filename(filename: &str) -> String {
    filename.replace(['/', '\\'], "_")
}

/// The cloud key for an audio file on a browsable home:
/// `storage/{album_id}/{release_id}/{filename}`. The `storage/` prefix is bae's
/// audio namespace (an opaque home shards audio under the same prefix), keeping
/// the key clear of coven's reserved root prefixes.
pub fn audio_key(album_id: &str, release_id: &str, filename: &str) -> String {
    format!(
        "storage/{album_id}/{release_id}/{}",
        safe_filename(filename)
    )
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
    fn audio_key_is_album_release_filename_under_storage() {
        assert_eq!(
            audio_key("album-1", "rel-1", "01 Track Title.flac"),
            "storage/album-1/rel-1/01 Track Title.flac"
        );
    }

    #[test]
    fn safe_filename_strips_path_separators_only() {
        // A stray separator can't open a sub-level; everything else is verbatim.
        assert_eq!(
            audio_key("album-1", "rel-1", "sub/dir\\track.flac"),
            "storage/album-1/rel-1/sub_dir_track.flac"
        );
        // Spaces, punctuation, unicode all pass through untouched — the
        // {release_id} folder already guarantees uniqueness.
        assert_eq!(
            audio_key("album-1", "rel-1", "01 — Track (Live) 音.flac"),
            "storage/album-1/rel-1/01 — Track (Live) 音.flac"
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
