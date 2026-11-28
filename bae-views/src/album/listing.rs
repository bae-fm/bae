//! The album grid's reads: its counts, its artists' sections and which chosen
//! albums it no longer lists; and, for its order under a sort, its window, its
//! page, the rows before a key, the key at a position, where an album's card
//! is, the albums between two cards, chosen albums in its order and every
//! album. Each is one compiled read.
//!
//! Its tables and the triggers keeping them are `listing.sql`.

use super::order::{AlbumLead, AlbumOrder, SortCode};
use super::*;
use crate::album::card::AlbumCardRow;
use bae_records::records::query::AlbumSummaryRow;
use bae_store::list_windows::sql_bound;
use bae_store::{statement_optional, statement_rows};

use super::key_reads::*;
use super::numbered_reads::*;
use super::page_reads::*;
use super::position_reads::*;
use super::top_reads::*;
use super::window_reads::*;

// The flat grid's counts: its albums, each of which is one row, and whether
// the library holds a track.
sqlive_compose::read!(
    writer FlatAlbumCounts { albums: u64, can_shuffle: bool },
    r#"
    SELECT (SELECT COUNT(*) FROM album_listing AS listing) AS albums,
           EXISTS (SELECT 1 FROM @bae_records::records::spread_fragments::TrackRow AS track)
               AS can_shuffle
"#
);

// The counts of the grid grouped by artist: its albums; its rows, one for
// each album under each of its artists, the sum of the artists' album counts;
// and whether the library holds a track.
sqlive_compose::read!(
    writer GroupedAlbumCounts { albums: u64, appearances: u64, can_shuffle: bool },
    r#"
    SELECT (SELECT COUNT(*) FROM album_listing AS listing) AS albums,
           (SELECT COALESCE(SUM(artist.album_count), 0) FROM artist_listing AS artist)
               AS appearances,
           EXISTS (SELECT 1 FROM @bae_records::records::spread_fragments::TrackRow AS track)
               AS can_shuffle
"#
);

/// How many rows the album grid has, flat or grouped by artist, and whether
/// the library has a track to shuffle.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn album_counts_on(
    sql: &SqlReadContext<'_>,
    group_by_artist: bool,
) -> Result<AlbumCounts, DbError> {
    Ok(match group_by_artist {
        true => grouped_counts_of(bae_store::statement_row(sql, &GroupedAlbumCountsParams {})?),
        false => flat_counts_of(bae_store::statement_row(sql, &FlatAlbumCountsParams {})?),
    })
}

/// [`album_counts_on`], live.
pub(crate) fn subscribe_album_counts(
    store: &bae_store::Store,
    group_by_artist: bool,
) -> coven::LiveQuery<AlbumCounts, DbError> {
    match group_by_artist {
        true => store
            .subscribe_row(GroupedAlbumCountsParams {})
            .process(|row| Ok(grouped_counts_of(row))),
        false => {
            store.subscribe_row(FlatAlbumCountsParams {}).process(|row| Ok(flat_counts_of(row)))
        }
    }
}

/// The counts of the grid grouped by artist, as their row holds them.
fn grouped_counts_of(row: GroupedAlbumCountsRow) -> AlbumCounts {
    AlbumCounts {
        albums: row.albums,
        rows: row.appearances,
        can_shuffle: row.can_shuffle,
        grouped: true,
    }
}

/// The counts of the flat grid, as their row holds them.
fn flat_counts_of(row: FlatAlbumCountsRow) -> AlbumCounts {
    AlbumCounts {
        albums: row.albums,
        rows: row.albums,
        can_shuffle: row.can_shuffle,
        grouped: false,
    }
}

// The sections of the grid grouped by artist with the artists ascending: each
// artist in the grouped order with its name, where its albums start in the
// grid's rows, how many there are and the version of its picture.
sqlive_compose::read!(
    writer AscendingArtistSections { first_position: u64, image_version: Option<String> },
    r#"
    SELECT artist.artist_id, artist.name,
           SUM(artist.album_count) OVER (
               ORDER BY artist.sort_name_sort_key, artist.artist_id ROWS UNBOUNDED PRECEDING)
               - artist.album_count AS first_position,
           artist.album_count,
           (SELECT image.version
            FROM @bae_records::records::files::ArtistImageVersion AS image
            WHERE image.id = artist.artist_id) AS image_version
    FROM artist_listing AS artist
    ORDER BY artist.sort_name_sort_key, artist.artist_id
"#
);

