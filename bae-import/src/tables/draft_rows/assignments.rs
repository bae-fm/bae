//! A draft's artist assignments, as a candidate's tables and a release
//! editor's form both hold them: the columns of one assignment, the JSON a
//! form binds its assignments as, and the statement that reads assignments
//! back with their picked artists as the library holds them.

use crate::{ArtistAssignment, Credited, TrackArtistAssignments};
use bae_catalogs::credits::ArtistCredit;
use bae_records::ExistingArtist;
use bae_store::DbError;

pub(crate) fn assignment_kind_column(assignments: &TrackArtistAssignments) -> &'static str {
    match assignments {
        TrackArtistAssignments::AlbumArtists => "album_artists",
        TrackArtistAssignments::Explicit(_) => "explicit",
    }
}

/// Decode `json`, a stored JSON column that holds `what`.
pub(crate) fn decoded<T: serde::de::DeserializeOwned>(
    json: &str,
    what: &str,
) -> Result<T, DbError> {
    serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json(format!("a draft's stored {what}"), json, error))
}

/// The credits a JSON array of [`assignments_json_sql`] holds, in order.
pub(crate) fn assignments_of(json: &str, what: &str) -> Result<Vec<Credited>, DbError> {
    decoded::<Vec<StoredAssignment>>(json, what)?
        .into_iter()
        .map(StoredAssignment::credited)
        .collect()
}

/// One stored assignment row, with the picked artist's fields as the
/// library holds them and its credit name, if it names one still there, as
/// [`assignments_json_sql`] holds it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredAssignment {
    kind: String,
    artist_id: Option<String>,
    name: Option<String>,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credit_key: Option<String>,
    credit_name: Option<String>,
    credited_as: Option<String>,
    join_phrase: Option<String>,
}

/// The rows of `table`, aliased `assignment`, that match the SQL condition
/// `belongs`, as a JSON array in their order: each joined to its picked
/// artist and that artist's credit name.
pub(crate) fn assignments_json_sql(table: &str, belongs: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'kind', assignment.assignment_kind, 'artist_id', assignment.artist_id, \
             'name', CASE assignment.assignment_kind WHEN 'picked' THEN picked.name \
                 WHEN 'credit' THEN assignment.name END, \
             'sort_name', CASE assignment.assignment_kind WHEN 'picked' THEN picked.sort_name \
                 WHEN 'credit' THEN assignment.sort_name END, \
             'musicbrainz_artist_id', CASE assignment.assignment_kind \
                 WHEN 'picked' THEN picked.musicbrainz_artist_id \
                 WHEN 'credit' THEN assignment.musicbrainz_artist_id END, \
             'discogs_artist_id', CASE assignment.assignment_kind \
                 WHEN 'picked' THEN picked.discogs_artist_id \
                 WHEN 'credit' THEN assignment.discogs_artist_id END, \
             'credit_key', picked_credit.name_key, 'credit_name', picked_credit.name, \
             'credited_as', assignment.credited_as, 'join_phrase', assignment.join_phrase) \
             ORDER BY assignment.position) \
         FROM {table} AS assignment \
         LEFT JOIN artists picked ON picked.id = assignment.artist_id \
         {credit_name} \
         WHERE {belongs})",
        credit_name = bae_records::records::read::credit_name_join("assignment", "picked_credit"),
    )
}

impl StoredAssignment {
    /// The draft's credit. When the library no longer holds the credit name
    /// the person picked for an artist, the credit has no credit name and
    /// shows the artist's own name.
    pub(crate) fn credited(self) -> Result<Credited, DbError> {
        let artist = match self.kind.as_str() {
            "picked" => ArtistAssignment::Picked {
                artist: ExistingArtist {
                    artist_id: self.artist_id.ok_or_else(|| {
                        DbError::unreadable(
                            "the artist_id of a picked artist assignment",
                            None::<String>,
                        )
                    })?,
                    // Deleting an artist deletes its assignments, so the
                    // artist an assignment names is in the library.
                    name: self.name.ok_or_else(|| {
                        DbError::unreadable(
                            "the name of a picked assignment's artist",
                            None::<String>,
                        )
                    })?,
                    sort_name: self.sort_name,
                    musicbrainz_artist_id: self.musicbrainz_artist_id,
                    discogs_artist_id: self.discogs_artist_id,
                },
                credit_name: match (self.credit_key, self.credit_name) {
                    (Some(key), Some(name)) => Some(bae_records::credits::CreditName { key, name }),
                    (None, None) => None,
                    (key, name) => {
                        return Err(DbError::unreadable(
                            "a stored artist assignment's credit name and key",
                            (key, name),
                        ))
                    }
                },
            },
            "credit" => ArtistAssignment::Credit {
                credit: ArtistCredit {
                    name: self.name.ok_or_else(|| {
                        DbError::unreadable(
                            "the name of an artist credit assignment",
                            None::<String>,
                        )
                    })?,
                    sort_name: self.sort_name,
                    musicbrainz_artist_id: self.musicbrainz_artist_id,
                    discogs_artist_id: self.discogs_artist_id,
                    credited_as: self.credited_as,
                },
            },
            other => return Err(DbError::unreadable("a stored artist assignment's kind", other)),
        };
        Ok(Credited { artist, join: self.join_phrase })
    }
}

