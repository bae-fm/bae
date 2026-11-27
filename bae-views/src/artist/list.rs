//! The artist list's reads: its windows, pages, positions and jumps under
//! the person's sort of up to 2 criteria, and its length.
//!
//! After the criteria, rows go by name, then id, which tells every two apart.
//! Every term of the order sorts ascending and none is NULL. The first
//! criterion's term is a column: a descending one is generated to sort
//! ascending (`artist.sql`), a count negated, and the name's key written in
//! hex, which sorts ascending in the `descending` collation. Each later place
//! has two terms, one compared by its bytes and one in that collation, each a
//! CASE over `field_N`, which names the Nth criterion's term. The term a
//! place does not use, and both past the sort's last criterion, hold one value
//! in every row. A key is a row's value for each of those terms, its name key
//! and its id. The CASEs write out the generated columns' expressions, because
//! coven narrows an expression over stored columns alone.
//!
//! SQLite fixes the column a statement's index leads with, so each read has
//! one statement for each term the sort can lead with. Each statement writes
//! the order over the list table's own columns, and what coven reruns a window
//! or a count for depends on the term the sort leads with. Under an ascending
//! one, only a write on their side of the key reruns them, and a full window
//! only for a write at or before its last row. coven narrows a WHERE by stored
//! columns alone, so under a descending count, whose column is generated, a
//! write before a window's start reruns it too, and any write reruns the
//! count. Under the name descending, whose generated column is in a collation
//! of its own, coven narrows neither bound, so any write to the list reruns
//! them. No index covers the terms after the first, so SQLite sorts the rows
//! that tie on the first term, and a window reads all of them: few when the
//! sort leads with the name, and every artist with one count when it leads
//! with a count.

use super::*;
use crate::list_sort::{before_every_row, key, key_values, ListSort, ListSortField};
use bae_store::list_windows::{sql_bound, SortValue};
use bae_store::{statement_optional, statement_row, statement_rows};

impl ListSortField for ArtistSortField {
    const NAME: Self = Self::Name;

    fn term(self, direction: SortDirection) -> &'static str {
        match (self, direction) {
            (ArtistSortField::Name, SortDirection::Ascending) => "name",
            (ArtistSortField::Name, SortDirection::Descending) => "name_descending",
            (ArtistSortField::AlbumCount, SortDirection::Ascending) => "albums",
            (ArtistSortField::AlbumCount, SortDirection::Descending) => "albums_descending",
        }
    }
}

// The artist list's rows from a key on, at most `limit` of them, for each
// first term. Each term is returned ahead of the row, so coven reruns a full
// window only for a write at or before its last row.
sqlive_compose::read!(
    writer ArtistWindowByName(
        field_2: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: String,
        limit: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue
    },
    r#"
    SELECT listing.name_sort_key AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id,
           shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    WHERE (listing.name_sort_key,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_name, :key_id)
    ORDER BY listing.name_sort_key,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);

sqlive_compose::read!(
    writer ArtistWindowByNameDescending(
        field_2: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: String,
        limit: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue
    },
    r#"
    SELECT listing.sort_name_back AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id,
           shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    WHERE (listing.sort_name_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_name, :key_id)
    ORDER BY listing.sort_name_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);

sqlive_compose::read!(
    writer ArtistWindowByAlbums(
        field_2: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: String,
        limit: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue
    },
    r#"
    SELECT listing.album_count AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id,
           shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    WHERE (listing.album_count,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_name, :key_id)
    ORDER BY listing.album_count,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);

