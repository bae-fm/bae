//! The composer views: the composer list and a work's detail. A composer's
//! own page is `composer_detail`.
//!
//! Its tables and the triggers keeping them are `composer.sql`.

use super::*;
use crate::album_detail::{ComposerSummary, WorkDetail, WorkReleaseSummary, WorkSummary};

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
                Ok(composer_rows_on(&sql, &order, &order.everything(), limit, offset)?
                    .into_iter()
                    .map(|(row, _)| row)
                    .collect())
            })
            .await
    }

    /// Follow the composer list under `sort` in the windows its request
    /// names, each read from the row beside it when one is known
    /// (`list_windows`).
    pub fn subscribe_composer_browse(
        &self,
        sort: &[ComposerSortCriterion],
        initial_windows: bae_store::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        bae_store::LibraryPageWindows,
        ComposerBrowseProjection,
        bae_store::DbError,
    > {
        let order = composer_order(sort);
        let places = KnownPlaces::new();
        self.store.subscribe_reconfigurable(initial_windows, move |requested, sql| {
            let versions = list_versions_on(&sql, crate::versions::TABLE, &["composer_listing"])?;
            let total_count = composer_count_on(&sql)?;
            let WindowsRead { windows, found } =
                read_windows(&order, requested, places.under(&versions), |seek, limit, offset| {
                    composer_rows_on(&sql, &order, seek, limit, offset)
                })?;
            places.keep(versions, found);
            Ok(ComposerBrowseProjection {
                windows: windows
                    .into_iter()
                    .map(|(window, rows)| bae_store::LibraryBrowseWindow { window, rows })
                    .collect(),
                total_count,
            })
        })
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn find_work_detail(
        &self,
        work_id: &str,
    ) -> Result<Option<DbWorkDetail>, DbError> {
        let work_id = work_id.to_string();
        self.store.read(move |sql| find_work_detail_on(&sql, &work_id)).await
    }

    /// The work's page as its view shows it; `None` for an unknown work.
    #[cfg(any(test, feature = "test-utils"))]
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
}

/// One delivery of the composer list as the list shows it.
pub fn resolve_composer_browse(
    projection: ComposerBrowseProjection,
    request_revision: u64,
    cause: coven::LiveQueryCause,
) -> crate::LibraryBrowseSnapshot<ComposerSummary> {
    crate::LibraryBrowseSnapshot {
        windows: projection
            .windows
            .into_iter()
            .map(|window| bae_store::LibraryBrowseWindow {
                window: window.window,
                rows: window.rows.into_iter().map(ComposerSummary::from_raw).collect(),
            })
            .collect(),
        total_count: projection.total_count,
        request_revision,
        cause,
    }
}

/// One delivery of the work detail as the detail view shows it.
pub fn resolve_work_detail_projection(projection: WorkDetailProjection) -> Option<WorkDetail> {
    projection.detail.map(work_detail_of)
}

/// A work's page as its rows read it, each with its cover's version.
fn work_detail_of(raw: DbWorkDetail) -> WorkDetail {
    WorkDetail {
        work: WorkSummary::from_raw(raw.work),
        child_works: raw.child_works.into_iter().map(WorkSummary::from_raw).collect(),
        releases: raw.releases.into_iter().map(WorkReleaseSummary::from_raw).collect(),
        tracks: raw.tracks,
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
    let child_works = sql.query(
        &work_summary_query(
            Some("JOIN work_parts wp ON wp.child_work_id = w.id AND wp.parent_work_id = ?"),
            Some("ORDER BY wp.position, w.title"),
        ),
        params![work.work.id],
        row_to_work_summary,
    )?;
    let releases = work_release_rows(sql, &work.work.id)?;
    let tracks = sql.query(
        &format!(
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
         FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE tw.work_id = ?
         ORDER BY {releases}, t.position, tw.position",
            releases = work_release_order("a", "r"),
        ),
        params![work.work.id],
        |row| {
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
        },
    )?;
    Ok(Some(DbWorkDetail { work, child_works, releases, tracks }))
}

fn composer_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COUNT(*) FROM composer_listing", [], |row| row.get::<_, i64>(0))
        .map(|count| count as u64)
        .map_err(DbError::from)
}

/// The composer list's rows `seek` reads in `order`, at most `limit` from
/// `offset`, each with its key.
pub(crate) fn composer_rows_on(
    sql: &SqlReadContext<'_>,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<DbComposerSummary>>, DbError> {
    let statement = listing_window_query(composer_listing_query, order, seek);
    Ok(seek.bound(limit, offset, |params| {
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
                listing.linked_release_count,
                listing.unlinked_credit_count{keys}
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
/// `release`: albums by title, compared without case as the composer page
/// compares titles, then by id, so two albums sharing a title don't
/// interleave; each album's releases in album order. The work's page lists
/// its releases and their tracks in it, and its cover is the first.
fn work_release_order(album: &str, release: &str) -> String {
    format!("{album}.title COLLATE NOCASE, {album}.id, {release}.album_order, {release}.id")
}

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
                (
                    SELECT GROUP_CONCAT(composer.name, ', ' ORDER BY credit.position)
                    FROM (
                        SELECT {work_artist} AS artist_id, MIN(wa.position) AS position
                        FROM work_artists wa
                        WHERE wa.work_id = w.id
                        GROUP BY 1
                    ) credit
                    JOIN artists composer ON composer.id = credit.artist_id
                ) AS composer_names,
                (
                    SELECT COUNT(DISTINCT tr.release_id)
                    FROM track_works tw
                    JOIN tracks tr ON tr.id = tw.track_id
                    WHERE tw.work_id = w.id
                ) AS linked_release_count",
        work_artist = shown_artist_id("wa.artist_id"),
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
        representative_release_id,
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
        unlinked_credit_count: row.get("unlinked_credit_count")?,
    })
}

fn work_release_rows(
    sql: &SqlReadContext<'_>,
    work_id: &str,
) -> Result<Vec<DbWorkReleaseSummary>, DbError> {
    sql.query(
        &format!(
            "SELECT DISTINCT
            r.id AS release_id,
            r.album_id,
            a.title AS album_title,
            r.release_name,
            r.year AS release_year,
            {media} AS release_media,
            (
                SELECT COUNT(*)
                FROM releases indexed_release
                WHERE indexed_release.album_id = r.album_id
                  AND (
                      indexed_release.album_order < r.album_order
                      OR (
                          indexed_release.album_order = r.album_order
                          AND indexed_release.id <= r.id
                      )
                  )
            ) AS release_index,
            {cover} AS cover_version
         FROM track_works tw
         JOIN tracks t ON t.id = tw.track_id
         JOIN releases r ON r.id = t.release_id
         JOIN albums a ON a.id = r.album_id
         WHERE tw.work_id = ?
         ORDER BY {order}",
            order = work_release_order("a", "r"),
            cover = cover_version_sql("r.id"),
            media = release_media_sql("r.id"),
        ),
        params![work_id],
        row_to_work_release_summary,
    )
    .map_err(DbError::from)
}

fn row_to_work_release_summary(row: &Row<'_>) -> rusqlite::Result<DbWorkReleaseSummary> {
    Ok(DbWorkReleaseSummary {
        release_id: row.get("release_id")?,
        album_id: row.get("album_id")?,
        album_title: row.get("album_title")?,
        release_name: row.get("release_name")?,
        year: row.get("release_year")?,
        media: read_media(row, "release_media")?,
        release_index: row.get("release_index")?,
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
                ComposerSortField::Name => "listing.name",
                ComposerSortField::WorkCount => "listing.work_count",
                ComposerSortField::LinkedReleaseCount => "listing.linked_release_count",
            };
            SortTerm::running(field, c.direction)
        })
        .collect::<Vec<_>>();
    let ordered_by_name = sort.iter().any(|c| c.field == ComposerSortField::Name);
    ListOrder::named(terms, ordered_by_name, "listing.name", "listing.artist_id")
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerBrowseProjection {
    pub windows: Vec<bae_store::LibraryBrowseWindow<DbComposerSummary>>,
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkDetailProjection {
    pub detail: Option<DbWorkDetail>,
}

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;
