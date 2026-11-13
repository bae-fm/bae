//! The album views: the album grid, flat or grouped by artist, its
//! selection, an album's detail, and library search.

use super::*;
use listing::{
    album_appearance_count_on, album_browse_sections_query, album_browse_sources, album_count_on,
    grouped_artist_direction, SectionsDelivery,
};
pub(crate) use listing::{album_browse_order, album_rows_on, AlbumAppearanceRow};

mod listing;
pub(super) use listing::SQL as LISTING_SQL;

impl LibraryViews {
    /// Search the library for a parsed, non-blank query. The result count is
    /// the core-owned [`crate::SEARCH_RESULT_LIMIT`]; no caller passes
    /// one.
    pub async fn search_library(
        &self,
        query: &crate::LibrarySearchQuery,
    ) -> Result<crate::album_detail::SearchResults, DbError> {
        let rows = self.search_rows(query.as_str(), crate::SEARCH_RESULT_LIMIT).await?;
        Ok(crate::album_detail::SearchResults::from_raw(rows))
    }

    /// The albums, tracks, composers and works whose title or name `query`
    /// finds, at most `limit` of each, as their rows read.
    pub async fn search_rows(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<DbLibrarySearchResults, DbError> {
        let pattern = search_pattern(query);
        let limit_i64 = limit as i64;
        self.store
            .read(move |sql| search_library_on(&sql, &pattern, limit_i64))
            .process(LibrarySearchRows::process)
            .await
    }

    /// Follow the results of one library search. The request is the parsed
    /// query; `None` is no search, which reads nothing and delivers no results.
    /// A new query moves this subscription's request rather than opening
    /// another.
    pub fn subscribe_library_search(
        &self,
        initial: Option<crate::LibrarySearchQuery>,
        limit: usize,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::LibrarySearchQuery>,
        LibrarySearchProjection,
        bae_store::DbError,
    > {
        let limit_i64 = limit as i64;
        self.store
            .subscribe_reconfigurable(initial, move |request, sql| {
                let Some(query) = request else {
                    return Ok(None);
                };
                let pattern = search_pattern(query.as_str());
                Ok(Some(search_library_on(&sql, &pattern, limit_i64)?))
            })
            .process(|raw| {
                let Some(results) = raw else {
                    return Ok(LibrarySearchProjection::default());
                };
                Ok(LibrarySearchProjection { results: results.process()? })
            })
    }

    /// Every album, sorted by `sort` — or `created_at DESC` (newest first) when
    /// `sort` is empty.
    pub async fn get_albums(&self, sort: &[AlbumSortCriterion]) -> Result<Vec<DbAlbum>, DbError> {
        let (relation, order) = album_browse_order(sort, false);

        let query = format!(
            "SELECT \
                a.id, a.title, a.year, \
                a.is_compilation, \
                a.created_at \
            {relation} \
            ORDER BY {}",
            order.clause()
        );

        self.store.read(move |sql| sql.query(&query, [], row_to_album).map_err(DbError::from)).await
    }

    /// One window of the album list under `sort`, read by its offset.
    pub async fn get_album_page(
        &self,
        sort: &[AlbumSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<crate::album_detail::AlbumSummary>, DbError> {
        let (_, order) = album_browse_order(sort, false);
        let rows: Vec<DbAlbumSummary> = self
            .store
            .read(move |sql| {
                Ok(album_rows_on(&sql, false, &order, &order.everything(), limit, offset)?
                    .into_iter()
                    .map(|(appearance, _)| appearance.row)
                    .collect::<Vec<_>>())
            })
            .process(|rows| rows.into_iter().map(AlbumSummaryRow::process).collect())
            .await?;
        Ok(rows.into_iter().map(crate::album_detail::AlbumSummary::from_raw).collect())
    }

    /// Follow the album list under `sort`, flat or grouped by artist, in the
    /// windows its request names. A window next to one this read or the
    /// last read held is read from the row beside it (`list_windows`).
    pub fn subscribe_album_browse(
        &self,
        sort: &[AlbumSortCriterion],
        group_by_artist: bool,
        initial_windows: bae_store::LibraryPageWindows,
    ) -> coven::ReconfigurableLiveQuery<
        bae_store::LibraryPageWindows,
        AlbumBrowseProjection,
        bae_store::DbError,
    > {
        let (_, order) = album_browse_order(sort, group_by_artist);
        let sources = album_browse_sources(group_by_artist);
        let places = KnownPlaces::new();
        let sections = std::sync::Arc::new(SectionsDelivery::new(
            group_by_artist.then(|| album_browse_sections_query(grouped_artist_direction(sort))),
        ));
        let delivery = sections.clone();
        self.store
            .subscribe_reconfigurable(initial_windows, move |requested, sql| {
                let versions = list_versions_on(&sql, crate::versions::TABLE, sources)?;
                let sections_found = sections.read(&sql)?;
                let total_count = album_count_on(&sql)?;
                let row_count =
                    if group_by_artist { album_appearance_count_on(&sql)? } else { total_count };
                let WindowsRead { windows, found } = read_windows(
                    &order,
                    requested,
                    places.under(&versions),
                    |seek, limit, offset| {
                        album_rows_on(&sql, group_by_artist, &order, seek, limit, offset)
                    },
                )?;
                places.keep(versions, found);
                let windows = windows
                    .into_iter()
                    .map(|(window, rows)| bae_store::LibraryBrowseWindow { window, rows })
                    .collect::<Vec<_>>();
                Ok((windows, total_count, row_count, sections_found))
            })
            .process(move |(windows, total_count, row_count, sections_found)| {
                let windows = windows
                    .into_iter()
                    .map(|window: bae_store::LibraryBrowseWindow<AlbumAppearanceRow>| {
                        Ok(bae_store::LibraryBrowseWindow {
                            window: window.window,
                            rows: window
                                .rows
                                .into_iter()
                                .map(AlbumAppearanceRow::process)
                                .collect::<Result<_, _>>()?,
                        })
                    })
                    .collect::<Result<_, DbError>>()?;
                let sections = delivery.deliver(sections_found);
                Ok(AlbumBrowseProjection { windows, total_count, row_count, sections })
            })
    }

    /// Follow which of the albums `initial` names — the grid's
    /// multi-selection — the library no longer shows, by id. The selection
    /// changes by pointing the same query at new ids through its request
    /// handle, not by opening another one.
    pub fn subscribe_album_selection(
        &self,
        initial: BTreeSet<String>,
    ) -> coven::ReconfigurableLiveQuery<BTreeSet<String>, Vec<String>, bae_store::DbError> {
        self.store.subscribe_reconfigurable(initial, move |requested, sql| {
            let requested = serde_json::to_string(requested).expect("album ids serialize as JSON");
            Ok(sql.query(
                "SELECT requested.value FROM json_each(?1) requested \
                 WHERE NOT EXISTS (SELECT 1 FROM albums a WHERE a.id = requested.value) \
                 ORDER BY requested.value",
                [requested],
                |row| row.get::<_, String>(0),
            )?)
        })
    }

    /// The albums of `album_ids` the browse lists, each once at its first
    /// appearance in the browse order `sort` and `group_by_artist` name: the
    /// grid's selection as the grid lists it, whatever order it was picked in.
    pub async fn albums_in_browse_order(
        &self,
        sort: &[AlbumSortCriterion],
        album_ids: Vec<String>,
        group_by_artist: bool,
    ) -> Result<Vec<crate::album_detail::AlbumSummary>, DbError> {
        let (relation, order) = album_browse_order(sort, group_by_artist);
        let order_by = order.clause();
        let query = format!(
            "WITH appearance (album_id, idx) AS ( \
                 SELECT a.id, ROW_NUMBER() OVER (ORDER BY {order_by}) \
                 {relation} \
                 WHERE a.id IN (SELECT value FROM json_each(?1))) \
             {select} \
             FROM (SELECT album_id, MIN(idx) AS idx FROM appearance GROUP BY album_id) AS first \
             JOIN album_listing listing ON listing.album_id = first.album_id \
             JOIN albums a ON a.id = first.album_id \
             ORDER BY first.idx",
            select = album_summary_select(),
        );
        let album_ids = serde_json::to_string(&album_ids).expect("album ids serialize as JSON");
        let rows: Vec<DbAlbumSummary> = self
            .store
            .read(move |sql| Ok(sql.query(&query, [album_ids], AlbumSummaryRow::read)?))
            .process(|rows| rows.into_iter().map(AlbumSummaryRow::process).collect())
            .await?;
        Ok(rows.into_iter().map(crate::album_detail::AlbumSummary::from_raw).collect())
    }

    /// The album's first appearance in the browse order, or `None` if absent.
    /// Uses the same relation and order as the live page query.
    pub async fn get_album_index(
        &self,
        sort: &[AlbumSortCriterion],
        album_id: &str,
        group_by_artist: bool,
    ) -> Result<Option<u64>, DbError> {
        let (relation, order) = album_browse_order(sort, group_by_artist);
        let order_by = order.clause();
        let album_id = album_id.to_string();

        let query = format!(
            "SELECT idx FROM ( \
                SELECT a.id AS id, \
                    ROW_NUMBER() OVER (ORDER BY {order_by}) - 1 AS idx \
                {relation} \
            ) WHERE id = ? ORDER BY idx LIMIT 1"
        );

        self.store
            .read(move |sql| {
                sql.query_row(&query, params![album_id], |row| row.get::<_, i64>("idx"))
                    .optional()
                    .map(|idx| idx.map(|i| i as u64))
                    .map_err(DbError::from)
            })
            .await
    }

    pub async fn get_album_count(&self) -> Result<u64, DbError> {
        self.store.read(|sql| album_count_on(&sql)).await
    }

    /// The album's detail as its view shows it, against whether storage is
    /// set up. `None` for an unknown album, or one with no releases.
    pub async fn find_album_detail(
        &self,
        album_id: &str,
    ) -> Result<Option<crate::album_detail::AlbumDetail>, DbError> {
        let album_id = album_id.to_string();
        let Some(raw) = self
            .store
            .read(move |sql| find_album_detail_on(&sql, &album_id))
            .process(|rows| Ok::<_, DbError>(rows.map(AlbumDetailRows::process)))
            .await?
        else {
            return Ok(None);
        };
        let syncs = self.store.syncs().await?;
        Ok(Some(resolve_album_detail(raw, syncs)))
    }

    /// Follow the album `initial` names, or none. The detail view moves to
    /// another album through the request handle, not another query.
    pub fn subscribe_album_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, AlbumDetailProjection, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, |album_id, sql| {
                let syncs = syncs_on(&sql)?;
                let detail = match album_id {
                    Some(album_id) => find_album_detail_on(&sql, album_id)?,
                    None => None,
                };
                Ok((detail, syncs))
            })
            .process(|(detail, syncs)| {
                Ok(AlbumDetailProjection { detail: detail.map(AlbumDetailRows::process), syncs })
            })
    }
}

/// One delivery of the album grid as the grid shows it.
pub fn resolve_album_browse(
    projection: AlbumBrowseProjection,
    request_revision: u64,
    cause: coven::LiveQueryCause,
) -> crate::AlbumBrowseSnapshot {
    crate::AlbumBrowseSnapshot {
        list: crate::LibraryBrowseSnapshot {
            windows: projection
                .windows
                .into_iter()
                .map(|window| bae_store::LibraryBrowseWindow {
                    window: window.window,
                    rows: window
                        .rows
                        .into_iter()
                        .map(crate::album_detail::AlbumAppearance::from_raw)
                        .collect(),
                })
                .collect(),
            total_count: projection.total_count,
            request_revision,
            cause,
        },
        sections: projection.sections,
        row_count: projection.row_count,
    }
}

/// One delivery of the album detail as the detail view shows it.
pub fn resolve_album_detail_projection(
    projection: AlbumDetailProjection,
) -> Option<crate::album_detail::AlbumDetail> {
    let syncs = projection.syncs;
    projection.detail.map(|raw| resolve_album_detail(raw, syncs))
}

/// One delivery of a library search as its results show it.
pub fn resolve_library_search_projection(
    projection: LibrarySearchProjection,
) -> crate::album_detail::SearchResults {
    crate::album_detail::SearchResults::from_raw(projection.results)
}

/// Resolve a raw `DbAlbumDetail` into the display-ready `AlbumDetail`,
/// against whether storage is set up: joins artist names, formats labels,
/// groups tracks by side, and builds galleries.
fn resolve_album_detail(raw: DbAlbumDetail, syncs: bool) -> crate::album_detail::AlbumDetail {
    let artist_line = bae_records::credits::library_line(&raw.artists);
    let primary_release_id = raw.primary_release_id;

    let cover = bae_records::cover_ref(&primary_release_id, raw.primary_cover_version);
    let mut releases = Vec::with_capacity(raw.releases.len());
    for (i, ReleaseDetailRead { detail, download, my_stars }) in
        raw.releases.into_iter().enumerate()
    {
        let ctx = crate::album_detail::ReleaseResolveCtx {
            syncs,
            cover: bae_records::cover_ref(&detail.release.id, detail.cover_version.clone()),
            is_compilation: raw.album.is_compilation,
            album_year: raw.album.year,
            my_stars,
        };
        releases.push(crate::album_detail::ReleaseDetail::from_raw(detail, download, i, &ctx));
    }

    crate::album_detail::AlbumDetail {
        album: raw.album,
        artists: raw.artists,
        artist_line,
        releases,
        primary_release_id,
        cover,
        rating: crate::album_detail::AlbumRating::from_raw(raw.ratings),
    }
}

/// The library rows holding the search `pattern` (see [`search_pattern`]),
/// each kind's first `limit` in its order, ties going by id. Each kind finds
/// its rows through its search table, then reads what it shows for those
/// rows alone.
fn search_library_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: i64,
) -> Result<LibrarySearchRows, DbError> {
    let album_query = format!(
        r#"
            SELECT a.id, a.title, a.year,
                   {primary_release} AS primary_release_id,
                   {primary_cover} AS cover_version,
                   {artist_names} AS artist_name
            FROM (
                SELECT a.id, a.title, a.year FROM albums a
                WHERE {album_matches}
                ORDER BY {title_key}, a.id
                LIMIT ?2
            ) a
            ORDER BY {title_key}, a.id
            "#,
        primary_release = album_primary_release_sql(),
        primary_cover = bae_records::records::files::cover_version_sql(album_primary_release_sql()),
        artist_names = album_credits_json_sql(),
        album_matches = album_matches_sql("a.id", "?1"),
        title_key = crate::name_sort::sort_key_sql("a.title"),
    );
    let albums = sql.query(&album_query, params![pattern, limit], |row| {
        Ok(AlbumSearchRow {
            id: row.get("id")?,
            title: row.get("title")?,
            year: row.get("year")?,
            primary_release_id: row.get("primary_release_id")?,
            cover_version: row.get("cover_version")?,
            artist_line: credit_line_column(row, "artist_name")?,
        })
    })?;
    let tracks = sql.query(
        &format!(
            r#"
            SELECT t.id, t.title, t.duration_ms, t.release_id,
                   r.album_id, a.title as album_title, {} as artist_name,
                   {} as cover_version
            FROM tracks t
            JOIN releases r ON t.release_id = r.id
            JOIN albums a ON r.album_id = a.id
            WHERE t.id IN (
                SELECT track_id FROM track_search WHERE title GLOB ?1
                ORDER BY {sort_title_key}, track_id
                LIMIT ?2
            )
            ORDER BY {title_key}, t.id
            "#,
            track_credits_json_sql("t"),
            bae_records::records::files::cover_version_sql("t.release_id"),
            sort_title_key = crate::name_sort::sort_key_sql("sort_title"),
            title_key = crate::name_sort::sort_key_sql("t.title"),
        ),
        params![pattern, limit],
        |row| {
            Ok(DbTrackSearchResult {
                id: row.get("id")?,
                title: row.get("title")?,
                duration_ms: row.get("duration_ms")?,
                release_id: row.get("release_id")?,
                album_id: row.get("album_id")?,
                album_title: row.get("album_title")?,
                artist_line: credit_line_column(row, "artist_name")?,
                cover_version: row.get("cover_version")?,
            })
        },
    )?;
    let artists = sql.query(
        &artist_listing_query(
            None,
            Some(&format!("WHERE ar.id IN ({})", artists_found_sql("?1"))),
            Some("ORDER BY listing.name_sort_key, listing.artist_id LIMIT ?2"),
        ),
        params![pattern, limit],
        row_to_artist_summary,
    )?;
    let composers = sql.query(
        &composer_listing_query(
            None,
            Some(&format!("WHERE composer.id IN ({})", artists_found_sql("?1"))),
            Some("ORDER BY listing.name_sort_key, listing.artist_id LIMIT ?2"),
        ),
        params![pattern, limit],
        row_to_composer_summary,
    )?;
    let works = sql.query(
        &work_summary_query(
            Some(&format!(
                "WHERE w.id IN (
                    SELECT work_id FROM work_search WHERE title GLOB ?1
                    ORDER BY {}, work_id
                    LIMIT ?2
                )",
                crate::name_sort::sort_key_sql("sort_title"),
            )),
            Some(&format!("ORDER BY {}, w.id", crate::name_sort::sort_key_sql("w.title"))),
        ),
        params![pattern, limit],
        row_to_work_summary,
    )?;
    Ok(LibrarySearchRows { albums, artists, tracks, composers, works })
}

