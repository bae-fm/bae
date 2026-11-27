//! A composer's page: the composer and the work it opens first, and the
//! page's rows (its works, then its unlinked release credits, then its
//! unlinked track credits) read in windows from `composer_detail_row`, which
//! holds them in the page's order.
//!
//! Its tables and the triggers keeping them are `composer_detail.sql`.

use super::*;
use crate::album_detail::{ComposerDetail, ComposerDetailItem, ComposerDetailRow, ComposerHeading};
use crate::composer::work_summary;
use bae_store::list_windows::SortValue;
use bae_store::{statement_optional, statement_row, statement_rows};

impl LibraryViews {
    pub(crate) async fn find_composer_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbComposerDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.store.read(move |sql| find_composer_detail_on(&sql, &artist_id)).await
    }

    /// The composer's page above its rows, read once; `None` when the
    /// composer doesn't exist.
    pub async fn get_composer_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<ComposerDetail>, DbError> {
        Ok(self.find_composer_detail(artist_id).await?.map(composer_detail_of))
    }

    /// Follow the composer `initial` names, or none. The detail view moves to
    /// another composer through the request handle, not another query.
    pub fn subscribe_composer_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ComposerDetailProjection, bae_store::DbError>
    {
        self.store.subscribe_reconfigurable(initial, |artist_id, sql| {
            let Some(artist_id) = artist_id else {
                return Ok(ComposerDetailProjection { detail: None });
            };
            Ok(ComposerDetailProjection { detail: find_composer_detail_on(&sql, artist_id)? })
        })
    }

    /// The rows of `artist_id`'s page from `offset`, at most `limit`.
    #[cfg(test)]
    pub(crate) async fn get_composer_detail_rows(
        &self,
        artist_id: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<DbComposerDetailRow>, DbError> {
        let artist_id = artist_id.to_string();
        let page = ListPage { offset, limit };
        self.store.read(move |sql| composer_page_on(&sql, &artist_id, page)).await
    }

    /// Follow one window of `artist_id`'s page: the rows at or after a key of
    /// its order, which coven reruns for a write at or before the last of them.
    pub fn subscribe_composer_detail_window(
        &self,
        artist_id: &str,
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<
        ListWindow,
        WindowRead<DbComposerDetailRow>,
        bae_store::DbError,
    > {
        let artist_id = artist_id.to_string();
        self.store.subscribe_reconfigurable(initial, move |window, sql| {
            composer_page_window_on(&sql, &artist_id, window)
        })
    }

    /// Follow one page of the rows of `artist_id`'s composer page: the rows
    /// at the page's places as each write leaves them.
    pub fn subscribe_composer_detail_page(
        &self,
        artist_id: &str,
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let artist_id = artist_id.to_string();
        self.store
            .subscribe(move |sql| composer_page_on(&sql, &artist_id, page))
            .process(|rows| Ok(composer_detail_rows(rows)))
    }

    /// Follow how many rows `artist_id`'s page holds. The count runs as a
    /// separate query from the windows' rows.
    pub fn subscribe_composer_detail_count(
        &self,
        artist_id: &str,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let artist_id = artist_id.to_string();
        self.store.subscribe(move |sql| composer_page_length_on(&sql, &artist_id))
    }

    /// Follow how many rows of `artist_id`'s page come before the row at a
    /// key; the caller changes the key on the same query.
    pub fn subscribe_composer_detail_position(
        &self,
        artist_id: &str,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let artist_id = artist_id.to_string();
        self.store.subscribe_reconfigurable(initial, move |key, sql| {
            composer_page_rows_before_on(&sql, &artist_id, key)
        })
    }

    /// The key of the row at `position` of `artist_id`'s page, or of its last
    /// row when it ends before it; `None` for an empty page.
    pub async fn composer_detail_key_at(
        &self,
        artist_id: &str,
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let artist_id = artist_id.to_string();
        self.store.read(move |sql| composer_page_key_at_on(&sql, &artist_id, position)).await
    }
}

/// One delivery of a composer's page above its rows, as the detail view
/// shows it.
pub fn resolve_composer_detail_projection(
    projection: ComposerDetailProjection,
) -> Option<ComposerDetail> {
    projection.detail.map(composer_detail_of)
}

/// Converts the composer and the work its page opens first, as the database
/// read returned them, into the page's `ComposerDetail`.
fn composer_detail_of(raw: DbComposerDetail) -> ComposerDetail {
    ComposerDetail { composer: raw.composer, default_work_id: raw.default_work_id }
}

/// One delivery of a composer's page's window as the page shows it.
pub fn resolve_composer_detail_window(
    read: WindowRead<DbComposerDetailRow>,
) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, composer_detail_rows)
}

