//! The album grid's rows as the list reads them: their order, flat or
//! grouped by artist, the artists' sections, and the rows of one window.
//!
//! Its tables and the triggers keeping them are `listing.sql`.

use super::*;

/// The SQL creating this part's tables and triggers.
pub(crate) const SQL: &str = include_str!("listing.sql");

pub(super) fn album_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COUNT(*) FROM album_listing", [], |row| row.get::<_, i64>(0))
        .map(|count| count as u64)
        .map_err(DbError::from)
}

/// How many rows of the album grid come before the row at `key`.
pub(crate) fn album_rows_before_on(
    sql: &SqlReadContext<'_>,
    group_by_artist: bool,
    order: &ListOrder,
    key: &OrderKey,
) -> Result<u64, DbError> {
    bae_store::list_windows::rows_before_on(
        sql,
        album_order_tables(group_by_artist),
        None,
        &[],
        order,
        key,
    )
}

/// The grid grouped by artist's sections: each artist in the grouped order
/// with its name, its picture and where its albums start in the grid's rows.
pub(super) fn album_sections_on(
    sql: &SqlReadContext<'_>,
    direction: SortDirection,
) -> Result<Vec<crate::LibraryBrowseSection>, DbError> {
    Ok(sql.query(&album_browse_sections_query(direction), [], |row| {
        Ok(crate::LibraryBrowseSection {
            id: row.get(0)?,
            title: row.get(1)?,
            image: bae_records::artist_image_ref(&row.get::<_, String>(0)?, row.get(4)?),
            first_position: row.get::<_, i64>(2)? as u64,
            album_count: row.get::<_, i64>(3)? as u64,
        })
    })?)
}

/// How many places the grid grouped by artist has: one for each album under
/// each of its artists, the sum of the artists' album counts.
pub(super) fn album_appearance_count_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    sql.query_row("SELECT COALESCE(SUM(album_count), 0) FROM artist_listing", [], |row| {
        row.get::<_, i64>(0)
    })
    .map(|count| count as u64)
    .map_err(DbError::from)
}

/// The table holding every term of the album list's order, flat or grouped
/// by artist: the albums' listing rows, or their links to each artist their
/// credits show.
fn album_order_tables(group_by_artist: bool) -> &'static str {
    if group_by_artist {
        "FROM album_artist_link link"
    } else {
        "FROM album_listing listing"
    }
}

