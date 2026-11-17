//! The form's values a draft holds beside its own row (its label and
//! barcode rows, its album and track artist assignments) and its mediums, as
//! the statements that write and read them in the [`DraftTables`] of the
//! owner that holds them.

use super::DraftTables;
use crate::{ArtistAssignment, Credited, RawLabelEdit, RawReleaseEdit, TrackArtistAssignments};
use bae_catalogs::credits::ArtistCredit;
use bae_records::ExistingArtist;
use bae_store::DbError;
use coven::CovenWrite;
use rusqlite::{params, ToSql};

/// The columns an owner's row holds the form's album-level fields in.
pub(crate) const FORM_COLUMNS: &str =
    "album_title, album_year, year, country, region, media, status, packaging, discogs_details";

/// The values of [`FORM_COLUMNS`] for `form`, in their order.
pub(crate) fn form_row_values(form: &RawReleaseEdit) -> Vec<rusqlite::types::Value> {
    use rusqlite::types::Value;
    let facts = bae_records::records::pressing_columns::FactColumns::of(&form.pressing.facts);
    let optional = |value: Option<&str>| value.map_or(Value::Null, |v| Value::Text(v.to_string()));
    vec![
        Value::Text(form.album_title.clone()),
        Value::Text(form.album_year.clone()),
        Value::Text(form.pressing.year.clone()),
        optional(facts.country),
        optional(facts.region),
        Value::Text(bae_records::records::pressing_columns::media_column(
            &form.pressing.facts.media,
        )),
        optional(facts.status),
        optional(facts.packaging),
        Value::Text(facts.discogs_details),
    ]
}

/// Insert the draft's label rows, as the form holds them, where the draft
/// holds none.
pub(crate) fn insert_label_rows(
    sql: &CovenWrite<'_>,
    tables: &DraftTables,
    key: &dyn ToSql,
    labels: &[RawLabelEdit],
) -> Result<(), DbError> {
    for (position, label) in labels.iter().enumerate() {
        sql.execute(
            &format!(
                "INSERT INTO {table} \
                     ({key}, position, name, catalog_number, \
                      musicbrainz_label_id, discogs_label_id) VALUES (?, ?, ?, ?, ?, ?)",
                table = tables.label,
                key = tables.key,
            ),
            params![
                key,
                position as i64,
                label.name,
                label.catalog_number,
                label.musicbrainz_label_id,
                label.discogs_label_id
            ],
        )?;
    }
    Ok(())
}

/// Insert the draft's barcode rows, as the form holds them, where the draft
/// holds none.
pub(crate) fn insert_barcode_rows(
    sql: &CovenWrite<'_>,
    tables: &DraftTables,
    key: &dyn ToSql,
    barcodes: &[String],
) -> Result<(), DbError> {
    for (position, barcode) in barcodes.iter().enumerate() {
        sql.execute(
            &format!(
                "INSERT INTO {table} ({key}, position, barcode) VALUES (?, ?, ?)",
                table = tables.barcode,
                key = tables.key,
            ),
            params![key, position as i64, barcode],
        )?;
    }
    Ok(())
}

/// Insert the draft's mediums, in order, where the draft holds none.
pub(crate) fn insert_medium_rows(
    sql: &CovenWrite<'_>,
    tables: &DraftTables,
    key: &dyn ToSql,
    mediums: &[bae_types::mediums::ReleaseMedium],
) -> Result<(), DbError> {
    for (index, medium) in mediums.iter().enumerate() {
        sql.execute(
            &format!(
                "INSERT INTO {table} ({key}, position, medium, side_count) VALUES (?, ?, ?, ?)",
                table = tables.medium,
                key = tables.key,
            ),
            params![
                key,
                bae_types::mediums::medium_position(index),
                medium.carrier.map(bae_types::pressing::Medium::key),
                medium.sides,
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn assignment_kind_column(assignments: &TrackArtistAssignments) -> &'static str {
    match assignments {
        TrackArtistAssignments::AlbumArtists => "album_artists",
        TrackArtistAssignments::Explicit(_) => "explicit",
    }
}

/// The label rows of the owner `owner` (an SQL expression for its key), as
/// a JSON array in their order, which [`read_label_rows`] reads.
pub(crate) fn labels_json_sql(tables: &DraftTables, owner: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'name', label.name, 'catalog_number', label.catalog_number, \
             'musicbrainz_label_id', label.musicbrainz_label_id, \
             'discogs_label_id', label.discogs_label_id) \
             ORDER BY label.position) \
         FROM {table} AS label WHERE label.{key} = {owner})",
        table = tables.label,
        key = tables.key,
    )
}

