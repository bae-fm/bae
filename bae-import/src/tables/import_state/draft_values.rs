//! The values a candidate's draft holds beside its own row, and the
//! statements that write and read them: its form's columns, its label and
//! barcode rows, its album and track artist assignments, its mediums, and
//! the roles and works of its release and its tracks.

use crate::tables::draft_rows::{decoded, AssignmentRow};
use crate::{Credited, RawLabelEdit, RawReleaseEdit};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::credits::{ArtistCredit, CreditedRole};
use bae_catalogs::works::{GraphWork, WorkCredit, WorkGraph, WorkPart};
use bae_store::DbError;
use bae_types::credit_role::CreditedAs;
use coven::CovenWrite;
use rusqlite::{params, ToSql};
use std::str::FromStr;

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
    key: &dyn ToSql,
    labels: &[RawLabelEdit],
) -> Result<(), DbError> {
    for (position, label) in labels.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_edit_label \
                     (content_hash, position, name, catalog_number, \
                      musicbrainz_label_id, discogs_label_id) VALUES (?, ?, ?, ?, ?, ?)",
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
    key: &dyn ToSql,
    barcodes: &[String],
) -> Result<(), DbError> {
    for (position, barcode) in barcodes.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_edit_barcode (content_hash, position, barcode) VALUES (?, ?, ?)",
            params![key, position as i64, barcode],
        )?;
    }
    Ok(())
}

/// Insert the draft's mediums, in order, where the draft holds none.
pub(crate) fn insert_medium_rows(
    sql: &CovenWrite<'_>,
    key: &dyn ToSql,
    mediums: &[bae_types::mediums::ReleaseMedium],
) -> Result<(), DbError> {
    for (index, medium) in mediums.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_medium (content_hash, position, medium, side_count) VALUES (?, ?, ?, ?)",
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

pub(crate) fn insert_album_artist_assignments(
    sql: &CovenWrite<'_>,
    key: &dyn ToSql,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        store_album_artist_assignment(sql, key, position, assignment)?;
    }
    Ok(())
}

/// Rewrite in place each album artist that differs from the one stored at
/// its position, and delete the stored ones past the last.
pub(crate) fn store_album_artist_assignments(
    sql: &CovenWrite<'_>,
    key: &dyn ToSql,
    stored: &[Credited],
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        if stored.get(position) != Some(assignment) {
            store_album_artist_assignment(sql, key, position, assignment)?;
        }
    }
    if stored.len() > assignments.len() {
        sql.execute(
            "DELETE FROM import_candidate_album_artist_assignment WHERE content_hash = ? AND position >= ?",
            params![key, assignments.len() as i64],
        )?;
    }
    Ok(())
}

/// Make the album artist at `position` `assignment`, in place when one is
/// there.
fn store_album_artist_assignment(
    sql: &CovenWrite<'_>,
    key: &dyn ToSql,
    position: usize,
    assignment: &Credited,
) -> Result<(), DbError> {
    let row = AssignmentRow::of(assignment);
    sql.execute(
        "INSERT INTO import_candidate_album_artist_assignment \
             (content_hash, position, assignment_kind, artist_id, name, sort_name, \
              musicbrainz_artist_id, discogs_artist_id, credit_key, credited_as, join_phrase) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (content_hash, position) DO UPDATE SET \
                 assignment_kind = excluded.assignment_kind, artist_id = excluded.artist_id, \
                 name = excluded.name, sort_name = excluded.sort_name, \
                 musicbrainz_artist_id = excluded.musicbrainz_artist_id, \
                 discogs_artist_id = excluded.discogs_artist_id, \
                 credit_key = excluded.credit_key, credited_as = excluded.credited_as, \
                 join_phrase = excluded.join_phrase",
        params![
            key,
            position as i64,
            row.assignment_kind,
            row.artist_id,
            row.name,
            row.sort_name,
            row.musicbrainz_artist_id,
            row.discogs_artist_id,
            row.credit_key,
            row.credited_as,
            row.join_phrase
        ],
    )?;
    Ok(())
}