// [`AscendingArtistSections`] with the artists descending.
sqlive_compose::read!(
    writer DescendingArtistSections { first_position: u64, image_version: Option<String> },
    r#"
    SELECT artist.artist_id, artist.name,
           SUM(artist.album_count) OVER (
               ORDER BY artist.sort_name_sort_key DESC, artist.artist_id DESC
               ROWS UNBOUNDED PRECEDING)
               - artist.album_count AS first_position,
           artist.album_count,
           (SELECT image.version
            FROM @bae_records::records::files::ArtistImageVersion AS image
            WHERE image.id = artist.artist_id) AS image_version
    FROM artist_listing AS artist
    ORDER BY artist.sort_name_sort_key DESC, artist.artist_id DESC
"#
);

/// The sections of the grid grouped by artist, the artists in `direction`,
/// live.
pub(crate) fn subscribe_album_sections(
    store: &bae_store::Store,
    direction: SortDirection,
) -> coven::LiveQuery<Vec<crate::LibraryBrowseSection>, DbError> {
    macro_rules! sections {
        ($params:expr) => {
            store.subscribe($params).process(|rows| {
                rows.into_iter()
                    .map(|row| {
                        section(
                            row.artist_id,
                            row.name,
                            row.first_position,
                            row.album_count,
                            row.image_version,
                        )
                    })
                    .collect()
            })
        };
    }
    match direction {
        SortDirection::Ascending => sections!(AscendingArtistSectionsParams {}),
        SortDirection::Descending => sections!(DescendingArtistSectionsParams {}),
    }
}

/// One artist's section of the grid grouped by artist.
fn section(
    artist_id: String,
    name: String,
    first_position: u64,
    album_count: i64,
    image_version: Option<String>,
) -> Result<crate::LibraryBrowseSection, DbError> {
    Ok(crate::LibraryBrowseSection {
        image: bae_records::artist_image_ref(&artist_id, image_version),
        id: artist_id,
        title: name,
        first_position,
        album_count: u64::try_from(album_count)
            .map_err(|_error| DbError::unreadable("an artist's album count", album_count))?,
    })
}

// The ids of `requested`, a JSON array of album ids, that the grid does not
// show, in id order: the grid shows an album that has a listing row.
sqlive_compose::read!(
    writer UnshownAlbums(requested: String) { album_id: String },
    r#"
    SELECT requested.value AS album_id
    FROM json_each(:requested) AS requested
    WHERE requested.value IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM album_listing AS listing
                      WHERE listing.album_id = requested.value)
    ORDER BY requested.value
"#
);

/// The read of the albums of `requested` that the grid does not show, by id.
pub(crate) fn unshown_albums_read(
    requested: &std::collections::BTreeSet<String>,
) -> UnshownAlbumsParams {
    let requested = serde_json::to_string(requested).expect("album ids serialize as JSON");
    UnshownAlbumsParams { requested }
}

/// The key a row of a read with the columns `key_0` on says it is at.
macro_rules! key_of {
    ($row:ident) => {
        OrderKey::new(vec![
            $row.key_0, $row.key_1, $row.key_2, $row.key_3, $row.key_4, $row.key_5, $row.key_6,
            $row.key_7,
        ])
    };
}

/// A row of the grid's window as its read returns it: the row's key, its
/// album's card and, grouped by artist, the album's other credits.
pub(crate) type WindowRow =
    (OrderKey, AlbumCardRow, Option<bae_records::records::fragments::AlbumCreditsBeside>);