/// One artist assignment as a draft's assignment table holds it: the
/// columns a candidate's table stores it in, and a release editor's form
/// binds in a JSON array that [`assignment_rows_sql`] reads, with the track
/// it credits (none for the album's) and its places.
#[derive(serde::Serialize)]
pub(crate) struct AssignmentRow<'a> {
    track_id: Option<&'a str>,
    track_position: usize,
    position: usize,
    pub(crate) assignment_kind: &'static str,
    pub(crate) artist_id: Option<&'a str>,
    pub(crate) name: Option<&'a str>,
    pub(crate) sort_name: Option<&'a str>,
    pub(crate) musicbrainz_artist_id: Option<&'a str>,
    pub(crate) discogs_artist_id: Option<&'a str>,
    pub(crate) credit_key: Option<&'a str>,
    pub(crate) credited_as: Option<&'a str>,
    pub(crate) join_phrase: Option<&'a str>,
}

impl<'a> AssignmentRow<'a> {
    /// `credited` at `position`: the album's when `track` is `None`, else
    /// the track's of that id at that place.
    pub(crate) fn of(
        track: Option<(&'a str, usize)>,
        position: usize,
        credited: &'a Credited,
    ) -> Self {
        let join_phrase = credited.join.as_deref();
        let (track_id, track_position) = track.map_or((None, 0), |(id, place)| (Some(id), place));
        match &credited.artist {
            ArtistAssignment::Picked { artist, credit_name } => Self {
                track_id,
                track_position,
                position,
                assignment_kind: "picked",
                artist_id: Some(&artist.artist_id),
                name: None,
                sort_name: None,
                musicbrainz_artist_id: None,
                discogs_artist_id: None,
                credit_key: credit_name.as_ref().map(|credit_name| credit_name.key.as_str()),
                credited_as: None,
                join_phrase,
            },
            ArtistAssignment::Credit { credit } => Self {
                track_id,
                track_position,
                position,
                assignment_kind: "credit",
                artist_id: None,
                name: Some(credit.name.as_str()),
                sort_name: credit.sort_name.as_deref(),
                musicbrainz_artist_id: credit.musicbrainz_artist_id.as_deref(),
                discogs_artist_id: credit.discogs_artist_id.as_deref(),
                credit_key: None,
                credited_as: credit.credited_as.as_deref(),
                join_phrase,
            },
        }
    }
}

/// `assignments` as [`AssignmentRow`]s in their order: the album's when
/// `track` is `None`, else those of the track of that id at that place.
pub(crate) fn assignment_rows<'a>(
    track: Option<(&'a str, usize)>,
    assignments: &'a [Credited],
) -> impl Iterator<Item = AssignmentRow<'a>> {
    assignments
        .iter()
        .enumerate()
        .map(move |(position, assignment)| AssignmentRow::of(track, position, assignment))
}

/// A subquery over the JSON array of [`AssignmentRow`]s that the
/// parameter `param` binds, with the columns of a draft's assignment table
/// and `track_id` and `track_position`.
pub(crate) fn assignment_rows_sql(param: &str) -> String {
    format!(
        "(SELECT j.value ->> 'track_id' AS track_id, \
                 j.value ->> 'track_position' AS track_position, \
                 j.value ->> 'position' AS position, \
                 j.value ->> 'assignment_kind' AS assignment_kind, \
                 j.value ->> 'artist_id' AS artist_id, j.value ->> 'name' AS name, \
                 j.value ->> 'sort_name' AS sort_name, \
                 j.value ->> 'musicbrainz_artist_id' AS musicbrainz_artist_id, \
                 j.value ->> 'discogs_artist_id' AS discogs_artist_id, \
                 j.value ->> 'credit_key' AS credit_key, \
                 j.value ->> 'credited_as' AS credited_as, \
                 j.value ->> 'join_phrase' AS join_phrase \
          FROM json_each({param}) AS j)"
    )
}