pub(crate) fn insert_track_artist_assignments(
    sql: &CovenWrite<'_>,
    key: &dyn ToSql,
    track_id: &str,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        let row = AssignmentRow::of(assignment);
        sql.execute(
            "INSERT INTO import_candidate_track_artist_assignment \
                 (content_hash, track_id, position, assignment_kind, artist_id, name, sort_name, \
                  musicbrainz_artist_id, discogs_artist_id, credit_key, credited_as, \
                  join_phrase) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                key,
                track_id,
                position as i64,
                row.assignment_kind,
                row.artist_id,
                row.name,
                row.sort_name,
                row.musicbrainz_artist_id,
                row.discogs_artist_id,
                row.credit_key,
                row.credited_as,
                row.join_phrase
            ],
        )?;
    }
    Ok(())
}

/// Write a draft's works and the roles on its release, under
/// `key`. The caller has already written the owner's row.
pub(crate) fn insert_draft_values(
    sql: &CovenWrite<'_>,
    key: &dyn ToSql,
    works: &WorkGraph,
    release_roles: &[CreditedRole],
) -> Result<(), DbError> {
    for (position, work) in works.works.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_work \
                 (content_hash, musicbrainz_work_id, position, title, disambiguation, work_type) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            params![
                key,
                work.musicbrainz_work_id,
                position as i64,
                work.title,
                work.disambiguation,
                work.work_type,
            ],
        )?;
        for (position, credit) in work.credits.iter().enumerate() {
            let role = credit.role.columns();
            sql.execute(
                "INSERT INTO import_candidate_work_credit \
                     (content_hash, musicbrainz_work_id, position, name, sort_name, \
                      musicbrainz_artist_id, discogs_artist_id, credited_as, role, role_words, \
                      qualifiers) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    key,
                    work.musicbrainz_work_id,
                    position as i64,
                    credit.artist.name,
                    credit.artist.sort_name,
                    credit.artist.musicbrainz_artist_id,
                    credit.artist.discogs_artist_id,
                    credit.artist.credited_as,
                    role.role,
                    role.role_words,
                    role.qualifiers,
                ],
            )?;
        }
    }
    for (position, part) in works.parts.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_work_part (content_hash, position, parent_work_id, child_work_id) \
                 VALUES (?, ?, ?, ?)",
            params![key, position as i64, part.parent, part.child],
        )?;
    }
    for (position, role) in release_roles.iter().enumerate() {
        let columns = role.role.columns();
        sql.execute(
            "INSERT INTO import_candidate_release_role \
                 (content_hash, position, name, sort_name, musicbrainz_artist_id, \
                  discogs_artist_id, credited_as, source, role, role_words, qualifiers) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                key,
                position as i64,
                role.artist.name,
                role.artist.sort_name,
                role.artist.musicbrainz_artist_id,
                role.artist.discogs_artist_id,
                role.artist.credited_as,
                role.source.as_str(),
                columns.role,
                columns.role_words,
                columns.qualifiers,
            ],
        )?;
    }
    Ok(())
}

/// Write the rows keyed by the draft track `track_id`: the roles the source
/// credits on it and the works it performs, under `key`. The
/// caller has already written the track row and the works.
pub(crate) fn insert_track_values(
    sql: &CovenWrite<'_>,
    key: &dyn ToSql,
    track_id: &str,
    roles: &[CreditedRole],
    works: &[String],
) -> Result<(), DbError> {
    for (position, role) in roles.iter().enumerate() {
        let columns = role.role.columns();
        sql.execute(
            "INSERT INTO import_candidate_track_role \
                 (content_hash, track_id, position, name, sort_name, musicbrainz_artist_id, \
                  discogs_artist_id, credited_as, source, role, role_words, qualifiers) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                key,
                track_id,
                position as i64,
                role.artist.name,
                role.artist.sort_name,
                role.artist.musicbrainz_artist_id,
                role.artist.discogs_artist_id,
                role.artist.credited_as,
                role.source.as_str(),
                columns.role,
                columns.role_words,
                columns.qualifiers,
            ],
        )?;
    }
    for (position, work) in works.iter().enumerate() {
        sql.execute(
            "INSERT INTO import_candidate_track_work (content_hash, track_id, position, musicbrainz_work_id) \
                 VALUES (?, ?, ?, ?)",
            params![key, track_id, position as i64, work],
        )?;
    }
    Ok(())
}

