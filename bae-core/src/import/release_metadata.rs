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
    /// The year the album first came out, as its MusicBrainz release group's
    /// first release date or its Discogs master's year states it — never a
    /// pressing's year. `None` when no album document states it.
    pub first_year: Option<i32>,
}

impl AlbumMetadata {
    pub(crate) fn fill_missing(&mut self, other: Self) {
        if self.title.trim().is_empty() {
            self.title = other.title;
        }
        self.year = self.year.or(other.year);
        self.first_year = self.first_year.or(other.first_year);
        if self.artists.is_empty() {
            self.artists = other.artists;
        } else {
            for artist in &mut self.artists {
                if let Some(linked) = other
                    .artists
                    .iter()
                    .find(|linked| crate::text_match::same_artist_name(&linked.name, &artist.name))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn artist(name: &str, musicbrainz_artist_id: Option<&str>) -> ArtistRef {
        ArtistRef {
            name: name.to_string(),
            sort_name: None,
            musicbrainz_artist_id: musicbrainz_artist_id.map(str::to_string),
            discogs_artist_id: None,
        }
    }

    fn album(artists: Vec<ArtistRef>) -> AlbumMetadata {
        AlbumMetadata {
            title: "Album Title".to_string(),
            artists,
            year: None,
            first_year: None,
        }
    }

    /// A linked document's artist fills in the ids of the artist it names,
    /// however either writes the name's case or accents.
    #[test]
    fn a_linked_artist_fills_in_the_ids_of_the_artist_it_names() {
        let mut own = album(vec![artist("Ärtist Name", None)]);
        own.fill_missing(album(vec![artist("ARTIST NAME", Some("mb-artist-1"))]));
        assert_eq!(
            own.artists[0].musicbrainz_artist_id.as_deref(),
            Some("mb-artist-1")
        );

        let mut other = album(vec![artist("Other Name", None)]);
        other.fill_missing(album(vec![artist("Artist Name", Some("mb-artist-1"))]));
        assert_eq!(other.artists[0].musicbrainz_artist_id, None);
    }
}
