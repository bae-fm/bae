//! The album views: the album grid, flat or grouped by artist, its
//! selection, an album's detail, an album's tracks as a list, library
//! search, the albums a search lists as a list (`search_albums`), and where
//! an album's card is in the grid (`place`).

use super::*;
use bae_records::records::track_display::DisplayedTrack;
#[cfg(test)]
pub(crate) use card::ArtistAlbums;
pub(crate) use card::{artist_albums_on, AlbumAppearanceRow};
pub(crate) use listing::{
    album_counts_on, album_key_at_on, album_page_on, album_sections_on, album_window_on,
    albums_before_on, albums_in_order_on, every_album_on, unshown_albums_on,
};
use order::grouped_artist_direction;
pub(crate) use order::AlbumOrder;

mod card;
mod key_reads;
mod listing;
mod numbered_reads;
mod order;
pub mod order_fragments;
mod page_reads;
mod place;
mod position_reads;
mod window_reads;
pub use place::{AlbumCard, AlbumPlace};
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

    /// The track a spoken "play" of `query` plays: the first track the
    /// search finds, else the first track of the first album it finds. The
    /// method runs only the SQL statement that picks that track, not the
    /// whole search.
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
    /// page's places as each write leaves them, as a media browser lists
    /// them.
    pub fn subscribe_album_track_page(
        &self,
        album_id: &str,
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let album_id = album_id.to_string();
        self.store.subscribe(move |sql| album_track_page_on(&sql, &album_id, page)).process(
            |tracks| {
                Ok(crate::ListRows::AlbumTracks(
                    tracks.into_iter().map(release_track).collect::<Result<_, _>>()?,
                ))
            },
        )
    }

    /// Follow how many tracks the album `album_id` lists. The count runs as a
    /// separate query from the pages' rows.
    pub fn subscribe_album_track_count(
        &self,
        album_id: &str,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let statement = AlbumTrackCountParams { album: album_id.to_string() };
        self.store.subscribe(move |sql| Ok(bae_store::statement_row(&sql, &statement)?.tracks))
    }

    /// The track `track_id` of the release `release_id` as a browse item shows
    /// it, read once; `None` when the release has no such track.
    pub async fn find_release_track(
        &self,
        release_id: &str,
        track_id: &str,
    ) -> Result<Option<crate::album_detail::TrackSearchResult>, DbError> {
        let statement =
            ReleaseTrackParams { release: release_id.to_string(), track: track_id.to_string() };
        self.store
            .read(move |sql| statement_optional(&sql, &statement))
            .process(|row| row.map(|row| release_track(row.shown)).transpose())
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
        let order = AlbumOrder::new(sort, false);
        self.store.read(move |sql| every_album_on(&sql, &order)).await
    }

    /// One window of the album list under `sort`, read by its offset.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_album_page(
        &self,
        sort: &[AlbumSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<crate::album_detail::AlbumSummary>, DbError> {
        let (first, order) = AlbumOrder::flat(sort);
        let rows: Vec<DbAlbumSummary> = self
            .store
            .read(move |sql| {
                Ok(album_page_on(&sql, first, &order, ListPage { offset, limit })?
                    .into_iter()
                    .map(|appearance| appearance.row)
                    .collect::<Vec<_>>())
            })
            .process(|rows| rows.into_iter().map(AlbumSummaryRow::process).collect())
            .await?;
        Ok(rows.into_iter().map(crate::album_detail::AlbumSummary::from_raw).collect())
    }

    /// Follow one window of the album list under `sort`, flat or grouped by
    /// artist: the rows from the window's key on in the grid's order.
    pub fn subscribe_album_window(
        &self,
        sort: &[AlbumSortCriterion],
        group_by_artist: bool,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbAlbumAppearance>, bae_store::DbError>
    {
        let order = AlbumOrder::new(sort, group_by_artist);
        self.store
            .subscribe_reconfigurable(initial, move |window, sql| {
                album_window_on(&sql, &order, window)
            })
            .process(move |window| window.try_map_rows(|row| row.process(group_by_artist)))
    }

    /// Follow one page of the flat album list under `sort`: the albums from
    /// the page's first place, as the grid shows them.
    pub fn subscribe_album_page(
        &self,
        sort: &[AlbumSortCriterion],
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let (first, order) = AlbumOrder::flat(sort);
        self.store.subscribe(move |sql| album_page_on(&sql, first, &order, page)).process(|rows| {
            Ok(album_rows(
                rows.into_iter().map(|row| row.process(false)).collect::<Result<_, _>>()?,
            ))
        })
    }

    /// Follow how many rows the album grid has: its albums, and flat or
    /// grouped by artist, its rows, which grouped is an album under each of
    /// its artists. The count is a query of its own, apart from the window's
    /// rows, because adding or removing any album changes it.
    pub fn subscribe_album_count(
        &self,
        group_by_artist: bool,
    ) -> coven::LiveQuery<AlbumCounts, bae_store::DbError> {
        self.store.subscribe(move |sql| album_counts_on(&sql, group_by_artist))
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
        let order = AlbumOrder::new(sort, group_by_artist);
        self.store
            .subscribe_reconfigurable(initial, move |key, sql| albums_before_on(&sql, &order, key))
    }

    /// The key of the row at `position` of the album grid, or of its last row
    /// when it ends before it; `None` for an empty grid.
    pub async fn album_key_at(
        &self,
        sort: &[AlbumSortCriterion],
        group_by_artist: bool,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let order = AlbumOrder::new(sort, group_by_artist);
        self.store.read(move |sql| album_key_at_on(&sql, &order, position)).await
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
        self.store
            .subscribe_reconfigurable(initial, |requested, sql| unshown_albums_on(&sql, requested))
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
        let order = AlbumOrder::new(sort, group_by_artist);
        let rows: Vec<DbAlbumSummary> = self
            .store
            .read(move |sql| albums_in_order_on(&sql, &order, &album_ids))
            .process(|rows| rows.into_iter().map(AlbumSummaryRow::process).collect())
            .await?;
        Ok(rows.into_iter().map(crate::album_detail::AlbumSummary::from_raw).collect())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_album_count(&self) -> Result<u64, DbError> {
        self.store.read(|sql| Ok(album_counts_on(&sql, false)?.albums)).await
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
            .process(AlbumDetailRows::process)
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
            .subscribe_reconfigurable(initial, |request, sql| {
                request
                    .as_ref()
                    .map(|request| {
                        find_album_detail_on(
                            &sql,
                            &request.album_id,
                            request.chosen_release_id.as_deref(),
                        )
                    })
                    .transpose()
            })
            .process(|rows| {
                let detail = rows.map(AlbumDetailRows::process).transpose()?.flatten();
                Ok(AlbumDetailProjection { detail })
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
    for ReleaseDetailRead { detail, download, actions, tracks_rating_system } in raw.releases {
        let ctx = crate::album_detail::ReleaseResolveCtx {
            cover: bae_records::cover_ref(
                &detail.release.id,
                detail.cover.as_ref().map(|cover| cover.version.clone()),
            ),
            is_compilation: raw.album.is_compilation,
            album_year: raw.album.year,
        };
        releases.push(crate::album_detail::ReleaseDetail::from_raw(
            detail,
            download,
            actions,
            &ctx,
            tracks_rating_system,
        ));
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
        rating: crate::album_detail::AlbumRating::of(raw.album_rating_system, raw.ratings),
    }
}

// The track `track` of the release `release`, as playback shows a track.
sqlive_compose::read!(
    writer ReleaseTrack(release: String, track: String),
    r#"
    SELECT shown.*
    FROM @bae_records::records::track_display::DisplayedTrack AS shown
    WHERE shown.display_track_id = :track AND shown.release_id = :release
"#
);

/// The track `shown` as a media browser's item shows it.
fn release_track(shown: DisplayedTrack) -> Result<crate::album_detail::TrackSearchResult, DbError> {
    let id = shown.track_id().to_string();
    let duration_ms = shown.duration_ms();
    let display = shown.into_display()?;
    Ok(crate::album_detail::TrackSearchResult {
        id,
        title: display.title,
        duration_ms,
        release_id: display.release_id,
        album_id: display.album_id,
        album_title: display.album_title,
        artist_line: display.artist_line,
        cover: display.cover_image,
    })
}

/// What a track hit shows: the track, its release's album, its credit line and
/// its release's cover version, as columns over the track `t` and
/// [`TRACK_HIT_JOINS`].
static TRACK_HIT_COLUMNS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "t.id, t.title, t.duration_ms, t.release_id, \
         r.album_id, a.title as album_title, {} as artist_name, \
         {} as cover_version",
        track_credits_json_sql("t"),
        bae_records::records::files::cover_version_sql("t.release_id"),
    )
});

/// The tables a track hit reads past its track `t`: the track's release `r`
/// and the release's album `a`.
const TRACK_HIT_JOINS: &str = "JOIN releases r ON t.release_id = r.id \
     JOIN albums a ON r.album_id = a.id";

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
    // Each section counts the rows it lists.
    let (albums, album_count) = first_search_albums_on(sql, pattern, limit as u64)?;
    let (tracks, track_count) =
        counted_rows(sql, &first_tracks_found_sql(), params![pattern, limit], track_hit_row)?;
    let (artists, artist_count) = first_artists_found_on(sql, pattern, limit)?;
    let (composers, composer_count) = first_composers_found_on(sql, pattern, limit)?;
    let (works, work_count) = first_works_found_on(sql, pattern, limit)?;
    // Whether any kind has a match, through the conditions that list each
    // kind, stopping at the first.
    let found: bool = sql.query_row(
        &format!(
            "SELECT EXISTS (SELECT 1 FROM albums a WHERE {album_matches}) \
                 OR EXISTS (SELECT 1 FROM tracks t \
                            WHERE t.id IN (SELECT track_id FROM track_search WHERE title GLOB ?1)) \
                 OR EXISTS (SELECT 1 FROM artist_listing listed \
                            WHERE listed.artist_id IN ({artists_found})) \
                 OR EXISTS (SELECT 1 FROM composer_listing listed \
                            WHERE listed.artist_id IN ({artists_found})) \
                 OR EXISTS (SELECT 1 FROM work_search_entry entry WHERE {work_matches})",
            album_matches = album_matches_sql("a.id", "?1"),
            artists_found = artists_found_sql("?1"),
            work_matches = entries_found_sql("work", "?1"),
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
    Ok(DbLibrarySearchResults { albums, artists, tracks, composers, works, found, section_counts })
}

/// Whether the search entry `entry` of a `kind` (`track` or `work`) holds a
/// title the search pattern `pattern` (a SQL parameter) finds.
fn entries_found_sql(kind: &str, pattern: &str) -> String {
    format!("entry.entry IN (SELECT rowid FROM {kind}_search WHERE title GLOB {pattern})")
}

/// The first `limit` (`?2`) search entries of a `kind` (`track` or `work`)
/// whose title holds the search pattern `?1`, in title order, ties going by
/// id, as `{kind}_id` and `title_sort_key`. The read walks the entries' title
/// index, tests each entry against the titles the search finds, and stops at
/// the `limit`th; `INDEXED BY` holds it to that index, because left to choose,
/// SQLite reads the entries the search finds and sorts them all.
fn first_entries_found_sql(kind: &str) -> String {
    format!(
        "SELECT entry.{kind}_id, entry.title_sort_key \
         FROM {kind}_search_entry entry INDEXED BY idx_{kind}_search_entry_title \
         WHERE {found} \
         ORDER BY entry.title_sort_key, entry.{kind}_id LIMIT ?2",
        found = entries_found_sql(kind, "?1"),
    )
}

/// The track hits of the first `?2` tracks whose title holds the search
/// pattern `?1`, in title order, ties going by id. The statement reads each
/// hit's release, album, credits and cover only for those tracks.
fn first_tracks_found_sql() -> String {
    format!(
        "SELECT {columns} \
         FROM ({found}) found \
         CROSS JOIN tracks t ON t.id = found.track_id \
         {TRACK_HIT_JOINS} \
         ORDER BY found.title_sort_key, found.track_id",
        columns = TRACK_HIT_COLUMNS.as_str(),
        found = first_entries_found_sql("track"),
    )
}

// The first `limit` works whose title holds the search `pattern`, in title
// order, ties going by id, each beside how many rows the statement lists. The
// read walks the entries' title index, tests each entry against the titles the
// search finds, and stops at the `limit`th, as [`first_entries_found_sql`]
// does for tracks; it reads what each work shows for those works alone.
sqlive_compose::read!(
    writer FirstWorksFound(pattern: String, limit: i64) { listed: i64 },
    r#"
    SELECT work.*, cover.*, COUNT(*) OVER () AS listed
    FROM (SELECT entry.work_id, entry.title_sort_key
          FROM work_search_entry AS entry INDEXED BY idx_work_search_entry_title
          WHERE entry.entry IN (
              SELECT found_title.rowid FROM work_search AS found_title
              WHERE found_title.title GLOB :pattern)
          ORDER BY entry.title_sort_key, entry.work_id
          LIMIT :limit) AS found
    CROSS JOIN @bae_records::records::browse_fragments::WorkSummary AS work
            ON work.work_id = found.work_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover
           ON cover.id = work.representative_release_id
    ORDER BY found.title_sort_key, found.work_id
"#
);

/// The works [`FirstWorksFound`] lists for `pattern`, at most `limit`, with
/// how many it lists.
fn first_works_found_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: i64,
) -> Result<(Vec<DbWorkSummary>, u64), DbError> {
    let statement = FirstWorksFoundParams { pattern: pattern.to_string(), limit };
    let rows = bae_store::statement_rows(sql, &statement)?;
    let listed = list_count(rows.first().map_or(0, |row| row.listed))?;
    let works =
        rows.into_iter().map(|row| work_summary(row.work, row.cover)).collect::<Result<_, _>>()?;
    Ok((works, listed))
}

/// The first track `pattern` finds, and failing that the first track of the
/// first album it finds that has a track: one row, whichever kind of hit
/// ranks first. A spoken "play" of a search plays it.
fn top_playable_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
) -> Result<Option<DbTopPlayable>, DbError> {
    let read = |statement: String| {
        sql.query_row(&statement, params![pattern], |row| {
            Ok(DbTopPlayable { release_id: row.get(0)?, track_id: row.get(1)? })
        })
        .optional()
    };
    match read(top_track_statement())? {
        Some(top) => Ok(Some(top)),
        None => Ok(read(top_album_track_statement())?),
    }
}

/// The statement that reads the first track `?1` finds, in the order of the
/// track entries' stored title key, ties going by id.
fn top_track_statement() -> String {
    format!(
        "SELECT t.release_id, t.id \
         FROM track_search_entry entry INDEXED BY idx_track_search_entry_title \
         CROSS JOIN tracks t ON t.id = entry.track_id \
         WHERE {found} \
         ORDER BY entry.title_sort_key, entry.track_id LIMIT 1",
        found = entries_found_sql("track", "?1"),
    )
}

/// The statement that reads the first track of the first album `?1` finds
/// that has one: the albums in order of their stored title key, ties going
/// by id, each with its main release's first track. `INDEXED BY` holds the
/// read to the title index, because left to choose, SQLite reads the albums
/// the search finds and sorts them all.
fn top_album_track_statement() -> String {
    format!(
        "SELECT {primary_release} AS release_id, \
                (SELECT id FROM tracks WHERE release_id = {primary_release} \
                 ORDER BY position, id LIMIT 1) AS track_id \
         FROM album_listing listing INDEXED BY idx_album_listing_title \
         CROSS JOIN albums a ON a.id = listing.album_id \
         WHERE {album_matches} AND track_id IS NOT NULL \
         ORDER BY listing.title_sort_key, listing.album_id LIMIT 1",
        primary_release = album_primary_release_sql(),
        album_matches = album_matches_sql("a.id", "?1"),
    )
}

// The album `album` above its releases: its row, its credits and whether it
// has releases to choose among, and the library's rating system for albums with
// the ratings it shows; no row for an album the library does not hold.
sqlive_compose::read!(
    writer AlbumDetailHead(album: String),
    r#"
    SELECT album.*, detailed.*, ratings.*
    FROM @bae_records::records::fragments::AlbumRow AS album
    JOIN @bae_records::records::fragments::DetailedAlbum AS detailed
      ON detailed.detailed_album_id = :album
    JOIN @bae_people::people::fragments::AlbumRatingsShown(album: :album) AS ratings ON 1
    WHERE album.row_album_id = :album
"#
);

/// What [`AlbumDetailHead`] and the album's releases read of the album
/// `album_id`, in one snapshot, for a page showing the release
/// `chosen_release_id`.
fn find_album_detail_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
    chosen_release_id: Option<&str>,
) -> Result<AlbumDetailRows, DbError> {
    let head = AlbumDetailHeadParams { album: album_id.to_string() };
    Ok(AlbumDetailRows {
        head: bae_store::statement_optional(sql, &head)?,
        releases: build_album_release_details_on(sql, album_id)?,
        chosen_release_id: chosen_release_id.map(str::to_string),
    })
}