/// The statement reading the album list's rows `seek` reads (grouped by
/// artist or not) in `order`, at most `:limit`, each with the artist whose
/// group lists it and its key.
///
/// A window's statement is one SELECT over the table holding the order's
/// terms, joined to each row's album, returning the order's terms as its key
/// columns, so coven reruns it for a write at or before its last row alone.
/// A read by offset picks its rows in an inner statement that reads only the
/// ids of its rows from the table holding the order's terms, so a row
/// `:offset` skips costs its place in the order's indexes and nothing else:
/// every link has its listing row and every listing row its album, as their
/// foreign keys hold. Its outer statement reads the columns of just the
/// picked rows, in the same order.
pub(crate) fn album_window_statement(
    group_by_artist: bool,
    order: &ListOrder,
    seek: &Seek,
) -> String {
    let primary = album_primary_release_sql();
    let album = album_summary_columns_for_primary("primary_storage.release_id");
    let availability = "primary_storage.audio_elsewhere_device IS NOT NULL";
    if !seek.by_offset() {
        let listing = if group_by_artist {
            "CROSS JOIN album_listing listing ON listing.album_id = link.album_id"
        } else {
            ""
        };
        return format!(
            "SELECT {album}, {beside} AS credited_with, {availability} AS primary_release_unplayable, {keys} \
             {tables} {listing} \
             JOIN albums a ON a.id = listing.album_id \
             LEFT JOIN release_storage primary_storage ON primary_storage.release_id = {primary} \
             WHERE {condition} \
             ORDER BY {ordering} {limit}",
            beside = album_credits_beside_sql(group_by_artist),
            keys = order.key_columns(),
            tables = album_order_tables(group_by_artist),
            condition = order.condition(seek),
            ordering = order.ordering(seek),
            limit = seek.limit_clause(),
        );
    }
    let (picked, picked_ids) = if group_by_artist {
        (
            "link.artist_id AS picked_artist_id, link.album_id AS picked_album_id",
            "CROSS JOIN album_artist_link link ON link.artist_id = picked.picked_artist_id \
                 AND link.album_id = picked.picked_album_id \
             CROSS JOIN album_listing listing ON listing.album_id = picked.picked_album_id",
        )
    } else {
        (
            "listing.album_id AS picked_album_id",
            "CROSS JOIN album_listing listing ON listing.album_id = picked.picked_album_id",
        )
    };
    format!(
        "SELECT {album}, {beside} AS credited_with, {availability} AS primary_release_unplayable, {keys} \
         FROM (SELECT {picked} {tables} WHERE {condition} \
               ORDER BY {ordering} {limit}) picked \
         {picked_ids} \
         JOIN albums a ON a.id = picked.picked_album_id \
         LEFT JOIN release_storage primary_storage ON primary_storage.release_id = {primary} \
         ORDER BY {ordering}",
        beside = album_credits_beside_sql(group_by_artist),
        tables = album_order_tables(group_by_artist),
        keys = order.key_columns(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
        limit = seek.limit_clause(),
    )
}

/// The album list's rows `seek` reads (grouped by artist or not) in `order`,
/// at most `limit`, each with its key.
pub(crate) fn album_rows_on(
    sql: &SqlReadContext<'_>,
    group_by_artist: bool,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
) -> Result<Vec<Keyed<AlbumAppearanceRow>>, DbError> {
    let statement = album_window_statement(group_by_artist, order, seek);
    Ok(seek.bound(limit, |params| {
        sql.query(&statement, params, |row| {
            Ok((
                AlbumAppearanceRow {
                    row: AlbumSummaryRow::read(row)?,
                    credited_with: optional_credit_line_column(row, "credited_with")?,
                    primary_release_unplayable: row.get("primary_release_unplayable")?,
                },
                order.key_of(row)?,
            ))
        })
    })?)
}

/// One row of the album list as read: the album, and when the list is
/// grouped by artist, the line of the album's credits that show another
/// artist than the one whose group lists it (none when it has no others).
#[derive(Clone, PartialEq)]
pub(crate) struct AlbumAppearanceRow {
    pub(crate) row: AlbumSummaryRow,
    credited_with: Option<bae_types::credits::CreditLine>,
    primary_release_unplayable: bool,
}

impl AlbumAppearanceRow {
    pub(super) fn process(self, group_by_artist: bool) -> Result<DbAlbumAppearance, DbError> {
        let album = self.row.process()?;
        let display_artist_line =
            if group_by_artist { self.credited_with } else { Some(album.artist_line.clone()) };
        Ok(DbAlbumAppearance {
            album,
            display_artist_line,
            primary_release_unplayable: self.primary_release_unplayable,
        })
    }
}

/// Which rows an album listing's order reads.
#[derive(Clone, Copy)]
enum AlbumRows {
    /// One row per album, `album_listing listing`: its Artist sort compares
    /// the album's whole credit list (the listing's credit keys).
    Flat,
    /// One row per album under each artist its credits show (the browse
    /// grouped by artist), `album_artist_link link`: its Artist sort compares
    /// the link's copy of the sort key of the artist the row is listed under,
    /// and the album's other sorts compare the link's copies of the album's
    /// values, which its indexes hold.
    ByArtist,
}

impl AlbumRows {
    /// The alias of the table whose title, year, date added and album id the
    /// order compares.
    fn album_values(self) -> &'static str {
        match self {
            AlbumRows::Flat => "listing",
            AlbumRows::ByArtist => "link",
        }
    }
}

/// The album list's relation and order, flat or grouped by artist: grouped
/// rows are album appearances, flat rows distinct albums. The relation adds
/// each row's `album_listing listing` and album `a` to the order's tables,
/// for reveal and the selection's order; a window picks its rows from the
/// order's tables alone (`album_window_statement`).
pub(crate) fn album_browse_order(
    sort: &[AlbumSortCriterion],
    group_by_artist: bool,
) -> (String, ListOrder) {
    if !group_by_artist {
        return (
            format!("{} JOIN albums a ON a.id = listing.album_id", album_order_tables(false)),
            album_order(sort, AlbumRows::Flat),
        );
    }
    let direction = grouped_artist_direction(sort);
    let mut grouped_sort = vec![AlbumSortCriterion { field: AlbumSortField::Artist, direction }];
    grouped_sort.extend(sort.iter().filter(|c| c.field != AlbumSortField::Artist).cloned());
    if sort.is_empty() {
        grouped_sort.push(AlbumSortCriterion {
            field: AlbumSortField::DateAdded,
            direction: SortDirection::Descending,
        });
    }
    (
        format!(
            "{} CROSS JOIN album_listing listing ON listing.album_id = link.album_id \
             JOIN albums a ON a.id = listing.album_id",
            album_order_tables(true)
        ),
        album_order(&grouped_sort, AlbumRows::ByArtist),
    )
}

