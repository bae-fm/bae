//! Albums and releases a test seeds a test library with: rows written
//! straight to its store, and local releases whose files sit in a folder the
//! test made, imported as an import leaves them.

use super::TestLibrary;
use bae_records::{DbAlbum, DbRelease, NewReleaseFile};
use chrono::Utc;

/// An album with a fresh id, titled "Test Album".
pub fn test_album() -> DbAlbum {
    DbAlbum { year: Some(2024), ..DbAlbum::new_test("Test Album", &coven::UuidIds) }
}

/// A release of `album_id` with a fresh id and nothing on it.
pub fn test_release(album_id: &str) -> DbRelease {
    DbRelease {
        pressing: bae_types::pressing::Pressing {
            year: Some(2024),
            ..bae_types::pressing::Pressing::blank()
        },
        ..DbRelease::new_test(album_id, &coven::IdSource::new_id(&coven::UuidIds).to_string())
    }
}

/// Write `release` and its one medium.
pub async fn insert_release(records: &bae_records::Records, release: &DbRelease) {
    records.insert_release(release).await.unwrap();
    records
        .insert_release_medium(&bae_records::DbReleaseMedium::new_test(&release.id))
        .await
        .unwrap();
}

/// A local release on an album titled `album_title`, its `files` written
/// under `dir` and recorded as this device's originals.
pub async fn insert_local_release_with_files(
    library: &TestLibrary,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let mut album = test_album();
    album.title = album_title.to_string();
    library.records().insert_album(&album).await.unwrap();
    let release = test_release(&album.id);
    insert_release(library.records(), &release).await;
    std::fs::create_dir_all(dir).unwrap();
    let created_at = Utc::now();
    for (index, (name, bytes)) in files.iter().enumerate() {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let file = NewReleaseFile::new(
            &release.id,
            name,
            bae_util::content_type::ContentType::Flac,
            bae_records::ReleaseFileRole::Other,
            bae_util::test_ids::test_uuid(&format!("{}-test-file-{index}", release.id)),
            created_at,
        );
        library.manager().add_external_file_for_test(&file, &path).await.unwrap();
    }
    release
}

/// [`insert_local_release_with_files`] moved to the cloud, its files queued
/// in coven and not uploaded: the library must have storage set up, its
/// uploads paused.
pub async fn insert_release_with_queued_uploads(
    library: &TestLibrary,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
) -> DbRelease {
    let release = insert_local_release_with_files(library, dir, album_title, files).await;
    library.manager().upload_release(&release.id).await.unwrap();
    assert_eq!(
        library.records().queued_upload_count_for_release_for_test(&release.id).await.unwrap(),
        files.len(),
        "every file is queued"
    );
    release
}
