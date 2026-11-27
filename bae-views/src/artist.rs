//! The artist views: the artist list and an artist's detail.
//!
//! Its tables and the triggers keeping them are `artist.sql`.

use super::*;
use crate::album::{artist_albums_on, AlbumAppearanceRow};
use crate::album_detail::{AlbumAppearance, ArtistDetail};
use crate::list_sort::{key, key_values, ListSort};
use bae_records::records::files::ArtistImageVersion;
use bae_store::list_windows::{sql_bound, SortValue};
use bae_store::{statement_optional, statement_row, statement_rows};

impl LibraryViews {
    pub async fn get_artist_count(&self) -> Result<u64, DbError> {
        self.store.read(|sql| artist_list_length_on(&sql)).await
    }

    /// The artist list's rows at the places from `offset`, at most `limit`.
    pub async fn get_artist_page(
        &self,
        sort: &[ArtistSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbArtistSummary>, DbError> {
        let sort = artist_sort(sort);
        let page = ListPage { offset, limit };
        self.store.read(move |sql| artist_page_on(&sql, &sort, page)).await
    }

    /// Follow one window of the artist list under `sort`: the rows from a key
    /// of its order on.
    pub fn subscribe_artist_window(
        &self,
        sort: &[ArtistSortCriterion],
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbArtistSummary>, bae_store::DbError>
    {
        let sort = artist_sort(sort);
        self.store.subscribe_reconfigurable(initial, move |window, sql| {
            artist_window_on(&sql, &sort, window)
        })
    }

    /// Follow how many artists the list has. The count runs as a separate
    /// query from the windows, because every artist added or removed changes
    /// it.
    pub fn subscribe_artist_count(&self) -> coven::LiveQuery<u64, bae_store::DbError> {
        self.store.subscribe(|sql| artist_list_length_on(&sql))
    }

    /// Follow how many artists come before the row at a key; the caller
    /// changes the key on the same query.
    pub fn subscribe_artist_position(
        &self,
        sort: &[ArtistSortCriterion],
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let sort = artist_sort(sort);
        self.store
            .subscribe_reconfigurable(initial, move |key, sql| artists_before_on(&sql, &sort, key))
    }

    /// The key of the artist at `position`, or of the last one when the list
    /// ends before it; `None` for an empty list.
    pub async fn artist_key_at(
        &self,
        sort: &[ArtistSortCriterion],
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let sort = artist_sort(sort);
        self.store.read(move |sql| artist_key_at_on(&sql, &sort, position)).await
    }

    /// The artist's page as its view shows it. `None` when the artist
    /// doesn't exist or has no album links.
    pub async fn get_artist_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<ArtistDetail>, DbError> {
        Ok(self.find_artist_detail(artist_id).await?.map(artist_detail_of))
    }

    /// The artist's summary row plus every album whose credit list names it,
    /// in discography order: year ascending with unknown years last, then
    /// title in the device's name order, then id. `None` when the artist
    /// doesn't exist or has no album links, exactly the artists
    /// `get_artist_page` never lists.
    pub(crate) async fn find_artist_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtistDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.store
            .read(move |sql| find_artist_detail_on(&sql, &artist_id))
            .process(|rows| rows.map(ArtistDetailRows::process).transpose())
            .await
    }

    /// Follow the artist `initial` names, or none. The detail view moves to
    /// another artist through the request handle, not another query.
    pub fn subscribe_artist_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ArtistDetailProjection, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, |artist_id, sql| {
                let Some(artist_id) = artist_id else {
                    return Ok(None);
                };
                find_artist_detail_on(&sql, artist_id)
            })
            .process(|detail| {
                Ok(ArtistDetailProjection {
                    detail: detail.map(ArtistDetailRows::process).transpose()?,
                })
            })
    }
}

/// One delivery of an artist list window as the list shows it.
pub fn resolve_artist_window(read: WindowRead<DbArtistSummary>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, crate::ListRows::Artists)
}

/// One delivery of the artist detail as the detail view shows it.
pub fn resolve_artist_detail_projection(
    projection: ArtistDetailProjection,
) -> Option<ArtistDetail> {
    projection.detail.map(artist_detail_of)
}

/// An artist's page as its rows read it.
fn artist_detail_of(raw: DbArtistDetail) -> ArtistDetail {
    ArtistDetail {
        artist: raw.artist,
        credit_names: raw.credit_names,
        albums: raw.albums.into_iter().map(AlbumAppearance::from_raw).collect(),
        credit_name_count: raw.credit_name_count,
    }
}

