//! The albums a library search lists: the first albums it finds, in title
//! order, ties going by id. Library search reads its first albums here, and a
//! media browser reads the first [`crate::SEARCH_RESULT_LIMIT`] of them a
//! page at a time.

use super::*;

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
        let order = search_album_order();
        let read_order = order.clone();
        self.store.subscribe_list_page(
            page,
            order,
            search_album_statement,
            move |sql, seek, limit| match &pattern {
                Some(pattern) => {
                    search_album_rows_on(sql, pattern, LISTED, &read_order, seek, limit)
                }
                None => Ok(Vec::new()),
            },
            |rows| Ok(search_album_rows(rows)),
        )
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
            Some(pattern) => search_album_count_on(&sql, pattern),
            None => Ok(0),
        })
    }
}

/// The albums a search lists, as a media browser lists them.
fn search_album_rows(rows: Vec<DbAlbumSearchResult>) -> crate::ListRows {
    crate::ListRows::SearchAlbums(
        rows.into_iter().map(crate::album_detail::AlbumSearchResult::from_raw).collect(),
    )
}

/// How many albums a media browser's search lists.
const LISTED: u64 = crate::SEARCH_RESULT_LIMIT as u64;

/// The order of the albums a search lists: by title, ties going by id, over
/// the albums' `album_listing` rows as the alias `listing`. The rows hold
/// each title's sort key, so a read takes the albums in order from the
/// listing's title index and computes no key.
fn search_album_order() -> ListOrder {
    ListOrder::new(vec![
        SortTerm::ascending("listing.title_sort_key"),
        SortTerm::ascending("listing.album_id"),
    ])
}

/// The albums a search lists, as a FROM clause over the alias `listing`: the
/// listing rows of the first `:cap` albums that hold the search pattern
/// `:pattern`, in [`search_album_order`]. The read walks the listing's title
/// index, tests each album against the albums the search finds, and stops at
/// the `:cap`th album that holds the pattern. `INDEXED BY` holds it to that
/// index: left to choose, SQLite reads the albums the search finds by id and
/// sorts them all. Every album holds a release (§2.1), so every album has a
/// listing row.
fn search_album_from() -> String {
    format!(
        "FROM (SELECT listing.album_id, listing.title_sort_key \
               FROM album_listing listing INDEXED BY idx_album_listing_title \
               WHERE {matches} ORDER BY {order} LIMIT :cap) listing",
        matches = album_matches_sql("listing.album_id", ":pattern"),
        order = search_album_order().clause(),
    )
}

/// The albums `seek` reads in `order`, each with its key: at most `limit` of
/// them. The seek reads among the first `cap` albums that hold the search
/// `pattern`.
fn search_album_rows_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    cap: u64,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
) -> Result<Vec<Keyed<DbAlbumSearchResult>>, DbError> {
    let statement = search_album_statement(order, seek);
    with_search_album_params(pattern, cap, seek, limit, |params| {
        Ok(sql.query(&statement, params, |row| Ok((album_hit_row(row)?, order.key_of(row)?)))?)
    })
}

/// The first `limit` albums a library search for `pattern` lists, in
/// [`search_album_order`], and how many that is. The statement that lists
/// the albums also counts them.
pub(super) fn first_search_albums_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: u64,
) -> Result<(Vec<DbAlbumSearchResult>, u64), DbError> {
    let order = search_album_order();
    let seek = order.everything();
    let statement = search_album_statement(&order, &seek);
    with_search_album_params(pattern, limit, &seek, limit, |params| {
        counted_rows(sql, &statement, params, album_hit_row)
    })
}

/// The statement that reads the albums `seek` holds in `order`, each with
/// its key, over the parameters [`with_search_album_params`] binds. The
/// statement reads each album's main release, cover and credit line only for
/// the albums the seek returns.
fn search_album_statement(order: &ListOrder, seek: &Seek) -> String {
    format!(
        r#"
        SELECT a.id, a.title, a.year,
               {primary_release} AS primary_release_id,
               {primary_cover} AS cover_version,
               {artist_names} AS artist_name,
               {keys}
        FROM (
            SELECT listing.album_id, listing.title_sort_key {from}
            WHERE {condition}
            ORDER BY {ordering} {limit}
        ) listing
        CROSS JOIN albums a ON a.id = listing.album_id
        ORDER BY {ordering}
        "#,
        primary_release = album_primary_release_sql(),
        primary_cover = bae_records::records::files::cover_version_sql(album_primary_release_sql()),
        artist_names = album_credits_json_sql(),
        keys = order.key_columns(),
        from = search_album_from(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
        limit = seek.limit_clause(),
    )
}

/// Runs `run` with the parameters a [`search_album_statement`] over `seek`
/// reads: the seek's own, the window's `limit`, the search
/// `pattern`, and the `cap` on how many of the albums that hold it the seek
/// reads among.
fn with_search_album_params<T>(
    pattern: &str,
    cap: u64,
    seek: &Seek,
    limit: u64,
    run: impl FnOnce(&[(&str, &dyn rusqlite::ToSql)]) -> T,
) -> T {
    let cap = bae_store::list_windows::sql_bound(cap);
    seek.bound(limit, |params| {
        let mut params = params.to_vec();
        params.push((":pattern", &pattern as &dyn rusqlite::ToSql));
        params.push((":cap", &cap as &dyn rusqlite::ToSql));
        run(params.as_slice())
    })
}

fn album_hit_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DbAlbumSearchResult> {
    Ok(DbAlbumSearchResult {
        id: row.get("id")?,
        title: row.get("title")?,
        year: row.get("year")?,
        primary_release_id: row.get("primary_release_id")?,
        cover_version: row.get("cover_version")?,
        artist_line: credit_line_column(row, "artist_name")?,
    })
}

/// How many albums a search for `pattern` lists: those that hold it, counting
/// at most [`LISTED`]. The count needs no order, so it sorts nothing.
fn search_album_count_on(sql: &SqlReadContext<'_>, pattern: &str) -> Result<u64, DbError> {
    Ok(sql.query_row(
        &format!(
            "SELECT COUNT(*) AS count FROM (SELECT 1 FROM albums a WHERE {} LIMIT ?2)",
            album_matches_sql("a.id", "?1"),
        ),
        params![pattern, bae_store::list_windows::sql_bound(LISTED)],
        |row| count_column::<u64>(row, "count"),
    )?)
}

#[cfg(test)]
#[path = "search_albums_tests.rs"]
mod tests;