/// The barcode rows of the owner `owner`, as a JSON array in their order.
pub(crate) fn barcodes_json_sql(tables: &DraftTables, owner: &str) -> String {
    format!(
        "(SELECT json_group_array(barcode.barcode ORDER BY barcode.position) \
         FROM {table} AS barcode WHERE barcode.{key} = {owner})",
        table = tables.barcode,
        key = tables.key,
    )
}

/// The mediums of the owner `owner`, as a JSON array in their order, which
/// [`mediums_of`] reads.
pub(crate) fn mediums_json_sql(tables: &DraftTables, owner: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'carrier', medium.medium, 'sides', medium.side_count) ORDER BY medium.position) \
         FROM {table} AS medium WHERE medium.{key} = {owner})",
        table = tables.medium,
        key = tables.key,
    )
}

/// A stored JSON column of `what`, decoded.
pub(crate) fn decoded<T: serde::de::DeserializeOwned>(
    json: &str,
    what: &str,
) -> Result<T, DbError> {
    serde_json::from_str(json)
        .map_err(|error| DbError::unreadable_json(format!("a draft's stored {what}"), json, error))
}

/// The draft's label rows as typed, as the read's `labels` array holds them.
pub(crate) fn read_label_rows(row: &rusqlite::Row) -> rusqlite::Result<Vec<RawLabelEdit>> {
    bae_store::read::json_column(row, "labels")
}

/// The credits a JSON array of [`assignments_json_sql`] holds, in order.
pub(crate) fn assignments_of(json: &str, what: &str) -> Result<Vec<Credited>, DbError> {
    decoded::<Vec<StoredAssignment>>(json, what)?
        .into_iter()
        .map(StoredAssignment::credited)
        .collect()
}

/// One medium as [`mediums_json_sql`] holds it.
#[derive(serde::Deserialize)]
struct StoredMedium {
    carrier: Option<String>,
    sides: std::num::NonZeroU8,
}

/// The mediums a JSON array of [`mediums_json_sql`] holds, in order.
pub(crate) fn mediums_of(json: &str) -> Result<Vec<bae_types::mediums::ReleaseMedium>, DbError> {
    decoded::<Vec<StoredMedium>>(json, "mediums")?
        .into_iter()
        .map(|medium| {
            let carrier = medium
                .carrier
                .map(|key| {
                    bae_types::pressing::Medium::from_key(&key)
                        .ok_or_else(|| DbError::unreadable("a draft medium's carrier", &key))
                })
                .transpose()?;
            Ok(bae_types::mediums::ReleaseMedium { carrier, sides: medium.sides })
        })
        .collect()
}

struct AssignmentColumns<'a> {
    kind: &'static str,
    artist_id: Option<&'a str>,
    name: Option<&'a str>,
    sort_name: Option<&'a str>,
    musicbrainz_id: Option<&'a str>,
    discogs_id: Option<&'a str>,
    credit_key: Option<&'a str>,
    credited_as: Option<&'a str>,
    join_phrase: Option<&'a str>,
}

