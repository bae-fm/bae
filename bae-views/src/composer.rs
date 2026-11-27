//! The composer views: the composer list, a work's detail and a work's
//! children as a list. A composer's own page is `composer_detail`.
//!
//! Its tables and the triggers keeping them are `composer.sql`.

use super::*;
use crate::album_detail::{WorkChild, WorkDetail, WorkReleaseSummary};
use bae_records::records::browse_fragments::{WorkReleaseRow, WorkSummary};
use bae_records::records::files::CoverVersion;
use bae_store::{statement_optional, statement_row, statement_rows};

impl LibraryViews {
    #[cfg(test)]
    pub(crate) async fn get_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbComposerSummary>, DbError> {
        let order = composer_order(sort);
        self.store
            .read(move |sql| {
                Ok(composer_rows_on(&sql, &order, &order.at_offset(offset), limit)?
                    .into_iter()
                    .map(|(row, _)| row)
                    .collect())
            })
            .await
    }

    /// Follow one window of the composer list under `sort`: a range of its
    /// order (`list_windows`), which coven reruns for a write inside it alone.
    pub fn subscribe_composer_window(
        &self,
        sort: &[ComposerSortCriterion],
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbComposerSummary>, bae_store::DbError>
    {
        let order = composer_order(sort);
        let read_order = order.clone();
        self.store.subscribe_list_window(initial, order, move |sql, seek, limit| {
            composer_rows_on(sql, &read_order, seek, limit)
        })
    }

    /// Follow one page of the composer list under `sort`: the composers at
    /// the page's places as each write leaves them.
    pub fn subscribe_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let order = composer_order(sort);
        let read_order = order.clone();
        self.store.subscribe_list_page(
            page,
            order,
            |order, seek| listing_window_query(composer_listing_query, order, seek),
            move |sql, seek, limit| composer_rows_on(sql, &read_order, seek, limit),
            |rows| Ok(crate::ListRows::Composers(rows)),
        )
    }

    /// Follow how many composers the list has. The count runs as a separate
    /// query from the windows' rows.
    pub fn subscribe_composer_count(&self) -> coven::LiveQuery<u64, bae_store::DbError> {
        self.store.subscribe(|sql| composer_count_on(&sql))
    }

    /// Follow how many composers come before the row at a key, a count over
    /// index ranges; the caller changes the key on the same query.
    pub fn subscribe_composer_position(
        &self,
        sort: &[ComposerSortCriterion],
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let order = composer_order(sort);
        self.store.subscribe_reconfigurable(initial, move |key, sql| {
            composer_rows_before_on(&sql, &order, key)
        })
    }

    /// The key of the composer at `position`, or of the last one when the
    /// list ends before it; `None` for an empty list.
    pub async fn composer_key_at(
        &self,
        sort: &[ComposerSortCriterion],
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let order = composer_order(sort);
        self.store
            .read(move |sql| {
                key_at(&order, position, &mut |seek, limit| {
                    composer_rows_on(&sql, &order, seek, limit)
                })
            })
            .await
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

/// How many composers come before the row at `key`.
pub(crate) fn composer_rows_before_on(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    key: &OrderKey,
) -> Result<u64, DbError> {
    bae_store::list_windows::rows_before_on(
        sql,
        "FROM composer_listing listing",
        None,
        &[],
        order,
        key,
    )
}

fn composer_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COUNT(*) FROM composer_listing", [], |row| row.get::<_, i64>(0))
        .map(|count| count as u64)
        .map_err(DbError::from)
}

/// The composer list's rows `seek` reads in `order`, at most `limit`, each
/// with its key.
pub(crate) fn composer_rows_on(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
) -> Result<Vec<Keyed<DbComposerSummary>>, DbError> {
    let statement = listing_window_query(composer_listing_query, order, seek);
    Ok(seek.bound(limit, |params| {
        sql.query(&statement, params, |row| Ok((row_to_composer_summary(row)?, order.key_of(row)?)))
    })?)
}

/// The composer list's rows (`composer_listing`), each with its artist, as
/// [`row_to_composer_summary`] reads them, and each row's key in `keyed`'s
/// order when one is given; `filter` and `tail` follow the join.
pub(crate) fn composer_listing_query(
    keyed: Option<&ListOrder>,
    filter: Option<&str>,
    tail: Option<&str>,
) -> String {
    let mut query = format!(
        "SELECT composer.id AS artist_id,
                composer.name AS artist_name,
                composer.sort_name AS artist_sort_name,
                composer.discogs_artist_id AS artist_discogs_artist_id,
                composer.musicbrainz_artist_id AS artist_musicbrainz_artist_id,
                composer.created_at AS artist_created_at,
                {image} AS artist_image_version,
                listing.work_count,
                listing.linked_release_count{keys}
         FROM composer_listing listing
         JOIN artists composer ON composer.id = listing.artist_id
         ",
        image = artist_image_version_sql("composer.id"),
        keys = key_columns(keyed),
    );
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

pub(crate) fn row_to_composer_summary(row: &Row<'_>) -> rusqlite::Result<DbComposerSummary> {
    let artist = row_to_artist_with_prefix(row, "artist_")?;
    Ok(DbComposerSummary {
        image: bae_records::artist_image_ref(&artist.id, row.get("artist_image_version")?),
        artist,
        work_count: row.get("work_count")?,
        linked_release_count: row.get("linked_release_count")?,
    })
}

/// The composer list's order, over `composer_listing listing`. Ties break by
/// name ascending, then id — a window needs a total order and a name is not
/// unique — and that tie-break is the whole order when `sort` is empty. A
/// name the sort already orders by is not repeated, so an index of
/// `composer_listing` serves each one-field sort. Mirrors `album_order`'s
/// `album_id` tie-break.
pub(crate) fn composer_order(sort: &[ComposerSortCriterion]) -> ListOrder {
    let terms = sort
        .iter()
        .map(|c| {
            let field = match c.field {
                ComposerSortField::Name => "listing.name_sort_key",
                ComposerSortField::WorkCount => "listing.work_count",
                ComposerSortField::LinkedReleaseCount => "listing.linked_release_count",
            };
            SortTerm::running(field, c.direction)
        })
        .collect::<Vec<_>>();
    let ordered_by_name = sort.iter().any(|c| c.field == ComposerSortField::Name);
    ListOrder::named(terms, ordered_by_name, "listing.name_sort_key", "listing.artist_id")
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
