//! The artist picker's live search: each window of what its text finds, the
//! count of what it finds and the position of a key in it are live queries
//! of their own; the picker opens new queries for each new text.

use super::*;
use crate::list_sort::{key, key_values};
use bae_records::records::files::ArtistImageVersion;
use bae_store::list_windows::{sql_bound, SortValue};
use bae_store::{exactly_one, statement_optional};

// Each artist the text `text` finds, with what the search orders it by: its
// `match_rank`, 0 for a library or provider id or an exact name or sort-name
// match, 1 for a name or sort name starting with the text and 2 for the rest;
// and `ranked_name_key`, its name's sort key in the device's language. The
// text finds by library or provider id, or as library search finds artists,
// by the search pattern `pattern`. Merged-away artists are never found.
sqlive_compose::fragment!(
    RankedArtists(text: String, pattern: String) {
        match_rank: i64,
        ranked_name_key: Vec<u8>
    },
    r#"
    SELECT artist.artist_id AS ranked_artist_id,
           CASE
               WHEN artist.artist_id = :text
                 OR artist.discogs_artist_id = :text
                 OR artist.musicbrainz_artist_id = :text
                 OR bae_fold_case(artist.name) = bae_fold_case(:text)
                 OR bae_fold_case(artist.sort_name) = bae_fold_case(:text) THEN 0
               WHEN substr(bae_fold_case(artist.name), 1, length(bae_fold_case(:text)))
                      = bae_fold_case(:text)
                 OR substr(bae_fold_case(artist.sort_name), 1, length(bae_fold_case(:text)))
                      = bae_fold_case(:text) THEN 1
               ELSE 2
           END AS match_rank,
           bae_name_key(artist.name, (
               SELECT sort.language
               FROM @bae_records::records::name_sort::NameSortLanguage AS sort))
               AS ranked_name_key
    FROM @bae_records::records::browse_fragments::ArtistRecord AS artist
    JOIN @bae_records::records::browse_fragments::ShownArtist AS shown
      ON shown.artist_id = artist.artist_id AND shown.shown_artist_id = artist.artist_id
    WHERE artist.artist_id IN (
              SELECT :text
              UNION ALL
              SELECT discogs.artist_id
              FROM @bae_records::records::browse_fragments::ArtistRecord AS discogs
              WHERE discogs.discogs_artist_id = :text
              UNION ALL
              SELECT musicbrainz.artist_id
              FROM @bae_records::records::browse_fragments::ArtistRecord AS musicbrainz
              WHERE musicbrainz.musicbrainz_artist_id = :text
              UNION ALL
              SELECT found.artist_id
              FROM @crate::album_search::ArtistsFound(pattern: :pattern) AS found)
"#
);

// The window of what the text finds: the artists from the window's first key
// on, by rank, name and id, each with its key, image and other printed names.
// The subquery picks the window's artists, so only their rows are read.
sqlive_compose::read!(
    writer ArtistSearchWindow(
        text: String,
        pattern: String,
        limit: i64,
        key_rank: bae_store::list_windows::SortValue,
        key_name: bae_store::list_windows::SortValue,
        key_id: String,
    ) { order_rank: i64, order_name: Vec<u8>, order_id: String },
    r#"
    SELECT picked.match_rank AS order_rank, picked.ranked_name_key AS order_name,
           picked.ranked_artist_id AS order_id,
           artist.*, image.*, names.*
    FROM (SELECT ranked.ranked_artist_id, ranked.match_rank, ranked.ranked_name_key
          FROM @crate::artist_search::RankedArtists(text: :text, pattern: :pattern) AS ranked
          WHERE (ranked.match_rank, ranked.ranked_name_key, ranked.ranked_artist_id)
                >= (:key_rank, :key_name, :key_id)
          ORDER BY ranked.match_rank, ranked.ranked_name_key, ranked.ranked_artist_id
          LIMIT :limit) AS picked
    CROSS JOIN @bae_records::records::browse_fragments::ArtistRecord AS artist
            ON artist.artist_id = picked.ranked_artist_id
    LEFT JOIN @bae_records::records::files::ArtistImageVersion AS image
           ON image.id = picked.ranked_artist_id
    CROSS JOIN @bae_records::records::artist_credit_names::ArtistOtherNames AS names
            ON names.names_artist_id = picked.ranked_artist_id
    ORDER BY picked.match_rank, picked.ranked_name_key, picked.ranked_artist_id
"#
);