#[derive(Clone, PartialEq)]
struct AlbumDetailRows {
    head: Option<AlbumDetailHeadRow>,
    releases: Vec<ReleaseDetailRows>,
    chosen_release_id: Option<String>,
}

impl AlbumDetailRows {
    /// The album's detail, showing the chosen release while the album holds it
    /// and the primary release otherwise; `None` for an unknown album, or one
    /// with no releases.
    fn process(self) -> Result<Option<DbAlbumDetail>, DbError> {
        let releases: Vec<ReleaseDetailRead> =
            self.releases.into_iter().map(ReleaseDetailRows::process).collect::<Result<_, _>>()?;
        // The releases come in the album's order, whose first is the primary
        // release.
        let Some(primary) = releases.first() else {
            return Ok(None);
        };
        let primary_release_id = primary.detail.release.id.clone();
        let primary_cover_version =
            primary.detail.cover.as_ref().map(|cover| cover.version.clone());
        let Some(head) = self.head else {
            return Err(DbError::unreadable(
                format!("the album of release {primary_release_id}"),
                None::<String>,
            ));
        };
        let shown = self
            .chosen_release_id
            .and_then(|chosen| releases.iter().position(|held| held.detail.release.id == chosen))
            .unwrap_or(0);
        let shown_release_index = u32::try_from(shown)
            .map_err(|_error| DbError::unreadable("an album's count of releases", shown))?;
        let (album_rating_system, ratings) = head.ratings.into_ratings()?;
        let shows_release_choices = head.detailed.has_release_choices();
        Ok(Some(DbAlbumDetail {
            album: head.album.into_album()?,
            artists: head.detailed.into_credits()?,
            shown_release_index,
            shown_release_can_become_primary: shown_release_index > 0,
            shows_release_choices,
            releases,
            primary_release_id,
            primary_cover_version,
            ratings,
            album_rating_system,
        }))
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

// A page of the album `album`'s tracks: those of its primary release at the
// places from `offset` on, in playing order, at most `limit`. A release holds
// one track at each position, so the position tells every two tracks apart.
sqlive_compose::read!(
    writer AlbumTrackPage(album: String, offset: i64, limit: i64),
    r#"
    SELECT shown.*
    FROM @bae_records::records::fragments::PrimaryReleaseOfAlbum AS chosen
    CROSS JOIN @bae_records::records::track_display::DisplayedTrack AS shown
            ON shown.release_id = chosen.primary_release_id
    WHERE chosen.primary_of_album_id = :album
    ORDER BY shown.playing_position
    LIMIT :limit OFFSET :offset
"#
);

// How many tracks the album `album` lists: its primary release's.
sqlive_compose::read!(
    writer AlbumTrackCount(album: String) { tracks: u64 },
    r#"
    SELECT COUNT(*) AS tracks
    FROM @bae_records::records::fragments::PrimaryReleaseOfAlbum AS chosen
    CROSS JOIN @bae_records::records::spread_fragments::TrackRow AS held
            ON held.release_id = chosen.primary_release_id
    WHERE chosen.primary_of_album_id = :album
"#
);

/// The album `album_id`'s tracks at `page`'s places.
fn album_track_page_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
    page: ListPage,
) -> Result<Vec<DisplayedTrack>, DbError> {
    let statement = AlbumTrackPageParams {
        album: album_id.to_string(),
        offset: bae_store::list_windows::sql_bound(page.offset),
        limit: bae_store::list_windows::sql_bound(page.limit),
    };
    Ok(bae_store::statement_rows(sql, &statement)?.into_iter().map(|row| row.shown).collect())
}

#[cfg(test)]
mod item_tests;

#[cfg(test)]
#[path = "album/tracks_tests.rs"]
mod track_list_tests;

#[cfg(test)]
#[path = "album/order_tests.rs"]
mod stacked_sort_tests;

#[cfg(test)]
#[path = "album_tests.rs"]
mod tests;

#[cfg(test)]
mod detail_tests;