fn find_artist_detail_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
) -> Result<Option<ArtistDetailRows>, DbError> {
    let statement = ArtistByIdParams { artist: artist_id.to_string() };
    let Some(found) = statement_optional(sql, &statement)? else {
        return Ok(None);
    };
    let artist = found.shown.into_summary()?;
    let albums = artist_albums_on(sql, artist_id)?;
    let credit_names = bae_records::records::artist_credit_names::credit_names_of_on(
        sql,
        artist_id,
        &crate::sort_key_sql("credit_name.name"),
    )?;
    // How many other names the page lists, counted over the rows their
    // statement reads.
    let credit_name_count = sql.query_row(
        &format!(
            "SELECT {credit_names} AS credit_name_count",
            credit_names = bae_records::records::artist_credit_names::credit_name_count_sql("?1"),
        ),
        params![artist_id],
        |row| bae_store::read::count_column(row, "credit_name_count"),
    )?;
    Ok(Some(ArtistDetailRows { artist, credit_names, albums, credit_name_count }))
}

#[derive(Clone, PartialEq)]
struct ArtistDetailRows {
    artist: DbArtistSummary,
    credit_names: Vec<bae_records::credits::ArtistCreditName>,
    albums: Vec<AlbumAppearanceRow>,
    credit_name_count: u64,
}

impl ArtistDetailRows {
    fn process(self) -> Result<DbArtistDetail, DbError> {
        Ok(DbArtistDetail {
            artist: self.artist,
            credit_names: self.credit_names,
            albums: self
                .albums
                .into_iter()
                .map(|row| row.process(false))
                .collect::<Result<_, _>>()?,
            credit_name_count: self.credit_name_count,
        })
    }
}

// Each artist the artist list shows (`artist_listing`), with its count of the
// albums whose credit lists (`album_artists`) show it, and its image's
// version. A track's credits (`track_artists`) list no artist. Various Artists
// is an `artists` row like any other and lists like one.
sqlive_compose::fragment!(
    ListedArtist,
    r#"
    SELECT listing.artist_id AS listed_id, listing.album_count, artist.*, image.*
    FROM artist_listing AS listing
    JOIN @bae_records::records::browse_fragments::ArtistRecord AS artist
      ON artist.artist_id = listing.artist_id
    LEFT JOIN @bae_records::records::files::ArtistImageVersion AS image
           ON image.id = listing.artist_id
"#
);

impl ListedArtist {
    /// The artist as its row shows it.
    pub(crate) fn into_summary(self) -> Result<DbArtistSummary, DbError> {
        let artist = self.artist.into_artist()?;
        let version = self.image.and_then(ArtistImageVersion::into_version);
        Ok(DbArtistSummary {
            image: bae_records::artist_image_ref(&artist.id, version),
            artist,
            album_count: self.album_count,
        })
    }
}

// The artist `artist`, as the list shows it; no row for an artist the list
// doesn't show.
sqlive_compose::read!(
    ArtistById(artist: String),
    r#"
    SELECT shown.*
    FROM @crate::artist::ListedArtist AS shown
    WHERE shown.listed_id = :artist
"#
);

// The artist list's statements order by the person's sort of up to 2
// criteria, each of them one fixed statement for every sort. `field_N` names
// the Nth criterion's field, NULL past the sort's last, and `descending_N`
// runs it the other way. After the criteria, rows go by name, then id, which
// tells every two apart. A key is a row's value for each criterion, its name
// key and its id. A row comes before a key when the first of those it differs
// in puts it first; no criterion's value is NULL, so only the criteria past
// the sort's last compare two NULLs. A window from the top binds a NULL key,
// which no row comes before. The window and the count before a key write that
// comparison over the list table's own columns, so coven reruns each only for
// a write on its side of the key. No index serves a sort the parameters name,
// so each read goes through the whole list.

