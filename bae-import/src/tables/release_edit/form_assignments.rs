//! A release editor's artist assignments as its form binds them to the
//! draft's statements: one JSON array per field, and the subquery that reads
//! it with the columns of a draft's assignment table.

use crate::{ArtistAssignment, Credited};

/// One artist assignment as a release editor's form binds it in the JSON
/// array that [`assignment_rows_sql`] reads, with the track it credits (none
/// for the album's) and its places. A picked artist carries, beside its id
/// and picked credit name, the artist's values as the form holds them and
/// the name it prints: the credit the form's reads take it as once the
/// library no longer holds the artist.
#[derive(serde::Serialize)]
pub(super) struct FormAssignmentRow<'a> {
    track_id: Option<&'a str>,
    track_position: usize,
    position: usize,
    artist_id: Option<&'a str>,
    credit_key: Option<&'a str>,
    name: &'a str,
    sort_name: Option<&'a str>,
    musicbrainz_artist_id: Option<&'a str>,
    discogs_artist_id: Option<&'a str>,
    credited_as: Option<&'a str>,
    join_phrase: Option<&'a str>,
}

impl<'a> FormAssignmentRow<'a> {
    /// `credited` at `position`: the album's when `track` is `None`, else
    /// the track's of that id at that place.
    fn of(track: Option<(&'a str, usize)>, position: usize, credited: &'a Credited) -> Self {
        let join_phrase = credited.join.as_deref();
        let (track_id, track_position) = track.map_or((None, 0), |(id, place)| (Some(id), place));
        match &credited.artist {
            ArtistAssignment::Picked { artist, credit_name } => Self {
                track_id,
                track_position,
                position,
                artist_id: Some(&artist.artist_id),
                credit_key: credit_name.as_ref().map(|credit_name| credit_name.key.as_str()),
                name: &artist.name,
                sort_name: artist.sort_name.as_deref(),
                musicbrainz_artist_id: artist.musicbrainz_artist_id.as_deref(),
                discogs_artist_id: artist.discogs_artist_id.as_deref(),
                credited_as: credit_name.as_ref().map(|credit_name| credit_name.name.as_str()),
                join_phrase,
            },
            ArtistAssignment::Credit { credit } => Self {
                track_id,
                track_position,
                position,
                artist_id: None,
                credit_key: None,
                name: &credit.name,
                sort_name: credit.sort_name.as_deref(),
                musicbrainz_artist_id: credit.musicbrainz_artist_id.as_deref(),
                discogs_artist_id: credit.discogs_artist_id.as_deref(),
                credited_as: credit.credited_as.as_deref(),
                join_phrase,
            },
        }
    }
}

/// `assignments` as [`FormAssignmentRow`]s in their order: the album's when
/// `track` is `None`, else those of the track of that id at that place.
pub(super) fn assignment_rows<'a>(
    track: Option<(&'a str, usize)>,
    assignments: &'a [Credited],
) -> impl Iterator<Item = FormAssignmentRow<'a>> {
    assignments
        .iter()
        .enumerate()
        .map(move |(position, assignment)| FormAssignmentRow::of(track, position, assignment))
}

/// A subquery over the JSON array of [`FormAssignmentRow`]s that the
/// parameter `param` binds, with the columns of a draft's assignment table
/// and `track_id` and `track_position`. A picked artist the library no
/// longer holds is a credit of the values the form holds for it.
pub(crate) fn assignment_rows_sql(param: &str) -> String {
    format!(
        "(SELECT track_id, track_position, position, \
                 CASE WHEN held THEN 'picked' ELSE 'credit' END AS assignment_kind, \
                 CASE WHEN held THEN artist_id END AS artist_id, \
                 name, sort_name, musicbrainz_artist_id, discogs_artist_id, \
                 CASE WHEN held THEN credit_key END AS credit_key, \
                 CASE WHEN held THEN NULL ELSE credited_as END AS credited_as, \
                 join_phrase \
          FROM (SELECT j.value ->> 'track_id' AS track_id, \
                       j.value ->> 'track_position' AS track_position, \
                       j.value ->> 'position' AS position, \
                       j.value ->> 'artist_id' AS artist_id, \
                       j.value ->> 'name' AS name, j.value ->> 'sort_name' AS sort_name, \
                       j.value ->> 'musicbrainz_artist_id' AS musicbrainz_artist_id, \
                       j.value ->> 'discogs_artist_id' AS discogs_artist_id, \
                       j.value ->> 'credit_key' AS credit_key, \
                       j.value ->> 'credited_as' AS credited_as, \
                       j.value ->> 'join_phrase' AS join_phrase, \
                       EXISTS (SELECT 1 FROM artists AS library_artist \
                               WHERE library_artist.id = j.value ->> 'artist_id') AS held \
                FROM json_each({param}) AS j))"
    )
}
