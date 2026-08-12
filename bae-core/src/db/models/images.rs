use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryImageType {
    Cover,
    Artist,
}

impl LibraryImageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LibraryImageType::Cover => "cover",
            LibraryImageType::Artist => "artist",
        }
    }

    pub fn namespace(&self) -> &'static str {
        match self {
            LibraryImageType::Cover => crate::sync::COVERS_NAMESPACE,
            LibraryImageType::Artist => crate::sync::ARTIST_IMAGES_NAMESPACE,
        }
    }
}

impl std::str::FromStr for LibraryImageType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cover" => Ok(LibraryImageType::Cover),
            "artist" => Ok(LibraryImageType::Artist),
            other => Err(format!("Unknown library image type: {}", other)),
        }
    }
}

/// A cover or artist image. The bytes are a coven host-provided blob, in the
/// `covers` or `artist_images` namespace (per `image_type`), addressed by
/// `blob_id`.
#[derive(Debug, Clone)]
pub struct DbLibraryImage {
    /// release_id for covers, artist_id for artist images
    pub id: String,
    /// The id of the coven blob holding this image's bytes — distinct from `id`,
    /// which names the subject (the release or artist) and never moves. A coven
    /// blob id names one immutable byte-string, so each stored image gets a fresh
    /// `blob_id`: replacing a cover repoints the row at a new blob and deletes the
    /// old one, rather than writing new bytes under a live id.
    pub blob_id: String,
    pub image_type: LibraryImageType,
    pub content_type: ContentType,
    pub file_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    /// "local", "musicbrainz", "discogs"
    pub source: String,
    /// MB: CAA image ID, Discogs: URL, local: "release://{path}"
    pub source_url: Option<String>,
    /// Cloud object key for this image's blob, relative to the namespace coven
    /// prepends (`covers` / `artist_images`), mirroring coven's
    /// `BlobRef.cloud_path`. `None` = the hashed-by-id layout used by opaque
    /// homes; `Some` = the readable key set when the image entered a browsable
    /// home (cover: `{album_id}/{release_id}/cover.{ext}`, artist:
    /// `{artist_id}/artist.{ext}`). Only the cloud key becomes readable — coven's
    /// local cache layout is unaffected.
    pub cloud_path: Option<String>,
    /// SHA-256 (lowercase hex) of this image's plaintext bytes — coven's
    /// author-signed content hash (see [`crate::util::fs::hash_bytes`]). Not
    /// optional, for the same reason as [`DbFile::content_hash`].
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
}

impl DbLibraryImage {
    /// A release's cover row, describing the bytes that will be stored as its
    /// blob. `bytes` is [`crate::util::cover::resize_cover`]'s output — the
    /// thumbnail itself, never the image it was made from — so `file_size`,
    /// `content_hash` and `content_type` are all derived from it here and cannot
    /// disagree with the blob. coven verifies the blob against `content_hash` on
    /// every cloud fetch, so a hash of any other bytes makes the cover
    /// unreadable on every other device.
    ///
    /// `content_type` is JPEG because the resize only ever emits JPEG.
    /// `cloud_path` is left `None`: it depends on the home's storage mode, and is
    /// set by whoever writes the row (the finalize transaction at import,
    /// `change_cover` from the row's own content type).
    ///
    /// `blob_id` is the id of the blob these bytes become, minted fresh by the
    /// caller for every stored image — a coven blob id names one immutable
    /// byte-string, so a cover that changes becomes a new blob rather than new
    /// bytes under the old id.
    pub fn cover(
        release_id: &str,
        blob_id: &str,
        source: &str,
        source_url: Option<String>,
        bytes: &[u8],
        now: DateTime<Utc>,
    ) -> Self {
        DbLibraryImage {
            id: release_id.to_string(),
            blob_id: blob_id.to_string(),
            image_type: LibraryImageType::Cover,
            content_type: ContentType::Jpeg,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source: source.to_string(),
            source_url,
            cloud_path: None,
            content_hash: crate::util::fs::hash_bytes(bytes),
            created_at: now,
        }
    }
}