#[derive(Clone, PartialEq)]
struct AlbumSearchRow {
    id: String,
    title: String,
    year: Option<i32>,
    primary_release_id: String,
    cover_version: Option<String>,
    artist_line: bae_types::credits::CreditLine,
}

#[derive(Clone, PartialEq)]
struct LibrarySearchRows {
    albums: Vec<AlbumSearchRow>,
    artists: Vec<DbArtistSummary>,
    tracks: Vec<DbTrackSearchResult>,
    composers: Vec<DbComposerSummary>,
    works: Vec<DbWorkSummary>,
}

impl LibrarySearchRows {
    fn process(self) -> Result<DbLibrarySearchResults, DbError> {
        let albums = self
            .albums
            .into_iter()
            .map(|row| {
                Ok(DbAlbumSearchResult {
                    id: row.id,
                    title: row.title,
                    year: row.year,
                    primary_release_id: row.primary_release_id,
                    cover_version: row.cover_version,
                    artist_line: row.artist_line,
                })
            })
            .collect::<Result<_, DbError>>()?;
        Ok(DbLibrarySearchResults {
            albums,
            artists: self.artists,
            tracks: self.tracks,
            composers: self.composers,
            works: self.works,
        })
    }
}

fn find_album_detail_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> Result<Option<AlbumDetailRows>, DbError> {
    let Some(album) = find_album_by_id_on(sql, album_id)? else {
        return Ok(None);
    };
    let artists = get_artists_for_album_on(sql, album_id)?;
    let db_releases = get_releases_for_album_on(sql, album_id)?;
    if db_releases.is_empty() {
        return Ok(None);
    }
    let (primary_release_id, primary_cover_version): (String, Option<String>) = sql.query_row(
        &format!(
            "SELECT {primary}, {cover} FROM albums a WHERE a.id = ?",
            primary = album_primary_release_sql(),
            cover = bae_records::records::files::cover_version_sql(album_primary_release_sql()),
        ),
        params![album_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let releases = db_releases
        .into_iter()
        .map(|release| build_release_detail_on(sql, release))
        .collect::<Result<Vec<_>, _>>()?;
    let ratings = bae_people::people::album_ratings_on(sql, album_id)?;
    Ok(Some(AlbumDetailRows {
        ratings,
        album,
        artists,
        releases,
        primary_release_id,
        primary_cover_version,
    }))
}

#[derive(Clone, PartialEq)]
struct AlbumDetailRows {
    album: DbAlbum,
    artists: Vec<LibraryCredit>,
    releases: Vec<ReleaseDetailRows>,
    primary_release_id: String,
    primary_cover_version: Option<String>,
    ratings: bae_people::collections::AlbumRatings,
}

impl AlbumDetailRows {
    fn process(self) -> DbAlbumDetail {
        DbAlbumDetail {
            album: self.album,
            artists: self.artists,
            releases: self.releases.into_iter().map(ReleaseDetailRows::process).collect(),
            primary_release_id: self.primary_release_id,
            primary_cover_version: self.primary_cover_version,
            ratings: self.ratings,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumBrowseProjection {
    pub windows: Vec<bae_store::LibraryBrowseWindow<DbAlbumAppearance>>,
    pub total_count: u64,
    pub row_count: u64,
    /// The grid's artist sections as of this read, when they changed since
    /// the read before it or this is the first: `Some` is exactly the read
    /// whose rows they fit, and `None` says the last `Some` still stands.
    /// A flat grid has none: its first read is `Some` of nothing.
    pub sections: Option<Vec<crate::LibraryBrowseSection>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumDetailProjection {
    pub detail: Option<DbAlbumDetail>,
    /// Whether storage is set up on this device, as coven's sync status
    /// says.
    pub syncs: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibrarySearchProjection {
    pub results: DbLibrarySearchResults,
}

#[cfg(test)]
#[path = "album_tests.rs"]
mod tests;
