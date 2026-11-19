//! A draft's artist assignments as a candidate's tables and a release
//! editor's form both read them: the kind of a track's artists, and the
//! statement that reads assignments back with their picked artists as the
//! library holds them.

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
                    // Deleting an artist deletes a candidate's assignments
                    // of it, and a form's reads take a picked artist the
                    // library no longer holds as a credit, so the artist a
                    // picked assignment names is in the library.
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
