//! The personal lists: a playlist's entries come back in the order their
//! statement states, read along an index rather than sorted after the read,
//! and playlists and profiles list by name in the language they are asked in.

use crate::people::collections::EntriesOfPlaylistParams;
use crate::people::fragments::AlbumRatingsReadParams;
use bae_store::test_fixtures::query_plan::query_plan;

#[tokio::test]
async fn a_playlists_entries_read_in_order_without_a_sort() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let plan = query_plan(&db, EntriesOfPlaylistParams { playlist: "id".into() }).await;
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "the entries sort after reading: {plan:?}"
    );
}

/// SQLite finds the album's ratings through the album's index, without
/// scanning every rating.
#[tokio::test]
async fn ratings_are_found_by_index() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let album = query_plan(&db, AlbumRatingsReadParams { album: "id".into() }).await;
    assert!(
        album.iter().any(|step| step.contains("ratings_album")),
        "the album's ratings are not read along ratings_album: {album:?}"
    );
}
