//! The draft's values besides its editable fields, as the statements that
//! write and read them in the [`DraftTables`] of the owner that holds them:
//! the roles the source credits on each track and on the release, and the
//! works the tracks perform with the works they nest among.

use super::DraftTables;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::credits::ArtistCredit;
use bae_catalogs::credits::CreditedRole;
use bae_catalogs::works::{GraphWork, WorkCredit, WorkGraph, WorkPart};
use bae_store::DbError;
use bae_types::credit_role::CreditedAs;
use coven::CovenWrite;
use rusqlite::params;
use rusqlite::ToSql;
use std::str::FromStr;

/// Write a draft's works and the roles on its release, in `tables` under
/// `key`. The caller has already written the owner's row.
pub(crate) fn insert_draft_values(
    sql: &CovenWrite<'_>,
    tables: &DraftTables,
    key: &dyn ToSql,
    works: &WorkGraph,
    release_roles: &[CreditedRole],
) -> Result<(), DbError> {
    let k = tables.key;
    for (position, work) in works.works.iter().enumerate() {
        sql.execute(
            &format!(
                "INSERT INTO {table} \
                 ({k}, musicbrainz_work_id, position, title, disambiguation, work_type) \
                 VALUES (?, ?, ?, ?, ?, ?)",
                table = tables.work,
            ),
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
                &format!(
                    "INSERT INTO {table} \
                     ({k}, musicbrainz_work_id, position, name, sort_name, \
                      musicbrainz_artist_id, discogs_artist_id, credited_as, role, role_words, \
                      qualifiers) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    table = tables.work_credit,
                ),
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
            &format!(
                "INSERT INTO {table} ({k}, position, parent_work_id, child_work_id) \
                 VALUES (?, ?, ?, ?)",
                table = tables.work_part,
            ),
            params![key, position as i64, part.parent, part.child],
        )?;
    }
    for (position, role) in release_roles.iter().enumerate() {
        let columns = role.role.columns();
        sql.execute(
            &format!(
                "INSERT INTO {table} \
                 ({k}, position, name, sort_name, musicbrainz_artist_id, \
                  discogs_artist_id, credited_as, source, role, role_words, qualifiers) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                table = tables.release_role,
            ),
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
/// credits on it and the works it performs, in `tables` under `key`. The
/// caller has already written the track row and the works.
pub(crate) fn insert_track_values(
    sql: &CovenWrite<'_>,
    tables: &DraftTables,
    key: &dyn ToSql,
    track_id: &str,
    roles: &[CreditedRole],
    works: &[String],
) -> Result<(), DbError> {
    let k = tables.key;
    for (position, role) in roles.iter().enumerate() {
        let columns = role.role.columns();
        sql.execute(
            &format!(
                "INSERT INTO {table} \
                 ({k}, track_id, position, name, sort_name, musicbrainz_artist_id, \
                  discogs_artist_id, credited_as, source, role, role_words, qualifiers) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                table = tables.track_role,
            ),
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
            &format!(
                "INSERT INTO {table} ({k}, track_id, position, musicbrainz_work_id) \
                 VALUES (?, ?, ?, ?)",
                table = tables.track_work,
            ),
            params![key, track_id, position as i64, work],
        )?;
    }
    Ok(())
}

/// The works of the owner `owner` (an SQL expression for its key), each with
/// its credits, as a JSON array in the draft's order, which [`works_of`]
/// decodes.
pub(crate) fn works_json_sql(tables: &DraftTables, owner: &str) -> String {
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
                 FROM {credit_table} AS credit \
                 WHERE credit.{key} = work.{key} \
                   AND credit.musicbrainz_work_id = work.musicbrainz_work_id))) \
             ORDER BY work.position) \
         FROM {work_table} AS work WHERE work.{key} = {owner})",
        credit_table = tables.work_credit,
        work_table = tables.work,
        key = tables.key,
    )
}

/// Which of the owner `owner`'s works is part of which, as a JSON array in
/// the order the source relates them.
pub(crate) fn work_parts_json_sql(tables: &DraftTables, owner: &str) -> String {
    format!(
        "(SELECT json_group_array(json_object( \
             'parent', part.parent_work_id, 'child', part.child_work_id) ORDER BY part.position) \
         FROM {table} AS part WHERE part.{key} = {owner})",
        table = tables.work_part,
        key = tables.key,
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
pub(crate) fn release_roles_json_sql(tables: &DraftTables, owner: &str) -> String {
    format!(
        "(SELECT json_group_array({ROLE_JSON} ORDER BY role.position) \
         FROM {table} AS role WHERE role.{key} = {owner})",
        table = tables.release_role,
        key = tables.key,
    )
}

/// The roles the source credits on the owner `owner`'s track `track` (SQL
/// expressions for their keys), as a JSON array in its order, which
/// [`roles_of`] decodes.
pub(crate) fn track_roles_json_sql(tables: &DraftTables, owner: &str, track: &str) -> String {
    format!(
        "(SELECT json_group_array({ROLE_JSON} ORDER BY role.position) \
         FROM {table} AS role WHERE role.{key} = {owner} AND role.track_id = {track})",
        table = tables.track_role,
        key = tables.key,
    )
}

/// The works the owner `owner`'s track `track` performs, as a JSON array of
/// their ids in its order.
pub(crate) fn track_works_json_sql(tables: &DraftTables, owner: &str, track: &str) -> String {
    format!(
        "(SELECT json_group_array(performed.musicbrainz_work_id ORDER BY performed.position) \
         FROM {table} AS performed \
         WHERE performed.{key} = {owner} AND performed.track_id = {track})",
        table = tables.track_work,
        key = tables.key,
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
