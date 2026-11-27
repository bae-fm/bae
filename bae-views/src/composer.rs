//! The composer views: the composer list, a work's detail and a work's
//! children as a list. A composer's own page is `composer_detail`.
//!
//! Its tables and the triggers keeping them are `composer.sql`.

use super::*;
use crate::album_detail::{WorkChild, WorkDetail, WorkReleaseSummary};
use crate::list_sort::term;
use crate::list_sort::ListSort;
use bae_records::records::browse_fragments::{WorkReleaseRow, WorkSummary};
use bae_records::records::files::ArtistImageVersion;
use bae_records::records::files::CoverVersion;
use bae_store::list_windows::{sql_bound, SortValue};
use bae_store::{statement_optional, statement_row, statement_rows};

impl LibraryViews {
    /// The composer list's rows at the places from `offset`, at most `limit`.
    #[cfg(test)]
    pub(crate) async fn get_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbComposerSummary>, DbError> {
        let sort = composer_sort(sort);
        let page = ListPage { offset, limit };
        self.store.read(move |sql| composer_page_on(&sql, &sort, page)).await
    }

    /// Follow one window of the composer list under `sort`: the rows from a key
    /// of its order on.
    pub fn subscribe_composer_window(
        &self,
        sort: &[ComposerSortCriterion],
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbComposerSummary>, bae_store::DbError>
    {
        let sort = composer_sort(sort);
        self.store.subscribe_reconfigurable(initial, move |window, sql| {
            composer_window_on(&sql, &sort, window)
        })
    }

    /// Follow one page of the composer list under `sort`: the composers at the
    /// page's places as each write leaves them.
    pub fn subscribe_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let sort = composer_sort(sort);
        self.store
            .subscribe(move |sql| composer_page_on(&sql, &sort, page))
            .process(|rows| Ok(crate::ListRows::Composers(rows)))
    }

    /// Follow how many composers the list has. The count runs as a separate
    /// query from the windows, because every composer added or removed changes
    /// it.
    pub fn subscribe_composer_count(&self) -> coven::LiveQuery<u64, bae_store::DbError> {
        self.store.subscribe(|sql| composer_list_length_on(&sql))
    }

    /// Follow how many composers come before the row at a key; the caller
    /// changes the key on the same query.
    pub fn subscribe_composer_position(
        &self,
        sort: &[ComposerSortCriterion],
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let sort = composer_sort(sort);
        self.store.subscribe_reconfigurable(initial, move |key, sql| {
            composers_before_on(&sql, &sort, key)
        })
    }

    /// The key of the composer at `position`, or of the last one when the list
    /// ends before it; `None` for an empty list.
    pub async fn composer_key_at(
        &self,
        sort: &[ComposerSortCriterion],
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let sort = composer_sort(sort);
        self.store.read(move |sql| composer_key_at_on(&sql, &sort, position)).await
    }

    pub(crate) async fn find_work_detail(
        &self,
        work_id: &str,
    ) -> Result<Option<DbWorkDetail>, DbError> {
        let work_id = work_id.to_string();
        self.store.read(move |sql| find_work_detail_on(&sql, &work_id)).await
    }

    /// The work's page as its view shows it; `None` for an unknown work.
    pub async fn get_work_detail(&self, work_id: &str) -> Result<Option<WorkDetail>, DbError> {
        Ok(self.find_work_detail(work_id).await?.map(work_detail_of))
    }

    /// Follow the work `initial` names, or none. The detail view moves to
    /// another work through the request handle, not another query.
    pub fn subscribe_work_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, WorkDetailProjection, bae_store::DbError>
    {
        self.store.subscribe_reconfigurable(initial, |work_id, sql| {
            let Some(work_id) = work_id else {
                return Ok(WorkDetailProjection { detail: None });
            };
            Ok(WorkDetailProjection { detail: find_work_detail_on(&sql, work_id)? })
        })
    }

    /// Follow one page of the work `work_id`'s children: the children at the
    /// page's places as each write leaves them, as a media browser lists
    /// them.
    pub fn subscribe_work_child_page(
        &self,
        work_id: &str,
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let work_id = work_id.to_string();
        self.store
            .subscribe(move |sql| work_child_page_on(&sql, &work_id, page))
            .process(|rows| Ok(work_child_rows(rows)))
    }

