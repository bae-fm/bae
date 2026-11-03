use super::*;

fn artist(name: &str, musicbrainz_artist_id: Option<&str>) -> SourceCredit {
    SourceCredit::plain(crate::credits::ArtistCredit {
        musicbrainz_artist_id: musicbrainz_artist_id.map(str::to_string),
        ..crate::credits::ArtistCredit::named(name)
    })
}

fn album(artists: Vec<SourceCredit>) -> AlbumMetadata {
    AlbumMetadata { title: "Album Title".to_string(), artists, year: None, first_year: None }
}

/// A linked document's artist fills in the ids of the artist it names,
/// however either writes the name's case or accents.
#[test]
fn a_linked_artist_fills_in_the_ids_of_the_artist_it_names() {
    let mut own = album(vec![artist("Ärtist Name", None)]);
    own.fill_missing(album(vec![artist("ARTIST NAME", Some("mb-artist-1"))]));
    assert_eq!(own.artists[0].credit.musicbrainz_artist_id.as_deref(), Some("mb-artist-1"));

    let mut other = album(vec![artist("Other Name", None)]);
    other.fill_missing(album(vec![artist("Artist Name", Some("mb-artist-1"))]));
    assert_eq!(other.artists[0].credit.musicbrainz_artist_id, None);
}
