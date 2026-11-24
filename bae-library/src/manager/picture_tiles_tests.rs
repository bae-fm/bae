//! A release's pictures cut into the tiles the viewer zooms them from.

use super::*;
use crate::manager::tests::*;
use bae_records::NewReleaseFile;
use bae_util::content_type::ContentType;
use bae_util::test_files::TestFolder;
use time::UtcDateTime;

/// A PNG of `width` x `height` pixels of one colour.
fn png_of(width: u32, height: u32) -> Vec<u8> {
    let image = ::image::RgbImage::from_pixel(width, height, ::image::Rgb([20, 160, 90]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    ::image::DynamicImage::ImageRgb8(image)
        .write_to(&mut bytes, ::image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

/// An image file of a release is cut from the file as the release holds it,
/// and its tiles read back without reading the file again.
#[tokio::test(flavor = "multi_thread")]
async fn a_release_s_image_file_cuts_into_tiles() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    let source_dir = TestFolder::new();
    let source_path = source_dir.path().join("scan.png");
    source_dir.write(&source_path, png_of(1200, 700));
    let file = NewReleaseFile::new(
        &release.id,
        "scan.png",
        ContentType::Png,
        bae_records::ReleaseFileRole::Artwork,
        coven::IdSource::new_id(&coven::UuidIds).to_string(),
        UtcDateTime::now(),
    );
    manager.add_external_file_for_test(&file, &source_path).await.unwrap();
    let slot = GallerySource::ReleaseFile { file_id: file.id.clone() };

    let pyramid = manager.gallery_tiles(&release.id, &slot).await.unwrap();
    let tile = manager.gallery_tile(&release.id, &slot, 0, 2, 1).await.unwrap();

    assert_eq!((pyramid.width, pyramid.height), (1200, 700));
    assert_eq!(pyramid.levels().len(), 3);
    let decoded = ::image::load_from_memory(&tile).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (1200 - 1024, 700 - 512));
}

/// A slot that names a file of another release is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_of_no_such_release_has_no_tiles() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let slot = GallerySource::ReleaseFile { file_id: "no-such-file".to_string() };
    let result = manager.gallery_tiles("no-such-release", &slot).await;
    assert!(matches!(result, Err(LibraryError::Tiles(_))), "{result:?}");
}
