//! The album views: the album grid, flat or grouped by artist, its
//! selection, an album's detail, and library search.

use super::*;
use listing::{
    album_appearance_count_on, album_count_on, album_sections_on, grouped_artist_direction,
};
pub(crate) use listing::{
    album_browse_order, album_rows_on, album_seek_count_on, AlbumAppearanceRow,
};

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

    /// One window of the albums a search finds, in title order: at most `limit`
    /// albums starting `offset` albums in, and none past the search's
    /// [`crate::SEARCH_RESULT_LIMIT`]. A one-off read, so it pages by offset.
    pub async fn search_album_page(
        &self,
        query: &crate::LibrarySearchQuery,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<crate::album_detail::AlbumSearchResult>, DbError> {
        let pattern = search_pattern(query.as_str());
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let cap = crate::SEARCH_RESULT_LIMIT as i64;
        self.store
            .read(move |sql| search_albums_on(&sql, &pattern, limit, offset, cap))
            .process(|rows| Ok(rows.into_iter().map(AlbumSearchRow::process).collect::<Vec<_>>()))
            .await
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
            .subscribe_list_window(initial, order, move |sql, seek, limit, offset| {
                album_rows_on(sql, group_by_artist, &read_order, seek, limit, offset)
            })
            .process(|window| window.try_map_rows(AlbumAppearanceRow::process))
    }

    /// Follow how many rows the album grid has: its albums, and flat or
    /// grouped by artist, its rows, which grouped is an album under each of
    /// its artists. Its own query, never beside a window's rows: every album
    /// added or removed moves it.
    pub fn subscribe_album_count(
        &self,
        group_by_artist: bool,
    ) -> coven::LiveQuery<AlbumCounts, bae_store::DbError> {
        self.store.subscribe(move |sql| {
            let albums = album_count_on(&sql)?;
            let rows = if group_by_artist { album_appearance_count_on(&sql)? } else { albums };
            let can_shuffle =
                sql.query_row("SELECT EXISTS (SELECT 1 FROM tracks)", [], |row| row.get(0))?;
            Ok(AlbumCounts { albums, rows, can_shuffle })
        })
    }

    /// Follow how many rows of the album grid come before the row at a key,
    /// a count over index ranges that moves in place with the key.
    pub fn subscribe_album_position(
        &self,
        sort: &[AlbumSortCriterion],
        group_by_artist: bool,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let (_, order) = album_browse_order(sort, group_by_artist);
        self.store.subscribe_reconfigurable(initial, move |key, sql| {
            rows_before(&order, key, |seek| {
                album_seek_count_on(&sql, group_by_artist, &order, seek)
            })
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
                key_at(&order, position, &mut |seek, limit, offset| {
                    album_rows_on(&sql, group_by_artist, &order, seek, limit, offset)
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

    /// The album's detail as its view shows it. `None` for an unknown album,
    /// or one with no releases.
    pub async fn find_album_detail(
        &self,
        album_id: &str,
    ) -> Result<Option<crate::album_detail::AlbumDetail>, DbError> {
        let album_id = album_id.to_string();
        Ok(self
            .store
            .read(move |sql| find_album_detail_on(&sql, &album_id))
            .process(|rows| Ok::<_, DbError>(rows.map(AlbumDetailRows::process)))
            .await?
            .map(resolve_album_detail))
    }

    /// Follow the album `initial` names, or none. The detail view moves to
    /// another album through the request handle, not another query.
    pub fn subscribe_album_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, AlbumDetailProjection, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, |album_id, sql| match album_id {
                Some(album_id) => find_album_detail_on(&sql, album_id),
                None => Ok(None),
            })
            .process(|detail| {
                Ok(AlbumDetailProjection { detail: detail.map(AlbumDetailRows::process) })
            })
    }
}

/// One delivery of an album grid window as the grid shows it.
pub fn resolve_album_window(read: WindowRead<DbAlbumAppearance>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, |rows| {
        crate::ListRows::Albums(
            rows.into_iter().map(crate::album_detail::AlbumAppearance::from_raw).collect(),
        )
    })
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
    let mut releases = Vec::with_capacity(raw.releases.len());
    for ReleaseDetailRead { detail, download, actions } in raw.releases {
        let ctx = crate::album_detail::ReleaseResolveCtx {
            cover: bae_records::cover_ref(&detail.release.id, detail.cover_version.clone()),
            is_compilation: raw.album.is_compilation,
            album_year: raw.album.year,
        };
        releases
            .push(crate::album_detail::ReleaseDetail::from_raw(detail, download, actions, &ctx));
    }

    crate::album_detail::AlbumDetail {
        album: raw.album,
        artists: raw.artists,
        artist_line,
        releases,
        shows_release_choices: raw.shows_release_choices,
        primary_release_id,
        cover,
        rating: crate::album_detail::AlbumRating::from_raw(raw.ratings),
    }
}

/// What a track hit shows: the track, its release's album, its credit line and
/// its release's cover version.
static TRACK_HIT_SELECT: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "SELECT t.id, t.title, t.duration_ms, t.release_id, \
                r.album_id, a.title as album_title, {} as artist_name, \
                {} as cover_version \
         FROM tracks t \
         JOIN releases r ON t.release_id = r.id \
         JOIN albums a ON r.album_id = a.id",
        track_credits_json_sql("t"),
        bae_records::records::files::cover_version_sql("t.release_id"),
    )
});

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