/// A composer's page's rows as the page shows them.
fn composer_detail_rows(rows: Vec<DbComposerDetailRow>) -> crate::ListRows {
    crate::ListRows::ComposerPage(rows.into_iter().map(composer_detail_row_of).collect())
}

/// A row of a composer's page as its read found it, with the heading it
/// opens.
fn composer_detail_row_of(raw: DbComposerDetailRow) -> ComposerDetailRow {
    let DbComposerDetailRow { entry, opens_heading, item } = raw;
    let item = match item {
        DbComposerDetailItem::Work { work, part } => ComposerDetailItem::Work { work, part },
        DbComposerDetailItem::ReleaseCredit(credit) => ComposerDetailItem::ReleaseCredit(credit),
        DbComposerDetailItem::TrackCredit(credit) => ComposerDetailItem::TrackCredit(credit),
    };
    ComposerDetailRow { id: entry, heading: opens_heading.then(|| heading_over(&item)), item }
}

/// Where a composer's page puts its headings: one over each of its parts
/// (`composer_detail_row.section`), opened by the part's first row
/// ([`ComposerPageEntry`]), worded by what the part lists.
fn heading_over(item: &ComposerDetailItem) -> ComposerHeading {
    match item {
        ComposerDetailItem::Work { .. } => ComposerHeading::Works,
        ComposerDetailItem::ReleaseCredit(_) => ComposerHeading::Credits,
        ComposerDetailItem::TrackCredit(_) => ComposerHeading::Recordings,
    }
}

// What each row of a composer's page shows, by the row's number: a work with
// its summary and cover, or a credit with its role and its release's album,
// and for a track credit its track and the composer; and whether the row is
// the first of its part of the page, which opens the part's heading.
sqlive_compose::fragment!(
    ComposerPageEntry { opens_heading: bool },
    r#"
    SELECT listing.entry AS row_entry, listing.section AS part, listing.place AS indent,
           ((SELECT first.entry FROM composer_detail_row AS first
             WHERE first.artist_id = listing.artist_id AND first.section = listing.section
             ORDER BY first.section, first.group_title_sort_key, first.sort_group_album, first.sort_group_order, first.group_id, first.sort_place, first.sort_title, first.sort_position, first.item_id
             LIMIT 1) = listing.entry) AS opens_heading,
           work.*, work_cover.*, release_credit.*, track_credit.*, track.*, release.*,
           album.*, composer.*
    FROM composer_detail_row AS listing
    LEFT JOIN @bae_records::records::browse_fragments::WorkSummary AS work
           ON listing.section = 0 AND work.work_id = listing.item_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS work_cover
           ON work_cover.id = work.representative_release_id
    LEFT JOIN @bae_records::records::browse_fragments::ReleaseRoleCredit AS release_credit
           ON listing.section = 1 AND release_credit.role_id = listing.item_id
    LEFT JOIN @bae_records::records::browse_fragments::TrackRoleCredit AS track_credit
           ON listing.section = 2 AND track_credit.role_id = listing.item_id
    LEFT JOIN @bae_records::records::browse_fragments::TrackRecord AS track
           ON track.track_id = track_credit.track_id
    LEFT JOIN @bae_records::records::fragments::ReleaseAlbum AS release
           ON listing.section <> 0 AND release.release_id = listing.group_id
    LEFT JOIN @bae_records::records::browse_fragments::AlbumRecord AS album
           ON album.album_id = release.album_id
    LEFT JOIN @bae_records::records::browse_fragments::ArtistRecord AS composer
           ON listing.section = 2 AND composer.artist_id = listing.artist_id
"#
);

