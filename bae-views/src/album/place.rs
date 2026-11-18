//! Where an album's card is in the album grid: its position, and when the
//! grid groups by artist, the artist whose group holds it. The grid reads it
//! once to scroll to an album, and live to draw the open album's detail
//! under the row that holds its card.

use super::*;

impl LibraryViews {
    /// Where the album `album_id`'s card is in the grid under `sort`, flat
    /// or grouped by artist: its card in the group of the artist `section`
    /// when that group holds one, else its first card. `None` when the grid
    /// shows no card of the album.
    pub async fn album_place(
        &self,
        sort: &[AlbumSortCriterion],
        album_id: &str,
        group_by_artist: bool,
        section: Option<&str>,
    ) -> Result<Option<AlbumPlace>, DbError> {
        let query = album_place_query(sort, group_by_artist);
        let (album_id, section) = (album_id.to_string(), section.map(str::to_string));
        self.store
            .read(move |sql| album_place_on(&sql, &query, &album_id, section.as_deref()))
            .await
    }

    /// Follow where the album `album_id`'s card is in the grid, as
    /// [`Self::album_place`] reads it, again after each write that moves it.
    pub fn subscribe_album_place(
        &self,
        sort: &[AlbumSortCriterion],
        album_id: &str,
        group_by_artist: bool,
        section: Option<&str>,
    ) -> coven::LiveQuery<Option<AlbumPlace>, DbError> {
        let query = album_place_query(sort, group_by_artist);
        let (album_id, section) = (album_id.to_string(), section.map(str::to_string));
        self.store.subscribe(move |sql| album_place_on(&sql, &query, &album_id, section.as_deref()))
    }
}

/// Where an album's card is in the album grid: its position, and when the
/// grid groups by artist, the artist whose group holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumPlace {
    pub position: u64,
    pub section_id: Option<String>,
}

/// The statement reading an album's card's place in the grid under `sort`:
/// each card's position numbered in the grid's order, the album's card in the
/// group of the artist `?2` when that group holds one, else its first. The
/// flat grid's cards are in no group.
fn album_place_query(sort: &[AlbumSortCriterion], group_by_artist: bool) -> String {
    let (relation, order) = album_browse_order(sort, group_by_artist);
    let section = if group_by_artist { "link.artist_id" } else { "NULL" };
    format!(
        "SELECT idx, section FROM ( \
            SELECT a.id AS id, {section} AS section, \
                ROW_NUMBER() OVER (ORDER BY {order_by}) - 1 AS idx \
            {relation} \
        ) WHERE id = ?1 ORDER BY ?2 IS NOT NULL AND section = ?2 DESC, idx LIMIT 1",
        order_by = order.clause(),
    )
}

/// The place `query` ([`album_place_query`]) reads for the album
/// `album_id`'s card in the group of `section`.
fn album_place_on(
    sql: &coven::SqlReadContext<'_>,
    query: &str,
    album_id: &str,
    section: Option<&str>,
) -> Result<Option<AlbumPlace>, DbError> {
    Ok(sql
        .query_row(query, params![album_id, section], |row| {
            Ok(AlbumPlace {
                position: bae_store::read::count_column::<u64>(row, "idx")?,
                section_id: row.get("section")?,
            })
        })
        .optional()?)
}

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;
