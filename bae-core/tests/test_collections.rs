#![cfg(feature = "test-utils")]

use bae_core::collections::{CollectionError, RatingTarget, Stars};
use bae_core::people::Sharing;
use bae_test_support::{connect_storage, storage_library, test_uuid};

#[tokio::test]
async fn ratings_and_playlist_edits_preserve_identity_order_and_audience() {
    let directory = tempfile::tempdir().unwrap();
    let (manager, db, storage) =
        storage_library(directory.path(), bae_core::providers::Providers::offline()).await;
    connect_storage(&manager, &storage).await;
    let person = manager.create_profile("Robin").await.unwrap();
    let release = test_uuid("collection-release");
    let first = test_uuid("collection-first");
    let second = test_uuid("collection-second");
    db.execute_local_sql_for_test(&format!("INSERT INTO albums (id,title,created_at) VALUES ('collection-album','Collection','2026-01-01'); INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{release}','collection-album','2026-01-01', 0); INSERT INTO tracks (id,release_id,title,position,created_at) VALUES ('{first}','{release}','First',0,'2026-01-01'), ('{second}','{release}','Second',1,'2026-01-01');")).await.unwrap();
    let target = RatingTarget::Track(first.clone());
    db.set_rating(target.clone(), Some(Stars::Three))
        .await
        .unwrap();
    db.set_rating(target.clone(), Some(Stars::Five))
        .await
        .unwrap();
    let ratings = db.ratings().await.unwrap();
    assert_eq!(ratings.len(), 1);
    assert_eq!(ratings[0].person, person.id);
    assert_eq!(ratings[0].stars, Stars::Five);
    db.set_rating(target, None).await.unwrap();
    assert!(db.ratings().await.unwrap().is_empty());
    db.set_rating(
        RatingTarget::Album("collection-album".into()),
        Some(Stars::Two),
    )
    .await
    .unwrap();

    let id = uuid::Uuid::new_v4();
    let playlist = db
        .create_playlist(id, "Evening", Sharing::Private)
        .await
        .unwrap();
    assert_eq!(
        playlist.audience,
        coven::Audience::Circle(person.private_circle)
    );
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let c = uuid::Uuid::new_v4();
    db.append_playlist_entry(id, a, first.clone())
        .await
        .unwrap();
    db.append_playlist_entry(id, b, second).await.unwrap();
    db.append_playlist_entry(id, c, first.clone())
        .await
        .unwrap();
    assert!(matches!(
        db.reorder_playlist(id, vec![b, a]).await,
        Err(CollectionError::OrderChanged)
    ));
    assert!(matches!(
        db.reorder_playlist(id, vec![a, a, c]).await,
        Err(CollectionError::OrderChanged)
    ));
    assert_eq!(
        db.playlist_entries(id)
            .await
            .unwrap()
            .iter()
            .map(|entry| entry.position_id)
            .collect::<Vec<_>>(),
        vec![a, b, c]
    );
    db.reorder_playlist(id, vec![c, b, a]).await.unwrap();
    let entries = db.playlist_entries(id).await.unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.position_id)
            .collect::<Vec<_>>(),
        vec![c, b, a]
    );
    assert_eq!(entries[0].track_id, first);
    db.set_playlist_sharing(id, Sharing::Household)
        .await
        .unwrap();
    db.rename_playlist(id, "Gathering").await.unwrap();
    let playlists = db.playlists().await.unwrap();
    assert_eq!(playlists[0].audience, coven::Audience::Store);
    assert_eq!(playlists[0].name, "Gathering");
    assert_eq!(db.playlist_entries(id).await.unwrap(), entries);
    db.remove_playlist_entry(id, b).await.unwrap();
    assert_eq!(db.playlist_entries(id).await.unwrap().len(), 2);
    db.delete_playlist(id).await.unwrap();
    assert!(db.playlists().await.unwrap().is_empty());
    assert!(db.playlist_entries(id).await.unwrap().is_empty());
    manager.close().await.unwrap();
}