// [`ArtistSearchWindow`] from the top of what the text finds.
sqlive_compose::read!(
    writer ArtistSearchTop(
        text: String,
        pattern: String,
        limit: i64
    ) { order_rank: i64, order_name: Vec<u8>, order_id: String },
    r#"
    SELECT picked.match_rank AS order_rank, picked.ranked_name_key AS order_name,
           picked.ranked_artist_id AS order_id,
           artist.*, image.*, names.*
    FROM (SELECT ranked.ranked_artist_id, ranked.match_rank, ranked.ranked_name_key
          FROM @crate::artist_search::RankedArtists(text: :text, pattern: :pattern) AS ranked
          ORDER BY ranked.match_rank, ranked.ranked_name_key, ranked.ranked_artist_id
          LIMIT :limit) AS picked
    CROSS JOIN @bae_records::records::browse_fragments::ArtistRecord AS artist
            ON artist.artist_id = picked.ranked_artist_id
    LEFT JOIN @bae_records::records::files::ArtistImageVersion AS image
           ON image.id = picked.ranked_artist_id
    CROSS JOIN @bae_records::records::artist_credit_names::ArtistOtherNames AS names
            ON names.names_artist_id = picked.ranked_artist_id
    ORDER BY picked.match_rank, picked.ranked_name_key, picked.ranked_artist_id
"#
);

// How many of the artists the text finds come before the row at a key.
sqlive_compose::read!(
    writer ArtistSearchBefore(
        text: String,
        pattern: String,
        key_rank: bae_store::list_windows::SortValue,
        key_name: bae_store::list_windows::SortValue,
        key_id: String,
    ) { artists_before: u64 },
    r#"
    SELECT COUNT(*) AS artists_before
    FROM @crate::artist_search::RankedArtists(text: :text, pattern: :pattern) AS ranked
    WHERE (ranked.match_rank, ranked.ranked_name_key, ranked.ranked_artist_id)
          < (:key_rank, :key_name, :key_id)
"#
);

// The key of the artist at a position of what the text finds, or of the last
// one when it finds fewer; no row when it finds none, since SQLite reads its
// offset of -1 as 0.
sqlive_compose::read!(
    writer ArtistSearchKeyAt(text: String, pattern: String, position: i64) {
        order_rank: i64,
        order_name: Vec<u8>,
        order_id: String
    },
    r#"
    SELECT ranked.match_rank AS order_rank, ranked.ranked_name_key AS order_name,
           ranked.ranked_artist_id AS order_id
    FROM @crate::artist_search::RankedArtists(text: :text, pattern: :pattern) AS ranked
    ORDER BY ranked.match_rank, ranked.ranked_name_key, ranked.ranked_artist_id
    LIMIT 1 OFFSET min(:position, (
        SELECT COUNT(*)
        FROM @crate::artist_search::RankedArtists(text: :text, pattern: :pattern) AS counted) - 1)
"#
);

