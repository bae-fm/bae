//! What a server outside bae's screens lists (§8.1): every artist, an
//! artist's albums, a list of albums and a search, each one statement that
//! picks and pages its rows. An album there is a release, read through
//! records' [`ServedRelease`](bae_records::records::served::ServedRelease),
//! and a song a track, through
//! [`ServedTrack`](bae_records::records::served::ServedTrack).

use super::*;
use bae_records::records::served::{DbServedRelease, DbServedTrack};

// Each artist the artist list shows, as the list shows it, with how many
// releases the albums that credit it hold.
sqlive_compose::fragment!(
    ServedArtist { release_count: i64 },
    r#"
    SELECT shown.*,
           (SELECT COUNT(*)
            FROM album_artist_link AS link
            JOIN @bae_records::records::fragments::ReleaseAlbum AS held
              ON held.album_id = link.album_id
            WHERE link.artist_id = shown.listed_id) AS release_count
    FROM @crate::artist::ListedArtist AS shown
"#
);

// Every artist the artist list shows, in name order, ties going by id.
sqlive_compose::read!(
    writer EveryServedArtist,
    r#"
    SELECT served.*
    FROM artist_listing AS listing
    CROSS JOIN @crate::served::ServedArtist AS served ON served.listed_id = listing.artist_id
    ORDER BY listing.name_sort_key, listing.artist_id
"#
);

// The artist `artist` with each release of the albums that credit it, in its
// page's order (by year, an unknown year after every year, then by title and
// album), each album's releases in their album's order. The artist list shows
// only artists whose credits show an album, and every album holds a release
// (§2.1), so no row means the list doesn't show the artist.
sqlive_compose::read!(
    writer ServedArtistReleases(artist: String),
    r#"
    SELECT served.*, release.*
    FROM @crate::served::ServedArtist AS served
    JOIN album_artist_link AS link ON link.artist_id = served.listed_id
    JOIN @bae_records::records::served::ServedRelease AS release
      ON release.served_album_id = link.album_id
    WHERE served.listed_id = :artist
    ORDER BY link.sort_year, link.title_sort_key, link.album_id,
             release.album_order, release.served_release_id
"#
);

