//! The plans of the artist list's windows, from a key and from the top, one
//! statement for each term a sort can lead with.

use super::*;

/// Each window statement, the index leading with its first term, and that
/// term's column.
const ARTIST_WINDOWS: [(&str, &str, &str); 4] = [
    (ArtistWindowByName::SQL, "idx_artist_listing_name", "name_sort_key"),
    (ArtistWindowByNameDescending::SQL, "idx_artist_listing_name_back", "sort_name_back"),
    (ArtistWindowByAlbums::SQL, "idx_artist_listing_albums", "album_count"),
    (ArtistWindowByAlbumsDescending::SQL, "idx_artist_listing_albums_back", "sort_albums_back"),
];

#[tokio::test]
async fn every_artist_window_seeks_the_index_leading_with_its_first_term() {
    crate::tests::assert_windows_seek_their_first_term(&ARTIST_WINDOWS, 2).await;
}

#[tokio::test]
async fn every_artist_window_from_the_top_reads_the_index_leading_with_its_first_term() {
    crate::tests::assert_tops_read_their_first_term(&[
        (ArtistTopByName::SQL, "idx_artist_listing_name"),
        (ArtistTopByNameDescending::SQL, "idx_artist_listing_name_back"),
        (ArtistTopByAlbums::SQL, "idx_artist_listing_albums"),
        (ArtistTopByAlbumsDescending::SQL, "idx_artist_listing_albums_back"),
    ])
    .await;
}