    /// Follow how many children the work `work_id` has. The count runs as a
    /// separate query from the pages' rows.
    pub fn subscribe_work_child_count(
        &self,
        work_id: &str,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let work_id = work_id.to_string();
        self.store.subscribe(move |sql| {
            let statement = WorkChildCountParams { work: work_id.clone() };
            list_count(statement_row(&sql, &statement)?.children)
        })
    }
}

/// One delivery of a composer list window as the list shows it.
pub fn resolve_composer_window(read: WindowRead<DbComposerSummary>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, crate::ListRows::Composers)
}

/// A work's children as a media browser lists them.
fn work_child_rows(rows: Vec<DbWorkChild>) -> crate::ListRows {
    crate::ListRows::WorkChildren(
        rows.into_iter()
            .map(|child| match child {
                DbWorkChild::Part(work) => WorkChild::Part(work),
                DbWorkChild::Release(release) => {
                    WorkChild::Release(WorkReleaseSummary::from_raw(release))
                }
            })
            .collect(),
    )
}

/// One delivery of the work detail as the detail view shows it.
pub fn resolve_work_detail_projection(projection: WorkDetailProjection) -> Option<WorkDetail> {
    projection.detail.map(work_detail_of)
}

/// A work's page as its rows read it, each with its cover's version.
fn work_detail_of(raw: DbWorkDetail) -> WorkDetail {
    WorkDetail {
        work: raw.work,
        child_works: raw.child_works,
        releases: raw.releases.into_iter().map(WorkReleaseSummary::from_raw).collect(),
        tracks: raw.tracks,
        child_work_count: raw.child_work_count,
        release_count: raw.release_count,
        track_count: raw.track_count,
    }
}

fn find_work_detail_on(
    sql: &SqlReadContext<'_>,
    work_id: &str,
) -> Result<Option<DbWorkDetail>, DbError> {
    let work = || work_id.to_string();
    let Some(found) = statement_optional(sql, &WorkByIdParams { work: work() })? else {
        return Ok(None);
    };
    let parts = statement_rows(sql, &WorkPartRowsParams { work: work() })?;
    // Each section's statement counts its own rows beside them; a section
    // with no rows counts zero.
    let child_work_count = list_count(parts.first().map_or(0, |part| part.children))?;
    let child_works =
        parts.into_iter().map(|row| work_summary(row.part, row.cover)).collect::<Result<_, _>>()?;
    let performing = statement_rows(sql, &WorkReleaseRowsParams { work: work() })?;
    let release_count = list_count(performing.first().map_or(0, |release| release.children))?;
    let releases = performing
        .into_iter()
        .map(|row| work_release(row.release, row.cover))
        .collect::<Result<_, _>>()?;
    let tracks_query = format!(
        "SELECT tw.id AS track_work_id, tw.work_id,
                tw.position, tw.source AS link_source, tw.created_at,
                t.id AS track_id, t.release_id AS track_release_id,
                t.title AS track_title, t.medium_id AS track_medium_id, t.side AS track_side,
                t.track_number AS track_track_number,
                t.duration_ms AS track_duration_ms,
                t.stated_length_ms AS track_stated_length_ms,
                t.created_at AS track_created_at,
                a.id AS album_id, a.title AS album_title,
                a.year AS album_year,
                a.is_compilation AS album_is_compilation,
                a.created_at AS album_created_at
         FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE tw.work_id = ?1
         ORDER BY {album_title}, a.id, r.album_order, r.id, t.position, tw.position",
        album_title = crate::sort_key_sql("a.title"),
    );
    let tracks = sql.query(&tracks_query, params![work_id], |row| {
        Ok(DbWorkTrackSummary {
            link: DbTrackWork {
                id: row.get("track_work_id")?,
                track_id: row.get("track_id")?,
                work_id: row.get("work_id")?,
                position: row.get("position")?,
                source: parsed_column(row, "link_source")?,
                created_at: rfc3339_column(row, "created_at")?,
            },
            track: row_to_track_with_prefix(row, "track_")?,
            album: row_to_album_with_prefix(row, "album_")?,
        })
    })?;
    // The count counts the rows of the tracks' statement above.
    let track_count: i64 = sql.query_row(
        &format!("SELECT COUNT(*) FROM ({tracks_query})"),
        params![work_id],
        |row| row.get(0),
    )?;
    Ok(Some(DbWorkDetail {
        work: work_summary(found.work, found.cover)?,
        child_works,
        releases,
        tracks,
        child_work_count,
        release_count,
        track_count: track_count as u64,
    }))
}

