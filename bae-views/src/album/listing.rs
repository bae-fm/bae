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

/// How many rows of the album grid come before the rows `seek` reads: what a
/// count over one of the seeks before a key adds up to.
pub(crate) fn album_seek_count_on(
    sql: &SqlReadContext<'_>,
    group_by_artist: bool,
    order: &ListOrder,
    seek: &Seek,
) -> Result<u64, DbError> {
    let statement = format!(
        "SELECT COUNT(*) {tables} WHERE {condition}",
        tables = album_order_tables(group_by_artist),
        condition = order.condition(seek),
    );
    Ok(sql
        .query_row(&statement, seek.params().as_slice(), |row| row.get::<_, i64>(0))
        .map(|count| count as u64)?)
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

/// The tables holding every term of the album list's order, flat or grouped
/// by artist: the albums' listing rows, or the artists' with each one's links
/// after it. CROSS JOIN keeps the artists in their index's order and each
/// artist's albums after it in the link's index for the order's album sort,
/// so nothing is sorted.
fn album_order_tables(group_by_artist: bool) -> &'static str {
    if group_by_artist {
        "FROM artist_listing artist \
         CROSS JOIN album_artist_link link ON link.artist_id = artist.artist_id"
    } else {
        "FROM album_listing listing"
    }
}

/// The statement reading the album list's rows `seek` reads (grouped by
/// artist or not) in `order`, at most `:limit` from `:offset`, each with the
/// artist whose group lists it and its key.
///
/// The window is picked by an inner statement that reads only the ids of its
/// rows from the tables holding the order's terms, so a row `:offset` skips
/// costs its place in the order's indexes and nothing else: every link has
/// its listing row and every listing row its album, as their foreign keys
/// hold. The outer statement reads the columns of just the picked rows, in
/// the same order.
pub(crate) fn album_window_statement(
    group_by_artist: bool,
    order: &ListOrder,
    seek: &Seek,
) -> String {
    let (picked, picked_ids) = if group_by_artist {
        (
            "artist.artist_id AS picked_artist_id, link.album_id AS picked_album_id",
            "CROSS JOIN artist_listing artist ON artist.artist_id = picked.picked_artist_id \
             CROSS JOIN album_artist_link link ON link.artist_id = picked.picked_artist_id \
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
        "{select}, {beside} AS credited_with, {keys} \
         FROM (SELECT {picked} {tables} WHERE {condition} \
               ORDER BY {ordering} LIMIT :limit OFFSET :offset) picked \
         {picked_ids} \
         JOIN albums a ON a.id = picked.picked_album_id \
         ORDER BY {ordering}",
        select = album_summary_select(),
        beside = album_credits_beside_sql(group_by_artist),
        tables = album_order_tables(group_by_artist),
        keys = order.key_columns(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
    )
}

/// The album list's rows `seek` reads (grouped by artist or not) in `order`,
/// at most `limit` from `offset`, each with its key.
pub(crate) fn album_rows_on(
    sql: &SqlReadContext<'_>,
    group_by_artist: bool,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<AlbumAppearanceRow>>, DbError> {
    let statement = album_window_statement(group_by_artist, order, seek);
    Ok(seek.bound(limit, offset, |params| {
        sql.query(&statement, params, |row| {
            Ok((
                AlbumAppearanceRow {
                    row: AlbumSummaryRow::read(row)?,
                    credited_with: optional_credit_line_column(row, "credited_with")?,
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
}

impl AlbumAppearanceRow {
    pub(super) fn process(self) -> Result<DbAlbumAppearance, DbError> {
        Ok(DbAlbumAppearance { album: self.row.process()?, credited_with: self.credited_with })
    }
}

/// Which rows an album listing's order reads.
#[derive(Clone, Copy)]
enum AlbumRows {
    /// One row per album, `album_listing listing`: its Artist sort compares
    /// the album's whole credit list (the listing's credit keys).
    Flat,
    /// One row per album under each artist its credits show (the browse
    /// grouped by artist), `album_artist_link link` under `artist_listing
    /// artist`: its Artist sort compares the artist the row is listed under,
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
        bae_records::records::query::album_credits_beside_json_sql("artist.artist_id")
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
/// of the flat grid; grouped by artist, `artist_listing`'s index orders the
/// artists and an index of `album_artist_link` each artist's albums in the
/// field after Artist.
fn album_order(sort: &[AlbumSortCriterion], rows: AlbumRows) -> ListOrder {
    let values = rows.album_values();
    // Every order ends with the album id, a total order over albums. Without
    // it rows sharing a sort value order arbitrarily — and a window read by
    // its offset, one read from a row beside it and `get_album_index` (a
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
                        SortTerm::running("artist.sort_name_sort_key", dir),
                        SortTerm::running("artist.artist_id", dir),
                    ],
                },
                // An unknown year sorts as above every year: last ascending,
                // first descending.
                AlbumSortField::Year => vec![
                    SortTerm::running(format!("{values}.year IS NULL"), dir),
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
