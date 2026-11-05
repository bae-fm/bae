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

/// The statement reading the album list's rows `seek` reads over
/// `relation` (grouped by artist or not) in `order`, at most `:limit` from
/// `:offset`, each with the artist whose group lists it and its key.
pub(crate) fn album_window_statement(
    relation: &str,
    group_by_artist: bool,
    order: &ListOrder,
    seek: &Seek,
) -> String {
    format!(
        "{select}, {group} AS group_artist_id, {keys} {relation} WHERE {condition} \
         ORDER BY {ordering} LIMIT :limit OFFSET :offset",
        select = album_summary_select(),
        group = album_group_artist_sql(group_by_artist),
        keys = order.key_columns(),
        condition = order.condition(seek),
        ordering = order.ordering(seek),
    )
}

/// The album list's rows `seek` reads over `relation` (grouped by artist or
/// not) in `order`, at most `limit` from `offset`, each with its key.
pub(crate) fn album_rows_on(
    sql: &SqlReadContext<'_>,
    relation: &str,
    group_by_artist: bool,
    order: &ListOrder,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<AlbumAppearanceRow>>, DbError> {
    let statement = album_window_statement(relation, group_by_artist, order, seek);
    Ok(seek.bound(limit, offset, |params| {
        sql.query(&statement, params, |row| {
            Ok((
                AlbumAppearanceRow {
                    row: AlbumSummaryRow::read(row)?,
                    group_artist_id: row.get("group_artist_id")?,
                },
                order.key_of(row)?,
            ))
        })
    })?)
}

/// One row of the album list as read: the album, and the artist whose group
/// lists it when the list is grouped by artist.
#[derive(Clone, PartialEq)]
pub(crate) struct AlbumAppearanceRow {
    pub(crate) row: AlbumSummaryRow,
    group_artist_id: Option<String>,
}

impl AlbumAppearanceRow {
    pub(super) fn process(self) -> Result<DbAlbumAppearance, DbError> {
        let credits = self.row.credits()?;
        let credited_with = self.group_artist_id.and_then(|group| {
            let others = credits
                .iter()
                .filter(|credit| credit.artist.id != group)
                .cloned()
                .collect::<Vec<_>>();
            (!others.is_empty()).then(|| bae_records::credits::library_line(&others))
        });
        Ok(DbAlbumAppearance { album: self.row.process_with(&credits)?, credited_with })
    }
}

/// What an album listing's Artist sort compares.
#[derive(Clone, Copy)]
enum ArtistOrder {
    /// The album's whole credit list (`album_listing`'s credit keys).
    CreditList,
    /// The one artist the row is listed under, `artist` (the browse grouped
    /// by artist lists an album under each artist it credits).
    GroupArtist,
}

/// The same relation and order serve paging, section ranges, and reveal.
/// Grouped rows are album appearances; flat rows are distinct albums. Both
/// read `album_listing listing` and the album as `a`.
pub(crate) fn album_browse_order(
    sort: &[AlbumSortCriterion],
    group_by_artist: bool,
) -> (String, ListOrder) {
    if !group_by_artist {
        return (
            "FROM album_listing listing JOIN albums a ON a.id = listing.album_id".to_string(),
            album_order(sort, ArtistOrder::CreditList),
        );
    }
    let direction = sort
        .iter()
        .find(|c| c.field == AlbumSortField::Artist)
        .map_or(SortDirection::Ascending, |c| c.direction);
    let mut grouped_sort = vec![AlbumSortCriterion { field: AlbumSortField::Artist, direction }];
    grouped_sort.extend(sort.iter().filter(|c| c.field != AlbumSortField::Artist).cloned());
    if sort.is_empty() {
        grouped_sort.push(AlbumSortCriterion {
            field: AlbumSortField::DateAdded,
            direction: SortDirection::Descending,
        });
    }
    let order = album_order(&grouped_sort, ArtistOrder::GroupArtist);
    // CROSS JOIN keeps this join order: the artists in their index's order,
    // each artist's albums after it, so a window reads about its own rows.
    (
        "FROM artist_listing artist \
         CROSS JOIN album_artist_link link ON link.artist_id = artist.artist_id \
         CROSS JOIN album_listing listing ON listing.album_id = link.album_id \
         JOIN albums a ON a.id = listing.album_id"
            .to_string(),
        order,
    )
}

/// The tables whose rows the album list's order reads, flat or grouped by
/// artist, as `library_list_versions` names them.
pub(super) fn album_browse_sources(group_by_artist: bool) -> &'static [&'static str] {
    if group_by_artist {
        &["album_listing", "album_artist_link", "artist_listing"]
    } else {
        &["album_listing"]
    }
}

/// The grid's sections when grouped by artist: each artist in the grouped
/// order with its name, where its albums start in the rows, how many there
/// are, and the version of its picture. `direction` is the grouped order's
/// Artist direction.
pub(super) fn album_browse_sections_query(direction: SortDirection) -> String {
    let dir = sort_direction_sql(direction);
    let order = format!("artist.sort_key COLLATE NOCASE {dir}, artist.artist_id {dir}");
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

/// The artist whose group lists each row of the album grid (a SQL
/// expression over `album_browse_order`'s relation): the group's artist when
/// grouped by artist, else NULL.
fn album_group_artist_sql(group_by_artist: bool) -> &'static str {
    if group_by_artist {
        "artist.artist_id"
    } else {
        "NULL"
    }
}

fn sort_direction_sql(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    }
}

/// The order of an album listing over `album_listing listing`; `artist`
/// says what its Artist sort compares. With no criteria the newest albums
/// come first. An index of `album_listing` serves each one-field sort.
fn album_order(sort: &[AlbumSortCriterion], artist: ArtistOrder) -> ListOrder {
    // Every order ends with the album id, a total order over albums. Without
    // it rows sharing a sort value order arbitrarily — and a window read by
    // its offset, one read from a row beside it and `get_album_index` (a
    // ROW_NUMBER window) are different query shapes, so SQLite could
    // tie-break them differently and a reveal would scroll to the wrong
    // album.
    if sort.is_empty() {
        return ListOrder::new(vec![
            SortTerm::descending("listing.created_at"),
            SortTerm::ascending("listing.album_id"),
        ]);
    }
    let mut terms = sort
        .iter()
        .flat_map(|c| {
            let dir = c.direction;
            match c.field {
                AlbumSortField::Title => {
                    vec![SortTerm::running("listing.title COLLATE NOCASE", dir)]
                }
                AlbumSortField::Artist => match artist {
                    ArtistOrder::CreditList => vec![
                        SortTerm::running("listing.credit_sort_names COLLATE NOCASE", dir)
                            .nullable(),
                        SortTerm::running("listing.credit_artist_ids", dir).nullable(),
                    ],
                    ArtistOrder::GroupArtist => vec![
                        SortTerm::running("artist.sort_key COLLATE NOCASE", dir),
                        SortTerm::running("artist.artist_id", dir),
                    ],
                },
                // An unknown year sorts as above every year: last ascending,
                // first descending.
                AlbumSortField::Year => vec![
                    SortTerm::running("listing.year IS NULL", dir),
                    SortTerm::running("listing.year", dir).nullable(),
                ],
                AlbumSortField::DateAdded => vec![SortTerm::running("listing.created_at", dir)],
            }
        })
        .collect::<Vec<_>>();
    terms.push(SortTerm::ascending("listing.album_id"));
    ListOrder::new(terms)
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
