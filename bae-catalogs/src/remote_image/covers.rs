//! The covers catalogs offer a release: where each image and its downscaled
//! copies live, whether a catalog stated it is there, and which of them a
//! surface offers.

use crate::catalog::Catalog;

/// Where the Cover Art Archive serves images from. Every path under it is fixed
/// by the entity's MusicBrainz id, so an image's address is knowable without
/// asking the archive anything.
pub const ARCHIVE: &str = "https://coverartarchive.org";

/// A remote cover art option from an external source: where the image and its
/// downscaled copies live, and which service is offering them.
///
/// Its `standing` says whether a catalog stated the image is there. For the
/// Cover Art Archive the address is derived from the entity id, so it exists
/// before anything is known: a MusicBrainz release document's own
/// `cover-art-archive` block states whether the archive serves its front, and
/// nothing states it for a search result or a release group.
///
/// A record's cover is one of three things, read off
/// `MetadataResult::cover_art`: a stated cover (the record has one), an
/// unstated address (the record said nothing), or `None` (the record states
/// it has none). [`offered_covers`] is the one rule every surface that picks
/// among covers applies.
///
/// `Serialize`/`Deserialize`: reachable from `MetadataResult::cover_art`, which
/// `identify::TerminalVerdict` persists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemoteCover {
    pub image: RemoteImageSet,
    pub label: CoverLabel,
    pub source: Catalog,
    pub standing: CoverStanding,
}

/// What a catalog offers a cover as, left for each surface to word in its
/// own language: core sends no prose about a cover.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CoverLabel {
    /// The catalog's lead image of one entity: a Discogs search result's
    /// image, a MusicBrainz release's front image, or (`release_group`) the
    /// front image of the release group the album belongs to.
    Lead { release_group: bool },
    /// One image of the catalog's own gallery of an entity, numbered from 1
    /// in the order the catalog lists them.
    Gallery {
        /// How the catalog refers to the entity the gallery belongs to: a
        /// Discogs release `r123` or master `m456`. `None` for the Cover Art
        /// Archive, which a cover's `source` MusicBrainz already names.
        reference: Option<String>,
        position: u32,
        types: Vec<CoverImageType>,
        comment: String,
    },
}

/// What a Cover Art Archive image shows, by the archive's own vocabulary.
/// A name the archive adds later reads as `Other` until bae lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CoverImageType {
    Front,
    Back,
    Booklet,
    Medium,
    Obi,
    Spine,
    Track,
    Liner,
    Sticker,
    Poster,
    Watermark,
    #[serde(rename = "Raw/Unedited")]
    Raw,
    #[serde(rename = "Matrix/Runout")]
    Matrix,
    Top,
    Bottom,
    Tray,
    #[serde(other)]
    Other,
}

/// Whether a catalog stated a cover is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CoverStanding {
    /// A catalog's document lists this image: a Discogs image, an archive
    /// gallery entry, a MusicBrainz release whose document says the archive
    /// serves its front.
    Stated,
    /// An address nothing has said anything about: the archive's front for a
    /// MusicBrainz search result or release group. It may hold nothing.
    Unstated,
}

/// How a standing is stored. Only the desktop's candidate and source-release
/// tables store covers.
#[cfg(desktop)]
impl CoverStanding {
    /// The stored column value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stated => "stated",
            Self::Unstated => "unstated",
        }
    }

    /// The standing a stored column value names.
    pub fn from_column(value: &str) -> Option<Self> {
        match value {
            "stated" => Some(Self::Stated),
            "unstated" => Some(Self::Unstated),
            _ => None,
        }
    }
}

/// The covers to offer from `covers`, in their order: every one a catalog
/// stated, or — only when none is stated — the unstated addresses. A record
/// that states it has no cover contributes none, so it is never offered.
///
/// Every surface that picks a cover applies this: a card's heading, a pick's
/// default cover, and the cover options a pick offers.
pub fn offered_covers(covers: impl IntoIterator<Item = RemoteCover>) -> Vec<RemoteCover> {
    let (stated, unstated): (Vec<_>, Vec<_>) =
        covers.into_iter().partition(|cover| cover.standing == CoverStanding::Stated);
    if stated.is_empty() {
        unstated
    } else {
        stated
    }
}

/// The cover to show for `covers`: the first [`offered_covers`] offers.
pub fn preferred_cover(covers: impl IntoIterator<Item = RemoteCover>) -> Option<RemoteCover> {
    offered_covers(covers).into_iter().next()
}

/// One catalog image: the original, and every downscaled copy the catalog
/// serves of it with the size that copy is bounded to.
///
/// Which copy a slot draws follows from the slot's size, through
/// [`RemoteImageSet::url_covering`] — never from a caller naming a thumbnail —
/// so a large slot can never be handed a copy too small for it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemoteImageSet {
    /// The original: the largest image the catalog serves, and the one an
    /// import commits.
    pub url: String,
    /// Smaller copies, one per box size.
    pub downscaled: Vec<DownscaledCopy>,
}

