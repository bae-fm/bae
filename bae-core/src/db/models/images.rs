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

/// A cover or artist image. The bytes are a coven app-provided file in the
/// `covers` or `artist_images` namespace (per `image_type`), attached to the
/// row keyed by `id`. coven fixes a new file reference on each attachment and
/// never rewrites stored bytes, so replacing an image attaches new bytes to the
/// same row.
#[derive(Debug, Clone)]
pub struct DbLibraryImage {
    /// release_id for covers, artist_id for artist images
    pub id: String,
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
    /// The row describing the resized JPEG bytes handed to coven, which
    /// calculates their hash while attaching them.
    pub fn cover(
        release_id: &str,
        source: &str,
        source_url: Option<String>,
        bytes: &[u8],
        now: DateTime<Utc>,
    ) -> Self {
        DbLibraryImage {
            id: release_id.to_string(),
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
