//! The albums a library search lists: the first albums it finds, in title
//! order, ties going by id. Library search lists its first albums through
//! [`FoundAlbums`], and a media browser reads the first
//! [`crate::SEARCH_RESULT_LIMIT`] of them a page at a time.

use super::*;
use crate::album_detail::AlbumSearchResult;
use bae_records::records::fragments::AlbumSummary;

// The albums that hold the search pattern `pattern`, each with its title's
// sort key, which its listing row holds. A read that takes them in title
// order, ties going by id, walks the listing's title index, tests each album
// against the albums the search finds, and stops at its limit, computing no
// key. `INDEXED BY` holds the walk to that index: left to choose, SQLite reads
// the albums the search finds by id and sorts them all. Every album holds a
// release (§2.1), so every album has a listing row.
coven::fragment!(
    AlbumsFoundByTitle(pattern: Option<String>),
    r#"
    SELECT entry.album_id AS searched_album_id, entry.title_sort_key AS searched_title_key
    FROM album_listing AS entry INDEXED BY idx_album_listing_title
    WHERE entry.album_id IN (
        SELECT found.album_id
        FROM @crate::album_search::AlbumsFound(pattern: :pattern) AS found)
"#
);

// The albums a search for `pattern` lists: the first `cap` albums that hold
// it ([`AlbumsFoundByTitle`]), in title order, ties going by id.
coven::fragment!(
    SearchedAlbums(pattern: Option<String>, cap: i64),
    r#"
    SELECT titled.*
    FROM @crate::album::search_albums::AlbumsFoundByTitle(pattern: :pattern) AS titled
    ORDER BY titled.searched_title_key, titled.searched_album_id
    LIMIT :cap
"#
);

// A page of the albums a search lists ([`SearchedAlbums`]): those at the
// places from `offset` on, at most `limit`. The statement reads what an album
// shows only for the albums of the page.
coven::read!(
    writer SearchAlbums(pattern: Option<String>, cap: i64, offset: i64, limit: i64),
    r#"
    SELECT shown.*
    FROM (SELECT listed.searched_album_id, listed.searched_title_key
          FROM @crate::album::search_albums::SearchedAlbums(
                   pattern: :pattern, cap: :cap) AS listed
          ORDER BY listed.searched_title_key, listed.searched_album_id
          LIMIT :limit OFFSET :offset) AS picked
    CROSS JOIN @bae_records::records::fragments::AlbumSummary AS shown
            ON shown.summary_album_id = picked.searched_album_id
    ORDER BY picked.searched_title_key, picked.searched_album_id
"#
);

// The first `limit` albums a search lists ([`SearchedAlbums`]), each with its
// place in their order, from 1, how many albums these are, and what a list
// shows of it.
coven::fragment!(
    FoundAlbums(pattern: Option<String>, limit: i64) { place: i64, listed: i64 },
    r#"
    SELECT ROW_NUMBER() OVER (
               ORDER BY picked.searched_title_key, picked.searched_album_id) AS place,
           COUNT(*) OVER () AS listed, summary.*
    FROM @crate::album::search_albums::SearchedAlbums(
             pattern: :pattern, cap: :limit) AS picked
    CROSS JOIN @bae_records::records::fragments::AlbumSummary AS summary
            ON summary.summary_album_id = picked.searched_album_id
"#
);

// How many albums a search for `pattern` lists: those that hold it, counting
// at most `cap`. The count needs no order, so it sorts nothing.
coven::read!(
    writer SearchAlbumCount(pattern: Option<String>, cap: i64) { albums: u64 },
    r#"
    SELECT COUNT(*) AS albums
    FROM (SELECT entry.album_id
          FROM album_listing AS entry
          WHERE entry.album_id IN (
              SELECT found.album_id
              FROM @crate::album_search::AlbumsFound(pattern: :pattern) AS found)
          LIMIT :cap) AS counted
"#
);

/// How many albums a media browser's search lists.
const LISTED: u64 = crate::SEARCH_RESULT_LIMIT as u64;

impl LibraryViews {
    /// Follow one page of the albums a search for `query` lists: the albums
    /// at the page's places as each write leaves them. No query (`None`)
    /// lists none; its page answers with no rows.
    pub fn subscribe_search_album_page(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let pattern = query.map(|query| search_pattern(query.as_str()));
        let read = pattern.map(|pattern| search_albums_read(pattern, LISTED, page));
        self.store.subscribe(coven::Maybe(read)).process(|rows| {
            let albums = rows.unwrap_or_default().into_iter().map(|row| row.shown).collect();
            Ok(crate::ListRows::SearchAlbums(
                album_summaries(albums)?.into_iter().map(AlbumSearchResult::from_raw).collect(),
            ))
        })
    }

    /// Follow how many albums a search for `query` lists: the albums it
    /// finds, up to [`crate::SEARCH_RESULT_LIMIT`]. The count runs as a
    /// separate query from the pages' rows. No query (`None`) lists none.
    pub fn subscribe_search_album_count(
        &self,
        query: Option<crate::LibrarySearchQuery>,
    ) -> coven::LiveQuery<u64, DbError> {
        let pattern = query.map(|query| search_pattern(query.as_str()));
        let read = pattern.map(|pattern| SearchAlbumCountParams {
            pattern: Some(pattern),
            cap: bae_store::list_windows::sql_bound(LISTED),
        });
        self.store.subscribe(coven::Maybe(read)).process(|rows| match rows {
            Some(rows) => Ok(bae_store::exactly_one::<SearchAlbumCountParams, _>(rows)?.albums),
            None => Ok(0),
        })
    }
}

/// The read of the albums of the first `cap` a search for `pattern` lists
/// that are at `page`'s places.
fn search_albums_read(pattern: String, cap: u64, page: ListPage) -> SearchAlbumsParams {
    SearchAlbumsParams {
        pattern: Some(pattern),
        cap: bae_store::list_windows::sql_bound(cap),
        offset: bae_store::list_windows::sql_bound(page.offset),
        limit: bae_store::list_windows::sql_bound(page.limit),
    }
}

/// The albums as read, each made into its summary.
fn album_summaries(albums: Vec<AlbumSummary>) -> Result<Vec<DbAlbumSummary>, DbError> {
    albums.into_iter().map(|shown| shown.into_row()?.process()).collect()
}

#[cfg(test)]
#[path = "search_albums_tests.rs"]
mod tests;
