//! The artist picker's live search: each window of what its text finds, the
//! count of what it finds and the position of a key in it are live queries
//! of their own; a new text is new queries.

use super::*;
use crate::name_sort::sort_key_sql;
use bae_records::records::artist_credit_names::{credit_names_column, credit_names_json_sql};
use rusqlite::named_params;

/// One artist an artist search found, with its image's version and its other
/// printed names in name order, read in the artist's own row.
#[derive(Debug, Clone, PartialEq)]
pub struct DbArtistSearchRow {
    pub artist: DbArtist,
    pub image_version: Option<String>,
    pub credit_names: Vec<bae_records::credits::ArtistCreditName>,
}

impl LibraryViews {
    /// Search every stored artist, including artists without an album link,
    /// reading the first `limit` of what `query` finds.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn search_artists(
        &self,
        query: &crate::LibrarySearchQuery,
        limit: u64,
    ) -> Result<Vec<DbArtist>, DbError> {
        let query = query.clone();
        let order = artist_search_order();
        self.store
            .read(move |sql| {
                Ok(artist_search_rows_on(&sql, &query, &order, &order.everything(), limit, 0)?
                    .into_iter()
                    .map(|(row, _)| row.artist)
                    .collect())
            })
            .await
    }

    /// Follow one window of what the artist picker's text finds: a range of
    /// the search's order (`list_windows`), which coven reruns for a write
    /// inside it alone. No query (a blank text) finds nothing; its window
    /// answers with no rows.
    pub fn subscribe_artist_search_window(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbArtistSearchRow>, bae_store::DbError>
    {
        let order = artist_search_order();
        let read_order = order.clone();
        self.store.subscribe_list_window(initial, order, move |sql, seek, limit, offset| {
            match &query {
                Some(query) => artist_search_rows_on(sql, query, &read_order, seek, limit, offset),
                None => Ok(Vec::new()),
            }
        })
    }

    /// Follow how many artists the text finds. Its own query, never beside a
    /// window's rows.
    pub fn subscribe_artist_search_count(
        &self,
        query: Option<crate::LibrarySearchQuery>,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        self.store.subscribe(move |sql| match &query {
            Some(query) => artist_search_count_on(&sql, query),
            None => Ok(0),
        })
    }

    /// Follow how many of the artists the text finds come before the row at
    /// a key, a count over the search's order that moves in place with the
    /// key.
    pub fn subscribe_artist_search_position(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let order = artist_search_order();
        self.store.subscribe_reconfigurable(initial, move |key, sql| match &query {
            Some(query) => rows_before(&order, key, |seek| {
                artist_search_seek_count_on(&sql, query, &order, seek)
            }),
            None => Ok(0),
        })
    }

    /// The key of the artist at `position` of what the text finds, or of the
    /// last one when the list ends before it; `None` when it finds none.
    pub async fn artist_search_key_at(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let Some(query) = query else {
            return Ok(None);
        };
        let order = artist_search_order();
        self.store
            .read(move |sql| {
                key_at(&order, position, &mut |seek, limit, offset| {
                    artist_search_rows_on(&sql, &query, &order, seek, limit, offset)
                })
            })
            .await
    }
}

/// One delivery of an artist search window: the artists it holds, each
/// beside its image and its other printed names.
pub fn resolve_artist_search_window(read: WindowRead<DbArtistSearchRow>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, |rows| {
        crate::ListRows::ArtistSearch(
            rows.into_iter()
                .map(|row| crate::album_detail::ArtistSearchResult {
                    image: bae_records::artist_image_ref(&row.artist.id, row.image_version),
                    credit_names: row.credit_names,
                    artist: row.artist,
                })
                .collect(),
        )
    })
}

/// The artists the text `:text` finds: by library or provider id, or as
/// library search finds artists, by the search pattern `:pattern` binds
/// ([`artists_found_sql`]). Merged-away artists are never found.
fn artist_search_filter() -> String {
    format!(
        r#"
        {shown}
          AND (id = :text
           OR discogs_artist_id = :text
           OR musicbrainz_artist_id = :text
           OR id IN ({found}))
        "#,
        shown = artist_is_shown("artists"),
        found = artists_found_sql(":pattern"),
    )
}