/// The read of the grid's rows in `window` under `order`, each with its key. A
/// window from the top reads from the order's first row, and one from a key
/// from the key's row.
pub(crate) fn album_window_read(
    order: &AlbumOrder,
    window: &ListWindow,
) -> Result<coven::ReadAs<WindowRow>, DbError> {
    let [first, second, third, fourth] = order.codes();
    let limit = sql_bound(window.limit);
    macro_rules! rows {
        ($statement:expr, |$row:ident| $beside:expr) => {
            coven::ReadAs::new($statement, |$row| (key_of!($row), $row.card, $beside))
        };
    }
    let Some(from) = &window.from else {
        macro_rules! top {
            ($params:ident, |$row:ident| $beside:expr) => {
                rows!($params { first, second, third, fourth, limit }, |$row| $beside)
            };
        }
        return Ok(match order.lead {
            AlbumLead::Flat(SortCode::Title) => top!(FlatTopTitleParams, |row| None),
            AlbumLead::Flat(SortCode::TitleBack) => top!(FlatTopTitleBackParams, |row| None),
            AlbumLead::Flat(SortCode::Artist) => top!(FlatTopArtistParams, |row| None),
            AlbumLead::Flat(SortCode::ArtistBack) => top!(FlatTopArtistBackParams, |row| None),
            AlbumLead::Flat(SortCode::Year) => top!(FlatTopYearParams, |row| None),
            AlbumLead::Flat(SortCode::YearBack) => top!(FlatTopYearBackParams, |row| None),
            AlbumLead::Flat(SortCode::Added) => top!(FlatTopAddedParams, |row| None),
            AlbumLead::Flat(SortCode::AddedBack) => top!(FlatTopAddedBackParams, |row| None),
            AlbumLead::Grouped(SortDirection::Ascending) => {
                top!(GroupedTopArtistParams, |row| Some(row.beside))
            }
            AlbumLead::Grouped(SortDirection::Descending) => {
                top!(GroupedTopArtistBackParams, |row| Some(row.beside))
            }
        });
    };
    let [key_0, key_1, key_2, key_3, key_4, key_5, key_6, key_7] = AlbumOrder::values_of(from)?;
    macro_rules! from_key {
        ($params:ident, |$row:ident| $beside:expr) => {
            rows!(
                $params {
                    first,
                    second,
                    third,
                    fourth,
                    limit,
                    key_0,
                    key_1,
                    key_2,
                    key_3,
                    key_4,
                    key_5,
                    key_6,
                    key_7,
                },
                |$row| $beside
            )
        };
    }
    Ok(match order.lead {
        AlbumLead::Flat(SortCode::Title) => from_key!(FlatWindowTitleParams, |row| None),
        AlbumLead::Flat(SortCode::TitleBack) => from_key!(FlatWindowTitleBackParams, |row| None),
        AlbumLead::Flat(SortCode::Artist) => from_key!(FlatWindowArtistParams, |row| None),
        AlbumLead::Flat(SortCode::ArtistBack) => {
            from_key!(FlatWindowArtistBackParams, |row| None)
        }
        AlbumLead::Flat(SortCode::Year) => from_key!(FlatWindowYearParams, |row| None),
        AlbumLead::Flat(SortCode::YearBack) => from_key!(FlatWindowYearBackParams, |row| None),
        AlbumLead::Flat(SortCode::Added) => from_key!(FlatWindowAddedParams, |row| None),
        AlbumLead::Flat(SortCode::AddedBack) => from_key!(FlatWindowAddedBackParams, |row| None),
        AlbumLead::Grouped(SortDirection::Ascending) => {
            from_key!(GroupedWindowArtistParams, |row| Some(row.beside))
        }
        AlbumLead::Grouped(SortDirection::Descending) => {
            from_key!(GroupedWindowArtistBackParams, |row| Some(row.beside))
        }
    })
}

