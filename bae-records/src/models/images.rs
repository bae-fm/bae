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

/// An artist's image, as its row records it. The bytes are a coven
/// app-provided file in the `artist_images` namespace, which the row keyed by
/// the artist's id references. A coven file never changes its bytes, so
/// replacing an image stages a new file and points the same row at it.
#[derive(Debug, Clone)]
pub struct DbArtistImage {
    pub artist_id: String,
    pub content_type: ContentType,
}

impl DbArtistImage {
    /// The row describing the image kept for `artist_id`, a JPEG
    /// [`bae_util::stored_image::stored_jpeg`] made, whose bytes the write
    /// stages with coven.
    pub fn stored_jpeg(artist_id: &str) -> Self {
        DbArtistImage { artist_id: artist_id.to_string(), content_type: ContentType::Jpeg }
    }
}

/// What a cover is made from, as its row records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverSource {
    /// One of the release's image files, by its release file id.
    ReleaseFile { file_id: String },
    /// A picture of the cover's own, kept as its original: a catalog's
    /// image or art embedded in one of the release's audio files.
    Original,
}

/// The copy of a cover a slot draws: the browse copy for grids and rows, the
/// large copy for the album panel, the full player and Now Playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverCopy {
    Browse,
    Large,
}

impl CoverCopy {
    /// The step the copy is made at.
    pub fn step(self) -> bae_pictures::PictureStep {
        match self {
            Self::Browse => bae_pictures::PictureStep::S768,
            Self::Large => bae_pictures::PictureStep::S1536,
        }
    }

    /// The smaller copy that holds a slot `pixels` wide on its longer side,
    /// or the large copy for a slot above the browse copy.
    pub fn covering(pixels: u32) -> Self {
        if pixels <= u32::from(Self::Browse.step().edge()) {
            Self::Browse
        } else {
            Self::Large
        }
    }
}

/// A cover to write: the picture it is made from and its two copies, which
/// the caller made from that picture.
#[derive(Debug, Clone)]
pub struct NewCover {
    pub release_id: String,
    pub picture: NewCoverPicture,
    pub browse: Vec<u8>,
    pub large: Vec<u8>,
    /// The format of both copies.
    pub content_type: ContentType,
}

/// The picture a new cover is made from.
#[derive(Debug, Clone)]
pub enum NewCoverPicture {
    /// One of the release's image files, by its release file id.
    ReleaseFile { file_id: String },
    /// A picture the cover keeps as its own original: these bytes.
    Original { bytes: Vec<u8> },
}

/// A preview of one image file of a release, as the import writes it: the
/// release file it is made from, and the copy's bytes and format.
#[derive(Debug, Clone)]
pub struct NewReleasePreview {
    /// The id of the release file the preview is made from.
    pub release_file_id: String,
    pub bytes: Vec<u8>,
    /// A picture with transparency makes a PNG, any other a JPEG.
    pub content_type: ContentType,
}

impl NewReleasePreview {
    /// The step a preview is made at.
    pub const STEP: bae_pictures::PictureStep = bae_pictures::PictureStep::S384;
}
