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
