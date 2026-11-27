//! A draft's artist assignments as a candidate's tables and a release
//! editor's form both read them: the kind of a track's artists, and the
//! assignments a read gives back with their picked artists as the library
//! holds them.

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

/// The credits a JSON array of [`StoredAssignment`]s holds, in order.
pub(crate) fn assignments_of(json: &str, what: &str) -> Result<Vec<Credited>, DbError> {
    decoded::<Vec<StoredAssignment>>(json, what)?
        .into_iter()
        .map(StoredAssignment::credited)
        .collect()
}

/// One stored assignment row, with the picked artist's fields as the
/// library holds them and its credit name, if it names one still there.
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
