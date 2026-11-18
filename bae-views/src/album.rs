//! The album views: the album grid, flat or grouped by artist, its
//! selection, an album's detail, an album's tracks as a list, library
//! search, the albums a search lists as a list (`search_albums`), and where
//! an album's card is in the grid (`place`).

use super::*;
use listing::{
    album_appearance_count_on, album_count_on, album_sections_on, grouped_artist_direction,
};
pub(crate) use listing::{
    album_browse_order, album_rows_before_on, album_rows_on, AlbumAppearanceRow,
};

mod listing;
pub(super) use listing::SQL as LISTING_SQL;
mod place;
pub use place::AlbumPlace;
mod search_albums;
use search_albums::first_search_albums_on;

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

    /// The track a spoken "play" of `query` plays: the search's
    /// `top_playable`. The method runs only the SQL statement that picks
    /// that track, not the whole search.
    pub async fn search_top_playable(
        &self,
        query: &crate::LibrarySearchQuery,
    ) -> Result<Option<DbTopPlayable>, DbError> {
        let pattern = search_pattern(query.as_str());
        self.store.read(move |sql| top_playable_on(&sql, &pattern)).await
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
        self.store.read(move |sql| search_library_on(&sql, &pattern, limit_i64)).await
    }

    /// Follow one page of the album `album_id`'s tracks: the tracks at the
    /// page's places, as a media browser lists them.
    pub fn subscribe_album_track_page(
        &self,
        album_id: &str,
        page: ListPage,
    ) -> bae_store::live_query::LivePage<crate::ListRows> {
        let album_id = album_id.to_string();
        let order = album_track_order();
        let read_order = order.clone();
        self.store.subscribe_list_page(
            page,
            order,
            move |sql, seek, limit| album_track_rows_on(sql, &album_id, &read_order, seek, limit),
            |rows| Ok(album_track_rows(rows)),
        )
    }

    /// Follow how many tracks the album `album_id` lists. The count runs as a
    /// separate query from the pages' rows.
    pub fn subscribe_album_track_count(
        &self,
        album_id: &str,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let album_id = album_id.to_string();
        self.store.subscribe(move |sql| {
            Ok(sql.query_row(
                &format!(
                    "SELECT COUNT(*) AS count {TRACK_HIT_FROM} WHERE {}",
                    album_track_filter()
                ),
                rusqlite::named_params! { ":album_id": album_id },
                |row| count_column::<u64>(row, "count"),
            )?)
        })
    }

    /// The track `track_id` of the release `release_id` as a browse item shows
    /// it; `None` when the release has no such track.
    pub async fn find_release_track(
        &self,
        release_id: &str,
        track_id: &str,
    ) -> Result<Option<crate::album_detail::TrackSearchResult>, DbError> {
        let release_id = release_id.to_string();
        let track_id = track_id.to_string();
        self.store
            .read(move |sql| {
                Ok(sql.query(
                    &format!("{} WHERE t.release_id = ?1 AND t.id = ?2", TRACK_HIT_SELECT.as_str()),
                    params![release_id, track_id],
                    track_hit_row,
                )?)
            })
            .process(|rows| {
                Ok(rows.into_iter().next().map(crate::album_detail::TrackSearchResult::from_raw))
            })
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
                Ok(LibrarySearchProjection { results })
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
                Ok(album_rows_on(&sql, false, &order, &order.at_offset(offset), limit)?
                    .into_iter()
                    .map(|(appearance, _)| appearance.row)
                    .collect::<Vec<_>>())
            })
            .process(|rows| rows.into_iter().map(AlbumSummaryRow::process).collect())
            .await?;
        Ok(rows.into_iter().map(crate::album_detail::AlbumSummary::from_raw).collect())
    }

    /// Follow one window of the album list under `sort`, flat or grouped by
    /// artist: a range of the grid's order (`list_windows`), which coven
    /// reruns for a write inside it alone.
    pub fn subscribe_album_window(
        &self,
        sort: &[AlbumSortCriterion],
        group_by_artist: bool,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbAlbumAppearance>, bae_store::DbError>
    {
        let (_, order) = album_browse_order(sort, group_by_artist);
        let read_order = order.clone();
        self.store
            .subscribe_list_window(initial, order, move |sql, seek, limit| {
                album_rows_on(sql, group_by_artist, &read_order, seek, limit)
            })
            .process(|window| window.try_map_rows(AlbumAppearanceRow::process))
    }

    /// Follow one page of the flat album list under `sort`: the albums from
    /// the page's first place, as the grid shows them.
    pub fn subscribe_album_page(
        &self,
        sort: &[AlbumSortCriterion],
        page: ListPage,
    ) -> bae_store::live_query::LivePage<crate::ListRows> {
        let (_, order) = album_browse_order(sort, false);
        let read_order = order.clone();
        self.store.subscribe_list_page(
            page,
            order,
            move |sql, seek, limit| album_rows_on(sql, false, &read_order, seek, limit),
            |rows| {
                Ok(album_rows(
                    rows.into_iter().map(AlbumAppearanceRow::process).collect::<Result<_, _>>()?,
                ))
            },
        )
    }

    /// Follow how many rows the album grid has: its albums, and flat or
    /// grouped by artist, its rows, which grouped is an album under each of
    /// its artists. The count is a query of its own, apart from the window's
    /// rows, because adding or removing any album changes it.
    pub fn subscribe_album_count(
        &self,
        group_by_artist: bool,
    ) -> coven::LiveQuery<AlbumCounts, bae_store::DbError> {
        self.store.subscribe(move |sql| {
            let albums = album_count_on(&sql)?;
            let rows = if group_by_artist { album_appearance_count_on(&sql)? } else { albums };
            let can_shuffle =
                sql.query_row("SELECT EXISTS (SELECT 1 FROM tracks)", [], |row| row.get(0))?;
            Ok(AlbumCounts { albums, rows, can_shuffle, grouped: group_by_artist })
        })
    }

    /// Follow how many rows of the album grid come before the row at a key,
    /// a count over index ranges; a new key changes this query's request
    /// instead of opening another query.
    pub fn subscribe_album_position(
        &self,
        sort: &[AlbumSortCriterion],
        group_by_artist: bool,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let (_, order) = album_browse_order(sort, group_by_artist);
        self.store.subscribe_reconfigurable(initial, move |key, sql| {
            album_rows_before_on(&sql, group_by_artist, &order, key)
        })
    }

    /// The key of the row at `position` of the album grid, or of its last row
    /// when it ends before it; `None` for an empty grid.
    pub async fn album_key_at(
        &self,
        sort: &[AlbumSortCriterion],
        group_by_artist: bool,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let (_, order) = album_browse_order(sort, group_by_artist);
        self.store
            .read(move |sql| {
                key_at(&order, position, &mut |seek, limit| {
                    album_rows_on(&sql, group_by_artist, &order, seek, limit)
                })
            })
            .await
    }

    /// Follow the artist sections of the album grid grouped by artist under
    /// `sort`: each artist's heading and where its albums start, read again
    /// whenever what they read changes.
    pub fn subscribe_album_sections(
        &self,
        sort: &[AlbumSortCriterion],
    ) -> coven::LiveQuery<Vec<crate::LibraryBrowseSection>, bae_store::DbError> {
        let direction = grouped_artist_direction(sort);
        self.store.subscribe(move |sql| album_sections_on(&sql, direction))
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

    pub async fn get_album_count(&self) -> Result<u64, DbError> {
        self.store.read(|sql| album_count_on(&sql)).await
    }

    /// The album's detail as its view shows it, showing its primary release.
    /// `None` for an unknown album, or one with no releases.
    pub async fn find_album_detail(
        &self,
        album_id: &str,
    ) -> Result<Option<crate::album_detail::AlbumDetail>, DbError> {
        let album_id = album_id.to_string();
        Ok(self
            .store
            .read(move |sql| find_album_detail_on(&sql, &album_id, None))
            .process(|rows| Ok::<_, DbError>(rows.map(AlbumDetailRows::process)))
            .await?
            .map(resolve_album_detail))
    }

    /// Follow the album `initial` names, or none, showing the release it
    /// chose. The detail view moves to another album or release through the
    /// request handle, not another query.
    pub fn subscribe_album_detail(
        &self,
        initial: Option<crate::album_detail::AlbumDetailRequest>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::album_detail::AlbumDetailRequest>,
        AlbumDetailProjection,
        bae_store::DbError,
    > {
        self.store
            .subscribe_reconfigurable(initial, |request, sql| match request {
                Some(request) => find_album_detail_on(
                    &sql,
                    &request.album_id,
                    request.chosen_release_id.as_deref(),
                ),
                None => Ok(None),
            })
            .process(|detail| {
                Ok(AlbumDetailProjection { detail: detail.map(AlbumDetailRows::process) })
            })
    }
}

/// One delivery of an album grid window as the grid shows it.
pub fn resolve_album_window(read: WindowRead<DbAlbumAppearance>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, album_rows)
}

/// The album grid's rows as the grid shows them.
fn album_rows(rows: Vec<DbAlbumAppearance>) -> crate::ListRows {
    crate::ListRows::Albums(
        rows.into_iter().map(crate::album_detail::AlbumAppearance::from_raw).collect(),
    )
}

/// One delivery of the album detail as the detail view shows it.
pub fn resolve_album_detail_projection(
    projection: AlbumDetailProjection,
) -> Option<crate::album_detail::AlbumDetail> {
    projection.detail.map(resolve_album_detail)
}

/// One delivery of a library search as its results show it.
pub fn resolve_library_search_projection(
    projection: LibrarySearchProjection,
) -> crate::album_detail::SearchResults {
    crate::album_detail::SearchResults::from_raw(projection.results)
}

/// Resolve a raw `DbAlbumDetail` into the display-ready `AlbumDetail`: joins artist names, formats labels,
/// groups tracks by side, and builds galleries.
fn resolve_album_detail(raw: DbAlbumDetail) -> crate::album_detail::AlbumDetail {
    let artist_line = bae_records::credits::library_line(&raw.artists);
    let primary_release_id = raw.primary_release_id;

    let cover = bae_records::cover_ref(&primary_release_id, raw.primary_cover_version);
    let mut releases: Vec<crate::album_detail::ReleaseDetail> =
        Vec::with_capacity(raw.releases.len());
    for ReleaseDetailRead { detail, download, actions } in raw.releases {
        let ctx = crate::album_detail::ReleaseResolveCtx {
            cover: bae_records::cover_ref(&detail.release.id, detail.cover_version.clone()),
            is_compilation: raw.album.is_compilation,
            album_year: raw.album.year,
        };
        releases
            .push(crate::album_detail::ReleaseDetail::from_raw(detail, download, actions, &ctx));
    }

    // One read returned the releases and the shown release's place among
    // them, so the place always names one of them.
    let shown_release = releases[raw.shown_release_index as usize].clone();
    crate::album_detail::AlbumDetail {
        album: raw.album,
        artists: raw.artists,
        artist_line,
        releases,
        shown_release,
        shown_release_index: raw.shown_release_index,
        shown_release_can_become_primary: raw.shown_release_can_become_primary,
        shows_release_choices: raw.shows_release_choices,
        primary_release_id,
        cover,
        rating: crate::album_detail::AlbumRating::from_raw(raw.ratings),
    }
}

/// What a track hit shows: the track, its release's album, its credit line and
/// its release's cover version, as columns of [`TRACK_HIT_FROM`].
static TRACK_HIT_COLUMNS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "t.id, t.title, t.duration_ms, t.release_id, \
         r.album_id, a.title as album_title, {} as artist_name, \
         {} as cover_version",
        track_credits_json_sql("t"),
        bae_records::records::files::cover_version_sql("t.release_id"),
    )
});

