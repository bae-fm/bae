use bae_library::test_libraries::{connect_storage, made_profile, storage_library};
use bae_people::collections::{CollectionError, RatingTarget, Stars};
use bae_people::people::Sharing;
use bae_util::test_ids::test_uuid;

#[tokio::test]
async fn ratings_and_playlist_edits_preserve_identity_order_and_audience() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let store = library.store().clone();
    let people = bae_people::People::new(store.clone());
    connect_storage(library.manager(), library.store(), &storage).await;
    let person = made_profile(library.manager(), library.people(), "Robin").await;
    let release = test_uuid("collection-release");
    let first = test_uuid("collection-first");
    let second = test_uuid("collection-second");
    let album = test_uuid("collection-album");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&album, "Collection"),
        &release,
        &[(&first, "First"), (&second, "Second")],
    )
    .await;
    let target = RatingTarget::Track(first.clone());
    people.set_rating(target.clone(), Some(Stars::Three)).await.unwrap();
    people.set_rating(target.clone(), Some(Stars::Five)).await.unwrap();
    let ratings = people.ratings().await.unwrap();
    assert_eq!(ratings.len(), 1);
    assert_eq!(ratings[0].person, person.id);
    assert_eq!(ratings[0].stars, Stars::Five);
    people.set_rating(target, None).await.unwrap();
    assert!(people.ratings().await.unwrap().is_empty());
    people.set_rating(RatingTarget::Album(album.clone()), Some(Stars::Two)).await.unwrap();

    let id = coven::IdSource::new_id(&coven::UuidIds);
    let playlist =
        people.create_playlist(id, "Evening", Sharing::Private, Vec::new()).await.unwrap();
    assert_eq!(playlist.audience, coven::Audience::Circle(person.private_circle));
    let a = coven::IdSource::new_id(&coven::UuidIds);
    let b = coven::IdSource::new_id(&coven::UuidIds);
    let c = coven::IdSource::new_id(&coven::UuidIds);
    people.append_playlist_entry(id, a, first.clone()).await.unwrap();
    people.append_playlist_entry(id, b, second).await.unwrap();
    people.append_playlist_entry(id, c, first.clone()).await.unwrap();
    assert!(matches!(
        people.reorder_playlist(id, vec![b, a]).await,
        Err(CollectionError::OrderChanged)
    ));
    assert!(matches!(
        people.reorder_playlist(id, vec![a, a, c]).await,
        Err(CollectionError::OrderChanged)
    ));
    assert_eq!(
        people
            .playlist_entries(id)
            .await
            .unwrap()
            .iter()
            .map(|entry| entry.position_id)
            .collect::<Vec<_>>(),
        vec![a, b, c]
    );
    people.reorder_playlist(id, vec![c, b, a]).await.unwrap();
    let entries = people.playlist_entries(id).await.unwrap();
    assert_eq!(entries.iter().map(|entry| entry.position_id).collect::<Vec<_>>(), vec![c, b, a]);
    assert_eq!(entries[0].track_id, first);
    people.set_playlist_sharing(id, Sharing::Household).await.unwrap();
    people.rename_playlist(id, "Gathering").await.unwrap();
    let playlists = people.playlists().await.unwrap();
    assert_eq!(playlists[0].audience, coven::Audience::Store);
    assert_eq!(playlists[0].name, "Gathering");
    assert_eq!(people.playlist_entries(id).await.unwrap(), entries);
    people.remove_playlist_entry(id, b).await.unwrap();
    assert_eq!(people.playlist_entries(id).await.unwrap().len(), 2);
    people.delete_playlist(id).await.unwrap();
    assert!(people.playlists().await.unwrap().is_empty());
    assert!(people.playlist_entries(id).await.unwrap().is_empty());
    library.manager().close().await.unwrap();
}

#[tokio::test]
async fn a_playlist_is_made_holding_its_tracks_or_not_at_all() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = bae_people::People::new(library.store().clone());
    connect_storage(library.manager(), library.store(), &storage).await;
    made_profile(library.manager(), library.people(), "Robin").await;
    let release = test_uuid("made-release");
    let first = test_uuid("made-first");
    let second = test_uuid("made-second");
    bae_records::test_fixtures::album_with_tracks(
        library.records(),
        (&test_uuid("made-album"), "Made"),
        &release,
        &[(&first, "First"), (&second, "Second")],
    )
    .await;

    let id = library
        .manager()
        .create_playlist(
            "Evening",
            Sharing::Private,
            vec![second.clone(), first.clone(), second.clone()],
        )
        .await
        .unwrap();
    let tracks: Vec<String> =
        people.playlist_entries(id).await.unwrap().into_iter().map(|e| e.track_id).collect();
    assert_eq!(tracks, vec![second.clone(), first.clone(), second.clone()]);

    let unknown = test_uuid("made-unknown");
    let refused =
        library.manager().create_playlist("Morning", Sharing::Private, vec![first, unknown]).await;
    assert!(refused.is_err());
    let names: Vec<String> =
        people.playlists().await.unwrap().into_iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["Evening".to_string()]);
    library.manager().close().await.unwrap();
}
