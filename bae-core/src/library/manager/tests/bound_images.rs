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

/// When a schema raise falls between a cover's write and the end of its
/// upload, the write and the raise's marker go out in one slot, then the
/// raise, then the write completing the upload. A device still on the old
/// app stops at that slot. Once its app updates it loads the raise, whose
/// cover row names a file on its uploader only, which it can't read; the
/// cover's version names that device. When the completion arrives the
/// version becomes the file's alone, so an app that failed to read the cover
/// asks for it again, and the cover reads.
#[test]
fn a_cover_whose_row_arrives_before_its_file_moves_its_version_when_the_file_lands() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        let manager = devices.a().manager();
        let album = create_test_album();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        insert_release(manager, &release).await;
        devices.converge("SELECT id FROM releases").await;

        // While the first device's storage is out of reach the cover's
        // upload can't finish, so its write is held, and the update below
        // puts its raise's marker after it.
        devices.a().storage().set_online(false);
        store_test_cover(devices.a().manager(), &release.id, b"the cover").await;
        let cover = cover_file(devices.a().manager(), &release.id).await.id();
        let uploader = devices.a().manager().device_id();
        let mut raised = crate::migrations::all();
        raised.push(coven::Migration::breaking(
            2,
            "index works by title",
            "CREATE INDEX works_by_title ON works(title)",
            coven::OldWrites::Drop,
        ));
        devices.a_mut().reopen_with(raised.clone()).await;
        devices.a().storage().set_online(true);
        devices.a().manager().network_changed();

        let b = devices.b();
        b.wait_until("the second device stopping for the update", || async {
            matches!(
                *b.database().subscribe_sync_status().borrow(),
                coven::SyncStatus::Blocked {
                    reason: coven::PendingReason::UpdateRequired(
                        coven::RequiredUpdate::AppSchema { version: 2 }
                    ),
                    ..
                }
            )
        })
        .await;
        assert_eq!(
            b.database().cover_version(&release.id).await.unwrap(),
            None,
            "the old app shows no cover row past the slot it stops at"
        );

        // The first device's pass that sent the slot publishes the raise
        // after it, and the completion waits for a later pass. Stopping its
        // sync once that pass has ended keeps the completion from going out
        // before the second device has loaded the raise.
        let a = devices.a();
        a.stop_sync().await;
        assert!(
            a.database()
                .queued_write_rows_for_test()
                .await
                .unwrap()
                .iter()
                .any(|write| write.iter().any(|table| table == "coven_files")),
            "the write completing the cover's upload is still queued"
        );

        devices.b_mut().reopen_with(raised).await;
        let b = devices.b();
        b.wait_until("the cover's row on the second device", || async {
            b.database()
                .cover_version(&release.id)
                .await
                .unwrap()
                .is_some()
        })
        .await;
        let early = cover_file(b.manager(), &release.id).await;
        assert_eq!(early.id(), cover);
        assert_eq!(
            early.place(),
            coven::FilePlace::DeviceOnly { device: uploader },
            "the row arrives naming a file on its uploader only"
        );
        assert_eq!(
            b.database().cover_version(&release.id).await.unwrap(),
            Some(format!(
                "{}@{}",
                cover.0,
                hex::encode(uploader.0.to_be_bytes())
            ))
        );
        assert!(
            b.manager()
                .read_cover_image_bytes(&release.id)
                .await
                .is_err(),
            "the cover can't be read while its file is on its uploader only"
        );

        devices.a().database().start_sync().await.unwrap();
        let b = devices.b();
        b.wait_until("the cover's file on the second device", || async {
            matches!(
                cover_file(b.manager(), &release.id).await.place(),
                coven::FilePlace::Cloud { .. }
            )
        })
        .await;
        assert_eq!(
            b.database().cover_version(&release.id).await.unwrap(),
            Some(cover.0.to_string()),
            "the version moves to the file's alone as the file lands"
        );
        assert_eq!(
            b.manager()
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