/// The tables a track hit reads: the track `t`, its release `r` and the
/// release's album `a`.
const TRACK_HIT_FROM: &str = "FROM tracks t \
     JOIN releases r ON t.release_id = r.id \
     JOIN albums a ON r.album_id = a.id";

/// A track hit's statement up to its WHERE clause.
static TRACK_HIT_SELECT: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| format!("SELECT {} {TRACK_HIT_FROM}", TRACK_HIT_COLUMNS.as_str()));

fn track_hit_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DbTrackSearchResult> {
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
}

/// The rows `statement` returns, in its order, each as `read` reads it, and
/// how many rows that is. The count is a window over the statement's own
/// rows in the same statement, so it counts the rows the statement lists
/// (after its `LIMIT`); a statement that returns no rows counts zero.
fn counted_rows<T>(
    sql: &SqlReadContext<'_>,
    statement: &str,
    params: impl rusqlite::Params,
    mut read: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Result<(Vec<T>, u64), DbError> {
    let rows = sql.query(
        &format!("SELECT listed.*, COUNT(*) OVER () AS section_count FROM ({statement}) AS listed"),
        params,
        |row| Ok((read(row)?, bae_store::read::count_column::<u64>(row, "section_count")?)),
    )?;
    let count = rows.first().map_or(0, |(_, count)| *count);
    Ok((rows.into_iter().map(|(row, _)| row).collect(), count))
}

/// The library rows holding the search `pattern` (see [`search_pattern`]),
/// each kind's first `limit` in its order, ties going by id. Each kind finds
/// its rows through its search table, then reads what it shows for those
/// rows alone.
fn search_library_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: i64,
) -> Result<DbLibrarySearchResults, DbError> {
    // Each section's statement counts its own rows beside them.
    let (albums, album_count) = first_search_albums_on(sql, pattern, limit as u64)?;
    let (tracks, track_count) = counted_rows(
        sql,
        &format!(
            r#"
            {select}
            WHERE t.id IN (
                SELECT found.id FROM tracks found
                WHERE found.id IN (SELECT track_id FROM track_search WHERE title GLOB ?1)
                ORDER BY {sort_title_key}, found.id
                LIMIT ?2
            )
            ORDER BY {title_key}, t.id
            "#,
            select = TRACK_HIT_SELECT.as_str(),
            sort_title_key = crate::sort_key_sql("found.title"),
            title_key = crate::sort_key_sql("t.title"),
        ),
        params![pattern, limit],
        track_hit_row,
    )?;
    let (artists, artist_count) = counted_rows(
        sql,
        &artist_listing_query(
            None,
            Some(&format!("WHERE ar.id IN ({})", artists_found_sql("?1"))),
            Some("ORDER BY listing.name_sort_key, listing.artist_id LIMIT ?2"),
        ),
        params![pattern, limit],
        row_to_artist_summary,
    )?;
    let (composers, composer_count) = counted_rows(
        sql,
        &composer_listing_query(
            None,
            Some(&format!("WHERE composer.id IN ({})", artists_found_sql("?1"))),
            Some("ORDER BY listing.name_sort_key, listing.artist_id LIMIT ?2"),
        ),
        params![pattern, limit],
        row_to_composer_summary,
    )?;
    let (works, work_count) = counted_rows(
        sql,
        &work_summary_query(
            Some(&format!(
                "WHERE w.id IN (
                    SELECT found.id FROM works found
                    WHERE found.id IN (SELECT work_id FROM work_search WHERE title GLOB ?1)
                    ORDER BY {}, found.id
                    LIMIT ?2
                )",
                crate::sort_key_sql("found.title"),
            )),
            Some(&format!("ORDER BY {}, w.id", crate::sort_key_sql("w.title"))),
        ),
        params![pattern, limit],
        row_to_work_summary,
    )?;
    let top_playable = top_playable_on(sql, pattern)?;
    // This query checks whether any album, track, artist, composer or work
    // matches, through the statements that list each kind, and stops at the
    // first match.
    let found: bool = sql.query_row(
        &format!(
            "SELECT EXISTS (SELECT 1 FROM albums a WHERE {album_matches}) \
                 OR EXISTS (SELECT 1 FROM tracks t \
                            WHERE t.id IN (SELECT track_id FROM track_search WHERE title GLOB ?1)) \
                 OR EXISTS ({artists}) OR EXISTS ({composers}) OR EXISTS ({works})",
            album_matches = album_matches_sql("a.id", "?1"),
            artists = artist_listing_query(
                None,
                Some(&format!("WHERE ar.id IN ({})", artists_found_sql("?1"))),
                Some("LIMIT 1"),
            ),
            composers = composer_listing_query(
                None,
                Some(&format!("WHERE composer.id IN ({})", artists_found_sql("?1"))),
                Some("LIMIT 1"),
            ),
            works = work_summary_query(
                Some("WHERE w.id IN (SELECT work_id FROM work_search WHERE title GLOB ?1)"),
                Some("LIMIT 1"),
            ),
        ),
        params![pattern],
        |row| row.get(0),
    )?;
    let section_counts = crate::SearchSectionCounts {
        albums: album_count,
        artists: artist_count,
        tracks: track_count,
        composers: composer_count,
        works: work_count,
    };
    Ok(DbLibrarySearchResults {
        albums,
        artists,
        tracks,
        composers,
        works,
        top_playable,
        found,
        section_counts,
    })
}

