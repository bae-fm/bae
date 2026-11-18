//! Where an album's card is in the album grid: its position, and when the
//! grid groups by artist, the artist whose group holds it. The grid reads it
//! live to draw the open album's detail under the row that holds its card,
//! and to scroll a revealed album's card into view. A shift-click reads the
//! albums of the cards between two cards.

use super::*;

impl LibraryViews {
    /// Follow where the album `album_id`'s card is in the grid under
    /// `sort`, flat or grouped by artist, again after each write that moves
    /// it: its card in the group of the artist `section` when that group
    /// holds one, else its first card. `None` when the grid shows no card of
    /// the album.
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

    /// The albums of the grid's cards from the card `from` to the card `to`,
    /// both included, in the grid's order under `sort`, flat or grouped by
    /// artist, each album once: what a shift-click selects. Each card is its
    /// album's card in its artist's group when that group holds one, else
    /// the album's first card. With no card of `from`'s album, the albums
    /// are `to`'s alone; with no card of `to`'s, there are none.
    pub async fn album_ids_between(
        &self,
        sort: &[AlbumSortCriterion],
        group_by_artist: bool,
        from: AlbumCard<'_>,
        to: AlbumCard<'_>,
    ) -> Result<Vec<String>, DbError> {
        let query = format!(
            "WITH card AS MATERIALIZED ({cards}), \
             ends (low, high) AS ( \
                 SELECT min(coalesce(f, t), t), max(coalesce(f, t), t) FROM ( \
                     SELECT (SELECT idx FROM card {from}) AS f, \
                            (SELECT idx FROM card {to}) AS t)) \
             SELECT card.id FROM card JOIN ends ON card.idx BETWEEN ends.low AND ends.high \
             GROUP BY card.id ORDER BY MIN(card.idx)",
            cards = numbered_cards(sort, group_by_artist),
            from = card_of("?1", "?2"),
            to = card_of("?3", "?4"),
        );
        let params = (
            from.album_id.to_string(),
            from.section_id.map(str::to_string),
            to.album_id.to_string(),
            to.section_id.map(str::to_string),
        );
        self.store
            .read(move |sql| Ok(sql.query(&query, params, |row| row.get::<_, String>(0))?))
            .await
    }
}

/// One card of the album grid: the album's card in the group of the artist
/// `section_id` when the grid groups by artist and that group holds one,
/// else the album's first card.
#[derive(Debug, Clone, Copy)]
pub struct AlbumCard<'a> {
    pub album_id: &'a str,
    pub section_id: Option<&'a str>,
}

/// Where an album's card is in the album grid: its position, and when the
/// grid groups by artist, the artist whose group holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumPlace {
    pub position: u64,
    pub section_id: Option<String>,
}

/// The statement reading an album's card's place in the grid under `sort`:
/// the album `?1`'s card in the group of the artist `?2` when that group
/// holds one, else its first.
fn album_place_query(sort: &[AlbumSortCriterion], group_by_artist: bool) -> String {
    format!(
        "SELECT idx, section FROM ({cards}) {card}",
        cards = numbered_cards(sort, group_by_artist),
        card = card_of("?1", "?2"),
    )
}

/// Every card of the grid under `sort`: its album's `id`, the artist whose
/// group holds it as `section`, and its position numbered in the grid's
/// order as `idx`. The flat grid's cards are in no group.
fn numbered_cards(sort: &[AlbumSortCriterion], group_by_artist: bool) -> String {
    let (relation, order) = album_browse_order(sort, group_by_artist);
    let section = if group_by_artist { "link.artist_id" } else { "NULL" };
    format!(
        "SELECT a.id AS id, {section} AS section, \
            ROW_NUMBER() OVER (ORDER BY {order_by}) - 1 AS idx \
         {relation}",
        order_by = order.clause(),
    )
}

/// The clauses that keep, of [`numbered_cards`], the card of the album
/// `album` in the group of the artist `section` when that group holds one,
/// else the album's first card.
fn card_of(album: &str, section: &str) -> String {
    format!(
        "WHERE id = {album} ORDER BY {section} IS NOT NULL AND section = {section} DESC, idx LIMIT 1"
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