// The artist list's rows from a key on, at most `limit` of them, in the sort's
// order. Each criterion has an ascending and a descending term, one of which is
// NULL in every row, as both are past the sort's last criterion. Each term is
// written over columns of the list table and returned ahead of the row, so
// coven reruns a full window only for a write at or before its last row.
sqlive_compose::read!(
    ArtistWindow(
        field_1: Option<String>,
        field_2: Option<String>,
        descending_1: bool,
        descending_2: bool,
        key_1: SortValue,
        key_2: SortValue,
        key_name: SortValue,
        key_id: Option<String>,
        limit: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue
    },
    r#"
    SELECT CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END AS order_1,
           CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END AS order_2,
           CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END AS order_3,
           CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END AS order_4,
           listing.name_sort_key AS order_name, listing.artist_id AS order_id,
           shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    WHERE NOT COALESCE(
            CASE WHEN :descending_1
                 THEN CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END > :key_1
                 ELSE CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END < :key_1 END
            OR (CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END IS :key_1 AND (
                CASE WHEN :descending_2
                     THEN CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END > :key_2
                     ELSE CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END < :key_2 END
                OR (CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END IS :key_2 AND (
                    (listing.name_sort_key, listing.artist_id) < (:key_name, :key_id))))),
            0)
    ORDER BY CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END,
             CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END DESC,
             CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END,
             CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END DESC,
             listing.name_sort_key, listing.artist_id
    LIMIT :limit
"#
);

// The artist list's rows at the places from `offset` in the sort's order, at
// most `limit` of them.
sqlive_compose::read!(
    ArtistRows(
        field_1: Option<String>,
        field_2: Option<String>,
        descending_1: bool,
        descending_2: bool,
        offset: i64,
        limit: i64,
    ),
    r#"
    SELECT shown.*
    FROM artist_listing AS listing
    JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = listing.artist_id
    ORDER BY CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END,
             CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END DESC,
             CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END,
             CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END DESC,
             listing.name_sort_key, listing.artist_id
    LIMIT :limit OFFSET :offset
"#
);

// How many artists of the sort's order come before the row at a key.
sqlive_compose::read!(
    ArtistsBefore(
        field_1: Option<String>,
        field_2: Option<String>,
        descending_1: bool,
        descending_2: bool,
        key_1: SortValue,
        key_2: SortValue,
        key_name: SortValue,
        key_id: Option<String>
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM artist_listing AS listing
    WHERE COALESCE(
            CASE WHEN :descending_1
                 THEN CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END > :key_1
                 ELSE CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END < :key_1 END
            OR (CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END IS :key_1 AND (
                CASE WHEN :descending_2
                     THEN CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END > :key_2
                     ELSE CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END < :key_2 END
                OR (CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END IS :key_2 AND (
                    (listing.name_sort_key, listing.artist_id) < (:key_name, :key_id))))),
            0)
"#
);

// The key of the row at the place `index` of the sort's order, or of the last
// row when the list ends before it. SQLite reads the offset -1 of an empty
// list as 0, where there is no row.
sqlive_compose::read!(
    ArtistKeyAt(
        field_1: Option<String>,
        field_2: Option<String>,
        descending_1: bool,
        descending_2: bool,
        index: i64
    ) {
        order_name: Vec<u8>,
        order_id: String,
        order_1: SortValue,
        order_2: SortValue,
        order_3: SortValue,
        order_4: SortValue
    },
    r#"
    SELECT CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END AS order_1,
           CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END AS order_2,
           CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END AS order_3,
           CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END AS order_4,
           listing.name_sort_key AS order_name, listing.artist_id AS order_id
    FROM artist_listing AS listing
    ORDER BY CASE WHEN NOT :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END,
             CASE WHEN :descending_1 THEN
               CASE :field_1 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END DESC,
             CASE WHEN NOT :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END,
             CASE WHEN :descending_2 THEN
               CASE :field_2 WHEN 'name' THEN listing.name_sort_key WHEN 'albums' THEN listing.album_count END END DESC,
             listing.name_sort_key, listing.artist_id
    LIMIT 1 OFFSET min(:index, (SELECT COUNT(*) FROM artist_listing AS counted) - 1)
"#
);

// How many artists the list has.
sqlive_compose::read!(
    ArtistListLength { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM artist_listing AS listing
"#
);

/// The artist list's `window` under `sort`, each row with its key.
pub(crate) fn artist_window_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<2>,
    window: &ListWindow,
) -> Result<WindowRead<DbArtistSummary>, DbError> {
    let (mut key_1, mut key_2, mut key_name, mut key_id) =
        (SortValue::Null, SortValue::Null, SortValue::Null, None);
    if let Some(from) = &window.from {
        let ([sort_1, sort_2], name, id) = key_values(from)?;
        (key_1, key_2, key_name, key_id) = (sort_1, sort_2, name, Some(id));
    }
    // A window from the top starts after no row. One from a key starts at the
    // key's row: the rows that don't come before it.
    let statement = ArtistWindowParams {
        field_1: sort.field(0),
        field_2: sort.field(1),
        descending_1: sort.descending(0),
        descending_2: sort.descending(1),
        key_1,
        key_2,
        key_name,
        key_id,
        limit: sql_bound(window.limit),
    };
    let rows = statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| {
            let key = key(
                [sort.term(0, row.order_1, row.order_2), sort.term(1, row.order_3, row.order_4)],
                row.order_name,
                row.order_id,
            );
            Ok((row.shown.into_summary()?, key))
        })
        .collect::<Result<_, DbError>>()?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// The artist list's rows at `page`'s places under `sort`.
pub(crate) fn artist_page_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<2>,
    page: ListPage,
) -> Result<Vec<DbArtistSummary>, DbError> {
    let statement = ArtistRowsParams {
        field_1: sort.field(0),
        field_2: sort.field(1),
        descending_1: sort.descending(0),
        descending_2: sort.descending(1),
        offset: sql_bound(page.offset),
        limit: sql_bound(page.limit),
    };
    statement_rows(sql, &statement)?.into_iter().map(|row| row.shown.into_summary()).collect()
}

