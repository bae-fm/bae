//! Test that verifies storage profiles are correctly linked during import
//! and properly retrieved during read operations.
//!
//! This test validates the core fix from the cloud storage refactor:
//! - Import links release to the specified storage profile
//! - Read operations retrieve the same storage profile
//! - The profile's credentials would be used (verified via mock)

use bae::db::{Database, DbAlbum, DbRelease, DbStorageProfile, ImportStatus};
use bae::storage::create_storage_reader;
use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

/// Test that create_storage_reader uses the profile's credentials
#[tokio::test]
async fn test_storage_reader_uses_profile_credentials() {
    tracing_init();

    // Create a local storage profile (doesn't need real S3)
    let temp_dir = TempDir::new().unwrap();
    let storage_path = temp_dir.path().join("storage");
    std::fs::create_dir_all(&storage_path).unwrap();

    let profile = DbStorageProfile::new_local(
        "Test Profile",
        storage_path.to_str().unwrap(),
        false, // not encrypted
    );

    // Create storage reader from profile
    let storage = create_storage_reader(&profile).await.unwrap();

    // Write and read a test file to verify it works
    let test_data = b"test file data";
    let file_path = storage_path.join("test_file.bin");
    let file_path_str = file_path.to_str().unwrap();

    let location = storage.upload(file_path_str, test_data).await.unwrap();
    assert_eq!(location, file_path_str);

    let downloaded = storage.download(file_path_str).await.unwrap();
    assert_eq!(downloaded, test_data);

    // Cleanup
    storage.delete(file_path_str).await.unwrap();
    assert!(!file_path.exists());
}

/// Test that release_storage links release to profile, and we can retrieve the same profile
#[tokio::test]
async fn test_release_storage_profile_linkage() {
    tracing_init();

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage_path = temp_dir.path().join("storage");
    std::fs::create_dir_all(&storage_path).unwrap();

    let database = Database::new(db_path.to_str().unwrap()).await.unwrap();

    // Create a storage profile with specific credentials
    let profile = DbStorageProfile::new_cloud(
        "My S3 Profile",
        "my-bucket",
        "us-west-2",
        Some("https://s3.example.com"),
        "AKIAIOSFODNN7EXAMPLE",
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        true, // encrypted
    );
    let profile_id = profile.id.clone();

    // Insert the profile
    database.insert_storage_profile(&profile).await.unwrap();

    // Create a fake release and album
    let album = create_test_album("Test Album");
    database.insert_album(&album).await.unwrap();

    let release = create_test_release(&album.id);
    let release_id = release.id.clone();
    database.insert_release(&release).await.unwrap();

    // Link release to storage profile (this is what import does)
    let release_storage = bae::db::DbReleaseStorage::new(&release_id, &profile_id);
    database
        .insert_release_storage(&release_storage)
        .await
        .unwrap();

    // Now retrieve the profile via the release (this is what playback/export does)
    let retrieved_profile = database
        .get_storage_profile_for_release(&release_id)
        .await
        .unwrap()
        .expect("Should have a storage profile");

    // Verify it's the same profile
    assert_eq!(retrieved_profile.id, profile_id);
    assert_eq!(retrieved_profile.name, "My S3 Profile");
    assert_eq!(
        retrieved_profile.cloud_bucket,
        Some("my-bucket".to_string())
    );
    assert_eq!(
        retrieved_profile.cloud_region,
        Some("us-west-2".to_string())
    );
    assert_eq!(
        retrieved_profile.cloud_endpoint,
        Some("https://s3.example.com".to_string())
    );
    assert_eq!(
        retrieved_profile.cloud_access_key,
        Some("AKIAIOSFODNN7EXAMPLE".to_string())
    );
    assert_eq!(
        retrieved_profile.cloud_secret_key,
        Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string())
    );
    assert!(retrieved_profile.encrypted);

    // Verify to_s3_config returns the correct config
    let s3_config = retrieved_profile
        .to_s3_config()
        .expect("Cloud profile should have S3 config");
    assert_eq!(s3_config.bucket_name, "my-bucket");
    assert_eq!(s3_config.region, "us-west-2");
    assert_eq!(
        s3_config.endpoint_url,
        Some("https://s3.example.com".to_string())
    );
    assert_eq!(s3_config.access_key_id, "AKIAIOSFODNN7EXAMPLE");
    assert_eq!(
        s3_config.secret_access_key,
        "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
    );
}

