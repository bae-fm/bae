//! The plans of the artist list's windows, from a key and from the top, one
//! statement for each term a sort can lead with.

use super::*;
use bae_store::test_fixtures::query_plan::{plan_steps, PlanOf};

/// Each window statement, the index leading with its first term, and that
/// term's column.
const ARTIST_WINDOWS: [(PlanOf, &str, &str); 4] = [
    (plan_steps::<ArtistWindowByNameParams>, "idx_artist_listing_name", "name_sort_key"),
    (
        plan_steps::<ArtistWindowByNameDescendingParams>,
        "idx_artist_listing_name_back",
        "sort_name_back",
    ),
    (plan_steps::<ArtistWindowByAlbumsParams>, "idx_artist_listing_albums", "album_count"),
    (
        plan_steps::<ArtistWindowByAlbumsDescendingParams>,
        "idx_artist_listing_albums_back",
        "sort_albums_back",
    ),
];

#[tokio::test]
async fn every_artist_window_seeks_the_index_leading_with_its_first_term() {
    crate::tests::assert_windows_seek_their_first_term(&ARTIST_WINDOWS).await;
}

#[tokio::test]
async fn every_artist_window_from_the_top_reads_the_index_leading_with_its_first_term() {
    crate::tests::assert_tops_read_their_first_term(&[
        (plan_steps::<ArtistTopByNameParams>, "idx_artist_listing_name"),
        (plan_steps::<ArtistTopByNameDescendingParams>, "idx_artist_listing_name_back"),
        (plan_steps::<ArtistTopByAlbumsParams>, "idx_artist_listing_albums"),
        (plan_steps::<ArtistTopByAlbumsDescendingParams>, "idx_artist_listing_albums_back"),
    ])
    .await;
}

/// A window from the top holds the list's first rows under every sort.
#[tokio::test]
async fn every_artist_sorts_window_from_the_top_holds_its_first_rows() {
    let (db, _tmp) = crate::tests::library().await;
    db.read(|sql| {
        for sort in crate::tests::sorts_of(&[ArtistSortField::Name, ArtistSortField::AlbumCount]) {
            let criteria: Vec<_> = sort
                .iter()
                .map(|&(field, direction)| ArtistSortCriterion { field, direction })
                .collect();
            let sort = artist_sort(&criteria);
            crate::tests::assert_top_is_first_rows(
                &format!("{criteria:?}"),
                || artist_key_at_on(&sql, &sort, 0),
                |window| artist_window_on(&sql, &sort, window),
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
}
