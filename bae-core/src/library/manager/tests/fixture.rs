/// A fixture's albums land as the grid lists them: added in the fixture's
/// order, so newest first is that order reversed, each with its artist and
/// its tracks in order.
#[tokio::test]
async fn fixture_albums_land_in_the_order_they_were_added() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = |title: &str, artist: &str, tracks: &[&str]| crate::library::FixtureAlbum {
        title: title.to_string(),
        artist: artist.to_string(),
        tracks: tracks.iter().map(|track| track.to_string()).collect(),
    };
    let fixture = crate::library::LibraryFixture {
        albums: vec![
            album("First", "Artist A", &["One"]),
            album("Second", "Artist A", &["One"]),
            album("Third", "Artist B", &["Opening", "Closing"]),
        ],
    };

    manager.write_fixture(&fixture).await.unwrap();

    let newest_first = manager
        .get_albums(&[crate::db::AlbumSortCriterion {
            field: crate::db::AlbumSortField::DateAdded,
            direction: crate::db::SortDirection::Descending,
        }])
        .await
        .unwrap();
    let titles: Vec<&str> = newest_first.iter().map(|album| album.title.as_str()).collect();
    assert_eq!(titles, ["Third", "Second", "First"]);
    assert_eq!(newest_first[1].artist_id, newest_first[2].artist_id);

    let third = &newest_first[0];
    let artists = manager.get_artists_for_album(&third.id).await.unwrap();
    assert_eq!(artists.len(), 1);
    assert_eq!(artists[0].name, "Artist B");
    let release_id = third.primary_release_id.clone().unwrap();
    let tracks = manager.get_tracks_for_release(&release_id).await.unwrap();
    let track_titles: Vec<&str> = tracks.iter().map(|track| track.title.as_str()).collect();
    assert_eq!(track_titles, ["Opening", "Closing"]);
}
