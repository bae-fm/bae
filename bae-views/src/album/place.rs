//! Where an album's card is in the album grid: its position, and when the
//! grid groups by artist, the artist whose group holds it. The grid reads it
//! live to draw the open album's detail under the row that holds its card,
//! and to scroll a revealed album's card into view. A shift-click reads the
//! albums of the cards between two cards.

use super::listing::{album_ids_between_on, album_place_on};
use super::order::AlbumOrder;
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
        let order = AlbumOrder::new(sort, group_by_artist);
        let (album_id, section) = (album_id.to_string(), section.map(str::to_string));
        self.store.subscribe(move |sql| album_place_on(&sql, &order, &album_id, section.as_deref()))
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
        let order = AlbumOrder::new(sort, group_by_artist);
        let owned =
            |card: AlbumCard<'_>| (card.album_id.to_string(), card.section_id.map(str::to_string));
        let (from, to) = (owned(from), owned(to));
        self.store
            .read(move |sql| {
                let from = AlbumCard { album_id: &from.0, section_id: from.1.as_deref() };
                let to = AlbumCard { album_id: &to.0, section_id: to.1.as_deref() };
                album_ids_between_on(&sql, &order, from, to)
            })
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

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;
