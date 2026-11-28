//! The artist list and an artist's page, read once through the queries their
//! screens follow.

use crate::*;

/// The first 100 rows of the artist list under `sort`, as its window reads
/// them.
pub(crate) async fn listed_artists(
    db: &Store,
    sort: &[ArtistSortCriterion],
) -> Vec<DbArtistSummary> {
    let sort = crate::artist::artist_sort(sort);
    db.read(move |sql| crate::artist::artist_window_on(&sql, &sort, &ListWindow::top(100)))
        .await
        .unwrap()
        .rows
        .into_iter()
        .map(|(row, _key)| row)
        .collect()
}

/// How many artists the artist list holds.
pub(crate) async fn artist_count(db: &Store) -> u64 {
    db.read(|sql| crate::artist::artist_list_length_on(&sql)).await.unwrap()
}

/// The artist `artist_id`'s page, as its view first reads it; `None` when
/// the artist list doesn't show the artist.
pub(crate) async fn artist_page(db: &Store, artist_id: &str) -> Option<DbArtistDetail> {
    let mut detail = LibraryViews::new(db.clone()).subscribe_artist_detail(Some(artist_id.into()));
    detail.next_inline().await.expect("the query stays open").result.unwrap().detail
}
