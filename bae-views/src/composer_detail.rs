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
use bae_store::statement_optional;

impl LibraryViews {
    #[cfg(test)]
    pub(crate) async fn find_composer_detail(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbComposerDetail>, DbError> {
        let artist_id = artist_id.to_string();
        self.store.read(move |sql| find_composer_detail_on(&sql, &artist_id)).await
    }

    /// Follow the composer `initial` names, or none. The detail view moves to
    /// another composer through the request handle, not another query.
    pub fn subscribe_composer_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ComposerDetailProjection, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, |artist_id| {
                Ok(coven::Maybe::new(artist_id.clone(), |artist| ComposerHeadParams {
                    artist: artist.clone(),
                }))
            })
            .process(|_, rows| {
                let row = rows
                    .map(|(_, rows)| at_most_one::<ComposerHeadParams, _>(rows))
                    .transpose()?
                    .flatten();
                Ok(ComposerDetailProjection { detail: composer_detail_rows_of(row)? })
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
        self.store
            .read(move |sql| {
                sql.reads(&composer_page_read(&artist_id, page))?
                    .into_iter()
                    .map(row_of_entry)
                    .collect()
            })
            .await
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
        self.store
            .subscribe_reconfigurable(initial, move |window| {
                composer_page_window_read(&artist_id, window)
            })
            .process(composer_page_window_of)
    }

    /// Follow one page of the rows of `artist_id`'s composer page: the rows
    /// at the page's places as each write leaves them.
    pub fn subscribe_composer_detail_page(
        &self,
        artist_id: &str,
        page: ListPage,
    ) -> coven::LiveQuery<crate::ListRows, bae_store::DbError> {
        let artist_id = artist_id.to_string();
        self.store.subscribe(composer_page_read(&artist_id, page)).process(|rows| {
            Ok(composer_detail_rows(rows.into_iter().map(row_of_entry).collect::<Result<_, _>>()?))
        })
    }

    /// Follow how many rows `artist_id`'s page holds. The count runs as a
    /// separate query from the windows' rows.
    pub fn subscribe_composer_detail_count(
        &self,
        artist_id: &str,
    ) -> coven::LiveQuery<u64, bae_store::DbError> {
        let artist_id = artist_id.to_string();
        self.store
            .subscribe_row(ComposerPageLengthParams { artist: artist_id })
            .process(|row| list_count(row.entries))
    }

    /// Follow how many rows of `artist_id`'s page come before the row at a
    /// key; the caller changes the key on the same query.
    pub fn subscribe_composer_detail_position(
        &self,
        artist_id: &str,
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let artist_id = artist_id.to_string();
        self.store
            .subscribe_reconfigurable(initial, move |key| {
                composer_page_rows_before_read(&artist_id, key)
            })
            .process(|_, rows| {
                list_count(exactly_one::<ComposerPageRowsBeforeParams, _>(rows)?.entries)
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
coven::fragment!(
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
coven::read!(
    writer ComposerPageWindow(
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

// [`ComposerPageWindow`] from the page's top.
coven::read!(
    writer ComposerPageTop(
        artist: String,
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
    ORDER BY listing.section, listing.group_title_sort_key, listing.sort_group_album, listing.sort_group_order, listing.group_id, listing.sort_place, listing.sort_title, listing.sort_position, listing.item_id
    LIMIT :limit
"#
);

// The rows of the composer `artist`'s page at the places from `offset`, at
// most `limit`.
coven::read!(
    writer ComposerPageRows(artist: String, offset: i64, limit: i64),
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
coven::read!(
    writer ComposerPageLength(artist: String) { entries: i64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM composer_detail_row AS listing
    WHERE listing.artist_id = :artist
"#
);

// How many rows of the composer `artist`'s page come before a key of its
// order.
coven::read!(
    writer ComposerPageRowsBefore(
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
coven::read!(
    writer ComposerPageKeyAt(artist: String, index: i64) { order_key_0: SortValue, order_key_1: SortValue, order_key_2: SortValue, order_key_3: SortValue, order_key_4: SortValue, order_key_5: SortValue, order_key_6: SortValue, order_key_7: SortValue, order_key_8: SortValue },
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

// The composer `artist` as the composer list shows it, with the first work its
// page lists; no row for an artist the list doesn't show.
coven::read!(
    writer ComposerHead(artist: String) { default_work_id: Option<String> },
    r#"
    SELECT shown.*,
           (SELECT listing.item_id
            FROM composer_detail_row AS listing
            WHERE listing.artist_id = :artist AND listing.section = 0
            ORDER BY listing.section, listing.group_title_sort_key, listing.sort_group_album, listing.sort_group_order, listing.group_id, listing.sort_place, listing.sort_title, listing.sort_position, listing.item_id
            LIMIT 1) AS default_work_id
    FROM @crate::composer::ListedComposer AS shown
    WHERE shown.listed_id = :artist
"#
);

/// The nine values of the page's order `key` holds. Only the page's reads make
/// its keys.
fn page_key_values(key: &OrderKey) -> Result<[SortValue; 9], DbError> {
    <[SortValue; 9]>::try_from(key.values().to_vec())
        .map_err(|values| DbError::unreadable("a composer page key", values))
}

/// The read of the `window` of `artist_id`'s page, each row with its key: from
/// the page's first row for a window from its top, else from the window's key.
fn composer_page_window_read(
    artist_id: &str,
    window: &ListWindow,
) -> Result<coven::ReadAs<(ComposerPageEntry, OrderKey)>, DbError> {
    let artist = artist_id.to_string();
    let limit = bae_store::list_windows::sql_bound(window.limit);
    macro_rules! rows {
        ($statement:expr) => {
            coven::ReadAs::new($statement, |row| {
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
                (row.shown, key)
            })
        };
    }
    Ok(match &window.from {
        None => rows!(ComposerPageTopParams { artist, limit }),
        Some(from) => {
            let [order_key_0, order_key_1, order_key_2, order_key_3, order_key_4, order_key_5, order_key_6, order_key_7, order_key_8] =
                page_key_values(from)?;
            rows!(ComposerPageWindowParams {
                artist,
                order_key_0,
                order_key_1,
                order_key_2,
                order_key_3,
                order_key_4,
                order_key_5,
                order_key_6,
                order_key_7,
                order_key_8,
                limit,
            })
        }
    })
}

/// The `window` of a composer's page as its read returned it, each row with
/// its key.
fn composer_page_window_of(
    window: &ListWindow,
    rows: Vec<(ComposerPageEntry, OrderKey)>,
) -> Result<WindowRead<DbComposerDetailRow>, DbError> {
    let rows = rows
        .into_iter()
        .map(|(shown, key)| Ok((row_of_entry(shown)?, key)))
        .collect::<Result<_, DbError>>()?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// The read of the rows of `artist_id`'s page at `page`'s places.
fn composer_page_read(artist_id: &str, page: ListPage) -> coven::ReadAs<ComposerPageEntry> {
    let statement = ComposerPageRowsParams {
        artist: artist_id.to_string(),
        offset: bae_store::list_windows::sql_bound(page.offset),
        limit: bae_store::list_windows::sql_bound(page.limit),
    };
    coven::ReadAs::new(statement, |row| row.shown)
}

/// The read of how many rows of `artist_id`'s page come before the row at
/// `key`.
fn composer_page_rows_before_read(
    artist_id: &str,
    key: &OrderKey,
) -> Result<ComposerPageRowsBeforeParams, DbError> {
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
    Ok(statement)
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
#[cfg(test)]
fn find_composer_detail_on(
    sql: &SqlReadContext<'_>,
    artist_id: &str,
) -> Result<Option<DbComposerDetail>, DbError> {
    let statement = ComposerHeadParams { artist: artist_id.to_string() };
    composer_detail_rows_of(statement_optional(sql, &statement)?)
}

/// The composer as its head row holds it.
fn composer_detail_rows_of(
    row: Option<ComposerHeadRow>,
) -> Result<Option<DbComposerDetail>, DbError> {
    row.map(|row| {
        Ok(DbComposerDetail {
            composer: row.shown.into_summary()?,
            default_work_id: row.default_work_id,
        })
    })
    .transpose()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerDetailProjection {
    pub detail: Option<DbComposerDetail>,
}

#[cfg(test)]
#[path = "composer_detail_tests.rs"]
mod tests;
