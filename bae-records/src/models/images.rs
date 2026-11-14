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
            LibraryImageType::Cover => crate::synced::COVERS_NAMESPACE,
            LibraryImageType::Artist => crate::synced::ARTIST_IMAGES_NAMESPACE,
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

/// A library image's reference: kind, subject id, and content version. The id is a
/// release id for a cover and an artist id for an artist image; the version is the
/// image's version (the coven file the row references, with the device that has
/// it while it is on one device only), so it moves whenever the image's bytes do
/// and again when such a file is stored. The UI passes the whole reference back to
/// read bytes, so core dispatches to the right table rather than probe every image
/// namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub id: String,
    pub version: String,
    pub image_type: LibraryImageType,
}

/// Where a gallery slot's bytes come from: a cover, read by its
/// [`ImageRef`], or one of the release's image files, read by its file id.
/// The library reads a slot's bytes by it, so a surface never picks a
/// source itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GallerySource {
    Cover(ImageRef),
    ReleaseFile { file_id: String },
}

/// The cover of the release `release_id` at `version`, as its row read it;
/// `None` when the release has no cover.
pub fn cover_ref(release_id: &str, version: Option<String>) -> Option<ImageRef> {
    version.map(|version| ImageRef {
        id: release_id.to_string(),
        version,
        image_type: LibraryImageType::Cover,
    })
}

/// The image of the artist `artist_id` at `version`, as its row read it;
/// `None` when the artist has no image.
pub fn artist_image_ref(artist_id: &str, version: Option<String>) -> Option<ImageRef> {
    version.map(|version| ImageRef {
        id: artist_id.to_string(),
        version,
        image_type: LibraryImageType::Artist,
    })
}

/// A cover or artist image. The bytes are a coven app-provided file in the
/// `covers` or `artist_images` namespace (per `image_type`), which the row
/// keyed by `id` references. A coven file never changes its bytes, so
/// replacing an image stages a new file and points the same row at it.
#[derive(Debug, Clone)]
pub struct DbLibraryImage {
    /// release_id for covers, artist_id for artist images
    pub id: String,
    pub image_type: LibraryImageType,
    pub content_type: ContentType,
}

impl DbLibraryImage {
    /// The row describing a release's cover, a JPEG
    /// [`bae_util::stored_image::stored_jpeg`] made.
    pub fn cover(release_id: &str) -> Self {
        Self::stored_jpeg(LibraryImageType::Cover, release_id)
    }

    /// The row describing the image of `image_type` kept for `id`, a JPEG
    /// [`bae_util::stored_image::stored_jpeg`] made, whose bytes the write
    /// stages with coven.
    pub fn stored_jpeg(image_type: LibraryImageType, id: &str) -> Self {
        DbLibraryImage { id: id.to_string(), image_type, content_type: ContentType::Jpeg }
    }
}
