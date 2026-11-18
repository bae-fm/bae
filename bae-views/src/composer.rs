//! The composer views: the composer list, a work's detail and a work's
//! children as a list. A composer's own page is `composer_detail`.
//!
//! Its tables and the triggers keeping them are `composer.sql`.

use super::*;
use crate::album_detail::{WorkChild, WorkDetail, WorkReleaseSummary};

/// The SQL creating this part's tables and triggers.
pub(super) const SQL: &str = include_str!("composer.sql");

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

    /// Follow one page of the composer list under `sort`: the composers from
    /// the one at the page's first place, read once.
    pub fn subscribe_composer_page(
        &self,
        sort: &[ComposerSortCriterion],
        page: ListPage,
    ) -> bae_store::live_query::LivePage<crate::ListRows> {
        let order = composer_order(sort);
        let read_order = order.clone();
        self.store.subscribe_list_page(
            page,
            order,
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

    /// Follow one page of the work `work_id`'s children: the children from
    /// the one at the page's first place, read once, as a media browser
    /// lists them.
    pub fn subscribe_work_child_page(
        &self,
        work_id: &str,
        page: ListPage,
    ) -> bae_store::live_query::LivePage<crate::ListRows> {
        let work_id = work_id.to_string();
        let order = work_child_order();
        let read_order = order.clone();
        self.store.subscribe_list_page(
            page,
            order,
            move |sql, seek, limit| work_child_rows_on(sql, &work_id, &read_order, seek, limit),
            |rows| Ok(work_child_rows(rows)),
        )
    }

    /// Follow how many children the work `work_id` has. The count runs as a
    /// separate query from the pages' rows.
    pub fn subscribe_work_child_count(
        &self,
        work_id: &str,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let work_id = work_id.to_string();
        self.store.subscribe(move |sql| {
            Ok(sql.query_row(
                &format!("SELECT COUNT(*) AS count FROM ({}) child", work_children_sql()),
                rusqlite::named_params! { ":work_id": work_id },
                |row| count_column::<u64>(row, "count"),
            )?)
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
    let work = sql
        .query_row(
            &work_summary_query(Some("WHERE w.id = ?"), None),
            params![work_id],
            row_to_work_summary,
        )
        .optional()?;
    let Some(work) = work else {
        return Ok(None);
    };
    // Each section's statement reads the work keyed `?1`.
    let child_works_query = work_summary_query(
        Some("JOIN work_parts wp ON wp.child_work_id = w.id AND wp.parent_work_id = ?1"),
        Some(&{
            let parts = WorkPartOrder::over("wp", "w");
            format!("ORDER BY {}, {}", parts.place, parts.title)
        }),
    );
    let releases_query = work_releases_query();
    let tracks_query = format!(
        "SELECT tw.id AS track_work_id, tw.work_id,
                tw.position, tw.source AS link_source, tw.created_at,
                t.id AS track_id, t.release_id AS track_release_id,
                t.title AS track_title, t.medium_id AS track_medium_id, t.side AS track_side,
                t.track_number AS track_track_number,
                t.duration_ms AS track_duration_ms,
                t.created_at AS track_created_at,
                a.id AS album_id, a.title AS album_title,
                a.year AS album_year,
                a.is_compilation AS album_is_compilation,
                a.created_at AS album_created_at
         {WORK_RELEASES_FROM}
         WHERE tw.work_id = ?1
         ORDER BY {releases}, t.position, tw.position",
        releases = work_release_order("a", "r"),
    );
    let child_works = sql.query(&child_works_query, params![work_id], row_to_work_summary)?;
    let releases = sql.query(&releases_query, params![work_id], row_to_work_release_summary)?;
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
    // Each count counts the rows of its section's statement above.
    let (child_work_count, release_count, track_count): (i64, i64, i64) = sql.query_row(
        &format!(
            "SELECT (SELECT COUNT(*) FROM ({child_works_query})),
                    (SELECT COUNT(*) FROM ({releases_query})),
                    (SELECT COUNT(*) FROM ({tracks_query}))"
        ),
        params![work_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Ok(Some(DbWorkDetail {
        work,
        child_works,
        releases,
        tracks,
        child_work_count: child_work_count as u64,
        release_count: release_count as u64,
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

/// The order a work's releases take over the album `album` and its release
/// `release`: albums by title, compared by sort key as the composer page
/// compares titles, then by id, so two albums sharing a title don't
/// interleave; each album's releases in album order. The work's page lists
/// its releases and their tracks in it, its cover is the first, and a media
/// browser lists its releases in it ([`work_children_sql`]).
struct WorkReleaseOrder {
    album_title: String,
    album_id: String,
    album_order: String,
    release_id: String,
}

impl WorkReleaseOrder {
    fn over(album: &str, release: &str) -> Self {
        Self {
            album_title: crate::sort_key_sql(&format!("{album}.title")),
            album_id: format!("{album}.id"),
            album_order: format!("{release}.album_order"),
            release_id: format!("{release}.id"),
        }
    }
}

/// [`WorkReleaseOrder`] as an ORDER BY list.
fn work_release_order(album: &str, release: &str) -> String {
    let order = WorkReleaseOrder::over(album, release);
    format!(
        "{}, {}, {}, {}",
        order.album_title, order.album_id, order.album_order, order.release_id
    )
}

/// The order a work's parts take over the link `part` (`work_parts`) and the
/// part's work `work`: by their place among the work's parts, then by title.
/// The work's page and a media browser both list its parts in it.
struct WorkPartOrder {
    place: String,
    title: String,
}

impl WorkPartOrder {
    fn over(part: &str, work: &str) -> Self {
        Self {
            place: format!("{part}.position"),
            title: crate::sort_key_sql(&format!("{work}.title")),
        }
    }
}

/// The tables a work's releases come from: each track `t` linked to a work
/// by `tw` (`track_works`), its release `r` and the release's album `a`.
/// Each release appears once per linked track, so a read of the releases
/// alone selects DISTINCT.
const WORK_RELEASES_FROM: &str = "FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id";

/// The release whose cover a work shows (a SQL expression over `w`): the
/// first of the releases performing it in [`work_release_order`]. That is the
/// first album's primary release when it performs the work, otherwise its
/// first release in album order that does: the primary release is first in
/// album order.
fn work_representative_release() -> String {
    format!(
        "(
                    SELECT r_cover.id
                    FROM track_works tw_cover
                    JOIN tracks tr ON tr.id = tw_cover.track_id
                    JOIN releases r_cover ON r_cover.id = tr.release_id
                    JOIN albums a_cover ON a_cover.id = r_cover.album_id
                    WHERE tw_cover.work_id = w.id
                    ORDER BY {order}
                    LIMIT 1
                )",
        order = work_release_order("a_cover", "r_cover"),
    )
}

/// The columns [`row_to_work_summary`] reads of the work `w`: its own, the
/// release whose cover it shows, its composers' names in credit order, and how
/// many releases perform it.
pub(crate) fn work_summary_columns() -> String {
    format!(
        "w.id AS work_id,
                w.title AS work_title,
                w.disambiguation AS work_disambiguation,
                w.work_type AS work_type,
                w.musicbrainz_work_id AS work_musicbrainz_id,
                w.created_at AS work_created_at,
                {representative_release} AS representative_release_id,
                {representative_cover} AS representative_cover_version,
                {composer_names} AS composer_names,
                (
                    SELECT COUNT(DISTINCT tr.release_id)
                    FROM track_works tw
                    JOIN tracks tr ON tr.id = tw.track_id
                    WHERE tw.work_id = w.id
                ) AS linked_release_count",
        composer_names = work_composer_names_sql("w.id"),
        representative_release = work_representative_release(),
        representative_cover = cover_version_sql(&work_representative_release()),
    )
}

/// The works `filter` names, each as [`row_to_work_summary`] reads it;
/// `filter` follows `FROM works w` and `tail` follows it.
pub(crate) fn work_summary_query(filter: Option<&str>, tail: Option<&str>) -> String {
    let mut query = format!("SELECT {}\n         FROM works w\n         ", work_summary_columns());
    if let Some(filter) = filter {
        query.push_str(filter);
    }
    if let Some(tail) = tail {
        query.push('\n');
        query.push_str(tail);
    }
    query
}

pub(crate) fn row_to_work_summary(row: &Row<'_>) -> rusqlite::Result<DbWorkSummary> {
    let representative_release_id: Option<String> = row.get("representative_release_id")?;
    let representative_cover = match &representative_release_id {
        Some(id) => bae_records::cover_ref(id, row.get("representative_cover_version")?),
        None => None,
    };
    Ok(DbWorkSummary {
        work: DbWork {
            id: row.get("work_id")?,
            title: row.get("work_title")?,
            disambiguation: row.get("work_disambiguation")?,
            work_type: row.get("work_type")?,
            musicbrainz_work_id: row.get("work_musicbrainz_id")?,
            created_at: rfc3339_column(row, "work_created_at")?,
        },
        representative_cover,
        composer_names: row.get("composer_names")?,
        linked_release_count: row.get("linked_release_count")?,
    })
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

/// The columns [`row_to_work_release_summary`] reads of the release `r`, its
/// album `a` and its place among the album's releases `number`.
fn work_release_columns() -> String {
    format!(
        "r.id AS release_id,
            r.album_id,
            a.title AS album_title,
            r.release_name,
            r.year AS release_year,
            {media} AS release_media,
            number.number AS release_number,
            {cover} AS cover_version",
        cover = cover_version_sql("r.id"),
        media = release_media_sql("r.id"),
    )
}

/// The releases holding a recording of the work keyed `?1`, in
/// [`work_release_order`], as [`row_to_work_release_summary`] reads them.
fn work_releases_query() -> String {
    format!(
        "SELECT DISTINCT {columns}
         {WORK_RELEASES_FROM}
         JOIN release_number_in_album number ON number.release_id = r.id
         WHERE tw.work_id = ?1
         ORDER BY {order}",
        columns = work_release_columns(),
        order = work_release_order("a", "r"),
    )
}

fn row_to_work_release_summary(row: &Row<'_>) -> rusqlite::Result<DbWorkReleaseSummary> {
    Ok(DbWorkReleaseSummary {
        release_id: row.get("release_id")?,
        album_id: row.get("album_id")?,
        album_title: row.get("album_title")?,
        release_name: row.get("release_name")?,
        year: row.get("release_year")?,
        media: read_media(row, "release_media")?,
        release_number: row.get("release_number")?,
        cover_version: row.get("cover_version")?,
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

/// The children of the work the parameter `:work_id` names, one row each,
/// with the columns its order ([`work_child_order`]) reads: its parts
/// (`section` 0) in [`WorkPartOrder`], then the releases performing it
/// (`section` 1) in [`WorkReleaseOrder`]. `item_id` is the part's
/// work or the release, which ends the order with a term no two children
/// share.
fn work_children_sql() -> String {
    let part = WorkPartOrder::over("part", "w");
    let release = WorkReleaseOrder::over("a", "r");
    format!(
        "SELECT 0 AS section, {place} AS place, {part_title} AS title_key,
                NULL AS album_id, NULL AS album_order, part.child_work_id AS item_id
         FROM work_parts part
         JOIN works w ON w.id = part.child_work_id
         WHERE part.parent_work_id = :work_id
         UNION ALL
         SELECT DISTINCT 1, NULL, {album_title}, {album_id}, {album_order}, {release_id}
         {WORK_RELEASES_FROM}
         WHERE tw.work_id = :work_id",
        place = part.place,
        part_title = part.title,
        album_title = release.album_title,
        album_id = release.album_id,
        album_order = release.album_order,
        release_id = release.release_id,
    )
}

/// The order of a work's children over [`work_children_sql`] aliased
/// `child`: its parts, then its releases, each in their own order.
fn work_child_order() -> ListOrder {
    let term = |column: &str| format!("child.{column}");
    ListOrder::new(vec![
        SortTerm::ascending(term("section")),
        SortTerm::ascending(term("place")).nullable(),
        SortTerm::ascending(term("title_key")),
        SortTerm::ascending(term("album_id")).nullable(),
        SortTerm::ascending(term("album_order")).nullable(),
        SortTerm::ascending(term("item_id")),
    ])
}

/// The work `work_id`'s children that `seek` reads in `order`, at most
/// `limit`, each with its key and what it shows: a part's summary or a
/// release's.
fn work_child_rows_on(
    sql: &SqlReadContext<'_>,
    work_id: &str,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
) -> Result<Vec<Keyed<DbWorkChild>>, DbError> {
    let statement = format!(
        "SELECT child.section, {work}, {release}, {keys}
         FROM ({children}) child
         LEFT JOIN works w ON child.section = 0 AND w.id = child.item_id
         LEFT JOIN releases r ON child.section = 1 AND r.id = child.item_id
         LEFT JOIN release_number_in_album number ON number.release_id = r.id
         LEFT JOIN albums a ON a.id = r.album_id
         WHERE {condition}
         ORDER BY {ordering} {limit}",
        work = work_summary_columns(),
        release = work_release_columns(),
        children = work_children_sql(),
        keys = order.key_columns(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
        limit = seek.limit_clause(),
    );
    Ok(seek.bound(limit, |params| {
        let mut params = params.to_vec();
        params.push((":work_id", &work_id as &dyn rusqlite::ToSql));
        sql.query(&statement, params.as_slice(), |row| {
            let child = match row.get::<_, i64>("section")? {
                0 => DbWorkChild::Part(row_to_work_summary(row)?),
                1 => DbWorkChild::Release(row_to_work_release_summary(row)?),
                other => {
                    return Err(column_conversion_error(
                        row,
                        "section",
                        format!("work child section {other} names no kind of child"),
                    ))
                }
            };
            Ok((child, order.key_of(row)?))
        })
    })?)
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