/// The artists a text finds in their order: library and provider ids and
/// exact name or sort-name matches ahead of prefixes, then substrings; name
/// and library id stabilize each rank, so windows never overlap or skip.
/// The order names the text by the parameter `:text`.
pub(crate) fn artist_search_order() -> ListOrder {
    ListOrder::new(vec![
        SortTerm::ascending(
            "CASE
                WHEN id = :text
                  OR discogs_artist_id = :text
                  OR musicbrainz_artist_id = :text
                  OR bae_fold_case(name) = bae_fold_case(:text)
                  OR bae_fold_case(sort_name) = bae_fold_case(:text) THEN 0
                WHEN substr(bae_fold_case(name), 1, length(bae_fold_case(:text)))
                       = bae_fold_case(:text)
                  OR substr(bae_fold_case(sort_name), 1, length(bae_fold_case(:text)))
                       = bae_fold_case(:text) THEN 1
                ELSE 2
            END",
        ),
        SortTerm::ascending(sort_key_sql("name")),
        SortTerm::ascending("id"),
    ])
}

fn artist_search_count_on(
    sql: &SqlReadContext<'_>,
    query: &crate::LibrarySearchQuery,
) -> Result<u64, DbError> {
    let pattern = search_pattern(query.as_str());
    sql.query_row(
        &format!("SELECT COUNT(*) FROM artists WHERE {}", artist_search_filter()),
        named_params! { ":text": query.as_str(), ":pattern": pattern },
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as u64)
    .map_err(DbError::from)
}

/// How many of the artists `query` finds come before the rows `seek` reads.
fn artist_search_seek_count_on(
    sql: &SqlReadContext<'_>,
    query: &crate::LibrarySearchQuery,
    order: &ListOrder,
    seek: &Seek,
) -> Result<u64, DbError> {
    let pattern = search_pattern(query.as_str());
    let statement = format!(
        "SELECT COUNT(*) FROM artists WHERE {filter} AND {condition}",
        filter = artist_search_filter(),
        condition = order.condition(seek),
    );
    let text = query.as_str();
    let mut params = seek.params();
    params.push((":text", &text as &dyn rusqlite::ToSql));
    params.push((":pattern", &pattern as &dyn rusqlite::ToSql));
    Ok(sql
        .query_row(&statement, params.as_slice(), |row| row.get::<_, i64>(0))
        .map(|count| count as u64)?)
}

/// The artists `query` finds that `seek` reads in `order`, at most `limit`
/// from `offset`, each with its key.
fn artist_search_rows_on(
    sql: &SqlReadContext<'_>,
    query: &crate::LibrarySearchQuery,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<DbArtistSearchRow>>, DbError> {
    let pattern = search_pattern(query.as_str());
    let statement = format!(
        r#"
        SELECT *,
               {image} AS image_version,
               {credit_names} AS credit_names_json,
               {keys}
        FROM artists
        WHERE {filter} AND {condition}
        ORDER BY {ordering}
        LIMIT :limit OFFSET :offset
        "#,
        filter = artist_search_filter(),
        image = artist_image_version_sql("artists.id"),
        credit_names = credit_names_json_sql("artists.id", &sort_key_sql("credit_name.name")),
        keys = order.key_columns(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
    );
    let text = query.as_str();
    Ok(seek.bound(limit, offset, |params| {
        let mut params = params.to_vec();
        params.push((":text", &text as &dyn rusqlite::ToSql));
        params.push((":pattern", &pattern as &dyn rusqlite::ToSql));
        sql.query(&statement, params.as_slice(), |row| {
            Ok((
                DbArtistSearchRow {
                    artist: row_to_artist(row)?,
                    image_version: row.get("image_version")?,
                    credit_names: credit_names_column(row, "credit_names_json")?,
                },
                order.key_of(row)?,
            ))
        })
    })?)
}

#[cfg(test)]
#[path = "artist_search_tests.rs"]
mod tests;