/// A copy of a catalog image scaled to fit in a `max_edge` × `max_edge` box:
/// its longer side is at most `max_edge` pixels. A catalog never scales up, so
/// a copy of an image smaller than the box is the image at its own size.
///
/// A copy is there exactly when its image is — each catalog serves its copies
/// of every image it holds — so it carries no standing of its own: the
/// cover's [`CoverStanding`] is the copies' too.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DownscaledCopy {
    pub url: String,
    pub max_edge: u32,
}

impl RemoteImageSet {
    /// An image the catalog serves at one size only.
    pub fn original(url: String) -> Self {
        Self { url, downscaled: Vec::new() }
    }

    /// The image with its downscaled copies, one per box size, smallest first.
    pub fn with_copies(url: String, mut downscaled: Vec<DownscaledCopy>) -> Self {
        downscaled.sort_by_key(|copy| copy.max_edge);
        downscaled.dedup_by_key(|copy| copy.max_edge);
        Self { url, downscaled }
    }

    /// Where to read the image for a slot `pixels` wide on its longer side:
    /// the smallest copy that still fills it, or the original when no copy
    /// does. `None` asks for the original outright — a viewer that zooms to
    /// the image's own resolution.
    pub fn url_covering(&self, pixels: Option<u32>) -> &str {
        let Some(pixels) = pixels else {
            return &self.url;
        };
        self.downscaled
            .iter()
            .filter(|copy| copy.max_edge >= pixels)
            .min_by_key(|copy| copy.max_edge)
            .map_or(&self.url, |copy| &copy.url)
    }
}

impl RemoteCover {
    /// The archive's address for a MusicBrainz release's front image — this
    /// pressing's own cover, if the archive holds one. Unstated: only the
    /// release's document says whether it does ([`musicbrainz_release_cover`]).
    pub fn musicbrainz_release(release_id: &str) -> Self {
        Self::cover_art_archive("release", release_id, CoverLabel::Lead { release_group: false })
    }

    /// The archive's address for a MusicBrainz release group's front image —
    /// the cover the album is represented by, which is some release in the
    /// group's and may not be this pressing's. Unstated: no document says
    /// whether the archive holds one.
    pub fn musicbrainz_release_group(release_group_id: &str) -> Self {
        Self::cover_art_archive(
            "release-group",
            release_group_id,
            CoverLabel::Lead { release_group: true },
        )
    }

    fn cover_art_archive(entity: &str, id: &str, label: CoverLabel) -> Self {
        let front = format!("{ARCHIVE}/{entity}/{id}/front");
        let downscaled = ARCHIVE_COPY_EDGES
            .iter()
            .map(|&max_edge| DownscaledCopy { url: format!("{front}-{max_edge}"), max_edge })
            .collect();
        Self {
            image: RemoteImageSet::with_copies(front, downscaled),
            label,
            source: Catalog::MusicBrainz,
            standing: CoverStanding::Unstated,
        }
    }
}

/// The bounding boxes the Cover Art Archive serves every image's copies at:
/// `front-250`, `front-500`, `front-1200` beside `front`. The archive's API
/// serves these for every image it holds (and each gallery entry lists them
/// under `thumbnails`), so a derived copy exists exactly when its image does:
/// the copies share the cover's [`CoverStanding`]. A copy that answers with
/// nothing is an image that is not there, not a copy missing beside it.
const ARCHIVE_COPY_EDGES: [u32; 3] = [250, 500, 1200];

/// This pressing's own front image, offered only when the release document
/// says the archive serves one — that block is the release's own statement,
/// so nothing has to be asked.
#[cfg(desktop)]
pub fn musicbrainz_release_cover(
    response: &crate::musicbrainz::MbReleaseResponse,
) -> Option<RemoteCover> {
    response.has_front_cover().then(|| RemoteCover {
        standing: CoverStanding::Stated,
        ..RemoteCover::musicbrainz_release(&response.id)
    })
}

/// The album this release belongs to, as the archive addresses it.
///
/// No statement anywhere in MusicBrainz's data says whether the archive holds
/// an image there — a release group document carries no `cover-art-archive`
/// block — so the address is offered and the fetch is what answers. That is
/// why it is an album option and never a pressing's: it may be some other
/// release's cover, and it may be nothing at all.
#[cfg(desktop)]
pub fn musicbrainz_album_cover(
    response: &crate::musicbrainz::MbReleaseResponse,
) -> Option<RemoteCover> {
    response.release_group.as_ref().map(|group| RemoteCover::musicbrainz_release_group(&group.id))
}

/// Append `cover` unless the list already offers the same image. Two identity
/// rows on one release can name the same archive entity, and the picker should
/// show that image once — stated, when either of the two stated it.
#[cfg(desktop)]
pub fn push_unique_cover(covers: &mut Vec<RemoteCover>, cover: RemoteCover) {
    match covers.iter_mut().find(|existing| existing.image.url == cover.image.url) {
        Some(existing) if cover.standing == CoverStanding::Stated => {
            existing.standing = CoverStanding::Stated;
        }
        Some(_) => {}
        None => covers.push(cover),
    }
}

#[cfg(test)]
#[path = "covers_tests.rs"]
mod tests;
