//! How a surface outside bae numbers a release's tracks: a saved file's
//! track and disc tags and file name, and a Subsonic song. Such a surface
//! knows discs and track numbers, not sides, so a track's number counts the
//! tracks of its medium in playing order across the medium's sides, and its
//! disc is its medium's position, stated only for a release of several
//! mediums.

use super::*;

/// The tracks `track_filter` keeps (a condition over `t`, one of the
/// `tracks` of the release `release`, an SQL expression naming its id), in
/// playing order, each with its number among its medium's tracks and, for a
/// release of several mediums, its medium's position, as
/// [`track_places_ctes`] places the release's tracks.
fn numbered_tracks_sql(release: &str, track_filter: &str) -> String {
    format!(
        "WITH {places}, \
         numbered AS ( \
            SELECT id, \
                   ROW_NUMBER() OVER (PARTITION BY medium_position ORDER BY position) \
                       AS export_number, \
                   CASE WHEN (SELECT COUNT(*) FROM medium) > 1 THEN medium_position END \
                       AS export_disc \
            FROM place) \
         SELECT t.*, n.export_number, n.export_disc \
         FROM tracks t JOIN numbered n ON n.id = t.id \
         WHERE {track_filter} \
         ORDER BY t.position, t.id",
        places = track_places_ctes(MediumsSource::Release, &format!("m.release_id = {release}")),
    )
}

fn row_to_numbered_track(row: &Row) -> rusqlite::Result<DbNumberedTrack> {
    Ok(DbNumberedTrack {
        track: row_to_track(row)?,
        number: row.get("export_number")?,
        disc: row.get("export_disc")?,
    })
}

/// Each track of `release_id` in playing order, with how a surface outside
/// bae numbers it.
pub(super) fn numbered_tracks_of_release_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Vec<DbNumberedTrack>, DbError> {
    Ok(sql.query(
        &numbered_tracks_sql("?1", "t.release_id = ?1"),
        params![release_id],
        row_to_numbered_track,
    )?)
}

/// The track `track_id` with how a surface outside bae numbers it, or `None`
/// when the library holds no such track.
pub(super) fn numbered_track_on(
    sql: &impl coven::SqlRead,
    track_id: &str,
) -> Result<Option<DbNumberedTrack>, DbError> {
    Ok(sql
        .query(
            &numbered_tracks_sql("(SELECT release_id FROM tracks WHERE id = ?1)", "t.id = ?1"),
            params![track_id],
            row_to_numbered_track,
        )?
        .pop())
}

#[cfg(test)]
#[path = "export_numbers_tests.rs"]
mod tests;
