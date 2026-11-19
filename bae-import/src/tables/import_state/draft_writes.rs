//! The statements that write a candidate's draft values beside its own
//! row: its label and barcode rows, its album and track artist assignments,
//! its mediums, and the roles and works of its release and its tracks.
//! `tables::draft_rows` reads them back.

use crate::tables::draft_rows::assignment_columns;
use crate::{Credited, RawLabelEdit};
use bae_catalogs::credits::CreditedRole;
use bae_catalogs::works::WorkGraph;
use bae_store::DbError;
use coven::CovenWrite;
use rusqlite::{params, ToSql};

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
    let columns = assignment_columns(assignment);
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
    key: &dyn ToSql,
    track_id: &str,
    assignments: &[Credited],
) -> Result<(), DbError> {
    for (position, assignment) in assignments.iter().enumerate() {
        let columns = assignment_columns(assignment);
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
