use bae_records::DbTrack;
use chrono::Utc;

use bae_library::test_libraries::{test_album, test_release};

fn create_test_track(release_id: &str, track_number: i32) -> DbTrack {
    let now = Utc::now();
    DbTrack {
        id: coven::IdSource::new_id(&coven::UuidIds).to_string(),
        release_id: release_id.to_string(),
        title: format!("Track {}", track_number),
        medium_id: bae_records::first_medium_id(release_id),
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
    let (library, _temp_dir) = bae_library::test_libraries::setup_test_library().await;
    let db = library.store().clone();
    let records = library.records().clone();
    let album = test_album();
    let release = test_release(&album.id);
    let track1 = create_test_track(&release.id, 1);
    let track2 = create_test_track(&release.id, 2);

    records.insert_album(&album).await.unwrap();
    records.insert_release(&release).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release.id))
        .await
        .unwrap();
    records.insert_track(&track1).await.unwrap();
    records.insert_track(&track2).await.unwrap();

    library.manager().delete_album(&album.id).await.unwrap();

    let album_result =
        bae_views::LibraryViews::new(db.clone()).find_album_detail(&album.id).await.unwrap();
    assert!(album_result.is_none());

    let releases = library.records().get_releases_for_album(&album.id).await.unwrap();
    assert!(releases.is_empty());

    let tracks = library.records().get_tracks_for_release(&release.id).await.unwrap();
    assert!(tracks.is_empty());
}

#[tokio::test]
async fn test_delete_release_integration() {
    let (library, _temp_dir) = bae_library::test_libraries::setup_test_library().await;
    let records = library.records().clone();
    let album = test_album();
    let release1 = test_release(&album.id);
    let release2 = test_release(&album.id);
    let track1 = create_test_track(&release1.id, 1);
    let track2 = create_test_track(&release2.id, 1);

    records.insert_album(&album).await.unwrap();
    records.insert_release(&release1).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release1.id))
        .await
        .unwrap();
    records.insert_release(&release2).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release2.id))
        .await
        .unwrap();
    records.insert_track(&track1).await.unwrap();
    records.insert_track(&track2).await.unwrap();

    library.manager().delete_release(&release1.id).await.unwrap();

    let album_result = library.manager().get_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_some());

    let releases = library.records().get_releases_for_album(&album.id).await.unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].id, release2.id);

    let tracks1 = library.records().get_tracks_for_release(&release1.id).await.unwrap();
    assert!(tracks1.is_empty());

    let tracks2 = library.records().get_tracks_for_release(&release2.id).await.unwrap();
    assert_eq!(tracks2.len(), 1);
}

#[tokio::test]
async fn test_delete_last_release_deletes_album() {
    let (library, _temp_dir) = bae_library::test_libraries::setup_test_library().await;
    let db = library.store().clone();
    let records = library.records().clone();
    let album = test_album();
    let release = test_release(&album.id);

    records.insert_album(&album).await.unwrap();
    records.insert_release(&release).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release.id))
        .await
        .unwrap();

    library.manager().delete_release(&release.id).await.unwrap();

    let album_result =
        bae_views::LibraryViews::new(db.clone()).find_album_detail(&album.id).await.unwrap();
    assert!(album_result.is_none());
    assert!(library.manager().get_album_by_id(&album.id).await.unwrap().is_none());

    let releases = library.records().get_releases_for_album(&album.id).await.unwrap();
    assert!(releases.is_empty());
}

/// The album's primary release, as its detail names it.
async fn primary_release_of(db: &bae_store::Store, album_id: &str) -> String {
    bae_views::LibraryViews::new(db.clone())
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
    let (library, _temp_dir) = bae_library::test_libraries::setup_test_library().await;
    let db = library.store().clone();
    let records = library.records().clone();
    let album = test_album();
    let release1 = test_release(&album.id);
    let release2 = test_release(&album.id);
    let release3 = test_release(&album.id);

    records.insert_album(&album).await.unwrap();
    for release in [&release1, &release2, &release3] {
        records.insert_release(release).await.unwrap();
        records
            .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release.id))
            .await
            .unwrap();
    }
    assert_eq!(
        primary_release_of(&db, &album.id).await,
        release1.id,
        "an album's first release is its primary"
    );
    library.records().set_album_primary_release(&album.id, &release3.id).await.unwrap();
    assert_eq!(primary_release_of(&db, &album.id).await, release3.id);

    library.manager().delete_release(&release3.id).await.unwrap();

    assert_eq!(
        primary_release_of(&db, &album.id).await,
        release1.id,
        "the lowest-ordered release left is the primary"
    );
}

#[tokio::test]
async fn test_deleting_another_release_keeps_the_primary() {
    let (library, _temp_dir) = bae_library::test_libraries::setup_test_library().await;
    let db = library.store().clone();
    let records = library.records().clone();
    let album = test_album();
    let release1 = test_release(&album.id);
    let release2 = test_release(&album.id);

    records.insert_album(&album).await.unwrap();
    records.insert_release(&release1).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release1.id))
        .await
        .unwrap();
    records.insert_release(&release2).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release2.id))
        .await
        .unwrap();

    library.records().set_album_primary_release(&album.id, &release2.id).await.unwrap();

    library.manager().delete_release(&release1.id).await.unwrap();

    assert_eq!(primary_release_of(&db, &album.id).await, release2.id);
}

/// Making a release primary that is not one of the album's is refused.
#[tokio::test]
async fn test_a_release_of_another_album_cannot_be_made_primary() {
    let (library, _temp_dir) = bae_library::test_libraries::setup_test_library().await;
    let db = library.store().clone();
    let records = library.records().clone();
    let album = test_album();
    let other = test_album();
    let release = test_release(&album.id);
    let elsewhere = test_release(&other.id);
    records.insert_album(&album).await.unwrap();
    records.insert_album(&other).await.unwrap();
    records.insert_release(&release).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release.id))
        .await
        .unwrap();
    records.insert_release(&elsewhere).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&elsewhere.id))
        .await
        .unwrap();

    let refused = library
        .records()
        .set_album_primary_release(&album.id, &elsewhere.id)
        .await
        .map_err(bae_library::LibraryError::from);
    assert!(
        matches!(
            &refused,
            Err(bae_library::LibraryError::PrimaryRelease(
                bae_records::PrimaryReleaseError::NotAlbumRelease { album_id, release_id }
            )) if *album_id == album.id && *release_id == elsewhere.id
        ),
        "another album's release is not this album's to make primary: {refused:?}"
    );
    assert_eq!(refused.unwrap_err().category(), bae_diagnostics::UiErrorCategory::ChangedMeanwhile);
    assert_eq!(primary_release_of(&db, &album.id).await, release.id);
}
