//! How a surface outside bae numbers a release's tracks: a saved file's
//! track and disc tags and file name, and a Subsonic song. Such a surface
//! knows discs and track numbers, not sides, so a track's number counts the
//! tracks of its medium in playing order across the medium's sides, and its
//! disc is its medium's position, stated only for a release of several
//! mediums.

use super::*;

/// Each track of the release `?1` in playing order, with its number among
/// its medium's tracks and, for a release of several mediums, its medium's
/// position.
const EXPORT_NUMBERS_SQL: &str = "SELECT t.id AS track_id, \
        ROW_NUMBER() OVER (PARTITION BY t.medium_id ORDER BY t.position) AS number, \
        CASE WHEN (SELECT COUNT(*) FROM release_mediums WHERE release_id = ?1) > 1 \
             THEN m.position END AS disc \
     FROM tracks t JOIN release_mediums m ON m.id = t.medium_id \
     WHERE t.release_id = ?1 \
     ORDER BY t.position";

/// How a surface outside bae numbers each track of `release_id`.
pub(super) fn export_numbers_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<DbExportNumbers, DbError> {
    Ok(DbExportNumbers(sql.query(EXPORT_NUMBERS_SQL, params![release_id], |row| {
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