// The rows of the composer `artist`'s page at or after a key of its order, at
// most `limit`, each with its key. The order is the page's index's: its terms
// after the artist, which end with terms that tell every row of a page apart.
sqlive_compose::read!(
    ComposerPageWindow(
        artist: String,
        order_key_0: SortValue,
        order_key_1: SortValue,
        order_key_2: SortValue,
        order_key_3: SortValue,
        order_key_4: SortValue,
        order_key_5: SortValue,
        order_key_6: SortValue,
        order_key_7: SortValue,
        order_key_8: SortValue,
        limit: i64
    ) { order_key_0: SortValue, order_key_1: SortValue, order_key_2: SortValue, order_key_3: SortValue, order_key_4: SortValue, order_key_5: SortValue, order_key_6: SortValue, order_key_7: SortValue, order_key_8: SortValue },
    r#"
    SELECT listing.section AS order_key_0,
           listing.group_title_sort_key AS order_key_1,
           listing.sort_group_album AS order_key_2,
           listing.sort_group_order AS order_key_3,
           listing.group_id AS order_key_4,
           listing.sort_place AS order_key_5,
           listing.sort_title AS order_key_6,
           listing.sort_position AS order_key_7,
           listing.item_id AS order_key_8,
           shown.*
    FROM composer_detail_row AS listing
    CROSS JOIN @crate::composer_detail::ComposerPageEntry AS shown
            ON shown.row_entry = listing.entry
    WHERE listing.artist_id = :artist
      AND (listing.section, listing.group_title_sort_key, listing.sort_group_album, listing.sort_group_order, listing.group_id, listing.sort_place, listing.sort_title, listing.sort_position, listing.item_id)
          >= (:order_key_0, :order_key_1, :order_key_2, :order_key_3, :order_key_4, :order_key_5, :order_key_6, :order_key_7, :order_key_8)
    ORDER BY listing.section, listing.group_title_sort_key, listing.sort_group_album, listing.sort_group_order, listing.group_id, listing.sort_place, listing.sort_title, listing.sort_position, listing.item_id
    LIMIT :limit
"#
);

// The rows of the composer `artist`'s page at the places from `offset`, at
// most `limit`.
sqlive_compose::read!(
    ComposerPageRows(artist: String, offset: i64, limit: i64),
    r#"
    SELECT shown.*
    FROM composer_detail_row AS listing
    CROSS JOIN @crate::composer_detail::ComposerPageEntry AS shown
            ON shown.row_entry = listing.entry
    WHERE listing.artist_id = :artist
    ORDER BY listing.section, listing.group_title_sort_key, listing.sort_group_album, listing.sort_group_order, listing.group_id, listing.sort_place, listing.sort_title, listing.sort_position, listing.item_id
    LIMIT :limit OFFSET :offset
"#
);

// How many rows the composer `artist`'s page lists.
sqlive_compose::read!(
    ComposerPageLength(artist: String) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_detail_row AS listing
    WHERE listing.artist_id = :artist
"#
);

// How many rows of the composer `artist`'s page come before a key of its
// order.
sqlive_compose::read!(
    ComposerPageRowsBefore(
        artist: String,
        order_key_0: SortValue,
        order_key_1: SortValue,
        order_key_2: SortValue,
        order_key_3: SortValue,
        order_key_4: SortValue,
        order_key_5: SortValue,
        order_key_6: SortValue,
        order_key_7: SortValue,
        order_key_8: SortValue
    ) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_detail_row AS listing
    WHERE listing.artist_id = :artist
      AND (listing.section, listing.group_title_sort_key, listing.sort_group_album, listing.sort_group_order, listing.group_id, listing.sort_place, listing.sort_title, listing.sort_position, listing.item_id)
          < (:order_key_0, :order_key_1, :order_key_2, :order_key_3, :order_key_4, :order_key_5, :order_key_6, :order_key_7, :order_key_8)
"#
);

