// Which of a pinning release's files coven is downloading now, and how far
// each has got, as the release's read and the pin file watch tell them.
//
// `include!`d into `manager::tests`.

/// While a pin downloads a file, the release reads it as downloading, by the
/// file coven fetches, and the pin file watch names that file; once kept, it
/// reads as kept and downloads no more.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_file_a_pin_downloads_reads_as_downloading() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release_id = uploaded_release(
        &manager,
        &temp_dir.path().join("pin-files"),
        "Pin Files",
        false,
    )
    .await;
    let file = release_files(&manager, &release_id).await.remove(0);
    manager
        .evict_file_for_test(&manager.release_file_ref(&file.id).await.unwrap())
        .await
        .unwrap();
    let mut files = manager.watch_pin_file_transfers();
    assert_eq!(
        files.next().await.unwrap(),
        vec![crate::library::FileTransferChange::Reset(Default::default())],
        "nothing downloads before the pin"
    );

    // The memory provider holds each request's answer.
    home.set_faults(coven::Faults {
        delay: std::time::Duration::from_secs(2),
        ..coven::Faults::none()
    })
    .await;
    let pin = tokio::spawn({
        let manager = manager.library.clone();
        let release_id = release_id.clone();
        async move {
            manager
                .pin_release_files_with_progress(&release_id, |_| {})
                .await
        }
    });
    let fetched = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            for change in files.next().await.unwrap() {
                if let crate::library::FileTransferChange::Updated { file, .. } = change {
                    return file;
                }
            }
        }
    })
    .await
    .expect("the pin file watch names the file the pin downloads");
    let pin_of = |rows: crate::db::DbStorageInspector| {
        rows.files
            .into_iter()
            .find(|row| row.file.id == file.id)
            .expect("the release's file")
            .pin
    };
    assert_eq!(
        pin_of(inspector_rows_until(&manager, &release_id, |_| true).await),
        Some(crate::album_detail::FilePin::Downloading { file: fetched }),
        "the file reads as downloading by the file coven fetches"
    );

    pin.await.unwrap().unwrap();
    assert_eq!(
        pin_of(inspector_rows_until(&manager, &release_id, |_| true).await),
        Some(crate::album_detail::FilePin::Kept),
        "a kept file downloads no more"
    );
}

/// A pin's bytes never fall back as a file lands, even while its pins query
/// lags behind coven's download watch: the file counts its share while it
/// downloads and its whole size once kept, never nothing in between.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn a_pins_bytes_never_fall_back_as_a_file_lands() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release_id = uploaded_release(
        &manager,
        &temp_dir.path().join("pin-forward"),
        "Pin Forward",
        false,
    )
    .await;
    let file = release_files(&manager, &release_id).await.remove(0);
    let size = u64::try_from(file.file_size).unwrap();
    let file = manager.release_file_ref(&file.id).await.unwrap();
    manager.evict_file_for_test(&file).await.unwrap();
    let coven::FilePlace::Cloud { location, .. } = file.place() else {
        panic!("the uploaded file is stored");
    };

    // Hold the download one byte into the file's encrypted chunk, past its
    // 11-byte header.
    let (reached, observed) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_body_at(
        coven::ObjectPath::file(
            manager.database.store_id_for_test(),
            location.uploader(),
            location.object(),
        ),
        12,
        reached,
        resumed,
    )
    .await;
    let (bytes_tx, mut bytes) = tokio::sync::mpsc::unbounded_channel();
    let pin = tokio::spawn({
        let manager = manager.library.clone();
        let release_id = release_id.clone();
        async move {
            manager
                .pin_release_files_with_progress(&release_id, move |progress| {
                    bytes_tx.send(progress.bytes_done).unwrap();
                })
                .await
        }
    });
    observed.await.unwrap();
    let mut seen = Vec::new();
    while seen.last().is_none_or(|done| *done == 0) {
        seen.push(
            bytes
                .recv()
                .await
                .expect("the pin counts the held download's share"),
        );
    }

    // From here the pins query runs no more until the pin ends, so it still
    // says the file downloads after coven has kept it and ended the download.
    let reads = manager.pin_reads_hold.hold();
    resume.send(()).unwrap();
    pin.await.unwrap().unwrap();
    drop(reads);
    while let Ok(done) = bytes.try_recv() {
        seen.push(done);
    }
    assert!(
        seen.windows(2).all(|pair| pair[0] <= pair[1]),
        "the bytes never fall back: {seen:?}"
    );
    assert_eq!(seen.last(), Some(&size), "the pin ends holding the file");
}
