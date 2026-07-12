#![cfg(feature = "test-utils")]
mod support;
use crate::support::{test_config_and_keys, tracing_init};
use bae_core::db::{Database, DbAlbum, DbRelease, DbTrack, Pressing, ReleaseMetadataSource};
use bae_core::library::LibraryManager;
use chrono::Utc;
use coven::StoreDir;
use tempfile::TempDir;
use uuid::Uuid;

async fn setup_test_environment() -> (LibraryManager, Database, TempDir) {
    tracing_init();
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("Failed to create database");
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    (library_manager, database, temp_dir)
}

fn create_test_artist() -> bae_core::db::DbArtist {
    bae_core::db::DbArtist {
        id: Uuid::new_v4().to_string(),
        name: "Test Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    }
}

fn create_test_album(artist_id: &str) -> DbAlbum {
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Test Album".to_string(),
        artist_id: artist_id.to_string(),
        year: Some(2024),
        primary_release_id: None,
        is_compilation: false,
        created_at: Utc::now(),
    }
}

fn create_test_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: Pressing {
            year: Some(2024),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        remote: true,
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
        side: 1,
        track_number: Some(track_number),
        duration_ms: Some(180000),
        discogs_position: None,
        created_at: now,
    }
}

#[tokio::test]
async fn test_delete_album_integration() {
    let (library_manager, database, _temp_dir) = setup_test_environment().await;
    let artist = create_test_artist();
    database.insert_artist(&artist).await.unwrap();
    let album = create_test_album(&artist.id);
    let release = create_test_release(&album.id);
    let track1 = create_test_track(&release.id, 1);
    let track2 = create_test_track(&release.id, 2);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release).await.unwrap();
    database.insert_track(&track1).await.unwrap();
    database.insert_track(&track2).await.unwrap();

    library_manager.delete_album(&album.id).await.unwrap();

    let album_result = library_manager.get_album_by_id(&album.id).await.unwrap();
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
    let (library_manager, database, _temp_dir) = setup_test_environment().await;
    let artist = create_test_artist();
    database.insert_artist(&artist).await.unwrap();
    let album = create_test_album(&artist.id);
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    let track1 = create_test_track(&release1.id, 1);
    let track2 = create_test_track(&release2.id, 1);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release1).await.unwrap();
    database.insert_release(&release2).await.unwrap();
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
async fn test_delete_last_release_deletes_album() {
    let (library_manager, database, _temp_dir) = setup_test_environment().await;
    let artist = create_test_artist();
    database.insert_artist(&artist).await.unwrap();
    let album = create_test_album(&artist.id);
    let release = create_test_release(&album.id);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release).await.unwrap();

    library_manager.delete_release(&release.id).await.unwrap();

    let album_result = library_manager.get_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_none());

    let releases = library_manager
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());
}

#[tokio::test]
async fn test_delete_primary_release_clears_album_primary_pointer() {
    let (library_manager, database, _temp_dir) = setup_test_environment().await;
    let artist = create_test_artist();
    database.insert_artist(&artist).await.unwrap();
    let album = create_test_album(&artist.id);
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release1).await.unwrap();
    database.insert_release(&release2).await.unwrap();

    library_manager
        .set_album_primary_release(&album.id, &release1.id)
        .await
        .unwrap();

    library_manager.delete_release(&release1.id).await.unwrap();

    let album_after = database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .expect("album survives deletion of one of two releases");
    assert_ne!(
        album_after.primary_release_id.as_deref(),
        Some(release1.id.as_str()),
        "primary_release_id must not dangle to the deleted release",
    );
}

#[tokio::test]
async fn test_delete_non_primary_release_preserves_primary_pointer() {
    let (library_manager, database, _temp_dir) = setup_test_environment().await;
    let artist = create_test_artist();
    database.insert_artist(&artist).await.unwrap();
    let album = create_test_album(&artist.id);
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release1).await.unwrap();
    database.insert_release(&release2).await.unwrap();

    library_manager
        .set_album_primary_release(&album.id, &release1.id)
        .await
        .unwrap();

    library_manager.delete_release(&release2.id).await.unwrap();

    let album_after = database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .expect("album survives deletion of non-primary release");
    assert_eq!(
        album_after.primary_release_id.as_deref(),
        Some(release1.id.as_str()),
        "primary_release_id must be preserved when a non-primary release is deleted",
    );
}