/// The first track `pattern` finds, and failing that the first track of the
/// first album it finds that has a track: one row, whichever kind of hit
/// ranks first. A spoken "play" of a search plays it.
fn top_playable_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
) -> Result<Option<DbTopPlayable>, DbError> {
    Ok(sql
        .query_row(
            &format!(
                r#"
                SELECT release_id, track_id FROM (
                    SELECT t.release_id AS release_id, t.id AS track_id, 0 AS kind,
                           {track_key} AS found_key, t.id AS tie
                    FROM tracks t
                    WHERE t.id IN (SELECT track_id FROM track_search WHERE title GLOB ?1)
                    UNION ALL
                    SELECT a.release_id, (SELECT id FROM tracks
                                          WHERE release_id = a.release_id
                                          ORDER BY position, id LIMIT 1),
                           1, {album_key}, a.id
                    FROM (SELECT a.id, a.title, {primary_release} AS release_id
                          FROM albums a WHERE {album_matches}) a
                )
                WHERE track_id IS NOT NULL
                ORDER BY kind, found_key, tie LIMIT 1
                "#,
                track_key = crate::sort_key_sql("t.title"),
                album_key = crate::sort_key_sql("a.title"),
                primary_release = album_primary_release_sql(),
                album_matches = album_matches_sql("a.id", "?1"),
            ),
            params![pattern],
            |row| Ok(DbTopPlayable { release_id: row.get(0)?, track_id: row.get(1)? }),
        )
        .optional()?)
}