/// The rows of `window` as the grid's read returned them, as the grid shows
/// them.
pub(crate) fn album_window_of(
    window: &ListWindow,
    rows: Vec<WindowRow>,
    group_by_artist: bool,
) -> Result<WindowRead<DbAlbumAppearance>, DbError> {
    let rows = rows
        .into_iter()
        .map(|(key, card, beside)| {
            let credited_with = match beside {
                Some(beside) => beside.into_line()?,
                None => None,
            };
            Ok((AlbumAppearanceRow::of(card, credited_with)?.process(group_by_artist)?, key))
        })
        .collect::<Result<_, DbError>>()?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// The read of the cards of the flat grid sorted first by `first` under
/// `order`, at the places `page` names.
pub(crate) fn album_page_read(
    first_sort: SortCode,
    order: &AlbumOrder,
    page: ListPage,
) -> coven::ReadAs<AlbumCardRow> {
    macro_rules! page {
        ($params:ident) => {{
            let [first, second, third, fourth] = order.codes();
            let statement = $params {
                first,
                second,
                third,
                fourth,
                offset: sql_bound(page.offset),
                limit: sql_bound(page.limit),
            };
            coven::ReadAs::new(statement, |row| row.card)
        }};
    }
    match first_sort {
        SortCode::Title => page!(FlatPageTitleParams),
        SortCode::TitleBack => page!(FlatPageTitleBackParams),
        SortCode::Artist => page!(FlatPageArtistParams),
        SortCode::ArtistBack => page!(FlatPageArtistBackParams),
        SortCode::Year => page!(FlatPageYearParams),
        SortCode::YearBack => page!(FlatPageYearBackParams),
        SortCode::Added => page!(FlatPageAddedParams),
        SortCode::AddedBack => page!(FlatPageAddedBackParams),
    }
}

/// How many rows of the grid under `order` come before the row at `key`.
#[cfg(test)]
pub(crate) fn albums_before_on(
    sql: &SqlReadContext<'_>,
    order: &AlbumOrder,
    key: &OrderKey,
) -> Result<u64, DbError> {
    exactly_one::<coven::ReadAs<u64>, _>(sql.reads(&albums_before_read(order, key)?)?)
}

/// The read of how many rows of the grid under `order` come before the row
/// at `key`.
pub(crate) fn albums_before_read(
    order: &AlbumOrder,
    key: &OrderKey,
) -> Result<coven::ReadAs<u64>, DbError> {
    let values = AlbumOrder::values_of(key)?;
    macro_rules! before {
        ($params:ident) => {{
            let [first, second, third, fourth] = order.codes();
            let [key_0, key_1, key_2, key_3, key_4, key_5, key_6, key_7] = values;
            let statement = $params {
                first,
                second,
                third,
                fourth,
                key_0,
                key_1,
                key_2,
                key_3,
                key_4,
                key_5,
                key_6,
                key_7,
            };
            coven::ReadAs::new(statement, |row| row.rows_before)
        }};
    }
    Ok(match order.lead {
        AlbumLead::Flat(SortCode::Title) => before!(FlatBeforeTitleParams),
        AlbumLead::Flat(SortCode::TitleBack) => before!(FlatBeforeTitleBackParams),
        AlbumLead::Flat(SortCode::Artist) => before!(FlatBeforeArtistParams),
        AlbumLead::Flat(SortCode::ArtistBack) => before!(FlatBeforeArtistBackParams),
        AlbumLead::Flat(SortCode::Year) => before!(FlatBeforeYearParams),
        AlbumLead::Flat(SortCode::YearBack) => before!(FlatBeforeYearBackParams),
        AlbumLead::Flat(SortCode::Added) => before!(FlatBeforeAddedParams),
        AlbumLead::Flat(SortCode::AddedBack) => before!(FlatBeforeAddedBackParams),
        AlbumLead::Grouped(SortDirection::Ascending) => before!(GroupedBeforeArtistParams),
        AlbumLead::Grouped(SortDirection::Descending) => before!(GroupedBeforeArtistBackParams),
    })
}

/// The key of the row at `position` of the grid under `order`, or of its last
/// row when it ends before it; `None` for an empty grid.
pub(crate) fn album_key_at_on(
    sql: &SqlReadContext<'_>,
    order: &AlbumOrder,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    macro_rules! key_at {
        ($params:ident) => {{
            let [first, second, third, fourth] = order.codes();
            statement_optional(
                sql,
                &$params { first, second, third, fourth, position: sql_bound(position) },
            )?
            .map(|row| key_of!(row))
        }};
    }
    Ok(match order.lead {
        AlbumLead::Flat(SortCode::Title) => key_at!(FlatKeyAtTitleParams),
        AlbumLead::Flat(SortCode::TitleBack) => key_at!(FlatKeyAtTitleBackParams),
        AlbumLead::Flat(SortCode::Artist) => key_at!(FlatKeyAtArtistParams),
        AlbumLead::Flat(SortCode::ArtistBack) => key_at!(FlatKeyAtArtistBackParams),
        AlbumLead::Flat(SortCode::Year) => key_at!(FlatKeyAtYearParams),
        AlbumLead::Flat(SortCode::YearBack) => key_at!(FlatKeyAtYearBackParams),
        AlbumLead::Flat(SortCode::Added) => key_at!(FlatKeyAtAddedParams),
        AlbumLead::Flat(SortCode::AddedBack) => key_at!(FlatKeyAtAddedBackParams),
        AlbumLead::Grouped(SortDirection::Ascending) => key_at!(GroupedKeyAtArtistParams),
        AlbumLead::Grouped(SortDirection::Descending) => key_at!(GroupedKeyAtArtistBackParams),
    })
}

/// Runs the flat or the grouped one of two reads over the numbered cards, as
/// `order` is, binding its sorts and the fields given.
macro_rules! numbered {
    ($sql:expr, $order:expr, $flat:ident, $grouped:ident
     { $($field:ident: $value:expr),* }, $map:expr) => {{
        let [first, second, third, fourth] = $order.codes();
        match $order.lead {
            AlbumLead::Flat(_) => statement_rows(
                $sql,
                &$flat { first, second, third, fourth, $($field: $value),* },
            )?
            .into_iter()
            .map($map)
            .collect::<Vec<_>>(),
            AlbumLead::Grouped(_) => statement_rows(
                $sql,
                &$grouped { first, second, third, fourth, $($field: $value),* },
            )?
            .into_iter()
            .map($map)
            .collect::<Vec<_>>(),
        }
    }};
}

/// The read of where the album `album_id`'s card is in the grid under `order`: its card in
/// the group of the artist `section` when that group holds one, else its first
/// card. `None` when the grid shows no card of the album.
pub(crate) fn album_place_read(
    order: &AlbumOrder,
    album_id: &str,
    section: Option<&str>,
) -> coven::ReadAs<AlbumPlace> {
    let [first, second, third, fourth] = order.codes();
    let (album, section) = (album_id.to_string(), section.map(str::to_string));
    match order.lead {
        AlbumLead::Flat(_) => coven::ReadAs::new(
            FlatAlbumPlaceReadParams { first, second, third, fourth, album, section },
            |row| AlbumPlace { position: row.position, section_id: row.section_id },
        ),
        AlbumLead::Grouped(_) => coven::ReadAs::new(
            GroupedAlbumPlaceReadParams { first, second, third, fourth, album, section },
            |row| AlbumPlace { position: row.position, section_id: Some(row.section_id) },
        ),
    }
}

/// The albums of the grid's cards under `order` from the card `from` to the
/// card `to`, both included, each album once.
pub(crate) fn album_ids_between_on(
    sql: &SqlReadContext<'_>,
    order: &AlbumOrder,
    from: AlbumCard<'_>,
    to: AlbumCard<'_>,
) -> Result<Vec<String>, DbError> {
    let owned =
        |card: AlbumCard<'_>| (card.album_id.to_string(), card.section_id.map(str::to_string));
    let ((from_album, from_section), (to_album, to_section)) = (owned(from), owned(to));
    Ok(numbered!(
        sql,
        order,
        FlatAlbumsBetweenParams,
        GroupedAlbumsBetweenParams {
            from_album: from_album,
            from_section: from_section,
            to_album: to_album,
            to_section: to_section
        },
        |row| row.album_id
    ))
}

/// The albums of `album_ids` the grid under `order` lists, each once at its
/// first card, in the grid's order.
pub(crate) fn albums_in_order_on(
    sql: &SqlReadContext<'_>,
    order: &AlbumOrder,
    album_ids: &[String],
) -> Result<Vec<AlbumSummaryRow>, DbError> {
    let albums = serde_json::to_string(album_ids).expect("album ids serialize as JSON");
    numbered!(
        sql,
        order,
        FlatAlbumsInOrderParams,
        GroupedAlbumsInOrderParams { albums: albums },
        |row| row.summary.into_row()
    )
    .into_iter()
    .collect()
}

/// Every album, in the order of the flat grid under `order`.
#[cfg(any(test, feature = "test-utils"))]
pub(crate) fn every_album_on(
    sql: &SqlReadContext<'_>,
    order: &AlbumOrder,
) -> Result<Vec<DbAlbum>, DbError> {
    let [first, second, third, fourth] = order.codes();
    statement_rows(sql, &EveryAlbumParams { first, second, third, fourth })?
        .into_iter()
        .map(|row| row.album.into_album())
        .collect()
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
