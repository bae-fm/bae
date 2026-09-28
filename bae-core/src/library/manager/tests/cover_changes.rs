// Changing a release's cover: what the stored thumbnail becomes, and where
// a replacement is written.

/// `change_cover` resizes whatever the user picks to a ≤600 JPEG thumbnail
/// before storing it: a 900×300 PNG release image lands as a 600×200 JPEG blob
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

async fn assert_changed_cover(
    filename: &str,
    cover_bytes: &[u8],
    content_type: ContentType,
    dimensions: (u32, u32),
) {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
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
        .read_cover_image_blob(&release.id)
        .await
        .unwrap()
        .expect("cover blob stored");
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

/// A cover can be changed again and again. coven's `(namespace, blob id)` names one
/// immutable byte-string — a blob's bytes are never rewritten under a live id — so
/// each change mints a NEW `blob_id`, repoints the `covers` row at it, and records
/// deletion of the blob it replaced. The row's hash and size describe the newly
/// stored bytes, and retained replay inputs own the old bytes until baseline
/// adoption.
#[tokio::test]
async fn change_cover_twice_replaces_the_cover_blob() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
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

    change_to(&green).await.unwrap();
    let first = cover_row().await;

    // The second change is the one that used to fail: it re-put a blob under an id
    // the `covers` row already referenced.
    change_to(&red).await.unwrap();
    let second = cover_row().await;

    // The row moved to a new blob, and describes the bytes that blob holds.
    assert_ne!(
        second.blob_id, first.blob_id,
        "a replaced cover is a new blob, not new bytes under the old id"
    );
    let stored = manager
        .read_cover_image_blob(&release.id)
        .await
        .unwrap()
        .expect("cover blob stored");
    assert_eq!(
        second.content_hash.as_str(),
        crate::util::fs::hash_bytes(&stored).as_str()
    );
    assert_eq!(second.file_size, stored.len() as i64);

    // The bytes really are the second image's, not the first's.
    let first_stored_len = first.file_size;
    assert_ne!(
        stored.len() as i64,
        first_stored_len,
        "the two source images must produce different thumbnails for this test to mean anything"
    );

    // The row now points at the new blob. The old insert and replacement remain
    // local replay inputs until baseline adoption, so coven keeps the old bytes
    // under their replay lease while the live row exposes only the new blob.
    assert!(
        manager
            .local_blob_exists_for_test(crate::sync::COVERS_NAMESPACE, &first.blob_id)
            .expect("a valid blob path"),
        "the replaced cover blob's replay input must remain readable"
    );
    assert_eq!(
        manager
            .local_blob_cleanup_intent_count_for_test(
                crate::sync::COVERS_NAMESPACE,
                &first.blob_id,
            )
            .await
            .unwrap(),
        1,
        "the replaced cover records eventual blob cleanup"
    );
}

/// On a browsable home, replacements have distinct readable paths and exact
/// stored objects. Reusing an object's key would let a replacement overwrite
/// bytes still referenced by an earlier change.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn replacing_a_cover_on_a_browsable_home_writes_a_distinct_cloud_key() {
    let (manager, _temp_dir) = setup_browsable_test_manager().await;
    manager
        .connect_test_cloud_home(Arc::new(InMemoryCloudHome::new()), CloudCipher::Plaintext)
        .await
        .expect("connect browsable in-memory cloud home");
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;

    let jpeg = |rgb: [u8; 3]| {
        let img = ::image::RgbImage::from_pixel(400, 400, ::image::Rgb(rgb));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ::image::ImageFormat::Png)
            .unwrap();
        crate::util::cover::resize_cover(&buf.into_inner()).unwrap()
    };
    let store_cover = async |bytes: Vec<u8>| {
        let mut image = DbLibraryImage::cover(
            &release.id,
            &Uuid::new_v4().to_string(),
            "local",
            None,
            &bytes,
            manager.clock.now(),
        );
        image.cloud_path = manager
            .database
            .cover_cloud_path_for_storage(
                crate::config::HomeStorage::Browsable,
                &image.id,
                &image.blob_id,
                &image.content_type,
            )
            .await
            .unwrap();
        manager
            .store_library_image_blob(&image, &bytes)
            .await
            .unwrap();
        wait_for_published_blob(&manager, crate::sync::COVERS_NAMESPACE, &release.id).await;
        let stored = manager
            .database
            .row_blob_ref(crate::sync::COVERS_NAMESPACE, &release.id)
            .await
            .unwrap();
        let image = manager
            .get_library_image(&release.id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row stored");
        (image, stored)
    };

    let (first, first_stored) = store_cover(jpeg([20, 160, 90])).await;
    let (second, second_stored) = store_cover(jpeg([200, 40, 40])).await;

    // Each row's readable path names its own blob, so the two never collide.
    assert_eq!(
        first.cloud_path.as_deref(),
        Some(format!("{}/{}/cover-{}.jpg", album.id, release.id, first.blob_id).as_str())
    );
    assert_ne!(
        first.cloud_path, second.cloud_path,
        "a replaced cover must not reuse the object its predecessor occupies"
    );

    // The two keys really are distinct objects, so writing the second never
    // overwrites the first.
    let old_stored = first_stored
        .stored()
        .expect("first cover reached the cloud");
    let new_stored = second_stored
        .stored()
        .expect("second cover reached the cloud");
    assert_ne!(
        old_stored.object().slot().logical_key(),
        new_stored.object().slot().logical_key()
    );
    assert_ne!(old_stored, new_stored);
}