/// The label rows of the owner `owner` (an SQL expression for its key), as
/// a JSON array in their order, which [`read_label_rows`] reads.
pub(crate) fn labels_json_sql(owner: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'name', label.name, 'catalog_number', label.catalog_number, \
             'musicbrainz_label_id', label.musicbrainz_label_id, \
             'discogs_label_id', label.discogs_label_id) \
             ORDER BY label.position) \
         FROM import_candidate_edit_label AS label WHERE label.content_hash = {owner})",
    )
}

/// The barcode rows of the owner `owner`, as a JSON array in their order.
pub(crate) fn barcodes_json_sql(owner: &str) -> String {
    format!(
        "(SELECT json_group_array(barcode.barcode ORDER BY barcode.position) \
         FROM import_candidate_edit_barcode AS barcode WHERE barcode.content_hash = {owner})",
    )
}

/// The mediums of the owner `owner`, as a JSON array in their order, which
/// [`mediums_of`] reads.
pub(crate) fn mediums_json_sql(owner: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'carrier', medium.medium, 'sides', medium.side_count) ORDER BY medium.position) \
         FROM import_candidate_medium AS medium WHERE medium.content_hash = {owner})",
    )
}

/// The draft's label rows as the person typed them, as the read's `labels`
/// array holds them.
pub(crate) fn read_label_rows(row: &rusqlite::Row) -> rusqlite::Result<Vec<RawLabelEdit>> {
    bae_store::read::json_column(row, "labels")
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

/// The works of the owner `owner` (an SQL expression for its key), each with
/// its credits, as a JSON array in the draft's order, which [`works_of`]
/// decodes.
pub(crate) fn works_json_sql(owner: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'musicbrainz_work_id', work.musicbrainz_work_id, 'title', work.title, \
             'disambiguation', work.disambiguation, 'work_type', work.work_type, \
             'credits', json((SELECT json_group_array(json_object( \
                     'name', credit.name, 'sort_name', credit.sort_name, \
                     'musicbrainz_artist_id', credit.musicbrainz_artist_id, \
                     'discogs_artist_id', credit.discogs_artist_id, \
                     'credited_as', credit.credited_as, 'role', credit.role, \
                     'role_words', credit.role_words, 'qualifiers', credit.qualifiers) \
                     ORDER BY credit.position) \
                 FROM import_candidate_work_credit AS credit \
                 WHERE credit.content_hash = work.content_hash \
                   AND credit.musicbrainz_work_id = work.musicbrainz_work_id))) \
             ORDER BY work.position) \
         FROM import_candidate_work AS work WHERE work.content_hash = {owner})",
    )
}

/// Which of the owner `owner`'s works is part of which, as a JSON array in
/// the order the source relates them.
pub(crate) fn work_parts_json_sql(owner: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'parent', part.parent_work_id, 'child', part.child_work_id) ORDER BY part.position) \
         FROM import_candidate_work_part AS part WHERE part.content_hash = {owner})",
    )
}

/// The columns of one role as a JSON object, which [`StoredRole`] reads.
const ROLE_JSON: &str = "json_object( \
     'name', role.name, 'sort_name', role.sort_name, \
     'musicbrainz_artist_id', role.musicbrainz_artist_id, \
     'discogs_artist_id', role.discogs_artist_id, \
     'credited_as', role.credited_as, 'source', role.source, \
     'role', role.role, 'role_words', role.role_words, \
     'qualifiers', role.qualifiers)";

/// The roles the source credits on the owner `owner`'s release, as a JSON
/// array in its order, which [`roles_of`] decodes.
pub(crate) fn release_roles_json_sql(owner: &str) -> String {
    format!(
        "(SELECT json_group_array({ROLE_JSON} ORDER BY role.position) \
         FROM import_candidate_release_role AS role WHERE role.content_hash = {owner})",
    )
}

