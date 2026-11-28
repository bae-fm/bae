//! The album views: the album grid, flat or grouped by artist, its
//! selection, an album's detail, an album's tracks as a list, the albums a
//! search lists (`search_albums`), and where an album's card is in the grid
//! (`place`).

use super::*;
use bae_records::records::track_display::DisplayedTrack;
#[cfg(test)]
pub(crate) use card::ArtistAlbums;
pub(crate) use card::{artist_albums_on, AlbumAppearanceRow};
#[cfg(any(test, feature = "test-utils"))]
pub(crate) use listing::every_album_on;
pub(crate) use listing::{
    album_counts_on, album_key_at_on, album_page_on, album_sections_on, album_window_on,
    albums_before_on, albums_in_order_on, unshown_albums_on,
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
mod top_reads;
mod window_reads;
pub use place::{AlbumCard, AlbumPlace};
pub mod search_albums;

impl LibraryViews {
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

    /// Every album, sorted by `sort` — or `created_at DESC` (newest first) when
    /// `sort` is empty.
    #[cfg(any(test, feature = "test-utils"))]
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
pub(crate) fn release_track(
    shown: DisplayedTrack,
) -> Result<crate::album_detail::TrackSearchResult, DbError> {
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