// Each artist the composer list shows (`composer_listing`), with how many
// works it is credited on and how many releases perform them, and its image's
// version.
sqlive_compose::fragment!(
    ListedComposer,
    r#"
    SELECT listing.artist_id AS listed_id, listing.work_count, listing.linked_release_count, artist.*, image.*
    FROM composer_listing AS listing
    JOIN @bae_records::records::browse_fragments::ArtistRecord AS artist
      ON artist.artist_id = listing.artist_id
    LEFT JOIN @bae_records::records::files::ArtistImageVersion AS image
           ON image.id = listing.artist_id
"#
);

impl ListedComposer {
    /// The composer as its row shows it.
    pub(crate) fn into_summary(self) -> Result<DbComposerSummary, DbError> {
        let artist = self.artist.into_artist()?;
        let version = self.image.and_then(ArtistImageVersion::into_version);
        Ok(DbComposerSummary {
            image: bae_records::artist_image_ref(&artist.id, version),
            artist,
            work_count: self.work_count,
            linked_release_count: self.linked_release_count,
        })
    }
}

// The composer list's statements order by the person's sort of up to 3
// criteria, each of them one fixed statement for every sort. `field_N` names
// the Nth criterion's field, NULL past the sort's last, and `descending_N`
// runs it the other way. After the criteria, rows go by name, then id, which
// tells every two apart. A key is a row's value for each criterion, its name
// key and its id. A row comes before a key when the first of those it differs
// in puts it first; no criterion's value is NULL, so only the criteria past
// the sort's last compare two NULLs. A window from the top binds a NULL key,
// which no row comes before. The window and the count before a key write that
// comparison over the list table's own columns, so coven reruns each only for
// a write on its side of the key. No index serves a sort the parameters name,
// so each read goes through the whole list.

// The composer list's rows from a key on, at most `limit` of them, in the sort's
// order. Each criterion has an ascending and a descending term, one of which is
// NULL in every row, as both are past the sort's last criterion. Each term is
// written over columns of the list table and returned ahead of the row, so
// coven reruns a full window only for a write at or before its last row.
sqlive_compose::read!(
    ComposerWindow(
        field_1: Option<String>,
        field_2: Option<String>,
        field_3: Option<String>,
        descending_1: bool,
        descending_2: bool,
        descending_3: bool,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: Option<String>,
        limit: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue,
        order_5: SortValue,
        order_6: SortValue
    },
    r#"
    SELECT CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_1,
           CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_2,
           CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_3,
           CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_4,
           CASE WHEN NOT :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_5,
           CASE WHEN :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_6,
           listing.name_sort_key AS order_name, listing.artist_id AS order_id,
           shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    WHERE NOT COALESCE(
            CASE WHEN :descending_1
                 THEN CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END > :key_1
                 ELSE CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END < :key_1 END
            OR (CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END IS :key_1 AND (
                CASE WHEN :descending_2
                     THEN CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END > :key_2
                     ELSE CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END < :key_2 END
                OR (CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END IS :key_2 AND (
                    CASE WHEN :descending_3
                         THEN CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END > :key_3
                         ELSE CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END < :key_3 END
                    OR (CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END IS :key_3 AND (
                        (listing.name_sort_key, listing.artist_id) < (:key_name, :key_id))))))),
            0)
    ORDER BY CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END,
             CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END DESC,
             CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END,
             CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END DESC,
             CASE WHEN NOT :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END,
             CASE WHEN :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END DESC,
             listing.name_sort_key, listing.artist_id
    LIMIT :limit
"#
);

