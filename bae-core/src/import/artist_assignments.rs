//! Project parsed source artists into editor assignments: each is a credit
//! carrying what the source said, its provider IDs included. A parser's
//! temporary artist ID is never taken for a library ID, and no credit claims
//! a library artist — that is read against the library when the draft is.

use crate::db::{DbAlbumArtist, DbArtist, DbTrackArtist};
use crate::import::{ArtistAssignment, ArtistCredit, TrackArtistAssignments};

pub(crate) fn album_artist_assignments(
    artists: &[DbArtist],
    album_artists: &[DbAlbumArtist],
    primary_artist_id: &str,
) -> Result<Vec<ArtistAssignment>, String> {
    let mut assignments = vec![source_credit(artist_of(artists, primary_artist_id)?)];
    let mut junction: Vec<&DbAlbumArtist> = album_artists.iter().collect();
    junction.sort_by_key(|assignment| assignment.position);
    for assignment in junction {
        assignments.push(source_credit(artist_of(artists, &assignment.artist_id)?));
    }
    Ok(assignments)
}

pub(crate) fn track_artist_assignments(
    artists: &[DbArtist],
    track_artists: &[DbTrackArtist],
    track_id: &str,
) -> Result<TrackArtistAssignments, String> {
    let mut credits: Vec<&DbTrackArtist> = track_artists
        .iter()
        .filter(|credit| credit.track_id == track_id)
        .collect();
    credits.sort_by_key(|credit| credit.position);
    if credits.is_empty() {
        return Ok(TrackArtistAssignments::AlbumArtists);
    }
    credits
        .into_iter()
        .map(|credit| artist_of(artists, &credit.artist_id).map(source_credit))
        .collect::<Result<Vec<_>, _>>()
        .map(TrackArtistAssignments::Explicit)
}

fn artist_of<'a>(artists: &'a [DbArtist], id: &str) -> Result<&'a DbArtist, String> {
    artists
        .iter()
        .find(|artist| artist.id == id)
        .ok_or_else(|| id.to_string())
}

fn source_credit(artist: &DbArtist) -> ArtistAssignment {
    ArtistAssignment::Credit { credit: ArtistCredit {
            name: artist.name.clone(),
            sort_name: artist.sort_name.clone(),
            musicbrainz_artist_id: artist.musicbrainz_artist_id.clone(),
            discogs_artist_id: artist.discogs_artist_id.clone(),
        },
    }
}