// How many artists the text finds.
sqlive_compose::read!(
    writer ArtistSearchCount(text: String, pattern: String) { artists: u64 },
    r#"
    SELECT COUNT(*) AS artists
    FROM @crate::artist_search::RankedArtists(text: :text, pattern: :pattern) AS ranked
"#
);

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
        self.store
            .read(move |sql| {
                Ok(artist_search_window_on(&sql, &query, &ListWindow::top(limit))?
                    .rows
                    .into_iter()
                    .map(|(row, _)| row.artist)
                    .collect())
            })
            .await
    }

    /// Follow one window of what the artist picker's text finds: the artists
    /// from a key of the search's order on. No query (a blank text) finds
    /// nothing; its window answers with no rows.
    pub fn subscribe_artist_search_window(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbArtistSearchRow>, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, move |window| {
                let read = query.as_ref().map(|query| artist_search_window_read(query, window));
                Ok(coven::Maybe(read.transpose()?))
            })
            .process(|window, rows| artist_search_window_of(window, rows.unwrap_or_default()))
    }

    /// Follow how many artists the text finds. The count runs as a separate
    /// query from the windows' rows.
    pub fn subscribe_artist_search_count(
        &self,
        query: Option<crate::LibrarySearchQuery>,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let count = query.map(|query| ArtistSearchCountParams {
            text: query.as_str().to_string(),
            pattern: search_pattern(query.as_str()),
        });
        self.store.subscribe(coven::Maybe(count)).process(|rows| match rows {
            Some(rows) => Ok(exactly_one::<ArtistSearchCountParams, _>(rows)?.artists),
            None => Ok(0),
        })
    }

    /// Follow how many of the artists the text finds come before the row at
    /// a key. The caller can change the key without opening a new live query.
    pub fn subscribe_artist_search_position(
        &self,
        query: Option<crate::LibrarySearchQuery>,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        self.store
            .subscribe_reconfigurable(initial, move |key| {
                let read = query.as_ref().map(|query| artist_search_rows_before_read(query, key));
                Ok(coven::Maybe(read.transpose()?))
            })
            .process(|_, rows| match rows {
                Some(rows) => Ok(exactly_one::<ArtistSearchBeforeParams, _>(rows)?.artists_before),
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
        self.store.read(move |sql| artist_search_key_at_on(&sql, &query, position)).await
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

/// The key of the artist ranked `rank` with the name key `name` and the id
/// `id`.
fn artist_search_key(rank: i64, name: Vec<u8>, id: String) -> OrderKey {
    key([SortValue::Integer(rank)], name, id)
}

/// The artists `query` finds that `window` holds, each with its key.
#[cfg(any(test, feature = "test-utils"))]
fn artist_search_window_on(
    sql: &SqlReadContext<'_>,
    query: &crate::LibrarySearchQuery,
    window: &ListWindow,
) -> Result<WindowRead<DbArtistSearchRow>, DbError> {
    artist_search_window_of(window, sql.reads(&artist_search_window_read(query, window)?)?)
}

/// An artist the search finds, as a window's read returns it: the artist, its
/// image's version and its other names, with its key.
type FoundArtist = (
    bae_records::records::browse_fragments::ArtistRecord,
    Option<ArtistImageVersion>,
    bae_records::records::artist_credit_names::ArtistOtherNames,
    OrderKey,
);

/// The read of the artists `query` finds that `window` holds: from the top of
/// what it finds with a statement of its own, else from the window's key.
fn artist_search_window_read(
    query: &crate::LibrarySearchQuery,
    window: &ListWindow,
) -> Result<coven::ReadAs<FoundArtist>, DbError> {
    let (text, pattern) = (query.as_str().to_string(), search_pattern(query.as_str()));
    let limit = sql_bound(window.limit);
    macro_rules! rows {
        ($statement:expr) => {
            coven::ReadAs::new($statement, |row| {
                let key = artist_search_key(row.order_rank, row.order_name, row.order_id);
                (row.artist, row.image, row.names, key)
            })
        };
    }
    Ok(match &window.from {
        None => rows!(ArtistSearchTopParams { text, pattern, limit }),
        Some(from) => {
            let ([key_rank], key_name, key_id) = key_values(from)?;
            rows!(ArtistSearchWindowParams { text, pattern, limit, key_rank, key_name, key_id })
        }
    })
}

/// The window `window` from the rows its read returned, each with its key.
fn artist_search_window_of(
    window: &ListWindow,
    rows: Vec<FoundArtist>,
) -> Result<WindowRead<DbArtistSearchRow>, DbError> {
    let rows = rows
        .into_iter()
        .map(|(artist, image, names, key)| {
            let found = DbArtistSearchRow {
                artist: artist.into_artist()?,
                image_version: image.and_then(ArtistImageVersion::into_version),
                credit_names: names.into_names().0,
            };
            Ok((found, key))
        })
        .collect::<Result<_, DbError>>()?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// The read of how many of the artists `query` finds come before the row at
/// `at`.
fn artist_search_rows_before_read(
    query: &crate::LibrarySearchQuery,
    at: &OrderKey,
) -> Result<ArtistSearchBeforeParams, DbError> {
    let ([key_rank], key_name, key_id) = key_values(at)?;
    Ok(ArtistSearchBeforeParams {
        text: query.as_str().to_string(),
        pattern: search_pattern(query.as_str()),
        key_rank,
        key_name,
        key_id,
    })
}

/// The key of the artist at `position` of what `query` finds, or of the last
/// one when it finds fewer.
fn artist_search_key_at_on(
    sql: &SqlReadContext<'_>,
    query: &crate::LibrarySearchQuery,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    let statement = ArtistSearchKeyAtParams {
        text: query.as_str().to_string(),
        pattern: search_pattern(query.as_str()),
        position: sql_bound(position),
    };
    Ok(statement_optional(sql, &statement)?
        .map(|row| artist_search_key(row.order_rank, row.order_name, row.order_id)))
}

#[cfg(test)]
#[path = "artist_search_tests.rs"]
mod tests;
