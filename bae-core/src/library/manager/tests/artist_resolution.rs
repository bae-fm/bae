#[tokio::test]
async fn exact_various_artists_source_match_wins_over_the_other_provider_row() {
    let (manager, _tmp) = setup_test_manager().await;
    let discogs_artist = DbArtist {
        id: bae_test_support::test_uuid("1da4c08e-afdf-45f9-aad8-0e5d681d2f29"),
        name: "Artist Group".to_string(),
        sort_name: None,
        discogs_artist_id: Some(crate::db::VARIOUS_ARTISTS.discogs.to_string()),
        musicbrainz_artist_id: None,
        created_at: manager.clock.now(),
    };
    let musicbrainz_artist = DbArtist {
        id: bae_test_support::test_uuid("9556efc5-bd4b-47b2-b699-4c2f44c83c91"),
        name: "Artist Group".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some(crate::db::VARIOUS_ARTISTS.musicbrainz.to_string()),
        created_at: manager.clock.now(),
    };
    manager.insert_artist(&discogs_artist).await.unwrap();
    manager.insert_artist(&musicbrainz_artist).await.unwrap();

    let incoming = DbArtist {
        id: bae_test_support::test_uuid("8a85b0df-729d-42d1-86d1-4de55db1415c"),
        name: "Artist Group".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some(crate::db::VARIOUS_ARTISTS.musicbrainz.to_string()),
        created_at: manager.clock.now(),
    };
    let resolved = manager.resolve_artists_for_import(&[incoming]).await.unwrap();

    assert_eq!(resolved.ids, [musicbrainz_artist.id]);
    assert!(resolved.inserts.is_empty());
}

#[tokio::test]
async fn cross_provider_mismatch_is_not_offered_as_a_two_artist_merge() {
    let (manager, _tmp) = setup_test_manager().await;
    let discogs_artist = DbArtist {
        id: bae_test_support::test_uuid("cross-provider-discogs-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: Some("discogs-1".to_string()),
        musicbrainz_artist_id: Some("musicbrainz-other".to_string()),
        created_at: manager.clock.now(),
    };
    let musicbrainz_artist = DbArtist {
        id: bae_test_support::test_uuid("cross-provider-musicbrainz-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("musicbrainz-1".to_string()),
        created_at: manager.clock.now(),
    };
    manager.insert_artist(&discogs_artist).await.unwrap();
    manager.insert_artist(&musicbrainz_artist).await.unwrap();
    let incoming = DbArtist {
        id: bae_test_support::test_uuid("cross-provider-incoming-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: Some("discogs-1".to_string()),
        musicbrainz_artist_id: Some("musicbrainz-1".to_string()),
        created_at: manager.clock.now(),
    };

    let error = manager
        .resolve_artists_for_import(&[incoming])
        .await
        .expect_err("a third provider identity cannot be resolved by merging two artists");

    assert!(matches!(error, LibraryError::Import(_)));
}