// The releases of the albums the album grid lists, the `limit` from the place
// `offset` in the order `order` names: `title`, by album title in the device's
// name order, then by when each was added; `newest`, latest added first;
// `year` and `year_back`, only those whose shown year is from `low` to `high`,
// by that year up and down; and `random`, in an order no two reads share.
// Ties go by when each release was added, then by id. The statement reads
// what a release shows only for the releases of the page.
sqlive_compose::read!(
    writer ServedAlbumList(order: String, low: i64, high: i64, offset: i64, limit: i64),
    r#"
    SELECT release.*
    FROM (SELECT held.served_release_id
          FROM album_listing AS listing
          CROSS JOIN @bae_records::records::served::ServedRelease AS held
                  ON held.served_album_id = listing.album_id
          WHERE :order NOT IN ('year', 'year_back') OR held.shown_year BETWEEN :low AND :high
          ORDER BY CASE :order WHEN 'title' THEN listing.title_sort_key END,
                   CASE :order WHEN 'newest' THEN held.created_at END DESC,
                   CASE :order WHEN 'year' THEN held.shown_year END,
                   CASE :order WHEN 'year_back' THEN held.shown_year END DESC,
                   CASE :order WHEN 'random' THEN random() END,
                   held.created_at, held.served_release_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @bae_records::records::served::ServedRelease AS release
            ON release.served_release_id = picked.served_release_id
"#
);

// The artists the artist list shows that library search finds for the search
// pattern `pattern`, every one for no pattern, in name order, ties going by
// id: the `limit` from the place `offset`, each with its place among them,
// from 1.
sqlive_compose::fragment!(
    FoundServedArtists(pattern: Option<String>, offset: i64, limit: i64) { place: i64 },
    r#"
    SELECT ROW_NUMBER() OVER (ORDER BY picked.name_sort_key, picked.artist_id) AS place,
           served.*
    FROM (SELECT listing.artist_id, listing.name_sort_key
          FROM artist_listing AS listing
          WHERE :pattern IS NULL OR listing.artist_id IN (
              SELECT matched.artist_id
              FROM @crate::album_search::ArtistsFound(pattern: :pattern) AS matched)
          ORDER BY listing.name_sort_key, listing.artist_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @crate::served::ServedArtist AS served ON served.listed_id = picked.artist_id
"#
);

// The releases of the albums the album grid lists that library search finds
// for the search pattern `pattern`, every one for no pattern, in album title
// order, ties going by album, each album's releases in its order: the `limit`
// from the place `offset`, each with its place among them, from 1.
sqlive_compose::fragment!(
    FoundServedAlbums(pattern: Option<String>, offset: i64, limit: i64) { place: i64 },
    r#"
    SELECT ROW_NUMBER() OVER (
               ORDER BY picked.title_sort_key, picked.album_id, picked.album_order,
                        picked.served_release_id) AS place,
           release.*
    FROM (SELECT listing.title_sort_key, listing.album_id, held.album_order,
                 held.served_release_id
          FROM album_listing AS listing INDEXED BY idx_album_listing_title
          CROSS JOIN @bae_records::records::served::ServedRelease AS held
                  ON held.served_album_id = listing.album_id
          WHERE :pattern IS NULL OR listing.album_id IN (
              SELECT matched.album_id
              FROM @crate::album_search::AlbumsFound(pattern: :pattern) AS matched)
          ORDER BY listing.title_sort_key, listing.album_id, held.album_order,
                   held.served_release_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @bae_records::records::served::ServedRelease AS release
            ON release.served_release_id = picked.served_release_id
"#
);

// The tracks library search finds for the search pattern `pattern`, every one
// for no pattern, in title order, ties going by id: the `limit` from the place
// `offset`, each with its place among them, from 1, and its release as an
// album.
sqlive_compose::fragment!(
    FoundServedSongs(pattern: Option<String>, offset: i64, limit: i64) { place: i64 },
    r#"
    SELECT ROW_NUMBER() OVER (ORDER BY picked.title_sort_key, picked.track_id) AS place,
           song.*, release.*
    FROM (SELECT entry.track_id, entry.title_sort_key
          FROM track_search_entry AS entry INDEXED BY idx_track_search_entry_title
          WHERE :pattern IS NULL OR entry.track_id IN (
              SELECT found.track_id
              FROM @crate::library_search::TracksFoundByTitle(pattern: :pattern) AS found)
          ORDER BY entry.title_sort_key, entry.track_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @bae_records::records::served::ServedTrack AS song
            ON song.served_track_id = picked.track_id
    CROSS JOIN @bae_records::records::served::ServedRelease AS release
            ON release.served_release_id = song.release_id
"#
);

// What a search for the search pattern `pattern` lists outside bae, every
// artist, album and song for no pattern, each kind its own page
// ([`FoundServedArtists`], [`FoundServedAlbums`], [`FoundServedSongs`]). The
// places are those any kind lists, and the row of each place holds each
// kind's row at that place, none for a kind that lists fewer.
sqlive_compose::read!(
    writer ServedSearch(
        pattern: Option<String>,
        artist_offset: i64,
        artist_limit: i64,
        album_offset: i64,
        album_limit: i64,
        song_offset: i64,
        song_limit: i64
    ),
    r#"
    SELECT artist.*, album.*, song.*
    FROM (SELECT listed.place
          FROM @crate::served::FoundServedArtists(
                   pattern: :pattern, offset: :artist_offset, limit: :artist_limit) AS listed
          UNION
          SELECT listed.place
          FROM @crate::served::FoundServedAlbums(
                   pattern: :pattern, offset: :album_offset, limit: :album_limit) AS listed
          UNION
          SELECT listed.place
          FROM @crate::served::FoundServedSongs(
                   pattern: :pattern, offset: :song_offset, limit: :song_limit) AS listed)
         AS slot
    LEFT JOIN @crate::served::FoundServedArtists(
                  pattern: :pattern, offset: :artist_offset, limit: :artist_limit) AS artist
           ON artist.place = slot.place
    LEFT JOIN @crate::served::FoundServedAlbums(
                  pattern: :pattern, offset: :album_offset, limit: :album_limit) AS album
           ON album.place = slot.place
    LEFT JOIN @crate::served::FoundServedSongs(
                  pattern: :pattern, offset: :song_offset, limit: :song_limit) AS song
           ON song.place = slot.place
    ORDER BY slot.place
"#
);

/// An artist as a server outside bae lists it, with how many releases its
/// albums hold.
#[derive(Debug, Clone, PartialEq)]
pub struct DbServedArtist {
    pub artist: DbArtistSummary,
    pub release_count: u64,
}

/// What a search lists outside bae: a page of each kind.
#[derive(Debug, Clone, Default)]
pub struct DbServedSearch {
    pub artists: Vec<DbServedArtist>,
    pub albums: Vec<DbServedRelease>,
    /// Each song with its release as an album.
    pub songs: Vec<(DbServedTrack, DbServedRelease)>,
}

/// The order a list of albums outside bae runs in, and the albums it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServedAlbumOrder {
    /// By album title, then by when each release was added.
    Title,
    /// Latest added first.
    Newest,
    /// The releases whose year is from `from` to `to`, running from `from`
    /// to `to`, which counts down when `from` is the later year.
    Years { from: i64, to: i64 },
    /// In an order no two reads share.
    Random,
}

/// A search outside bae: what it finds, and the page of each kind it lists.
#[derive(Debug, Clone)]
pub struct ServedSearchRequest {
    /// The query; `None` lists the whole library.
    pub query: Option<crate::LibrarySearchQuery>,
    pub artists: ListPage,
    pub albums: ListPage,
    pub songs: ListPage,
}

impl ServedArtist {
    fn decoded(self) -> Result<DbServedArtist, DbError> {
        let release_count = u64::try_from(self.release_count).map_err(|_error| {
            DbError::unreadable("an artist's release count", self.release_count)
        })?;
        Ok(DbServedArtist { artist: self.shown.into_summary()?, release_count })
    }
}

impl LibraryViews {
    /// Every artist the artist list shows, as a server outside bae lists
    /// them, in name order.
    pub async fn served_artists(&self) -> Result<Vec<DbServedArtist>, DbError> {
        self.store
            .read(|sql| Ok(statement_rows(&sql, &EveryServedArtistParams {})?))
            .process(|rows| rows.into_iter().map(|row| row.served.decoded()).collect())
            .await
    }

    /// The artist `artist_id` with the releases of the albums that credit
    /// it, in its page's order; `None` when the artist list doesn't show it.
    pub async fn served_artist(
        &self,
        artist_id: &str,
    ) -> Result<Option<(DbServedArtist, Vec<DbServedRelease>)>, DbError> {
        let statement = ServedArtistReleasesParams { artist: artist_id.to_string() };
        self.store
            .read(move |sql| Ok(statement_rows(&sql, &statement)?))
            .process(|rows| {
                let mut rows = rows.into_iter().peekable();
                let Some(first) = rows.peek() else {
                    return Ok(None);
                };
                let artist = first.served.clone().decoded()?;
                let releases = rows.map(|row| row.release.decoded()).collect::<Result<_, _>>()?;
                Ok(Some((artist, releases)))
            })
            .await
    }

    /// The page `page` of the releases of the albums the grid lists, in the
    /// order `order` names.
    pub async fn served_album_list(
        &self,
        order: ServedAlbumOrder,
        page: ListPage,
    ) -> Result<Vec<DbServedRelease>, DbError> {
        let (order, low, high) = match order {
            ServedAlbumOrder::Title => ("title", 0, 0),
            ServedAlbumOrder::Newest => ("newest", 0, 0),
            ServedAlbumOrder::Years { from, to } if from <= to => ("year", from, to),
            ServedAlbumOrder::Years { from, to } => ("year_back", to, from),
            ServedAlbumOrder::Random => ("random", 0, 0),
        };
        let statement = ServedAlbumListParams {
            order: order.to_string(),
            low,
            high,
            offset: list_windows::sql_bound(page.offset),
            limit: list_windows::sql_bound(page.limit),
        };
        self.store
            .read(move |sql| Ok(statement_rows(&sql, &statement)?))
            .process(|rows| rows.into_iter().map(|row| row.release.decoded()).collect())
            .await
    }

    /// What `request` lists: a page of each kind.
    pub async fn served_search(
        &self,
        request: ServedSearchRequest,
    ) -> Result<DbServedSearch, DbError> {
        let ServedSearchRequest { query, artists, albums, songs } = request;
        let bound = list_windows::sql_bound;
        let statement = ServedSearchParams {
            pattern: query.as_ref().map(|query| search_pattern(query.as_str())),
            artist_offset: bound(artists.offset),
            artist_limit: bound(artists.limit),
            album_offset: bound(albums.offset),
            album_limit: bound(albums.limit),
            song_offset: bound(songs.offset),
            song_limit: bound(songs.limit),
        };
        self.store
            .read(move |sql| Ok(statement_rows(&sql, &statement)?))
            .process(|rows| {
                let mut found = DbServedSearch::default();
                for row in rows {
                    if let Some(artist) = row.artist {
                        found.artists.push(artist.served.decoded()?);
                    }
                    if let Some(album) = row.album {
                        found.albums.push(album.release.decoded()?);
                    }
                    if let Some(song) = row.song {
                        found.songs.push((song.song.decoded()?, song.release.decoded()?));
                    }
                }
                Ok(found)
            })
            .await
    }
}

#[cfg(test)]
#[path = "served_tests.rs"]
mod tests;
