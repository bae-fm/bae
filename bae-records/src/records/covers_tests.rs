//! A release's cover names the picture it is made from and keeps two copies
//! of it, each a coven file in a namespace of its own.

use super::*;
use crate::test_fixtures::storage_listing::insert_file;
use crate::test_fixtures::{ALBUM_A, RELEASE_A};
use bae_util::content_type::ContentType;

const IMAGE_FILE: &str = "5b0e2f47-83a1-4d63-9f4e-0c7d6a1b2e93";
const AUDIO_FILE: &str = "9c41d2aa-0f6e-4b1d-8e2a-37f5c0b8d614";

async fn library() -> (bae_store::Store, bae_util::test_files::TestFolder) {
    let (db, folder) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    crate::test_fixtures::album_with_releases(&records, ALBUM_A, "Album", &[RELEASE_A]).await;
    insert_file(&db, &folder, (IMAGE_FILE, RELEASE_A, "scans/front.jpg", 10)).await;
    insert_file(&db, &folder, (AUDIO_FILE, RELEASE_A, "01.flac", 20)).await;
    (db, folder)
}

fn cover(picture: NewCoverPicture, browse: &str, large: &str) -> NewCover {
    NewCover {
        release_id: RELEASE_A.to_string(),
        picture,
        copies: CoverCopies {
            browse: browse.as_bytes().to_vec(),
            large: large.as_bytes().to_vec(),
            content_type: ContentType::Jpeg,
        },
    }
}

async fn read(db: &bae_store::Store, file: Option<coven::FileRef>) -> Option<Vec<u8>> {
    let file = file?;
    let lease = bae_store::lease_one(db.test_sources().files()).await;
    Some(db.read_file(&file, &lease).await.unwrap())
}

#[tokio::test]
async fn a_cover_made_from_a_release_file_names_it_and_keeps_no_original() {
    let (db, _folder) = library().await;
    let records = crate::test_fixtures::test_records(&db);

    records
        .write_cover(cover(
            NewCoverPicture::ReleaseFile { file_id: IMAGE_FILE.to_string() },
            "browse",
            "large",
        ))
        .await
        .unwrap();

    let row = records.cover_source_for_test(RELEASE_A).await.unwrap().expect("the cover row");
    assert_eq!(row, CoverSource::ReleaseFile { file_id: IMAGE_FILE.to_string() });
    let browse = records.cover_copy_file(RELEASE_A, CoverCopy::Browse).await.unwrap();
    let large = records.cover_copy_file(RELEASE_A, CoverCopy::Large).await.unwrap();
    assert_eq!(read(&db, browse).await.unwrap(), b"browse");
    assert_eq!(read(&db, large).await.unwrap(), b"large");
    assert!(records.cover_original_file_for_test(RELEASE_A).await.unwrap().is_none());
}

#[tokio::test]
async fn a_cover_made_from_embedded_art_names_its_audio_file_and_keeps_no_original() {
    let (db, _folder) = library().await;
    let records = crate::test_fixtures::test_records(&db);

    records
        .write_cover(cover(
            NewCoverPicture::Embedded { file_id: AUDIO_FILE.to_string() },
            "browse",
            "large",
        ))
        .await
        .unwrap();

    let row = records.cover_source_for_test(RELEASE_A).await.unwrap().expect("the cover row");
    assert_eq!(row, CoverSource::Embedded { file_id: AUDIO_FILE.to_string() });
    assert!(records.cover_original_file_for_test(RELEASE_A).await.unwrap().is_none());
    let browse = records.cover_copy_file(RELEASE_A, CoverCopy::Browse).await.unwrap();
    assert_eq!(read(&db, browse).await.unwrap(), b"browse");
}

#[tokio::test]
async fn a_cover_with_a_picture_of_its_own_keeps_it_as_its_original() {
    let (db, _folder) = library().await;
    let records = crate::test_fixtures::test_records(&db);

    records
        .write_cover(cover(
            NewCoverPicture::Original { bytes: b"original".to_vec() },
            "browse",
            "large",
        ))
        .await
        .unwrap();

    let row = records.cover_source_for_test(RELEASE_A).await.unwrap().expect("the cover row");
    assert_eq!(row, CoverSource::Original);
    let original = records.cover_original_file_for_test(RELEASE_A).await.unwrap();
    assert_eq!(read(&db, original).await.unwrap(), b"original");
}

#[tokio::test]
async fn a_new_cover_replaces_the_source_and_both_copies() {
    let (db, _folder) = library().await;
    let records = crate::test_fixtures::test_records(&db);
    records
        .write_cover(cover(
            NewCoverPicture::Original { bytes: b"old original".to_vec() },
            "old browse",
            "old large",
        ))
        .await
        .unwrap();
    let version_before = records.cover_version(RELEASE_A).await.unwrap();

    records
        .write_cover(cover(
            NewCoverPicture::ReleaseFile { file_id: IMAGE_FILE.to_string() },
            "new browse",
            "new large",
        ))
        .await
        .unwrap();

    let row = records.cover_source_for_test(RELEASE_A).await.unwrap().expect("the cover row");
    assert_eq!(row, CoverSource::ReleaseFile { file_id: IMAGE_FILE.to_string() });
    assert!(records.cover_original_file_for_test(RELEASE_A).await.unwrap().is_none());
    let browse = records.cover_copy_file(RELEASE_A, CoverCopy::Browse).await.unwrap();
    assert_eq!(read(&db, browse).await.unwrap(), b"new browse");
    assert_ne!(records.cover_version(RELEASE_A).await.unwrap(), version_before);
}

#[tokio::test]
async fn a_release_with_no_cover_has_no_row_and_no_files() {
    let (db, _folder) = library().await;
    let records = crate::test_fixtures::test_records(&db);

    assert!(records.cover_source_for_test(RELEASE_A).await.unwrap().is_none());
    assert!(records.cover_copy_file(RELEASE_A, CoverCopy::Large).await.unwrap().is_none());
    assert!(records.cover_original_file_for_test(RELEASE_A).await.unwrap().is_none());
}
