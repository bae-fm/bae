//! Changing a release's cover: the picked image stored as a JPEG thumbnail, an
//! undecodable pick refused, and a replaced cover's file going with its last
//! reference.

use super::*;
use crate::manager::tests::*;
use bae_records::{LibraryImageType, NewReleaseFile};
use bae_util::content_type::ContentType;
use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

/// `change_cover` resizes whatever the user picks to a ≤600 JPEG thumbnail
/// before storing it: a 900×300 PNG release image lands as a 600×200 JPEG file
/// (downscaled to fit 600, aspect kept), and the `covers` row records JPEG.
#[tokio::test]
async fn change_cover_stores_a_resized_jpeg_thumbnail() {
    let image = ::image::RgbImage::from_pixel(900, 300, ::image::Rgb([20, 160, 90]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    ::image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, ::image::ImageFormat::Png)
        .unwrap();
    assert_changed_cover("art.png", &bytes.into_inner(), ContentType::Png, (600, 200)).await;
}

#[tokio::test]
async fn change_cover_stores_gif_and_webp_as_jpeg_without_replacing_source() {
    for (name, bytes, content_type, dimensions) in [
        ("solid.gif", bae_util::test_cover_art::SOLID_GIF, ContentType::Gif, (16, 8)),
        ("solid.webp", bae_util::test_cover_art::SOLID_WEBP, ContentType::Webp, (16, 8)),
        ("animated.gif", bae_util::test_cover_art::ANIMATED_GIF, ContentType::Gif, (600, 300)),
        ("animated.webp", bae_util::test_cover_art::ANIMATED_WEBP, ContentType::Webp, (600, 300)),
    ] {
        assert_changed_cover(name, bytes, content_type, dimensions).await;
    }
}

/// A picked image that doesn't decode is refused as artwork that couldn't be
/// read, the class the import gives the same failure, and stores nothing.
#[tokio::test]
async fn an_undecodable_cover_pick_is_refused_as_unreadable_artwork() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    let source_dir = TempDir::new().unwrap();
    let source_path = source_dir.path().join("broken.png");
    let bytes = b"not an image".as_slice();
    std::fs::write(&source_path, bytes).unwrap();
    let file = NewReleaseFile::new(
        &release.id,
        "broken.png",
        ContentType::Png,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_external_file_for_test(&file, &source_path).await.unwrap();

    let error = manager
        .change_cover(&release.id, CoverSelection::ReleaseImage { file_id: file.id })
        .await
        .expect_err("bytes that are no image make no cover");
    assert_eq!(error.category(), bae_diagnostics::UiErrorCategory::ImportData, "{error}");
    assert!(manager
        .records
        .find_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());
}

async fn assert_changed_cover(
    filename: &str,
    cover_bytes: &[u8],
    content_type: ContentType,
    dimensions: (u32, u32),
) {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;

    let source_dir = TempDir::new().unwrap();
    let source_path = source_dir.path().join(filename);
    std::fs::write(&source_path, cover_bytes).unwrap();
    let file = NewReleaseFile::new(
        &release.id,
        filename,
        content_type,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_external_file_for_test(&file, &source_path).await.unwrap();

    manager
        .change_cover(&release.id, CoverSelection::ReleaseImage { file_id: file.id.clone() })
        .await
        .unwrap();

    // The cover is a normalized JPEG; the release file retains its original bytes.
    let stored =
        manager.read_cover_image_bytes(&release.id).await.unwrap().expect("cover file stored");
    assert_eq!(::image::guess_format(&stored).unwrap(), ::image::ImageFormat::Jpeg);
    let decoded = ::image::load_from_memory(&stored).unwrap();
    assert_eq!((decoded.width(), decoded.height()), dimensions);
    assert_eq!(std::fs::read(&source_path).unwrap(), cover_bytes);

    // The row describes the stored thumbnail, a JPEG, and its coven file
    // holds the thumbnail's bytes.
    let row = manager
        .records
        .find_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("cover row stored");
    assert_eq!(row.content_type, ContentType::Jpeg);
    assert_eq!(cover_file(manager, &release.id).await.size(), stored.len() as u64);
}

/// A cover can be changed again and again. A coven file never changes its
/// bytes: each change stages a new file the same `covers` row references,
/// which moves the cover's version. The replaced file goes with its last
/// reference: still on this device only, a reference taken to it before
/// reads nothing.
#[tokio::test]
async fn change_cover_twice_replaces_the_cover_file() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;

    // Two visibly different release images, so the two stored thumbnails differ.
    let source_dir = TempDir::new().unwrap();
    let png = |rgb: [u8; 3]| {
        let img = ::image::RgbImage::from_pixel(400, 400, ::image::Rgb(rgb));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ::image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    let add_source = |name: &str, bytes: &[u8]| {
        std::fs::write(source_dir.path().join(name), bytes).unwrap();
        NewReleaseFile::new(
            &release.id,
            name,
            ContentType::Png,
            Uuid::new_v4().to_string(),
            Utc::now(),
        )
    };
    let green = add_source("green.png", &png([20, 160, 90]));
    let red = add_source("red.png", &png([200, 40, 40]));
    manager.add_external_file_for_test(&green, &source_dir.path().join("green.png")).await.unwrap();
    manager.add_external_file_for_test(&red, &source_dir.path().join("red.png")).await.unwrap();

    let change_to = async |file: &NewReleaseFile| {
        manager
            .change_cover(&release.id, CoverSelection::ReleaseImage { file_id: file.id.clone() })
            .await
    };
    let cover_row = async || {
        manager
            .records
            .find_library_image(&release.id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row stored")
    };

    let cover_version =
        async || manager.cover_ref(&release.id).await.unwrap().expect("cover stored").version;

    change_to(&green).await.unwrap();
    cover_row().await;
    let first_version = cover_version().await;
    let first_ref = cover_file(manager, &release.id).await;

    // The second change points the row that already has a cover at a new
    // file.
    change_to(&red).await.unwrap();
    cover_row().await;

    // The row names a new file, which holds the second image's bytes.
    let second_ref = cover_file(manager, &release.id).await;
    assert_ne!(
        second_ref.id(),
        first_ref.id(),
        "a replaced cover is a new file, not new bytes under the old one"
    );
    assert_ne!(cover_version().await, first_version, "the cover's version moves");
    let stored =
        manager.read_cover_image_bytes(&release.id).await.unwrap().expect("cover file stored");
    assert_eq!(second_ref.size(), stored.len() as u64);

    // The bytes really are the second image's, not the first's.
    assert_ne!(
        stored.len() as u64,
        first_ref.size(),
        "the two source images must produce different thumbnails for this test to mean anything"
    );

    assert!(matches!(
        manager.store.read_file(&first_ref).await,
        Err(coven::FileReadError::Database(coven::DbError::FileAbsent { .. }))
    ));
}

/// Replacing an uploaded cover deletes its coven file's record with its last
/// reference, and the row's new file reads the new bytes.
#[tokio::test]
async fn replacing_an_uploaded_cover_deletes_its_file_record() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    store_test_cover(manager, &release.id, b"first cover").await;
    finish_bound_uploads(manager).await;
    let first = cover_file(manager, &release.id).await;
    assert!(matches!(first.place(), coven::FilePlace::Cloud { .. }), "{:?}", first.place());
    store_test_cover(manager, &release.id, b"second cover").await;
    finish_bound_uploads(manager).await;
    let second = cover_file(manager, &release.id).await;
    assert!(matches!(second.place(), coven::FilePlace::Cloud { .. }), "{:?}", second.place());
    assert_ne!(first.id(), second.id());
    assert!(matches!(
        manager.store.file_ref(first.id()).await,
        Err(bae_store::DbError::Coven(error))
            if matches!(*error, coven::CovenError::Database(coven::DbError::FileAbsent { .. }))
    ));
    assert_eq!(manager.store.read_file(&second).await.unwrap(), b"second cover");
}
