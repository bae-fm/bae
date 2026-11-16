//! The lists a media browser, such as Android Auto, reads a page at a time:
//! an album's tracks and a work's children. A browser names each page by its
//! offset, so each page is one plain read at that offset, and each list's
//! count is a live query of its own, which tells the browser when to read the
//! list again.

use super::*;
use crate::album::{track_hit_row, TRACK_HIT_SELECT};
use crate::album_detail::{TrackSearchResult, WorkChild, WorkReleaseSummary};
use crate::composer::{row_to_work_release_summary, work_release_columns, work_summary_columns};

impl LibraryViews {
    /// The tracks of the album `album_id` from `offset`, at most `limit`, read
    /// once: its main release's tracks, in playing order. An album that
    /// doesn't exist has none.
    pub async fn album_track_page(
        &self,
        album_id: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<TrackSearchResult>, DbError> {
        let album_id = album_id.to_string();
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.store
            .read(move |sql| {
                Ok(sql.query(
                    &format!(
                        "{select} WHERE a.id = ?1 AND r.id = {main}
                         ORDER BY t.position LIMIT ?2 OFFSET ?3",
                        select = TRACK_HIT_SELECT.as_str(),
                        main = album_primary_release_sql(),
                    ),
                    params![album_id, limit, offset],
                    track_hit_row,
                )?)
            })
            .process(|rows| Ok(rows.into_iter().map(TrackSearchResult::from_raw).collect()))
            .await
    }

    /// Follow how many tracks the album `album_id` has: the tracks
    /// [`Self::album_track_page`] pages through.
    pub fn subscribe_album_track_count(
        &self,
        album_id: &str,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let album_id = album_id.to_string();
        self.store.subscribe(move |sql| {
            Ok(sql.query_row(
                &format!(
                    "SELECT COUNT(*) AS count FROM albums a
                     JOIN tracks t ON t.release_id = {main}
                     WHERE a.id = ?1",
                    main = album_primary_release_sql(),
                ),
                params![album_id],
                |row| count_column::<u64>(row, "count"),
            )?)
        })
    }

    /// The children of the work `work_id` from `offset`, at most `limit`, read
    /// once: its parts, then the releases that perform it, in the order
    /// [`work_children_sql`] gives.
    pub async fn work_child_page(
        &self,
        work_id: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<WorkChild>, DbError> {
        let work_id = work_id.to_string();
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.store.read(move |sql| work_child_page_on(&sql, &work_id, offset, limit)).await
    }

    /// Follow how many children the work `work_id` has: the rows
    /// [`Self::work_child_page`] pages through.
    pub fn subscribe_work_child_count(
        &self,
        work_id: &str,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let work_id = work_id.to_string();
        self.store.subscribe(move |sql| work_child_count_on(&sql, &work_id))
    }
}

/// The children of the work the parameter `:work_id` names, one row each,
/// with the columns its order reads: its parts (`section` 0), by their place
/// among its parts and then by title, then the releases performing it
/// (`section` 1), in the order its page lists them (the albums by title, then
/// by id, then each album's releases in album order). `item_id` is the part's
/// work or the release, which ends the order with a term no two children
/// share.
fn work_children_sql() -> String {
    format!(
        "SELECT 0 AS section, part.position AS place, {work_title} AS title_key,
                NULL AS album_id, NULL AS album_order, part.child_work_id AS item_id
         FROM work_parts part
         JOIN works w ON w.id = part.child_work_id
         WHERE part.parent_work_id = :work_id
         UNION ALL
         SELECT DISTINCT 1, NULL, {album_title}, a.id, r.album_order, r.id
         FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE tw.work_id = :work_id",
        work_title = sort_key_sql("w.title"),
        album_title = sort_key_sql("a.title"),
    )
}

/// The children of `work_id` from `offset`, at most `limit`, each read with
/// what it shows: a part's summary or a release's.
fn work_child_page_on(
    sql: &SqlReadContext<'_>,
    work_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<WorkChild>, DbError> {
    let statement = format!(
        "SELECT child.section, {work}, {release}
         FROM ({children}) child
         LEFT JOIN works w ON child.section = 0 AND w.id = child.item_id
         LEFT JOIN releases r ON child.section = 1 AND r.id = child.item_id
         LEFT JOIN release_number_in_album number ON number.release_id = r.id
         LEFT JOIN albums a ON a.id = r.album_id
         ORDER BY child.section, child.place, child.title_key, child.album_id,
                  child.album_order, child.item_id
         LIMIT :limit OFFSET :offset",
        work = work_summary_columns(),
        release = work_release_columns(),
        children = work_children_sql(),
    );
    Ok(sql.query(
        &statement,
        rusqlite::named_params! { ":work_id": work_id, ":limit": limit, ":offset": offset },
        |row| match row.get::<_, i64>("section")? {
            0 => Ok(WorkChild::Part(row_to_work_summary(row)?)),
            1 => Ok(WorkChild::Release(WorkReleaseSummary::from_raw(row_to_work_release_summary(
                row,
            )?))),
            other => Err(column_conversion_error(
                row,
                "section",
                format!("work child section {other} names no kind of child"),
            )),
        },
    )?)
}

fn work_child_count_on(sql: &SqlReadContext<'_>, work_id: &str) -> Result<u64, DbError> {
    Ok(sql.query_row(
        &format!("SELECT COUNT(*) AS count FROM ({}) child", work_children_sql()),
        rusqlite::named_params! { ":work_id": work_id },
        |row| count_column::<u64>(row, "count"),
    )?)
}

#[cfg(test)]
#[path = "browse_pages_tests.rs"]
mod tests;