// The composer list's rows at the places from `offset` in the sort's order, at
// most `limit` of them.
sqlive_compose::read!(
    ComposerRows(
        field_1: Option<String>,
        field_2: Option<String>,
        field_3: Option<String>,
        descending_1: bool,
        descending_2: bool,
        descending_3: bool,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT shown.*
    FROM composer_listing AS listing
    JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = listing.artist_id
    ORDER BY CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END,
             CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END DESC,
             CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END,
             CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END DESC,
             CASE WHEN NOT :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END,
             CASE WHEN :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END DESC,
             listing.name_sort_key, listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

// How many composers of the sort's order come before the row at a key.
sqlive_compose::read!(
    ComposersBefore(
        field_1: Option<String>,
        field_2: Option<String>,
        field_3: Option<String>,
        descending_1: bool,
        descending_2: bool,
        descending_3: bool,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: Option<String>
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_listing AS listing
    WHERE COALESCE(
            CASE WHEN :descending_1
                 THEN CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END > :key_1
                 ELSE CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END < :key_1 END
            OR (CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END IS :key_1 AND (
                CASE WHEN :descending_2
                     THEN CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END > :key_2
                     ELSE CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END < :key_2 END
                OR (CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END IS :key_2 AND (
                    CASE WHEN :descending_3
                         THEN CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END > :key_3
                         ELSE CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END < :key_3 END
                    OR (CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END IS :key_3 AND (
                        (listing.name_sort_key, listing.artist_id) < (:key_name, :key_id))))))),
            0)
"#
);

// The key of the row at the place `index` of the sort's order, or of the last
// row when the list ends before it. SQLite reads the offset -1 of an empty
// list as 0, where there is no row.
sqlive_compose::read!(
    ComposerKeyAt(
        field_1: Option<String>,
        field_2: Option<String>,
        field_3: Option<String>,
        descending_1: bool,
        descending_2: bool,
        descending_3: bool,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue,
        order_5: SortValue,
        order_6: SortValue
    },
    r#"
    SELECT CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_1,
           CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_2,
           CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_3,
           CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_4,
           CASE WHEN NOT :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_5,
           CASE WHEN :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END AS order_6,
           listing.name_sort_key AS order_name, listing.artist_id AS order_id
    FROM composer_listing AS listing
    ORDER BY CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END,
             CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END DESC,
             CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END,
             CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END DESC,
             CASE WHEN NOT :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END,
             CASE WHEN :descending_3 THEN
               CASE :field_3 WHEN 'name' THEN listing.name_sort_key WHEN 'works' THEN listing.work_count WHEN 'releases' THEN listing.linked_release_count END END DESC,
             listing.name_sort_key, listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM composer_listing AS counted) - 1)
"#
);

// How many composers the list has.
sqlive_compose::read!(
    ComposerListLength { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_listing AS listing
"#
);

/// The key of a row with the value `sorts` for each criterion, the name key
/// `name` and the id `id`.
fn composer_key(sorts: [SortValue; 3], name: Vec<u8>, id: String) -> OrderKey {
    let mut values = Vec::from(sorts);
    values.extend([SortValue::Blob(name), SortValue::Text(id)]);
    OrderKey::new(values)
}

/// The values `key` holds, as the statements bind them: each criterion's, the
/// name key's and the id.
fn composer_key_values(key: &OrderKey) -> Result<([SortValue; 3], SortValue, String), DbError> {
    match key.values() {
        [sort_1, sort_2, sort_3, name, SortValue::Text(id)] => {
            Ok(([sort_1.clone(), sort_2.clone(), sort_3.clone()], name.clone(), id.clone()))
        }
        values => Err(DbError::unreadable("a composer list key", values)),
    }
}

/// The composer list's `window` under `sort`, each row with its key.
pub(crate) fn composer_window_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<3>,
    window: &ListWindow,
) -> Result<WindowRead<DbComposerSummary>, DbError> {
    let (mut key_1, mut key_2, mut key_3, mut key_name, mut key_id) =
        (SortValue::Null, SortValue::Null, SortValue::Null, SortValue::Null, None);
    if let Some(from) = &window.from {
        let ([sort_1, sort_2, sort_3], name, id) = composer_key_values(from)?;
        (key_1, key_2, key_3, key_name, key_id) = (sort_1, sort_2, sort_3, name, Some(id));
    }
    // A window from the top starts after no row. One from a key starts at the
    // key's row: the rows that don't come before it.
    let statement = ComposerWindowParams {
        field_1: sort.field(0),
        field_2: sort.field(1),
        field_3: sort.field(2),
        descending_1: sort.descending(0),
        descending_2: sort.descending(1),
        descending_3: sort.descending(2),
        key_1,
        key_2,
        key_3,
        key_name,
        key_id,
        limit: sql_bound(window.limit),
    };
    let rows = statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| {
            let key = composer_key(
                [
                    term(row.order_1, row.order_2),
                    term(row.order_3, row.order_4),
                    term(row.order_5, row.order_6),
                ],
                row.order_name,
                row.order_id,
            );
            Ok((row.shown.into_summary()?, key))
        })
        .collect::<Result<_, DbError>>()?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// The composer list's rows at `page`'s places under `sort`.
pub(crate) fn composer_page_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<3>,
    page: ListPage,
) -> Result<Vec<DbComposerSummary>, DbError> {
    let statement = ComposerRowsParams {
        field_1: sort.field(0),
        field_2: sort.field(1),
        field_3: sort.field(2),
        descending_1: sort.descending(0),
        descending_2: sort.descending(1),
        descending_3: sort.descending(2),
        offset: sql_bound(page.offset),
        limit: sql_bound(page.limit),
    };
    statement_rows(sql, &statement)?.into_iter().map(|row| row.shown.into_summary()).collect()
}