fn assignment_columns(credited: &Credited) -> AssignmentColumns<'_> {
    let join_phrase = credited.join.as_deref();
    match &credited.artist {
        ArtistAssignment::Picked { artist, credit_name } => AssignmentColumns {
            kind: "picked",
            artist_id: Some(&artist.artist_id),
            name: None,
            sort_name: None,
            musicbrainz_id: None,
            discogs_id: None,
            credit_key: credit_name.as_ref().map(|credit_name| credit_name.key.as_str()),
            credited_as: None,
            join_phrase,
        },
        ArtistAssignment::Credit { credit } => AssignmentColumns {
            kind: "credit",
            artist_id: None,
            name: Some(credit.name.as_str()),
            sort_name: credit.sort_name.as_deref(),
            musicbrainz_id: credit.musicbrainz_artist_id.as_deref(),
            discogs_id: credit.discogs_artist_id.as_deref(),
            credit_key: None,
            credited_as: credit.credited_as.as_deref(),
            join_phrase,
        },
    }
}

/// One stored assignment row, the picked artist's fields read from the
/// library and its credit name, if it names one still there, as
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

pub(crate) fn insert_album_artist_assignments(
    sql: &CovenWrite<'_>,
    tables: &DraftTables,
    key: &dyn ToSql,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        store_album_artist_assignment(sql, tables, key, position, assignment)?;
    }
    Ok(())
}

/// Rewrite in place each album artist that differs from the one stored at
/// its position, and delete the stored ones past the last.
pub(crate) fn store_album_artist_assignments(
    sql: &CovenWrite<'_>,
    tables: &DraftTables,
    key: &dyn ToSql,
    stored: &[Credited],
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        if stored.get(position) != Some(assignment) {
            store_album_artist_assignment(sql, tables, key, position, assignment)?;
        }
    }
    if stored.len() > assignments.len() {
        sql.execute(
            &format!(
                "DELETE FROM {table} WHERE {key} = ? AND position >= ?",
                table = tables.album_artist,
                key = tables.key,
            ),
            params![key, assignments.len() as i64],
        )?;
    }
    Ok(())
}

/// Make the album artist at `position` `assignment`, in place when one is
/// there.
fn store_album_artist_assignment(
    sql: &CovenWrite<'_>,
    tables: &DraftTables,
    key: &dyn ToSql,
    position: usize,
    assignment: &Credited,
) -> Result<(), DbError> {
    let columns = assignment_columns(assignment);
    sql.execute(
        &format!(
            "INSERT INTO {table} \
             ({key}, position, assignment_kind, artist_id, name, sort_name, \
              musicbrainz_artist_id, discogs_artist_id, credit_key, credited_as, join_phrase) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT ({key}, position) DO UPDATE SET \
                 assignment_kind = excluded.assignment_kind, artist_id = excluded.artist_id, \
                 name = excluded.name, sort_name = excluded.sort_name, \
                 musicbrainz_artist_id = excluded.musicbrainz_artist_id, \
                 discogs_artist_id = excluded.discogs_artist_id, \
                 credit_key = excluded.credit_key, credited_as = excluded.credited_as, \
                 join_phrase = excluded.join_phrase",
            table = tables.album_artist,
            key = tables.key,
        ),
        params![
            key,
            position as i64,
            columns.kind,
            columns.artist_id,
            columns.name,
            columns.sort_name,
            columns.musicbrainz_id,
            columns.discogs_id,
            columns.credit_key,
            columns.credited_as,
            columns.join_phrase
        ],
    )?;
    Ok(())
}

pub(crate) fn insert_track_artist_assignments(
    sql: &CovenWrite<'_>,
    tables: &DraftTables,
    key: &dyn ToSql,
    track_id: &str,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        let columns = assignment_columns(assignment);
        sql.execute(
            &format!(
                "INSERT INTO {table} \
                 ({key}, track_id, position, assignment_kind, artist_id, name, sort_name, \
                  musicbrainz_artist_id, discogs_artist_id, credit_key, credited_as, \
                  join_phrase) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                table = tables.track_artist,
                key = tables.key,
            ),
            params![
                key,
                track_id,
                position as i64,
                columns.kind,
                columns.artist_id,
                columns.name,
                columns.sort_name,
                columns.musicbrainz_id,
                columns.discogs_id,
                columns.credit_key,
                columns.credited_as,
                columns.join_phrase
            ],
        )?;
    }
    Ok(())
}