/// The album `album_id`'s detail, showing the release `chosen_release_id`
/// while the album holds it and the primary release otherwise.
fn find_album_detail_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
    chosen_release_id: Option<&str>,
) -> Result<Option<AlbumDetailRows>, DbError> {
    let Some(album) = find_album_by_id_on(sql, album_id)? else {
        return Ok(None);
    };
    let artists = get_artists_for_album_on(sql, album_id)?;
    let releases = build_album_release_details_on(sql, album_id)?;
    if releases.is_empty() {
        return Ok(None);
    }
    // The shown release is the chosen one when the album holds it, else the
    // first in the album's order, which is the primary release.
    let (primary_release_id, primary_cover_version, shows_release_choices, shown_release_index): (
        String,
        Option<String>,
        bool,
        u32,
    ) = sql.query_row(
        &format!(
            "SELECT {primary}, {cover}, \
                    (SELECT COUNT(*) > 1 FROM releases WHERE album_id = a.id), \
                    (SELECT place FROM ( \
                        SELECT id, ROW_NUMBER() OVER (ORDER BY album_order, id) - 1 AS place \
                        FROM releases WHERE album_id = a.id \
                     ) ORDER BY id IS ?2 DESC, place LIMIT 1) \
             FROM albums a WHERE a.id = ?1",
            primary = album_primary_release_sql(),
            cover = bae_records::records::files::cover_version_sql(album_primary_release_sql()),
        ),
        params![album_id, chosen_release_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let ratings = bae_people::people::album_ratings_on(sql, album_id)?;
    Ok(Some(AlbumDetailRows {
        ratings,
        album,
        artists,
        releases,
        shown_release_index,
        shows_release_choices,
        primary_release_id,
        primary_cover_version,
    }))
}

#[derive(Clone, PartialEq)]
struct AlbumDetailRows {
    album: DbAlbum,
    artists: Vec<LibraryCredit>,
    releases: Vec<ReleaseDetailRows>,
    shown_release_index: u32,
    shows_release_choices: bool,
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
            shown_release_index: self.shown_release_index,
            shown_release_can_become_primary: self.shown_release_index > 0,
            shows_release_choices: self.shows_release_choices,
            primary_release_id: self.primary_release_id,
            primary_cover_version: self.primary_cover_version,
            ratings: self.ratings,
        }
    }
}

