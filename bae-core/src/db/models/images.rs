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
    pub created_at: DateTime<Utc>,
}

impl DbLibraryImage {
    /// The row describing the resized JPEG bytes handed to coven. Each stored
    /// image has a fresh file id; coven calculates its hash while attaching it.
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
            created_at: now,
        }
    }
}