/// How many composers come before the row at `key` under `sort`.
pub(crate) fn composers_before_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<3>,
    key: &OrderKey,
) -> Result<u64, DbError> {
    let ([key_1, key_2, key_3], key_name, id) = composer_key_values(key)?;
    let statement = ComposersBeforeParams {
        field_1: sort.field(0),
        field_2: sort.field(1),
        field_3: sort.field(2),
        descending_1: sort.descending(0),
        descending_2: sort.descending(1),
        descending_3: sort.descending(2),
        key_1,
        key_2,
        key_3,
        key_name,
        key_id: Some(id),
    };
    list_count(statement_row(sql, &statement)?.entries)
}

/// The key of the composer at `position` under `sort`, or of the last one.
pub(crate) fn composer_key_at_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<3>,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    let statement = ComposerKeyAtParams {
        field_1: sort.field(0),
        field_2: sort.field(1),
        field_3: sort.field(2),
        descending_1: sort.descending(0),
        descending_2: sort.descending(1),
        descending_3: sort.descending(2),
        index: sql_bound(position),
    };
    Ok(statement_optional(sql, &statement)?.map(|row| {
        composer_key(
            [
                term(row.order_1, row.order_2),
                term(row.order_3, row.order_4),
                term(row.order_5, row.order_6),
            ],
            row.order_name,
            row.order_id,
        )
    }))
}

/// How many composers the list has.
pub(crate) fn composer_list_length_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    list_count(statement_row(sql, &ComposerListLengthParams {})?.entries)
}

// The first `limit` composers library search finds for `pattern`, in name order,
// ties going by id, each beside how many rows the statement lists.
sqlive_compose::read!(
    FirstComposersFound(pattern: String, limit: i64) { listed: i64 },
    r#"
    SELECT shown.*, COUNT(*) OVER () AS listed
    FROM (SELECT listing.artist_id, listing.name_sort_key
          FROM composer_listing AS listing
          WHERE listing.artist_id IN (
              SELECT matched.artist_id
              FROM @crate::album_search::ArtistsFound(pattern: :pattern) AS matched)
          ORDER BY listing.name_sort_key, listing.artist_id
          LIMIT :limit) AS found
    CROSS JOIN @crate::composer::ListedComposer AS shown ON shown.listed_id = found.artist_id
    ORDER BY found.name_sort_key, found.artist_id
"#
);

/// The composers [`FirstComposersFound`] lists for `pattern`, at most `limit`,
/// with how many it lists.
pub(crate) fn first_composers_found_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: i64,
) -> Result<(Vec<DbComposerSummary>, u64), DbError> {
    let statement = FirstComposersFoundParams { pattern: pattern.to_string(), limit };
    let rows = statement_rows(sql, &statement)?;
    let listed = list_count(rows.first().map_or(0, |row| row.listed))?;
    let found = rows.into_iter().map(|row| row.shown.into_summary()).collect::<Result<_, _>>()?;
    Ok((found, listed))
}

/// The composer list's sort as its statements bind it.
pub(crate) fn composer_sort(sort: &[ComposerSortCriterion]) -> ListSort<3> {
    ListSort::of(sort.iter().map(|criterion| {
        let field = match criterion.field {
            ComposerSortField::Name => "name",
            ComposerSortField::WorkCount => "works",
            ComposerSortField::LinkedReleaseCount => "releases",
        };
        (field, criterion.direction)
    }))
}

// The work `work`, as its rows show it, with its cover.
sqlive_compose::read!(
    WorkById(work: String),
    r#"
    SELECT work.*, cover.*
    FROM @bae_records::records::browse_fragments::WorkSummary AS work
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover
           ON cover.id = work.representative_release_id
    WHERE work.work_id = :work
"#
);