/// How many rows the album grid has, and whether the library has a track to shuffle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumCounts {
    /// The albums.
    pub albums: u64,
    /// The grid's rows: the albums, or grouped by artist an appearance of
    /// each album under each of its artists.
    pub rows: u64,
    /// Whether the library holds at least one track, so Shuffle Library has
    /// something to play.
    pub can_shuffle: bool,
    /// Whether the grid groups its rows by artist, so the grid lays them out
    /// from the read that counts them.
    pub grouped: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumDetailProjection {
    pub detail: Option<DbAlbumDetail>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibrarySearchProjection {
    pub results: DbLibrarySearchResults,
}

/// An album's tracks as a media browser lists them.
fn album_track_rows(rows: Vec<DbTrackSearchResult>) -> crate::ListRows {
    crate::ListRows::AlbumTracks(
        rows.into_iter().map(crate::album_detail::TrackSearchResult::from_raw).collect(),
    )
}

/// The order of an album's tracks: playing order. A release holds one track
/// at each position, and the list holds one release's tracks, so the position
/// tells every two tracks apart.
fn album_track_order() -> ListOrder {
    ListOrder::new(vec![SortTerm::ascending("t.position")])
}

/// Which track hits ([`TRACK_HIT_FROM`]) the album `:album_id` lists: its
/// main release's tracks.
fn album_track_filter() -> String {
    format!("a.id = :album_id AND r.id = {}", album_primary_release_sql())
}

/// The album `album_id`'s tracks that `seek` reads in `order`, at most
/// `limit`, each with its key.
fn album_track_rows_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
) -> Result<Vec<Keyed<DbTrackSearchResult>>, DbError> {
    let statement = format!(
        "SELECT {columns}, {keys} {TRACK_HIT_FROM}
         WHERE {filter} AND {condition}
         ORDER BY {ordering} {limit}",
        columns = TRACK_HIT_COLUMNS.as_str(),
        keys = order.key_columns(),
        filter = album_track_filter(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
        limit = seek.limit_clause(),
    );
    Ok(seek.bound(limit, |params| {
        let mut params = params.to_vec();
        params.push((":album_id", &album_id as &dyn rusqlite::ToSql));
        sql.query(&statement, params.as_slice(), |row| {
            Ok((track_hit_row(row)?, order.key_of(row)?))
        })
    })?)
}

#[cfg(test)]
#[path = "album/tracks_tests.rs"]
mod track_list_tests;

#[cfg(test)]
#[path = "album_tests.rs"]
mod tests;

#[cfg(test)]
mod detail_tests;
