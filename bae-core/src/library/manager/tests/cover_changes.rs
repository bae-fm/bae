// Changing a release's cover: what the stored thumbnail becomes, and where
// a replacement is written.

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
        (
            "solid.gif",
            include_bytes!("../../../../test-fixtures/cover-art/solid.gif").as_slice(),
            ContentType::Gif,
            (16, 8),
        ),
        (
            "solid.webp",
            include_bytes!("../../../../test-fixtures/cover-art/solid.webp").as_slice(),
            ContentType::Webp,
            (16, 8),
        ),
        (
            "animated.gif",
            include_bytes!("../../../../test-fixtures/cover-art/animated.gif").as_slice(),
            ContentType::Gif,
            (600, 300),
        ),
        (
            "animated.webp",
            include_bytes!("../../../../test-fixtures/cover-art/animated.webp").as_slice(),
            ContentType::Webp,
            (600, 300),
        ),
    ] {
        assert_changed_cover(name, bytes, content_type, dimensions).await;
    }
}

/// A picked image that doesn't decode is refused as artwork that couldn't be
/// read, the class the import gives the same failure, and stores nothing.
#[tokio::test]
async fn an_undecodable_cover_pick_is_refused_as_unreadable_artwork() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    let source_dir = TempDir::new().unwrap();
    let source_path = source_dir.path().join("broken.png");
    let bytes = b"not an image".as_slice();
    std::fs::write(&source_path, bytes).unwrap();
    let file = DbFile::new(
        &release.id,
        "broken.png",
        bytes.len() as i64,
        ContentType::Png,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager
        .add_external_file_for_test(&file, &source_path)
        .await
        .unwrap();

    let error = manager
        .change_cover(&release.id, CoverSelection::ReleaseImage { file_id: file.id })
        .await
        .expect_err("bytes that are no image make no cover");
    assert_eq!(error.category(), crate::ui::UiErrorCategory::ImportData, "{error}");
    assert!(manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
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
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;

    let source_dir = TempDir::new().unwrap();
    let source_path = source_dir.path().join(filename);
    std::fs::write(&source_path, cover_bytes).unwrap();
    let file = DbFile::new(
        &release.id,
        filename,
        cover_bytes.len() as i64,
        content_type,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager
        .add_external_file_for_test(&file, &source_path)
        .await
        .unwrap();

    manager
        .change_cover(
            &release.id,
            CoverSelection::ReleaseImage {
                file_id: file.id.clone(),
            },
        )
        .await
        .unwrap();

    // The cover is a normalized JPEG; the release file retains its original bytes.
    let stored = manager
        .read_cover_image_bytes(&release.id)
        .await
        .unwrap()
        .expect("cover file stored");
    assert_eq!(
        ::image::guess_format(&stored).unwrap(),
        ::image::ImageFormat::Jpeg
    );
    let decoded = ::image::load_from_memory(&stored).unwrap();
    assert_eq!((decoded.width(), decoded.height()), dimensions);
    assert_eq!(std::fs::read(&source_path).unwrap(), cover_bytes);

    // The row describes the stored thumbnail: JPEG, and its size matches.
    let row = manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("cover row stored");
    assert_eq!(row.content_type, ContentType::Jpeg);
    assert_eq!(row.file_size, stored.len() as i64);
}

/// A cover can be changed again and again. coven never rewrites stored bytes:
/// each change attaches a new file to the same `covers` row, which moves the
/// cover's version. Captured references to the replaced file must be refused.
#[tokio::test]
async fn change_cover_twice_replaces_the_cover_file() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;

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
        DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            ContentType::Png,
            Uuid::new_v4().to_string(),
            Utc::now(),
        )
    };
    let green = add_source("green.png", &png([20, 160, 90]));
    let red = add_source("red.png", &png([200, 40, 40]));
    manager
        .add_external_file_for_test(&green, &source_dir.path().join("green.png"))
        .await
        .unwrap();
    manager
        .add_external_file_for_test(&red, &source_dir.path().join("red.png"))
        .await
        .unwrap();

    let change_to = async |file: &DbFile| {
        manager
            .change_cover(
                &release.id,
                CoverSelection::ReleaseImage {
                    file_id: file.id.clone(),
                },
            )
            .await
    };
    let cover_row = async || {
        manager
            .get_library_image(&release.id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row stored")
    };

    let cover_version = async || {
        manager
            .cover_ref(&release.id)
            .await
            .unwrap()
            .expect("cover stored")
            .version
    };

    change_to(&green).await.unwrap();
    let first = cover_row().await;
    let first_version = cover_version().await;
    let first_ref = manager.database.file_ref("covers", &release.id).await.unwrap();

    // The second change attaches a new file to the row that already has one.
    change_to(&red).await.unwrap();
    let second = cover_row().await;
    assert!(
        manager.database.abandoned_attachments_for_test().await.unwrap().is_empty(),
        "replacing a cover still uploading is the person's own change and lists nothing"
    );

    // The row names a new file, and describes the bytes that file holds.
    let second_ref = manager.database.file_ref("covers", &release.id).await.unwrap();
    assert_ne!(
        second_ref.id(),
        first_ref.id(),
        "a replaced cover is a new file, not new bytes under the old one"
    );
    assert_ne!(cover_version().await, first_version, "the cover's version moves");
    let stored = manager
        .read_cover_image_bytes(&release.id)
        .await
        .unwrap()
        .expect("cover file stored");
    assert_eq!(
        manager.database.file_ref("covers", &release.id).await.unwrap().content_hash().as_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>().as_str(),
        bae_util::fs::hash_bytes(&stored).as_str()
    );
    assert_eq!(second_ref.plaintext_size(), stored.len() as u64);
    // The row describes the file every device sees: the first, until the
    // second's upload completes.
    assert_eq!(second.file_size, first.file_size);

    // The bytes really are the second image's, not the first's.
    let first_stored_len = first.file_size;
    assert_ne!(
        stored.len() as i64,
        first_stored_len,
        "the two source images must produce different thumbnails for this test to mean anything"
    );

    assert!(matches!(manager.database.read_file(&first_ref).await,
        Err(coven::FileReadError::Database(coven::DbError::FileRefChanged { .. }))));
}

#[tokio::test]
async fn replacing_an_uploaded_cover_refuses_its_captured_reference() {
    let (manager, _temp) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    store_test_cover(&manager, &release.id, b"first cover").await;
    let first = manager.database.file_ref("covers", &release.id).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    store_test_cover(&manager, &release.id, b"second cover").await;
    let second = manager.database.file_ref("covers", &release.id).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    assert_ne!(first.id(), second.id());
    assert!(matches!(manager.database.read_file(&first).await,
        Err(coven::FileReadError::Database(coven::DbError::FileRefChanged { .. }))));
    assert_eq!(manager.database.read_file(&second).await.unwrap(), b"second cover");
}