sqlive_compose::read!(
    writer ArtistWindowByAlbumsDescending(
        field_2: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: String,
        limit: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue
    },
    r#"
    SELECT listing.sort_albums_back AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id,
           shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    WHERE (listing.sort_albums_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          >= (:key_1, :key_2, :key_3, :key_name, :key_id)
    ORDER BY listing.sort_albums_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit
"#
);

// The artist list's rows at the places from `offset`, at most `limit` of
// them, for each first term.
sqlive_compose::read!(
    writer ArtistRowsByName(field_2: Option<String>, offset: i64, limit: i64),
    r#"
    SELECT shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.name_sort_key,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

sqlive_compose::read!(
    writer ArtistRowsByNameDescending(field_2: Option<String>, offset: i64, limit: i64),
    r#"
    SELECT shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.sort_name_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

sqlive_compose::read!(
    writer ArtistRowsByAlbums(field_2: Option<String>, offset: i64, limit: i64),
    r#"
    SELECT shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.album_count,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

sqlive_compose::read!(
    writer ArtistRowsByAlbumsDescending(field_2: Option<String>, offset: i64, limit: i64),
    r#"
    SELECT shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    ORDER BY listing.sort_albums_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

// How many artists come before the row at a key, for each first term.
sqlive_compose::read!(
    writer ArtistBeforeByName(
        field_2: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM artist_listing AS listing
    WHERE (listing.name_sort_key,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_name, :key_id)
"#
);

sqlive_compose::read!(
    writer ArtistBeforeByNameDescending(
        field_2: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM artist_listing AS listing
    WHERE (listing.sort_name_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_name, :key_id)
"#
);

sqlive_compose::read!(
    writer ArtistBeforeByAlbums(
        field_2: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM artist_listing AS listing
    WHERE (listing.album_count,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_name, :key_id)
"#
);

sqlive_compose::read!(
    writer ArtistBeforeByAlbumsDescending(
        field_2: Option<String>,
        key_1: SortValue,
        key_2: SortValue,
        key_3: SortValue,
        key_name: SortValue,
        key_id: String
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM artist_listing AS listing
    WHERE (listing.sort_albums_back,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
           listing.name_sort_key,
           listing.artist_id)
          < (:key_1, :key_2, :key_3, :key_name, :key_id)
"#
);

// The key of the row at the place `index`, or of the last row when the list
// ends before it, for each first term. SQLite reads the offset -1 of an empty
// list as 0, where there is no row.
sqlive_compose::read!(
    writer ArtistKeyAtByName(
        field_2: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue
    },
    r#"
    SELECT listing.name_sort_key AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM artist_listing AS listing
    ORDER BY listing.name_sort_key,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM artist_listing AS counted) - 1)
"#
);

sqlive_compose::read!(
    writer ArtistKeyAtByNameDescending(
        field_2: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue
    },
    r#"
    SELECT listing.sort_name_back AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM artist_listing AS listing
    ORDER BY listing.sort_name_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM artist_listing AS counted) - 1)
"#
);

sqlive_compose::read!(
    writer ArtistKeyAtByAlbums(
        field_2: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue
    },
    r#"
    SELECT listing.album_count AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM artist_listing AS listing
    ORDER BY listing.album_count,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM artist_listing AS counted) - 1)
"#
);

sqlive_compose::read!(
    writer ArtistKeyAtByAlbumsDescending(
        field_2: Option<String>,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue
    },
    r#"
    SELECT listing.sort_albums_back AS order_1,
           CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END AS order_2,
           CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END AS order_3,
           listing.name_sort_key AS order_name,
           listing.artist_id AS order_id
    FROM artist_listing AS listing
    ORDER BY listing.sort_albums_back,
             CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count WHEN 'albums_descending' THEN -listing.album_count ELSE 0 END,
             CASE :field_2 WHEN 'name_descending' THEN hex(listing.name_sort_key) ELSE '' END COLLATE descending,
             listing.name_sort_key,
             listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM artist_listing AS counted) - 1)
"#
);

// How many artists the list has.
sqlive_compose::read!(
    writer ArtistListLength { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM artist_listing AS listing
"#
);

/// The artist list's `window` under `sort`, each row with its key.
pub(crate) fn artist_window_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<ArtistSortField, 2>,
    window: &ListWindow,
) -> Result<WindowRead<DbArtistSummary>, DbError> {
    // A window from a key starts at the key's row.
    let ([key_1, key_2, key_3], key_name, key_id) = match &window.from {
        Some(from) => key_values(from)?,
        None => before_every_row(),
    };
    let limit = sql_bound(window.limit);
    macro_rules! rows {
        ($params:ident) => {{
            let statement =
                $params { field_2: sort.later(1), key_1, key_2, key_3, key_name, key_id, limit };
            statement_rows(sql, &statement)?
                .into_iter()
                .map(|row| {
                    let key =
                        key([row.order_1, row.order_2, row.order_3], row.order_name, row.order_id);
                    Ok((row.shown.into_summary()?, key))
                })
                .collect::<Result<_, DbError>>()?
        }};
    }
    let rows = match sort.first() {
        (ArtistSortField::Name, SortDirection::Ascending) => rows!(ArtistWindowByNameParams),
        (ArtistSortField::Name, SortDirection::Descending) => {
            rows!(ArtistWindowByNameDescendingParams)
        }
        (ArtistSortField::AlbumCount, SortDirection::Ascending) => {
            rows!(ArtistWindowByAlbumsParams)
        }
        (ArtistSortField::AlbumCount, SortDirection::Descending) => {
            rows!(ArtistWindowByAlbumsDescendingParams)
        }
    };
    Ok(WindowRead { window: window.clone(), rows })
}

/// The artist list's rows at `page`'s places under `sort`.
pub(crate) fn artist_page_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<ArtistSortField, 2>,
    page: ListPage,
) -> Result<Vec<DbArtistSummary>, DbError> {
    let (offset, limit) = (sql_bound(page.offset), sql_bound(page.limit));
    macro_rules! rows {
        ($params:ident) => {{
            let statement = $params { field_2: sort.later(1), offset, limit };
            statement_rows(sql, &statement)?
                .into_iter()
                .map(|row| row.shown.into_summary())
                .collect()
        }};
    }
    match sort.first() {
        (ArtistSortField::Name, SortDirection::Ascending) => rows!(ArtistRowsByNameParams),
        (ArtistSortField::Name, SortDirection::Descending) => {
            rows!(ArtistRowsByNameDescendingParams)
        }
        (ArtistSortField::AlbumCount, SortDirection::Ascending) => rows!(ArtistRowsByAlbumsParams),
        (ArtistSortField::AlbumCount, SortDirection::Descending) => {
            rows!(ArtistRowsByAlbumsDescendingParams)
        }
    }
}

