/// One album of a library fixture, credited to `artists`, with no cover.
fn fixture_album(title: &str, artists: &[&str], tracks: &[&str]) -> crate::library::FixtureAlbum {
    crate::library::FixtureAlbum {
        title: title.to_string(),
        artists: artists.iter().map(|artist| artist.to_string()).collect(),
        tracks: tracks.iter().map(|track| track.to_string()).collect(),
        cover: None,
    }
}

/// The library fixture holding `albums` and nothing else.
fn fixture_of_albums(albums: Vec<crate::library::FixtureAlbum>) -> crate::library::LibraryFixture {
    crate::library::LibraryFixture { albums }
}

/// A fixture's albums land as the grid lists them: added in the fixture's
/// order, so newest first is that order reversed, each with its artist and
/// its tracks in order.
#[tokio::test]
async fn fixture_albums_land_in_the_order_they_were_added() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let fixture = fixture_of_albums(vec![
        fixture_album("First", &["Artist A"], &["One"]),
        fixture_album("Second", &["Artist A"], &["One"]),
        fixture_album("Third", &["Artist B"], &["Opening", "Closing"]),
    ]);

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

/// An album credited to two artists is credited to both in order, and the
/// grid grouped by artist lists it under each.
#[tokio::test]
async fn a_fixture_album_credited_to_two_artists_is_listed_under_each() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let fixture = fixture_of_albums(vec![
        fixture_album("Shared Album", &["Artist A", "Artist B"], &["One"]),
        fixture_album("Own Album", &["Artist B"], &["One"]),
    ]);

    manager.write_fixture(&fixture).await.unwrap();

    let albums = manager.get_albums(&[]).await.unwrap();
    let shared = albums
        .iter()
        .find(|album| album.title == "Shared Album")
        .expect("the shared album is in the library");
    let credited: Vec<String> = manager
        .get_artists_for_album(&shared.id)
        .await
        .unwrap()
        .into_iter()
        .map(|artist| artist.name)
        .collect();
    assert_eq!(credited, ["Artist A", "Artist B"]);

    let mut browse = manager.subscribe_album_browse(
        &[],
        true,
        [crate::library::LibraryPageWindow {
            offset: 0,
            limit: 10,
        }]
        .into(),
    );
    let grouped = browse.next().await.into_result().unwrap();
    let rows = &grouped.windows[0].rows;
    let sections: Vec<(String, Vec<String>)> = grouped
        .sections
        .iter()
        .map(|section| {
            let start = section.window.offset as usize;
            let end = start + section.window.limit as usize;
            (
                section.title.clone(),
                rows[start..end]
                    .iter()
                    .map(|album| album.title.clone())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        sections,
        [
            ("Artist A".to_string(), vec!["Shared Album".to_string()]),
            (
                "Artist B".to_string(),
                vec!["Own Album".to_string(), "Shared Album".to_string()]
            ),
        ]
    );
}

/// An album's cover is stored as its release's cover: the image resized as
/// every stored cover is.
#[tokio::test]
async fn a_fixture_albums_cover_is_its_releases_cover() {
    let (manager, temp_dir) = setup_test_manager().await;
    let cover = temp_dir.path().join("cover.png");
    ::image::RgbImage::from_pixel(4, 4, ::image::Rgb([200, 40, 40]))
        .save(&cover)
        .unwrap();
    let mut album = fixture_album("Covered Album", &["Artist A"], &["One"]);
    album.cover = Some(cover.clone());

    manager
        .write_fixture(&fixture_of_albums(vec![album]))
        .await
        .unwrap();

    let albums = manager.get_albums(&[]).await.unwrap();
    let release_id = albums[0].primary_release_id.clone().unwrap();
    let stored = manager
        .read_cover_image_blob(&release_id)
        .await
        .unwrap()
        .expect("the release has a cover");
    assert_eq!(
        stored,
        crate::util::cover::resize_cover(&std::fs::read(&cover).unwrap()).unwrap()
    );
}

/// An album that credits no artist is no album a library holds, and the
/// fixture naming it writes nothing.
#[tokio::test]
async fn a_fixture_album_crediting_no_artist_writes_nothing() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let fixture = fixture_of_albums(vec![
        fixture_album("Credited Album", &["Artist A"], &["One"]),
        fixture_album("Uncredited Album", &[], &["One"]),
    ]);

    let error = manager.write_fixture(&fixture).await.unwrap_err();

    assert!(matches!(
        error,
        crate::library::LibraryFixtureError::NoArtist { ref album } if album == "Uncredited Album"
    ));
    assert!(manager.get_albums(&[]).await.unwrap().is_empty());
}
