//! How a surface outside bae numbers a release's tracks: a saved file's
//! track and disc tags and file name, and a Subsonic song. Such a surface
//! knows discs and track numbers, not sides, so a track's number counts the
//! tracks of its medium in playing order across the medium's sides, and its
//! disc is its medium's position, stated only for a release of several
//! mediums.

use super::*;

/// Each track of the release `?1` in playing order, with its number among
/// its medium's tracks and, for a release of several mediums, its medium's
/// position, as [`track_places_ctes`] places the release's tracks.
fn export_numbers_sql() -> String {
    format!(
        "WITH {places} \
         SELECT id AS track_id, \
                ROW_NUMBER() OVER (PARTITION BY medium_position ORDER BY position) AS number, \
                CASE WHEN (SELECT COUNT(*) FROM medium) > 1 THEN medium_position END AS disc \
         FROM place ORDER BY position",
        places = track_places_ctes(MediumsSource::Release, "m.release_id = ?1"),
    )
}

/// How a surface outside bae numbers each track of `release_id`.
pub(super) fn export_numbers_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<DbExportNumbers, DbError> {
    Ok(DbExportNumbers(sql.query(&export_numbers_sql(), params![release_id], |row| {
        Ok(DbExportNumber {
            track_id: row.get("track_id")?,
            number: row.get("number")?,
            disc: row.get("disc")?,
        })
    })?))
}

#[cfg(test)]
#[path = "export_numbers_tests.rs"]
mod tests;