/// How many artists come before the row at `at` under `sort`.
pub(crate) fn artists_before_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<ArtistSortField, 2>,
    at: &OrderKey,
) -> Result<u64, DbError> {
    let ([key_1, key_2, key_3], key_name, key_id) = key_values(at)?;
    macro_rules! entries {
        ($params:ident) => {{
            let statement =
                $params { field_2: sort.later(1), key_1, key_2, key_3, key_name, key_id };
            statement_row(sql, &statement)?.entries
        }};
    }
    list_count(match sort.first() {
        (ArtistSortField::Name, SortDirection::Ascending) => entries!(ArtistBeforeByNameParams),
        (ArtistSortField::Name, SortDirection::Descending) => {
            entries!(ArtistBeforeByNameDescendingParams)
        }
        (ArtistSortField::AlbumCount, SortDirection::Ascending) => {
            entries!(ArtistBeforeByAlbumsParams)
        }
        (ArtistSortField::AlbumCount, SortDirection::Descending) => {
            entries!(ArtistBeforeByAlbumsDescendingParams)
        }
    })
}

/// The key of the artist at `position` under `sort`, or of the last one.
pub(crate) fn artist_key_at_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<ArtistSortField, 2>,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    let index = sql_bound(position);
    macro_rules! key_at {
        ($params:ident) => {{
            let statement = $params { field_2: sort.later(1), index };
            statement_optional(sql, &statement)?.map(|row| {
                key([row.order_1, row.order_2, row.order_3], row.order_name, row.order_id)
            })
        }};
    }
    Ok(match sort.first() {
        (ArtistSortField::Name, SortDirection::Ascending) => key_at!(ArtistKeyAtByNameParams),
        (ArtistSortField::Name, SortDirection::Descending) => {
            key_at!(ArtistKeyAtByNameDescendingParams)
        }
        (ArtistSortField::AlbumCount, SortDirection::Ascending) => {
            key_at!(ArtistKeyAtByAlbumsParams)
        }
        (ArtistSortField::AlbumCount, SortDirection::Descending) => {
            key_at!(ArtistKeyAtByAlbumsDescendingParams)
        }
    })
}

/// How many artists the list has.
pub(crate) fn artist_list_length_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    list_count(statement_row(sql, &ArtistListLengthParams {})?.entries)
}

/// The artist list's sort as its statements bind it.
pub(crate) fn artist_sort(sort: &[ArtistSortCriterion]) -> ListSort<ArtistSortField, 2> {
    ListSort::of(sort.iter().map(|criterion| (criterion.field, criterion.direction)))
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
