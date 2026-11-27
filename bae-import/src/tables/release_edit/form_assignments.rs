//! A release editor's artist assignments as its form binds them to the
//! draft's reads: one JSON array per field.

use crate::{ArtistAssignment, Credited};

/// One artist assignment as a release editor's form binds it in the JSON
/// array that [`FormArtistFields`](super::artist_fields::FormArtistFields) reads, with
/// the track it credits (none
/// for the album's) and its places. A picked artist carries, beside its id
/// and picked credit name, the artist's values as the form holds them and
/// the name it prints: the credit the form's reads take it as once the
/// library no longer holds the artist.
#[derive(serde::Serialize)]
pub(super) struct FormAssignmentRow<'a> {
    track_id: Option<&'a str>,
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
    /// `credited` at `position`: the album's when `track_id` is `None`, else
    /// the track's of that id.
    fn of(track_id: Option<&'a str>, position: usize, credited: &'a Credited) -> Self {
        let join_phrase = credited.join.as_deref();
        match &credited.artist {
            ArtistAssignment::Picked { artist, credit_name } => Self {
                track_id,
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
/// `track_id` is `None`, else those of the track of that id.
pub(super) fn assignment_rows<'a>(
    track_id: Option<&'a str>,
    assignments: &'a [Credited],
) -> impl Iterator<Item = FormAssignmentRow<'a>> {
    assignments
        .iter()
        .enumerate()
        .map(move |(position, assignment)| FormAssignmentRow::of(track_id, position, assignment))
}
