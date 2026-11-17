//! The albums a library search lists: the first albums it finds, in title
//! order, ties going by id. Library search reads its first albums here, and a
//! media browser reads the first [`crate::SEARCH_RESULT_LIMIT`] of them as a
//! list, a page at a time, each page a window bounded by the keys of its
//! first row and of the row after its last.

use super::*;

impl LibraryViews {
    /// Follow one window of the albums a search for `query` lists: a range
    /// of [`search_album_order`] (`list_windows`), which coven reruns for a
    /// write inside it alone. No query (`None`) lists none; its window
    /// answers with no rows.
    pub fn subscribe_search_album_window(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbAlbumSearchResult>, DbError> {
        let pattern = query.map(|query| search_pattern(query.as_str()));
        let order = search_album_order();
        let read_order = order.clone();
        self.store.subscribe_list_window(initial, order, move |sql, seek, limit, offset| {
            match &pattern {
                Some(pattern) => {
                    search_album_rows_on(sql, pattern, LISTED, &read_order, seek, limit, offset)
                }
                None => Ok(Vec::new()),
            }
        })
    }

    /// Follow how many albums a search for `query` lists: the albums it
    /// finds, up to [`crate::SEARCH_RESULT_LIMIT`]. The count runs as a
    /// separate query from the windows' rows. No query (`None`) lists none.
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

    /// Follow how many of the albums a search for `query` lists come before
    /// the row at a key; the caller changes the key on the same query.
    pub fn subscribe_search_album_position(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, DbError> {
        let pattern = query.map(|query| search_pattern(query.as_str()));
        let order = search_album_order();
        let cap = bae_store::list_windows::sql_bound(LISTED);
        self.store.subscribe_reconfigurable(initial, move |key, sql| match &pattern {
            Some(pattern) => bae_store::list_windows::rows_before_on(
                &sql,
                &search_album_from(),
                None,
                &[(":pattern", pattern), (":cap", &cap)],
                &order,
                key,
            ),
            None => Ok(0),
        })
    }

    /// The key of the album at `position` of those a search for `query`
    /// lists, or of the last one when the list ends before it; `None` when
    /// it lists none.
    pub async fn search_album_key_at(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let Some(query) = query else {
            return Ok(None);
        };
        let pattern = search_pattern(query.as_str());
        let order = search_album_order();
        self.store
            .read(move |sql| {
                key_at(&order, position, &mut |seek, limit, offset| {
                    search_album_rows_on(&sql, &pattern, LISTED, &order, seek, limit, offset)
                })
            })
            .await
    }

    /// The window of the albums a search for `query` lists that holds the
    /// `limit` albums from `position`, read once: a media browser names a
    /// page by its offset and follows this window live. `None` when the
    /// search lists fewer albums than `position`, and for no query.
    pub async fn search_album_page_window(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        position: u64,
        limit: u64,
    ) -> Result<Option<ListWindow>, DbError> {
        let Some(query) = query else {
            return Ok(None);
        };
        let pattern = search_pattern(query.as_str());
        let order = search_album_order();
        self.store
            .read(move |sql| {
                page_window(&order, position, limit, &mut |seek, limit, offset| {
                    search_album_rows_on(&sql, &pattern, LISTED, &order, seek, limit, offset)
                })
            })
            .await
    }
}

/// One delivery of a window of the albums a search lists.
pub fn resolve_search_album_window(
    read: WindowRead<DbAlbumSearchResult>,
) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, |rows| {
        crate::ListRows::SearchAlbums(
            rows.into_iter().map(crate::album_detail::AlbumSearchResult::from_raw).collect(),
        )
    })
}

/// How many albums a media browser's search lists.
const LISTED: u64 = crate::SEARCH_RESULT_LIMIT as u64;

/// The order of the albums a search lists: by title, ties going by id.
pub(super) fn search_album_order() -> ListOrder {
    ListOrder::new(vec![
        SortTerm::ascending(crate::sort_key_sql("a.title")),
        SortTerm::ascending("a.id"),
    ])
}

/// The albums a search lists, as a FROM clause over the alias `a`: the first
/// `:cap` albums that hold the search pattern `:pattern`, in
/// [`search_album_order`].
fn search_album_from() -> String {
    format!(
        "FROM (SELECT a.id, a.title, a.year FROM albums a WHERE {matches} \
               ORDER BY {order} LIMIT :cap) a",
        matches = album_matches_sql("a.id", ":pattern"),
        order = search_album_order().clause(),
    )
}

/// The albums of the first `cap` that hold the search `pattern` which `seek`
/// reads in `order`, at most `limit` from `offset`, each with its key. The
/// statement reads each album's main release, cover and credit line for the
/// albums the seek returns alone.
pub(super) fn search_album_rows_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    cap: u64,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<DbAlbumSearchResult>>, DbError> {
    let statement = format!(
        r#"
        SELECT a.id, a.title, a.year,
               {primary_release} AS primary_release_id,
               {primary_cover} AS cover_version,
               {artist_names} AS artist_name,
               {keys}
        FROM (
            SELECT a.id, a.title, a.year {from}
            WHERE {condition}
            ORDER BY {ordering} LIMIT :limit OFFSET :offset
        ) a
        ORDER BY {ordering}
        "#,
        primary_release = album_primary_release_sql(),
        primary_cover = bae_records::records::files::cover_version_sql(album_primary_release_sql()),
        artist_names = album_credits_json_sql(),
        keys = order.key_columns(),
        from = search_album_from(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
    );
    let cap = bae_store::list_windows::sql_bound(cap);
    Ok(seek.bound(limit, offset, |params| {
        let mut params = params.to_vec();
        params.push((":pattern", &pattern as &dyn rusqlite::ToSql));
        params.push((":cap", &cap as &dyn rusqlite::ToSql));
        sql.query(&statement, params.as_slice(), |row| {
            Ok((
                DbAlbumSearchResult {
                    id: row.get("id")?,
                    title: row.get("title")?,
                    year: row.get("year")?,
                    primary_release_id: row.get("primary_release_id")?,
                    cover_version: row.get("cover_version")?,
                    artist_line: credit_line_column(row, "artist_name")?,
                },
                order.key_of(row)?,
            ))
        })
    })?)
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