/// How many artists come before the row at `key` under `sort`.
pub(crate) fn artists_before_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<2>,
    key: &OrderKey,
) -> Result<u64, DbError> {
    let ([key_1, key_2], key_name, id) = key_values(key)?;
    let statement = ArtistsBeforeParams {
        field_1: sort.field(0),
        field_2: sort.field(1),
        descending_1: sort.descending(0),
        descending_2: sort.descending(1),
        key_1,
        key_2,
        key_name,
        key_id: Some(id),
    };
    list_count(statement_row(sql, &statement)?.entries)
}

/// The key of the artist at `position` under `sort`, or of the last one.
pub(crate) fn artist_key_at_on(
    sql: &SqlReadContext<'_>,
    sort: &ListSort<2>,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    let statement = ArtistKeyAtParams {
        field_1: sort.field(0),
        field_2: sort.field(1),
        descending_1: sort.descending(0),
        descending_2: sort.descending(1),
        index: sql_bound(position),
    };
    Ok(statement_optional(sql, &statement)?.map(|row| {
        key(
            [sort.term(0, row.order_1, row.order_2), sort.term(1, row.order_3, row.order_4)],
            row.order_name,
            row.order_id,
        )
    }))
}

/// How many artists the list has.
pub(crate) fn artist_list_length_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    list_count(statement_row(sql, &ArtistListLengthParams {})?.entries)
}

// The first `limit` artists library search finds for `pattern`, in name order,
// ties going by id, each beside how many rows the statement lists.
sqlive_compose::read!(
    FirstArtistsFound(pattern: String, limit: i64) { listed: i64 },
    r#"
    SELECT shown.*, COUNT(*) OVER () AS listed
    FROM (SELECT listing.artist_id, listing.name_sort_key
          FROM artist_listing AS listing
          WHERE listing.artist_id IN (
              SELECT matched.artist_id
              FROM @crate::album_search::ArtistsFound(pattern: :pattern) AS matched)
          ORDER BY listing.name_sort_key, listing.artist_id
          LIMIT :limit) AS found
    CROSS JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = found.artist_id
    ORDER BY found.name_sort_key, found.artist_id
"#
);

/// The artists [`FirstArtistsFound`] lists for `pattern`, at most `limit`,
/// with how many it lists.
pub(crate) fn first_artists_found_on(
    sql: &SqlReadContext<'_>,
    pattern: &str,
    limit: i64,
) -> Result<(Vec<DbArtistSummary>, u64), DbError> {
    let statement = FirstArtistsFoundParams { pattern: pattern.to_string(), limit };
    let rows = statement_rows(sql, &statement)?;
    let listed = list_count(rows.first().map_or(0, |row| row.listed))?;
    let found = rows.into_iter().map(|row| row.shown.into_summary()).collect::<Result<_, _>>()?;
    Ok((found, listed))
}

/// The artist list's sort as its statements bind it.
pub(crate) fn artist_sort(sort: &[ArtistSortCriterion]) -> ListSort<2> {
    ListSort::of(sort.iter().map(|criterion| {
        let field = match criterion.field {
            ArtistSortField::Name => "name",
            ArtistSortField::AlbumCount => "albums",
        };
        (field, criterion.direction)
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtistDetailProjection {
    pub detail: Option<DbArtistDetail>,
}

#[cfg(test)]
#[path = "artist_tests.rs"]
mod tests;