/// The direction the grid grouped by artist orders its artists in: its
/// Artist criterion's, else ascending.
pub(super) fn grouped_artist_direction(sort: &[AlbumSortCriterion]) -> SortDirection {
    sort.iter()
        .find(|c| c.field == AlbumSortField::Artist)
        .map_or(SortDirection::Ascending, |c| c.direction)
}

/// The grid's sections when grouped by artist: each artist in the grouped
/// order with its name, where its albums start in the rows, how many there
/// are, and the version of its picture. `direction` is the grouped order's
/// Artist direction.
pub(super) fn album_browse_sections_query(direction: SortDirection) -> String {
    let dir = sort_direction_sql(direction);
    let order = format!("artist.sort_name_sort_key {dir}, artist.artist_id {dir}");
    format!(
        "SELECT artist.artist_id, artist.name, \
                SUM(artist.album_count) OVER (ORDER BY {order} ROWS UNBOUNDED PRECEDING) \
                    - artist.album_count, \
                artist.album_count, \
                {image} AS image_version \
         FROM artist_listing artist \
         ORDER BY {order}",
        image = bae_records::records::files::artist_image_version_sql("artist.artist_id"),
    )
}

/// The credits of each row of the album grid that show another artist than
/// the one whose group lists it, as a JSON array (a SQL expression over
/// `album_browse_order`'s relation): NULL when the album has none, and for
/// every row of the flat grid.
fn album_credits_beside_sql(group_by_artist: bool) -> String {
    if group_by_artist {
        bae_records::records::query::album_credits_beside_json_sql("link.artist_id")
    } else {
        "NULL".to_string()
    }
}

fn sort_direction_sql(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    }
}

/// The order of an album listing over `rows`. With no criteria the newest
/// albums come first. An index of `album_listing` serves each one-field sort
/// of the flat grid, and an index of `album_artist_link` each way of the
/// artists with each way of the field after Artist grouped by artist.
fn album_order(sort: &[AlbumSortCriterion], rows: AlbumRows) -> ListOrder {
    let values = rows.album_values();
    // Every order ends with the album id, a total order over albums. Without
    // it rows sharing a sort value order arbitrarily — and a window read by
    // its offset, one read from a row beside it and an album's place (a
    // ROW_NUMBER window) are different query shapes, so SQLite could
    // tie-break them differently and a reveal would scroll to the wrong
    // album.
    if sort.is_empty() {
        return ListOrder::new(vec![
            SortTerm::descending(format!("{values}.created_at")),
            SortTerm::ascending(format!("{values}.album_id")),
        ]);
    }
    let mut terms = sort
        .iter()
        .flat_map(|c| {
            let dir = c.direction;
            match c.field {
                AlbumSortField::Title => {
                    vec![SortTerm::running(format!("{values}.title_sort_key"), dir)]
                }
                AlbumSortField::Artist => match rows {
                    AlbumRows::Flat => vec![
                        SortTerm::running("listing.credit_sort_key", dir).nullable(),
                        SortTerm::running("listing.credit_artist_ids", dir).nullable(),
                    ],
                    AlbumRows::ByArtist => vec![
                        SortTerm::running("link.artist_sort_key", dir),
                        SortTerm::running("link.artist_id", dir),
                    ],
                },
                // An unknown year sorts as above every year: last ascending,
                // first descending.
                AlbumSortField::Year => vec![
                    SortTerm::running(format!("{values}.year_missing"), dir),
                    SortTerm::running(format!("{values}.year"), dir).nullable(),
                ],
                AlbumSortField::DateAdded => {
                    vec![SortTerm::running(format!("{values}.created_at"), dir)]
                }
            }
        })
        .collect::<Vec<_>>();
    terms.push(SortTerm::ascending(format!("{values}.album_id")));
    ListOrder::new(terms)
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
