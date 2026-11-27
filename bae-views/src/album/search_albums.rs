//! The albums a library search lists: the first albums it finds, in title
//! order, ties going by id. Library search reads its first albums here, and a
//! media browser reads the first [`crate::SEARCH_RESULT_LIMIT`] of them a
//! page at a time.

use super::*;
use crate::album_detail::AlbumSearchResult;
use bae_records::records::fragments::AlbumSummary;
use bae_store::{statement_row, statement_rows};

// The albums a search for `pattern` lists: the first `cap` albums that hold
// it, in title order, ties going by id. The read walks the listing's title
// index, tests each album against the albums the search finds, and stops at
// the `cap`th album that holds the pattern. `INDEXED BY` holds it to that
// index: left to choose, SQLite reads the albums the search finds by id and
// sorts them all. Every album holds a release (§2.1), so every album has a
// listing row.
sqlive_compose::fragment!(
    SearchedAlbums(pattern: Option<String>, cap: i64),
    r#"
    SELECT entry.album_id AS searched_album_id, entry.title_sort_key AS searched_title_key
    FROM album_listing AS entry INDEXED BY idx_album_listing_title
    WHERE entry.album_id IN (
        SELECT found.album_id
        FROM @crate::album_search::AlbumsFound(pattern: :pattern) AS found)
    ORDER BY entry.title_sort_key, entry.album_id
    LIMIT :cap
"#
);

// A page of the albums a search lists ([`SearchedAlbums`]): those at the
// places from `offset` on, at most `limit`. The statement reads what an album
// shows only for the albums of the page.
sqlive_compose::read!(
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

// The first `limit` albums a search lists ([`SearchedAlbums`]), as library
// search shows them, each beside how many rows the statement lists. The count
// makes SQLite sort those rows, at most `limit` of them, once more.
sqlive_compose::read!(
    writer FirstSearchAlbums(pattern: Option<String>, limit: i64) { listed: u64 },
    r#"
    SELECT shown.*, COUNT(*) OVER () AS listed
    FROM @crate::album::search_albums::SearchedAlbums(
             pattern: :pattern, cap: :limit) AS picked
    CROSS JOIN @bae_records::records::fragments::AlbumSummary AS shown
            ON shown.summary_album_id = picked.searched_album_id
    ORDER BY picked.searched_title_key, picked.searched_album_id
"#
);

// How many albums a search for `pattern` lists: those that hold it, counting
// at most `cap`. The count needs no order, so it sorts nothing.
sqlive_compose::read!(
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
        self.store
            .subscribe(move |sql| match &pattern {
                Some(pattern) => search_albums_on(&sql, pattern, LISTED, page),
                None => Ok(Vec::new()),
            })
            .process(|albums| {
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
        self.store.subscribe(move |sql| match &pattern {
            Some(pattern) => {
                let statement = SearchAlbumCountParams {
                    pattern: Some(pattern.clone()),
                    cap: bae_store::list_windows::sql_bound(LISTED),
                };
                Ok(statement_row(&sql, &statement)?.albums)
            }
            None => Ok(0),
        })
    }
}

/// The albums of the first `cap` a search for `pattern` lists that are at
/// `page`'s places, as read.
fn search_albums_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    cap: u64,
    page: ListPage,
) -> Result<Vec<AlbumSummary>, DbError> {
    let statement = SearchAlbumsParams {
        pattern: Some(pattern.to_string()),
        cap: bae_store::list_windows::sql_bound(cap),
        offset: bae_store::list_windows::sql_bound(page.offset),
        limit: bae_store::list_windows::sql_bound(page.limit),
    };
    Ok(statement_rows(sql, &statement)?.into_iter().map(|row| row.shown).collect())
}

/// The first `limit` albums a library search for `pattern` lists, in title
/// order, and how many that is.
pub(super) fn first_search_albums_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: u64,
) -> Result<(Vec<DbAlbumSummary>, u64), DbError> {
    let statement = FirstSearchAlbumsParams {
        pattern: Some(pattern.to_string()),
        limit: bae_store::list_windows::sql_bound(limit),
    };
    let rows = statement_rows(sql, &statement)?;
    let listed = rows.first().map_or(0, |row| row.listed);
    let albums = rows.into_iter().map(|row| row.shown).collect();
    Ok((album_summaries(albums)?, listed))
}

/// The albums as read, each made into its summary.
fn album_summaries(albums: Vec<AlbumSummary>) -> Result<Vec<DbAlbumSummary>, DbError> {
    albums.into_iter().map(|shown| shown.into_row()?.process()).collect()
}

#[cfg(test)]
#[path = "search_albums_tests.rs"]
mod tests;