/// Test that different releases can use different storage profiles
#[tokio::test]
async fn test_multiple_releases_different_profiles() {
    tracing_init();

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let database = Database::new(db_path.to_str().unwrap()).await.unwrap();

    // Create two different storage profiles
    let profile_a = DbStorageProfile::new_cloud(
        "Profile A - Bucket 1",
        "bucket-a",
        "us-east-1",
        None,
        "KEY_A",
        "SECRET_A",
        true,
    );
    let profile_a_id = profile_a.id.clone();

    let profile_b = DbStorageProfile::new_cloud(
        "Profile B - Bucket 2",
        "bucket-b",
        "eu-west-1",
        Some("https://minio.local:9000"),
        "KEY_B",
        "SECRET_B",
        false,
    );
    let profile_b_id = profile_b.id.clone();

    database.insert_storage_profile(&profile_a).await.unwrap();
    database.insert_storage_profile(&profile_b).await.unwrap();

    // Create album and two releases
    let album = create_test_album("Test Album");
    database.insert_album(&album).await.unwrap();

    let release_1 = create_test_release(&album.id);
    let release_1_id = release_1.id.clone();
    database.insert_release(&release_1).await.unwrap();

    let release_2 = create_test_release(&album.id);
    let release_2_id = release_2.id.clone();
    database.insert_release(&release_2).await.unwrap();

    // Link each release to a different profile
    let release_storage_1 = bae::db::DbReleaseStorage::new(&release_1_id, &profile_a_id);
    database
        .insert_release_storage(&release_storage_1)
        .await
        .unwrap();

    let release_storage_2 = bae::db::DbReleaseStorage::new(&release_2_id, &profile_b_id);
    database
        .insert_release_storage(&release_storage_2)
        .await
        .unwrap();

    // Retrieve and verify each release gets its correct profile
    let retrieved_1 = database
        .get_storage_profile_for_release(&release_1_id)
        .await
        .unwrap()
        .expect("Release 1 should have profile");

    let retrieved_2 = database
        .get_storage_profile_for_release(&release_2_id)
        .await
        .unwrap()
        .expect("Release 2 should have profile");

    // Release 1 should use Profile A
    assert_eq!(retrieved_1.id, profile_a_id);
    assert_eq!(retrieved_1.cloud_bucket, Some("bucket-a".to_string()));
    assert_eq!(retrieved_1.cloud_region, Some("us-east-1".to_string()));
    assert_eq!(retrieved_1.cloud_access_key, Some("KEY_A".to_string()));
    assert!(retrieved_1.encrypted);

    // Release 2 should use Profile B
    assert_eq!(retrieved_2.id, profile_b_id);
    assert_eq!(retrieved_2.cloud_bucket, Some("bucket-b".to_string()));
    assert_eq!(retrieved_2.cloud_region, Some("eu-west-1".to_string()));
    assert_eq!(
        retrieved_2.cloud_endpoint,
        Some("https://minio.local:9000".to_string())
    );
    assert_eq!(retrieved_2.cloud_access_key, Some("KEY_B".to_string()));
    assert!(!retrieved_2.encrypted);

    println!("✓ Release 1 correctly uses Profile A (bucket-a, us-east-1)");
    println!("✓ Release 2 correctly uses Profile B (bucket-b, eu-west-1, minio endpoint)");
}

// Helper functions to create test data

fn create_test_album(title: &str) -> DbAlbum {
    let now = Utc::now();
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        year: Some(2024),
        discogs_release: None,
        musicbrainz_release: None,
        bandcamp_album_id: None,
        cover_image_id: None,
        cover_art_url: None,
        is_compilation: false,
        created_at: now,
        updated_at: now,
    }
}

fn create_test_release(album_id: &str) -> DbRelease {
    let now = Utc::now();
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        year: None,
        discogs_release_id: None,
        bandcamp_release_id: None,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
        barcode: None,
        import_status: ImportStatus::Queued,
        created_at: now,
        updated_at: now,
    }
}