/// The roles the source credits on the owner `owner`'s track `track` (SQL
/// expressions for their keys), as a JSON array in its order, which
/// [`roles_of`] decodes.
pub(crate) fn track_roles_json_sql(owner: &str, track: &str) -> String {
    format!(
        "(SELECT json_group_array({ROLE_JSON} ORDER BY role.position) \
         FROM import_candidate_track_role AS role WHERE role.content_hash = {owner} AND role.track_id = {track})",
    )
}

/// The works the owner `owner`'s track `track` performs, as a JSON array of
/// their ids in its order.
pub(crate) fn track_works_json_sql(owner: &str, track: &str) -> String {
    format!(
        "(SELECT json_group_array(performed.musicbrainz_work_id ORDER BY performed.position) \
         FROM import_candidate_track_work AS performed \
         WHERE performed.content_hash = {owner} AND performed.track_id = {track})",
    )
}

/// One stored work as [`works_json_sql`] holds it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredWork {
    musicbrainz_work_id: String,
    title: String,
    disambiguation: Option<String>,
    work_type: Option<String>,
    credits: Vec<StoredWorkCredit>,
}

/// One stored work credit as [`works_json_sql`] holds it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredWorkCredit {
    name: String,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credited_as: Option<String>,
    role: String,
    role_words: Option<String>,
    qualifiers: String,
}

/// One stored part as [`work_parts_json_sql`] holds it.
#[derive(serde::Deserialize)]
pub(crate) struct StoredPart {
    parent: String,
    child: String,
}

/// The work graph [`works_json_sql`] and [`work_parts_json_sql`] hold.
pub(crate) fn works_of(
    works: Vec<StoredWork>,
    parts: Vec<StoredPart>,
) -> Result<WorkGraph, DbError> {
    Ok(WorkGraph {
        works: works
            .into_iter()
            .map(|work| {
                Ok(GraphWork {
                    musicbrainz_work_id: work.musicbrainz_work_id,
                    title: work.title,
                    disambiguation: work.disambiguation,
                    work_type: work.work_type,
                    credits: work
                        .credits
                        .into_iter()
                        .map(|credit| {
                            Ok(WorkCredit {
                                artist: ArtistCredit {
                                    name: credit.name,
                                    sort_name: credit.sort_name,
                                    musicbrainz_artist_id: credit.musicbrainz_artist_id,
                                    discogs_artist_id: credit.discogs_artist_id,
                                    credited_as: credit.credited_as,
                                },
                                role: CreditedAs::from_columns(
                                    &credit.role,
                                    credit.role_words,
                                    &credit.qualifiers,
                                )
                                .map_err(|error| {
                                    DbError::unreadable("a draft work credit's role", error)
                                })?,
                            })
                        })
                        .collect::<Result<_, DbError>>()?,
                })
            })
            .collect::<Result<_, DbError>>()?,
        parts: parts
            .into_iter()
            .map(|part| WorkPart { parent: part.parent, child: part.child })
            .collect(),
    })
}

/// One stored role as [`release_roles_json_sql`] and [`track_roles_json_sql`] hold
/// it, its source still the text [`roles_of`] reads.
#[derive(serde::Deserialize)]
pub(crate) struct StoredRole {
    name: String,
    sort_name: Option<String>,
    musicbrainz_artist_id: Option<String>,
    discogs_artist_id: Option<String>,
    credited_as: Option<String>,
    source: String,
    role: String,
    role_words: Option<String>,
    qualifiers: String,
}

/// The credited roles a roles column holds.
pub(crate) fn roles_of(roles: Vec<StoredRole>) -> Result<Vec<CreditedRole>, DbError> {
    roles
        .into_iter()
        .map(|stored| {
            Ok(CreditedRole {
                artist: ArtistCredit {
                    name: stored.name,
                    sort_name: stored.sort_name,
                    musicbrainz_artist_id: stored.musicbrainz_artist_id,
                    discogs_artist_id: stored.discogs_artist_id,
                    credited_as: stored.credited_as,
                },
                source: Catalog::from_str(&stored.source).map_err(|_unknown| {
                    DbError::unreadable("a draft role's stored source", &stored.source)
                })?,
                role: CreditedAs::from_columns(&stored.role, stored.role_words, &stored.qualifiers)
                    .map_err(|error| DbError::unreadable("a draft role's stored role", error))?,
            })
        })
        .collect()
}
