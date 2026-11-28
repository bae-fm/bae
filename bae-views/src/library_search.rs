//! Library search (§2.3): the albums, tracks, artists, composers and works a
//! query finds, read as one statement, and the track a spoken "play" of the
//! query plays.

use super::*;
use crate::album::release_track;

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
    /// search finds, else the first track of the first album it finds that
    /// has one. The method reads that track alone, not the whole search.
    pub async fn search_top_playable(
        &self,
        query: &crate::LibrarySearchQuery,
    ) -> Result<Option<DbTopPlayable>, DbError> {
        let statement = top_playable(query.as_str());
        let row = self.store.read(move |sql| statement_optional(&sql, &statement)).await?;
        Ok(row.map(|row| DbTopPlayable { release_id: row.release_id, track_id: row.track_id }))
    }

    /// The albums, tracks, composers and works whose title or name `query`
    /// finds, at most `limit` of each, as their rows read.
    pub async fn search_rows(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<DbLibrarySearchResults, DbError> {
        let statement = library_search(query, limit);
        self.store
            .read(move |sql| Ok(statement_rows(&sql, &statement)?))
            .process(search_results)
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
        self.store
            .subscribe_reconfigurable(initial, move |request| {
                Ok(coven::Maybe(
                    request.as_ref().map(|query| library_search(query.as_str(), limit)),
                ))
            })
            .process(|_, rows| {
                let results = rows.map(search_results).transpose()?.unwrap_or_default();
                Ok(LibrarySearchProjection { results })
            })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibrarySearchProjection {
    pub results: DbLibrarySearchResults,
}

/// One delivery of a library search as its results show it.
pub fn resolve_library_search_projection(
    projection: LibrarySearchProjection,
) -> crate::album_detail::SearchResults {
    crate::album_detail::SearchResults::from_raw(projection.results)
}

/// [`LibrarySearch`] for `query`, each kind listing at most `limit` rows.
fn library_search(query: &str, limit: usize) -> LibrarySearchParams {
    // A row for each place a list can fill, and one at least, which says
    // whether the search found anything.
    let places: Vec<usize> = (1..=limit.max(1)).collect();
    LibrarySearchParams {
        pattern: Some(search_pattern(query)),
        places: serde_json::to_string(&places).expect("numbers serialize as JSON"),
        limit: bae_store::list_windows::sql_bound(limit as u64),
    }
}

/// [`TopPlayable`] for `query`.
fn top_playable(query: &str) -> TopPlayableParams {
    TopPlayableParams { pattern: Some(search_pattern(query)) }
}

/// The five lists [`LibrarySearch`]'s rows hold, each in its order, and how
/// many rows each lists, as its first row counts them.
fn search_results(rows: Vec<LibrarySearchRow>) -> Result<DbLibrarySearchResults, DbError> {
    let mut results = DbLibrarySearchResults::default();
    let counts = &mut results.section_counts;
    for row in rows {
        results.found = row.found;
        if let Some(hit) = row.album {
            counts.albums = list_count(hit.listed)?;
            results.albums.push(hit.summary.into_row()?.process()?);
        }
        if let Some(hit) = row.track {
            counts.tracks = list_count(hit.listed)?;
            results.tracks.push(release_track(hit.shown)?);
        }
        if let Some(hit) = row.artist {
            counts.artists = list_count(hit.listed)?;
            results.artists.push(hit.shown.into_summary()?);
        }
        if let Some(hit) = row.composer {
            counts.composers = list_count(hit.listed)?;
            results.composers.push(hit.shown.into_summary()?);
        }
        if let Some(hit) = row.work {
            counts.works = list_count(hit.listed)?;
            results.works.push(work_summary(hit.work, hit.cover)?);
        }
    }
    Ok(results)
}

// The tracks whose title holds the search pattern `pattern`, none for no
// pattern, each with its title's sort key, which its search entry holds. A read
// that takes them in title order, ties going by id, walks the entries' title
// index, tests each entry against the titles the search finds, and stops at its
// limit, computing no key. `INDEXED BY` holds the walk to that index: left to
// choose, SQLite reads the entries the search finds and sorts them all.
coven::fragment!(
    TracksFoundByTitle(pattern: Option<String>),
    r#"
    SELECT entry.track_id, entry.title_sort_key
    FROM track_search_entry AS entry INDEXED BY idx_track_search_entry_title
    WHERE entry.entry IN (
        SELECT titled.rowid FROM track_search AS titled WHERE titled.title GLOB :pattern)
"#
);

// The first `limit` tracks that hold the search pattern `pattern`, in title
// order, ties going by id, each with its place in that order, from 1, how many
// tracks these are, and the track as playback shows it.
coven::fragment!(
    FoundTracks(pattern: Option<String>, limit: i64) { place: i64, listed: i64 },
    r#"
    SELECT ROW_NUMBER() OVER (ORDER BY picked.title_sort_key, picked.track_id) AS place,
           COUNT(*) OVER () AS listed, shown.*
    FROM (SELECT found.track_id, found.title_sort_key
          FROM @crate::library_search::TracksFoundByTitle(pattern: :pattern) AS found
          ORDER BY found.title_sort_key, found.track_id
          LIMIT :limit) AS picked
    CROSS JOIN @bae_records::records::track_display::DisplayedTrack AS shown
            ON shown.display_track_id = picked.track_id
"#
);

// The works whose title holds the search pattern `pattern`, each with its
// title's sort key. A read that takes them in title order walks the entries'
// title index, as [`TracksFoundByTitle`] does for tracks.
coven::fragment!(
    WorksFoundByTitle(pattern: Option<String>),
    r#"
    SELECT entry.work_id, entry.title_sort_key
    FROM work_search_entry AS entry INDEXED BY idx_work_search_entry_title
    WHERE entry.entry IN (
        SELECT titled.rowid FROM work_search AS titled WHERE titled.title GLOB :pattern)
"#
);

// The first `limit` works whose title holds the search pattern `pattern`, in
// title order, ties going by id, each with its place in that order, from 1, how
// many works these are, and the work as its rows show it, with its cover.
coven::fragment!(
    FoundWorks(pattern: Option<String>, limit: i64) { place: i64, listed: i64 },
    r#"
    SELECT ROW_NUMBER() OVER (ORDER BY picked.title_sort_key, picked.work_id) AS place,
           COUNT(*) OVER () AS listed, work.*, cover.*
    FROM (SELECT found.work_id, found.title_sort_key
          FROM @crate::library_search::WorksFoundByTitle(pattern: :pattern) AS found
          ORDER BY found.title_sort_key, found.work_id
          LIMIT :limit) AS picked
    CROSS JOIN @bae_records::records::browse_fragments::WorkSummary AS work
            ON work.work_id = picked.work_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS cover
           ON cover.id = work.representative_release_id
"#
);

// What the search pattern `pattern` finds: each kind's first `limit` rows in
// its order. The row of the place `slot` holds each kind's hit at that place,
// none for a kind that lists fewer; `places` is the JSON array of the places,
// from 1 to `limit`, and 1 for a `limit` of 0. `found`: whether any kind has a
// match, through the conditions that list each kind.
coven::read!(
    writer LibrarySearch(pattern: Option<String>, places: String, limit: i64) { found: bool },
    r#"
    SELECT (EXISTS (SELECT 1
                    FROM @bae_records::records::fragments::AlbumRow AS held
                    WHERE held.row_album_id IN (
                        SELECT matched.album_id
                        FROM @crate::album_search::AlbumsFound(pattern: :pattern) AS matched))
            OR EXISTS (SELECT 1 FROM track_search AS titled WHERE titled.title GLOB :pattern)
            OR EXISTS (SELECT 1 FROM artist_listing AS listing
                       WHERE listing.artist_id IN (
                           SELECT matched.artist_id
                           FROM @crate::album_search::ArtistsFound(pattern: :pattern) AS matched))
            OR EXISTS (SELECT 1 FROM composer_listing AS listing
                       WHERE listing.artist_id IN (
                           SELECT matched.artist_id
                           FROM @crate::album_search::ArtistsFound(pattern: :pattern) AS matched))
            OR EXISTS (SELECT 1
                       FROM @crate::library_search::WorksFoundByTitle(
                                pattern: :pattern) AS matched)) AS found,
           album.*, track.*, artist.*, composer.*, work.*
    FROM json_each(:places) AS slot
    LEFT JOIN @crate::album::search_albums::FoundAlbums(pattern: :pattern, limit: :limit) AS album
           ON album.place = slot.value
    LEFT JOIN @crate::library_search::FoundTracks(pattern: :pattern, limit: :limit) AS track
           ON track.place = slot.value
    LEFT JOIN @crate::artist::FoundArtists(pattern: :pattern, limit: :limit) AS artist
           ON artist.place = slot.value
    LEFT JOIN @crate::composer::FoundComposers(pattern: :pattern, limit: :limit) AS composer
           ON composer.place = slot.value
    LEFT JOIN @crate::library_search::FoundWorks(pattern: :pattern, limit: :limit) AS work
           ON work.place = slot.value
    ORDER BY slot.key
"#
);

// The track a spoken "play" of the search pattern `pattern` plays: the first
// track it finds in title order, and failing that, the first track of the
// first album it finds, in title order, whose primary release has one.
coven::read!(
    writer TopPlayable(pattern: Option<String>) {
        release_id: String,
        track_id: String
    },
    r#"
    SELECT playable.release_id, playable.track_id
    FROM (
        SELECT 0 AS rank, track.release_id, track.track_id
        FROM (SELECT found.track_id
              FROM @crate::library_search::TracksFoundByTitle(pattern: :pattern) AS found
              ORDER BY found.title_sort_key, found.track_id
              LIMIT 1) AS picked
        CROSS JOIN @bae_records::records::browse_fragments::TrackRecord AS track
                ON track.track_id = picked.track_id
        UNION ALL
        SELECT 1 AS rank, album_track.release_id, album_track.track_id
        FROM (SELECT held.release_id, held.track_id
              FROM @crate::album::search_albums::AlbumsFoundByTitle(pattern: :pattern) AS found
              CROSS JOIN @bae_records::records::fragments::PrimaryReleaseOfAlbum AS chosen
                      ON chosen.primary_of_album_id = found.searched_album_id
              CROSS JOIN @bae_records::records::spread_fragments::TrackRow AS held
                      ON held.release_id = chosen.primary_release_id
              ORDER BY found.searched_title_key, found.searched_album_id, held.position
              LIMIT 1) AS album_track
    ) AS playable
    ORDER BY playable.rank
    LIMIT 1
"#
);