// The key of the row at the place `index` of the composer `artist`'s page, or
// of its last row when the page ends before it. SQLite reads the offset -1 of
// an empty page as 0, where there is no row.
sqlive_compose::read!(
    ComposerPageKeyAt(artist: String, index: i64) { order_key_0: SortValue, order_key_1: SortValue, order_key_2: SortValue, order_key_3: SortValue, order_key_4: SortValue, order_key_5: SortValue, order_key_6: SortValue, order_key_7: SortValue, order_key_8: SortValue },
    r#"
    SELECT listing.section AS order_key_0,
           listing.group_title_sort_key AS order_key_1,
           listing.sort_group_album AS order_key_2,
           listing.sort_group_order AS order_key_3,
           listing.group_id AS order_key_4,
           listing.sort_place AS order_key_5,
           listing.sort_title AS order_key_6,
           listing.sort_position AS order_key_7,
           listing.item_id AS order_key_8
    FROM composer_detail_row AS listing
    WHERE listing.artist_id = :artist
    ORDER BY listing.section, listing.group_title_sort_key, listing.sort_group_album, listing.sort_group_order, listing.group_id, listing.sort_place, listing.sort_title, listing.sort_position, listing.item_id
    LIMIT 1 OFFSET min(
        :index,
        (SELECT COUNT(*) FROM composer_detail_row AS counted
         WHERE counted.artist_id = :artist) - 1)
"#
);

// The first work the composer `artist`'s page lists.
sqlive_compose::read!(
    FirstListedWork(artist: String),
    r#"
    SELECT listing.item_id
    FROM composer_detail_row AS listing
    WHERE listing.artist_id = :artist AND listing.section = 0
    ORDER BY listing.section, listing.group_title_sort_key, listing.sort_group_album, listing.sort_group_order, listing.group_id, listing.sort_place, listing.sort_title, listing.sort_position, listing.item_id
    LIMIT 1
"#
);

/// A key before every row of a composer's page: a part before the first.
fn top_of_page() -> [SortValue; 9] {
    let text = || SortValue::Text(String::new());
    let blob = || SortValue::Blob(Vec::new());
    let zero = || SortValue::Integer(0);
    [
        SortValue::Integer(-1),
        blob(),
        text(),
        SortValue::Real(0.0),
        text(),
        zero(),
        blob(),
        zero(),
        text(),
    ]
}

/// The nine values of the page's order `key` holds. Only the page's reads make
/// its keys.
fn page_key_values(key: &OrderKey) -> Result<[SortValue; 9], DbError> {
    <[SortValue; 9]>::try_from(key.values().to_vec())
        .map_err(|values| DbError::unreadable("a composer page key", values))
}

/// The `window` of `artist_id`'s page, each row with its key.
fn composer_page_window_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
    window: &ListWindow,
) -> Result<WindowRead<DbComposerDetailRow>, DbError> {
    let from = window.from.as_ref().map(page_key_values).transpose()?;
    let [order_key_0, order_key_1, order_key_2, order_key_3, order_key_4, order_key_5, order_key_6, order_key_7, order_key_8] =
        from.unwrap_or_else(top_of_page);
    let statement = ComposerPageWindowParams {
        artist: artist_id.to_string(),
        order_key_0,
        order_key_1,
        order_key_2,
        order_key_3,
        order_key_4,
        order_key_5,
        order_key_6,
        order_key_7,
        order_key_8,
        limit: bae_store::list_windows::sql_bound(window.limit),
    };
    let rows = statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| {
            let key = OrderKey::new(vec![
                row.order_key_0,
                row.order_key_1,
                row.order_key_2,
                row.order_key_3,
                row.order_key_4,
                row.order_key_5,
                row.order_key_6,
                row.order_key_7,
                row.order_key_8,
            ]);
            Ok((row_of_entry(row.shown)?, key))
        })
        .collect::<Result<_, DbError>>()?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// The rows of `artist_id`'s page at `page`'s places.
fn composer_page_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
    page: ListPage,
) -> Result<Vec<DbComposerDetailRow>, DbError> {
    let statement = ComposerPageRowsParams {
        artist: artist_id.to_string(),
        offset: bae_store::list_windows::sql_bound(page.offset),
        limit: bae_store::list_windows::sql_bound(page.limit),
    };
    statement_rows(sql, &statement)?.into_iter().map(|row| row_of_entry(row.shown)).collect()
}

