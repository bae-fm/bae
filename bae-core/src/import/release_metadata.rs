use super::assemble::ArtistRef;
use super::{Catalog, ImportError};
use crate::pressing::Pressing;

/// Album facts can come from a release or its parent without claiming a
/// particular pressing or borrowing that parent's tracklist.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AlbumMetadata {
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub year: Option<i32>,
}

impl AlbumMetadata {
    pub(crate) fn fill_missing(&mut self, other: Self) {
        if self.title.trim().is_empty() {
            self.title = other.title;
        }
        self.year = self.year.or(other.year);
        if self.artists.is_empty() {
            self.artists = other.artists;
        } else {
            for artist in &mut self.artists {
                if let Some(linked) = other
                    .artists
                    .iter()
                    .find(|linked| linked.name.eq_ignore_ascii_case(&artist.name))
                {
                    artist.musicbrainz_artist_id = artist
                        .musicbrainz_artist_id
                        .take()
                        .or_else(|| linked.musicbrainz_artist_id.clone());
                    artist.discogs_artist_id = artist
                        .discogs_artist_id
                        .take()
                        .or_else(|| linked.discogs_artist_id.clone());
                }
            }
        }
    }

    pub(crate) fn take_primary(
        &mut self,
        catalog: Catalog,
        key: &str,
    ) -> Result<ArtistRef, ImportError> {
        if self.artists.is_empty() {
            return Err(ImportError::SourceData {
                catalog,
                detail: format!(
                    "{} release {key} has no album artist",
                    catalog.display_name()
                ),
            });
        }
        Ok(self.artists.remove(0))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReleaseMetadata {
    pub album: AlbumMetadata,
    pub pressing: Pressing,
}

impl ReleaseMetadata {
    pub(crate) fn fill_missing(&mut self, other: Self) {
        self.album.fill_missing(other.album);
        self.pressing.fill_missing(other.pressing);
    }
}
