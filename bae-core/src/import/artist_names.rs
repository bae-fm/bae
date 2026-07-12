//! The artist-name projection, in one place because two sides must agree on it
//! exactly.
//!
//! `parsed_album_to_user_edit` projects artists *into* the editor;
//! `apply_user_edit_to_seed` rebuilds the same lists to compare the editor's
//! result *against*, and treats a mismatch as "the user changed the artists" —
//! which clears the junction rows and re-mints artists through `ensure_artist`,
//! losing the `musicbrainz_artist_id` / `discogs_artist_id` the mapper set. So a
//! drift between the two projections silently strips source ids off artists the
//! user never touched. Sharing the code is what stops that being possible.

use crate::db::{DbAlbumArtist, DbArtist, DbTrackArtist};

/// A junction row pointing at an artist that isn't in `artists`.
#[derive(Debug)]
pub(crate) struct MissingArtist(pub(crate) String);

/// Album artist names: the primary first, then the junction rows by position.
pub(crate) fn album_artist_names(
    artists: &[DbArtist],
    album_artists: &[DbAlbumArtist],
    primary_artist_id: &str,
) -> Result<Vec<String>, MissingArtist> {
    let mut names = vec![name_of(artists, primary_artist_id)?];
    let mut junction: Vec<&DbAlbumArtist> = album_artists.iter().collect();
    junction.sort_by_key(|aa| aa.position);
    for aa in junction {
        names.push(name_of(artists, &aa.artist_id)?);
    }
    Ok(names)
}

/// A track's credited artists, by position.
pub(crate) fn track_artist_names(
    artists: &[DbArtist],
    track_artists: &[DbTrackArtist],
    track_id: &str,
) -> Result<Vec<String>, MissingArtist> {
    let mut credits: Vec<&DbTrackArtist> = track_artists
        .iter()
        .filter(|ta| ta.track_id == track_id)
        .collect();
    credits.sort_by_key(|ta| ta.position);
    credits
        .into_iter()
        .map(|ta| name_of(artists, &ta.artist_id))
        .collect()
}

fn name_of(artists: &[DbArtist], id: &str) -> Result<String, MissingArtist> {
    artists
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.name.clone())
        .ok_or_else(|| MissingArtist(id.to_string()))
}