/// How many rows `artist_id`'s page lists.
fn composer_page_length_on(sql: &SqlReadContext<'_>, artist_id: &str) -> Result<u64, DbError> {
    let statement = ComposerPageLengthParams { artist: artist_id.to_string() };
    list_count(statement_row(sql, &statement)?.entries)
}

/// How many rows of `artist_id`'s page come before the row at `key`.
fn composer_page_rows_before_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
    key: &OrderKey,
) -> Result<u64, DbError> {
    let [order_key_0, order_key_1, order_key_2, order_key_3, order_key_4, order_key_5, order_key_6, order_key_7, order_key_8] =
        page_key_values(key)?;
    let statement = ComposerPageRowsBeforeParams {
        artist: artist_id.to_string(),
        order_key_0,
        order_key_1,
        order_key_2,
        order_key_3,
        order_key_4,
        order_key_5,
        order_key_6,
        order_key_7,
        order_key_8,
    };
    list_count(statement_row(sql, &statement)?.entries)
}

/// The key of the row at `position` of `artist_id`'s page, or of its last row.
fn composer_page_key_at_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    let statement = ComposerPageKeyAtParams {
        artist: artist_id.to_string(),
        index: bae_store::list_windows::sql_bound(position),
    };
    Ok(statement_optional(sql, &statement)?.map(|row| {
        OrderKey::new(vec![
            row.order_key_0,
            row.order_key_1,
            row.order_key_2,
            row.order_key_3,
            row.order_key_4,
            row.order_key_5,
            row.order_key_6,
            row.order_key_7,
            row.order_key_8,
        ])
    }))
}

/// A row of a composer's page, read by its part of the page: the works (0),
/// the release credits (1) or the track credits (2).
fn row_of_entry(entry: ComposerPageEntry) -> Result<DbComposerDetailRow, DbError> {
    let ComposerPageEntry {
        row_entry,
        part,
        indent,
        opens_heading,
        work,
        work_cover,
        release_credit,
        track_credit,
        track,
        release: _,
        album,
        composer,
    } = entry;
    let missing = |what: &str| {
        DbError::unreadable(format!("the {what} of composer page row {row_entry}"), part)
    };
    let item = match part {
        0 => DbComposerDetailItem::Work {
            work: work_summary(work.ok_or_else(|| missing("work"))?, work_cover)?,
            part: indent == Some(1),
        },
        1 => DbComposerDetailItem::ReleaseCredit(DbReleaseRoleSummary {
            role: release_credit.ok_or_else(|| missing("credit"))?.into_role()?,
            album: album.ok_or_else(|| missing("album"))?.into_album()?,
        }),
        2 => DbComposerDetailItem::TrackCredit(DbTrackRoleSummary {
            role: track_credit.ok_or_else(|| missing("credit"))?.into_role()?,
            track: track.ok_or_else(|| missing("track"))?.into_track()?,
            album: album.ok_or_else(|| missing("album"))?.into_album()?,
            artist: composer.ok_or_else(|| missing("composer"))?.into_artist()?,
        }),
        other => return Err(DbError::unreadable("a composer page row's section", other)),
    };
    Ok(DbComposerDetailRow { entry: row_entry, opens_heading, item })
}

/// The composer `artist_id` names, with the first work its page lists;
/// `None` when the composer list doesn't show it.
fn find_composer_detail_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
) -> Result<Option<DbComposerDetail>, DbError> {
    let composer = sql
        .query_row(
            &composer_listing_query(None, Some("WHERE listing.artist_id = ?"), None),
            params![artist_id],
            row_to_composer_summary,
        )
        .optional()?;
    let Some(composer) = composer else {
        return Ok(None);
    };
    let statement = FirstListedWorkParams { artist: artist_id.to_string() };
    let default_work_id = statement_optional(sql, &statement)?.map(|row| row.item_id);
    Ok(Some(DbComposerDetail { composer, default_work_id }))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerDetailProjection {
    pub detail: Option<DbComposerDetail>,
}

#[cfg(test)]
#[path = "composer_detail_tests.rs"]
mod tests;
