#![cfg(feature = "test-utils")]
mod support;
use crate::support::{test_encryption_service, tracing_init};
use bae_core::db::{Database, DbAlbum, DbRelease, DbTrack, ImportStatus};
use bae_core::library::{LibraryManager, SharedLibraryManager};
use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

async fn setup_test_environment() -> (SharedLibraryManager, Database, TempDir) {
    tracing_init();
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new(db_path.to_str().unwrap())
        .await
        .expect("Failed to create database");
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager = SharedLibraryManager::new(library_manager);
    (shared_library_manager, database, temp_dir)
}

fn create_test_album() -> DbAlbum {
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Test Album".to_string(),
        year: Some(2024),
        discogs_release: None,
        musicbrainz_release: None,
        bandcamp_album_id: None,
        cover_image_id: None,
        cover_art_url: None,
        is_compilation: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn create_test_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        year: Some(2024),
        discogs_release_id: None,
        bandcamp_release_id: None,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
        barcode: None,
        import_status: ImportStatus::Complete,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn create_test_track(release_id: &str, track_number: i32) -> DbTrack {
    DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release_id.to_string(),
        title: format!("Track {}", track_number),
        disc_number: None,
        track_number: Some(track_number),
        duration_ms: Some(180000),
        discogs_position: None,
        import_status: ImportStatus::Complete,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn test_delete_album_integration() {
    let (library_manager, database, _temp_dir) = setup_test_environment().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track1 = create_test_track(&release.id, 1);
    let track2 = create_test_track(&release.id, 2);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release).await.unwrap();
    database.insert_track(&track1).await.unwrap();
    database.insert_track(&track2).await.unwrap();

    library_manager.get().delete_album(&album.id).await.unwrap();

    let album_result = library_manager
        .get()
        .get_album_by_id(&album.id)
        .await
        .unwrap();
    assert!(album_result.is_none());

    let releases = library_manager
        .get()
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());

    let tracks = library_manager.get().get_tracks(&release.id).await.unwrap();
    assert!(tracks.is_empty());
}

#[tokio::test]
async fn test_delete_release_integration() {
    let (library_manager, database, _temp_dir) = setup_test_environment().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    let track1 = create_test_track(&release1.id, 1);
    let track2 = create_test_track(&release2.id, 1);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release1).await.unwrap();
    database.insert_release(&release2).await.unwrap();
    database.insert_track(&track1).await.unwrap();
    database.insert_track(&track2).await.unwrap();

    library_manager
        .get()
        .delete_release(&release1.id)
        .await
        .unwrap();

    let album_result = library_manager
        .get()
        .get_album_by_id(&album.id)
        .await
        .unwrap();
    assert!(album_result.is_some());

    let releases = library_manager
        .get()
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].id, release2.id);

    let tracks1 = library_manager
        .get()
        .get_tracks(&release1.id)
        .await
        .unwrap();
    assert!(tracks1.is_empty());

    let tracks2 = library_manager
        .get()
        .get_tracks(&release2.id)
        .await
        .unwrap();
    assert_eq!(tracks2.len(), 1);
}

#[tokio::test]
async fn test_delete_last_release_deletes_album() {
    let (library_manager, database, _temp_dir) = setup_test_environment().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);

    database.insert_album(&album).await.unwrap();
    database.insert_release(&release).await.unwrap();

    library_manager
        .get()
        .delete_release(&release.id)
        .await
        .unwrap();

    let album_result = library_manager
        .get()
        .get_album_by_id(&album.id)
        .await
        .unwrap();
    assert!(album_result.is_none());

    let releases = library_manager
        .get()
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());
}
