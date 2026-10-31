use super::*;

/// A cover reaches another device together with its stored file. The cover's
/// reference is bound, so the write naming it waits for its upload, and the
/// upload isn't held by the pause that holds the release's audio: another
/// device never sees the cover's row naming a file it can't read.
#[test]
fn a_cover_reaches_another_device_with_its_stored_file() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        let manager = devices.a().manager();
        let album = create_test_album();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        insert_release(manager, &release).await;
        devices.converge("SELECT id FROM releases").await;

        manager.database.set_uploads_paused(true).await.unwrap();
        store_test_cover(manager, &release.id, b"the cover").await;
        let cover = cover_file(manager, &release.id).await;
        devices
            .converge(
                "SELECT c.id, c.file, hex(f.hash), f.coordinates FROM covers AS c \
                 LEFT JOIN coven_files AS f ON f.id = c.file",
            )
            .await;

        let other = devices.b().manager();
        let arrived = cover_file(other, &release.id).await;
        assert_eq!(arrived.id(), cover.id());
        assert!(
            matches!(arrived.place(), coven::FilePlace::Cloud { .. }),
            "the cover's row arrives with its file stored: {:?}",
            arrived.place()
        );
        assert_eq!(
            other
                .read_cover_image_bytes(&release.id)
                .await
                .unwrap()
                .expect("the cover's bytes"),
            b"the cover"
        );
    });
}

/// An image larger than a bound reference's limit is refused when its write
/// commits: the row keeps the image it had, and coven's error names the
/// table, the column, the file's size and the limit.
#[tokio::test]
async fn an_image_over_the_bound_limit_is_refused() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(&manager, &release).await;
    store_test_cover(&manager, &release.id, b"the first cover").await;
    let first = cover_file(&manager, &release.id).await;

    let oversized = vec![0u8; usize::try_from(coven::BOUND_FILE_LIMIT).unwrap() + 1];
    let cover = DbLibraryImage::cover(&release.id, "local", None, manager.clock.now().into());
    let error = manager
        .store_library_image(&cover, &oversized)
        .await
        .expect_err("an image over the limit makes no cover");
    match &error {
        LibraryError::Database(crate::db::DbError::Coven(coven::CovenError::Database(
            coven::DbError::BoundFileTooLarge {
                table,
                column,
                size,
                limit,
            },
        ))) => {
            assert_eq!((table.as_str(), column.as_str()), ("covers", "file"));
            assert_eq!(*size, coven::BOUND_FILE_LIMIT + 1);
            assert_eq!(*limit, coven::BOUND_FILE_LIMIT);
        }
        other => panic!("expected BoundFileTooLarge, got {other:?}"),
    }
    assert_eq!(cover_file(&manager, &release.id).await.id(), first.id());
}

/// Every image bae stores fits a bound reference: noise at the largest size
/// bae stores, 600 pixels square, is what JPEG compresses least, and its JPEG
/// stays under the limit.
#[test]
fn the_largest_image_bae_stores_fits_a_bound_reference() {
    let edge = bae_util::stored_image::STORED_MAX_EDGE;
    let mut state: u32 = 0x9e37_79b9;
    let noise = ::image::RgbImage::from_fn(edge, edge, |_, _| {
        let mut channel = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        };
        ::image::Rgb([channel(), channel(), channel()])
    });
    let mut png = std::io::Cursor::new(Vec::new());
    ::image::DynamicImage::ImageRgb8(noise)
        .write_to(&mut png, ::image::ImageFormat::Png)
        .unwrap();
    let stored = bae_util::stored_image::stored_jpeg(png.get_ref()).unwrap();
    let size = u64::try_from(stored.len()).unwrap();
    assert!(
        size <= coven::BOUND_FILE_LIMIT,
        "a stored image of {size} bytes is over the bound limit of {}",
        coven::BOUND_FILE_LIMIT
    );
}