// The works that are parts of the work `work`, in its page's order, each
// beside how many there are.
sqlive_compose::read!(
    WorkPartRows(work: String) { children: i64 },
    r#"
    SELECT part.*, cover.*, COUNT(*) OVER () AS children
    FROM @bae_records::records::browse_fragments::WorkChildren(work: :work) AS child
    JOIN @bae_records::records::browse_fragments::WorkSummary AS part
      ON part.work_id = child.item_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover
           ON cover.id = part.representative_release_id
    WHERE child.section = 0
    ORDER BY child.place, child.title_key, child.item_id
"#
);

// The releases that perform the work `work`, in its page's order, each beside
// how many there are.
sqlive_compose::read!(
    WorkReleaseRows(work: String) { children: i64 },
    r#"
    SELECT release.*, cover.*, COUNT(*) OVER () AS children
    FROM @bae_records::records::browse_fragments::WorkChildren(work: :work) AS child
    JOIN @bae_records::records::browse_fragments::WorkReleaseRow AS release
      ON release.release_id = child.item_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover
           ON cover.id = release.release_id
    WHERE child.section = 1
    ORDER BY child.title_key, child.album_id, child.album_order, child.item_id
"#
);

// The children of the work `work` at the places from `offset`, at most
// `limit`, in their order: its parts, then the releases that perform it, each
// with what it shows.
sqlive_compose::read!(
    WorkChildRows(work: String, offset: i64, limit: i64),
    r#"
    SELECT child.*, part.*, part_cover.*, release.*, release_cover.*
    FROM @bae_records::records::browse_fragments::WorkChildren(work: :work) AS child
    LEFT JOIN @bae_records::records::browse_fragments::WorkSummary AS part
           ON child.section = 0 AND part.work_id = child.item_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS part_cover
           ON part_cover.id = part.representative_release_id
    LEFT JOIN @bae_records::records::browse_fragments::WorkReleaseRow AS release
           ON child.section = 1 AND release.release_id = child.item_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS release_cover
           ON release_cover.id = release.release_id
    ORDER BY child.section, child.place, child.title_key, child.album_id, child.album_order,
             child.item_id
    LIMIT :limit OFFSET :offset
"#
);

// How many children the work `work` has.
sqlive_compose::read!(
    WorkChildCount(work: String) { children: i64 },
    r#"
    SELECT COUNT(*) AS children
    FROM @bae_records::records::browse_fragments::WorkChildren(work: :work) AS child
"#
);

/// A work as its rows show it, with the cover [`CoverVersion`] read for the
/// release whose cover it shows.
pub(crate) fn work_summary(
    work: WorkSummary,
    cover: Option<CoverVersion>,
) -> Result<DbWorkSummary, DbError> {
    let summarized = work.into_summarized()?;
    let representative_cover = match &summarized.representative_release_id {
        Some(id) => bae_records::cover_ref(id, cover.and_then(CoverVersion::into_version)),
        None => None,
    };
    Ok(DbWorkSummary {
        work: summarized.work,
        representative_cover,
        composer_names: summarized.composer_names,
        linked_release_count: summarized.linked_release_count,
    })
}

/// A release that performs a work, with its cover.
fn work_release(
    release: WorkReleaseRow,
    cover: Option<CoverVersion>,
) -> Result<DbWorkReleaseSummary, DbError> {
    let release = release.into_release()?;
    Ok(DbWorkReleaseSummary {
        release_id: release.release_id,
        album_id: release.album_id,
        album_title: release.album_title,
        release_name: release.release_name,
        year: release.year,
        media: release.media,
        release_number: release.release_number,
        cover_version: cover.and_then(CoverVersion::into_version),
    })
}

/// The children of the work `work_id` at `page`'s places.
fn work_child_page_on(
    sql: &SqlReadContext<'_>,
    work_id: &str,
    page: ListPage,
) -> Result<Vec<DbWorkChild>, DbError> {
    let statement = WorkChildRowsParams {
        work: work_id.to_string(),
        offset: bae_store::list_windows::sql_bound(page.offset),
        limit: bae_store::list_windows::sql_bound(page.limit),
    };
    statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| {
            let missing =
                || DbError::unreadable("a work child with nothing to show", row.child.clone());
            Ok(if row.child.is_part()? {
                let part = row.part.ok_or_else(missing)?;
                DbWorkChild::Part(work_summary(part, row.part_cover)?)
            } else {
                let release = row.release.ok_or_else(missing)?;
                DbWorkChild::Release(work_release(release, row.release_cover)?)
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkDetailProjection {
    pub detail: Option<DbWorkDetail>,
}

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "composer_work_children_tests.rs"]
mod work_children_tests;