/// The albums holding the search `pattern`, in title order, ties going by id:
/// the window of at most `limit` starting `offset` albums in, and ending at
/// most `cap` albums in.
fn search_albums_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: i64,
    offset: i64,
    cap: i64,
) -> Result<Vec<AlbumSearchRow>, DbError> {
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
                LIMIT MAX(MIN(?2, ?4 - ?3), 0) OFFSET ?3
            ) a
            ORDER BY {title_key}, a.id
            "#,
        primary_release = album_primary_release_sql(),
        primary_cover = bae_records::records::files::cover_version_sql(album_primary_release_sql()),
        artist_names = album_credits_json_sql(),
        album_matches = album_matches_sql("a.id", "?1"),
        title_key = crate::sort_key_sql("a.title"),
    );
    Ok(sql.query(&album_query, params![pattern, limit, offset, cap], |row| {
        Ok(AlbumSearchRow {
            id: row.get("id")?,
            title: row.get("title")?,
            year: row.get("year")?,
            primary_release_id: row.get("primary_release_id")?,
            cover_version: row.get("cover_version")?,
            artist_line: credit_line_column(row, "artist_name")?,
        })
    })?)
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
    let albums = search_albums_on(sql, pattern, limit, 0, limit)?;
    let tracks = sql.query(
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
    // The first track found, and failing that the first track of the first
    // album found that has a track: one row, whichever kind of hit ranks first.
    let top_playable: Option<DbTopPlayable> = sql
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
        .optional()?;
    // Whether any kind has a row, found by the statements that list each kind
    // and stopping at the first row of any.
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
    Ok(LibrarySearchRows { albums, artists, tracks, composers, works, top_playable, found })
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
    top_playable: Option<DbTopPlayable>,
    found: bool,
}

impl AlbumSearchRow {
    fn process(self) -> crate::album_detail::AlbumSearchResult {
        crate::album_detail::AlbumSearchResult::from_raw(DbAlbumSearchResult {
            id: self.id,
            title: self.title,
            year: self.year,
            primary_release_id: self.primary_release_id,
            cover_version: self.cover_version,
            artist_line: self.artist_line,
        })
    }
}

impl LibrarySearchRows {
    fn process(self) -> Result<DbLibrarySearchResults, DbError> {
        let albums = self
            .albums
            .into_iter()
            .map(|row| DbAlbumSearchResult {
                id: row.id,
                title: row.title,
                year: row.year,
                primary_release_id: row.primary_release_id,
                cover_version: row.cover_version,
                artist_line: row.artist_line,
            })
            .collect();
        Ok(DbLibrarySearchResults {
            albums,
            artists: self.artists,
            tracks: self.tracks,
            composers: self.composers,
            works: self.works,
            top_playable: self.top_playable,
            found: self.found,
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
    let releases = build_album_release_details_on(sql, album_id)?;
    if releases.is_empty() {
        return Ok(None);
    }
    let (primary_release_id, primary_cover_version, shows_release_choices): (
        String,
        Option<String>,
        bool,
    ) = sql.query_row(
        &format!(
            "SELECT {primary}, {cover}, \
                    (SELECT COUNT(*) > 1 FROM releases WHERE album_id = a.id) \
             FROM albums a WHERE a.id = ?",
            primary = album_primary_release_sql(),
            cover = bae_records::records::files::cover_version_sql(album_primary_release_sql()),
        ),
        params![album_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let ratings = bae_people::people::album_ratings_on(sql, album_id)?;
    Ok(Some(AlbumDetailRows {
        ratings,
        album,
        artists,
        releases,
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
    /// Whether the library holds a track, which shuffling it plays.
    pub can_shuffle: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumDetailProjection {
    pub detail: Option<DbAlbumDetail>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibrarySearchProjection {
    pub results: DbLibrarySearchResults,
}

#[cfg(test)]
#[path = "album_tests.rs"]
mod tests;
