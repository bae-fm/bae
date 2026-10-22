#![cfg(feature = "test-utils")]
use bae_core::db::{DbAlbum, DbRelease, DbTrack};
use bae_test_support as support;
use chrono::Utc;
use uuid::Uuid;

fn create_test_album() -> DbAlbum {
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Test Album".to_string(),
        year: Some(2024),
        is_compilation: false,
        created_at: Utc::now(),
    }
}

fn create_test_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: bae_core::pressing::Pressing {
            year: Some(2024),
            labels: Vec::new(),
            facts: Default::default(),
            barcodes: Vec::new(),
        },
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: Utc::now(),
    }
}

fn create_test_track(release_id: &str, track_number: i32) -> DbTrack {
    let now = Utc::now();
    DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release_id.to_string(),
        title: format!("Track {}", track_number),
        medium_id: bae_core::db::identity::release_medium_id(release_id, 1),
        side: std::num::NonZeroU8::MIN,
        track_number: std::num::NonZeroU32::new(
            u32::try_from(track_number).expect("a test track number fits u32"),
        )
        .expect("track numbers count from one"),
        duration_ms: 180000,
        created_at: now,
    }
}

#[tokio::test]
async fn test_delete_album_integration() {
    let (library_manager, database, _temp_dir) = support::setup_test_library().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track1 = create_test_track(&release.id, 1);
    let track2 = create_test_track(&release.id, 2);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release).await.unwrap();
    database
        .insert_release_medium(&bae_core::db::DbReleaseMedium::new_test(&release.id))
        .await
        .unwrap();
    database.insert_track(&track1).await.unwrap();
    database.insert_track(&track2).await.unwrap();

    library_manager.delete_album(&album.id).await.unwrap();

    let album_result = library_manager.find_album_detail(&album.id).await.unwrap();
    assert!(album_result.is_none());

    let releases = library_manager
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());

    let tracks = library_manager
        .get_tracks_for_release(&release.id)
        .await
        .unwrap();
    assert!(tracks.is_empty());
}

#[tokio::test]
async fn test_delete_release_integration() {
    let (library_manager, database, _temp_dir) = support::setup_test_library().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    let track1 = create_test_track(&release1.id, 1);
    let track2 = create_test_track(&release2.id, 1);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release1).await.unwrap();
    database
        .insert_release_medium(&bae_core::db::DbReleaseMedium::new_test(&release1.id))
        .await
        .unwrap();
    database.insert_release(&release2).await.unwrap();
    database
        .insert_release_medium(&bae_core::db::DbReleaseMedium::new_test(&release2.id))
        .await
        .unwrap();
    database.insert_track(&track1).await.unwrap();
    database.insert_track(&track2).await.unwrap();

    library_manager.delete_release(&release1.id).await.unwrap();

    let album_result = library_manager.get_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_some());

    let releases = library_manager
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].id, release2.id);

    let tracks1 = library_manager
        .get_tracks_for_release(&release1.id)
        .await
        .unwrap();
    assert!(tracks1.is_empty());

    let tracks2 = library_manager
        .get_tracks_for_release(&release2.id)
        .await
        .unwrap();
    assert_eq!(tracks2.len(), 1);
}

#[tokio::test]
async fn test_delete_last_release_empties_album() {
    let (library_manager, database, _temp_dir) = support::setup_test_library().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release).await.unwrap();
    database
        .insert_release_medium(&bae_core::db::DbReleaseMedium::new_test(&release.id))
        .await
        .unwrap();

    library_manager.delete_release(&release.id).await.unwrap();

    let album_result = library_manager.find_album_detail(&album.id).await.unwrap();
    assert!(album_result.is_none());

    let releases = library_manager
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());
}

/// The album's primary release, as its detail names it.
async fn primary_release_of(
    library_manager: &bae_core::library::LibraryManager,
    album_id: &str,
) -> String {
    library_manager
        .find_album_detail(album_id)
        .await
        .unwrap()
        .expect("an album with a release has a detail")
        .primary_release_id
}

/// Deleting the primary release makes the next-ordered one primary in the
/// same write: nothing points at the release that went.
#[tokio::test]
async fn test_deleting_the_primary_release_makes_the_next_one_primary() {
    let (library_manager, database, _temp_dir) = support::setup_test_library().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    let release3 = create_test_release(&album.id);

    database.insert_album(&album).await.unwrap();
    for release in [&release1, &release2, &release3] {
        database.insert_release(release).await.unwrap();
        database
            .insert_release_medium(&bae_core::db::DbReleaseMedium::new_test(&release.id))
            .await
            .unwrap();
    }
    assert_eq!(
        primary_release_of(&library_manager, &album.id).await,
        release1.id,
        "an album's first release is its primary"
    );
    library_manager
        .set_album_primary_release(&album.id, &release3.id)
        .await
        .unwrap();
    assert_eq!(
        primary_release_of(&library_manager, &album.id).await,
        release3.id
    );

    library_manager.delete_release(&release3.id).await.unwrap();

    assert_eq!(
        primary_release_of(&library_manager, &album.id).await,
        release1.id,
        "the lowest-ordered release left is the primary"
    );
}

#[tokio::test]
async fn test_deleting_another_release_keeps_the_primary() {
    let (library_manager, database, _temp_dir) = support::setup_test_library().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release1).await.unwrap();
    database
        .insert_release_medium(&bae_core::db::DbReleaseMedium::new_test(&release1.id))
        .await
        .unwrap();
    database.insert_release(&release2).await.unwrap();
    database
        .insert_release_medium(&bae_core::db::DbReleaseMedium::new_test(&release2.id))
        .await
        .unwrap();

    library_manager
        .set_album_primary_release(&album.id, &release2.id)
        .await
        .unwrap();

    library_manager.delete_release(&release1.id).await.unwrap();

    assert_eq!(
        primary_release_of(&library_manager, &album.id).await,
        release2.id
    );
}

/// Making a release primary that is not one of the album's is refused.
#[tokio::test]
async fn test_a_release_of_another_album_cannot_be_made_primary() {
    let (library_manager, database, _temp_dir) = support::setup_test_library().await;
    let album = create_test_album();
    let other = create_test_album();
    let release = create_test_release(&album.id);
    let elsewhere = create_test_release(&other.id);
    database.insert_album(&album).await.unwrap();
    database.insert_album(&other).await.unwrap();
    database.insert_release(&release).await.unwrap();
    database
        .insert_release_medium(&bae_core::db::DbReleaseMedium::new_test(&release.id))
        .await
        .unwrap();
    database.insert_release(&elsewhere).await.unwrap();
    database
        .insert_release_medium(&bae_core::db::DbReleaseMedium::new_test(&elsewhere.id))
        .await
        .unwrap();

    library_manager
        .set_album_primary_release(&album.id, &elsewhere.id)
        .await
        .expect_err("another album's release is not this album's to make primary");
    assert_eq!(
        primary_release_of(&library_manager, &album.id).await,
        release.id
    );
}
