use super::*;

#[test]
fn a_cover_another_device_has_not_uploaded_arrives_quietly_until_it_lands() {
    crate::sync::test_devices::run_two_device_test(check_cover_arrival);
}

/// Run passes on both devices until B's sync status satisfies `condition`,
/// or fail with the last status B published.
async fn wait_for_status(
    devices: &crate::sync::test_devices::TwoDevices,
    what: &str,
    condition: impl Fn(&SyncStatusSnapshot) -> bool,
) -> SyncStatusSnapshot {
    let manager = devices.b().manager();
    let mut values = manager.subscribe_sync_status_values();
    let waited = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let current = values.borrow_and_update().clone();
            if condition(&current) {
                return current;
            }
            devices.a().database().sync_now();
            devices.b().database().sync_now();
            // A pass changes the status; a download landing changes only the
            // pending records, so look again after a while either way.
            let _ =
                tokio::time::timeout(std::time::Duration::from_millis(200), values.changed()).await;
        }
    })
    .await;
    match waited {
        Ok(snapshot) => snapshot,
        Err(_) => panic!(
            "{what} never happened; the last status was {:?}",
            manager.subscribe_sync_status_values().borrow().clone()
        ),
    }
}

/// Device A attaches a release and its cover while its uploads are paused.
/// Device B receives the rows and tries to fetch the cover, which storage
/// doesn't hold yet: the cover arrives from A, named by A's device name and
/// placed on its release, and is no pending work at all. Once A uploads it,
/// B's next download fetches it and the arrival is gone.
async fn check_cover_arrival() {
    let devices = crate::sync::test_devices::TwoDevices::new().await;
    devices.pause().await;
    let a = devices.a().manager();
    a.database.set_uploads_paused(true).await.unwrap();
    let original = TempDir::new().unwrap();
    let album = create_test_album();
    a.database
        .insert_artist(&DbArtist {
            id: album.artist_id.clone(),
            name: "Artist Name".into(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: album.created_at,
        })
        .await
        .unwrap();
    a.database.insert_album(&album).await.unwrap();
    let release =
        insert_local_release_in_album(a, &album.id, original.path(), &[("a.flac", b"first audio")])
            .await;
    store_test_cover(a, &release.id, b"cover").await;
    let b = devices.b().manager();
    b.start().unwrap();
    devices.resume().await;
    // B reads the cover as soon as its row arrives, so the arrival doesn't
    // wait for B's own download to come round.
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while b
            .get_library_image(&release.id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .is_none()
        {
            devices.a().database().sync_now();
            devices.b().database().sync_now();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("B receives the cover's row");
    let read = b.read_cover_image_blob(&release.id).await;
    assert!(
        matches!(
            &read,
            Err(LibraryError::Blob { error, .. })
                if matches!(**error, coven::FileReadError::Uploading { .. })
        ),
        "the cover is still uploading on A: {read:?}"
    );

    let arriving = wait_for_status(
        &devices,
        "the cover arriving from A",
        |snapshot| matches!(&snapshot.arrivals, Some(Ok(arrivals)) if !arrivals.is_empty()),
    )
    .await;
    let Some(Ok(arrivals)) = &arriving.arrivals else {
        unreachable!("waited for arrivals");
    };
    assert_eq!(arrivals.devices.len(), 1);
    assert_eq!(arrivals.devices[0].device_name, "Device A");
    assert_eq!(
        arrivals.devices[0].releases,
        vec![ReleaseArrival {
            release_id: release.id.clone(),
            album_id: album.id.clone(),
            album_title: album.title.clone(),
            cover: true,
            track_ids: Vec::new(),
        }]
    );
    assert_eq!(arrivals.albums()[&album.id], "Device A");
    let Some(Ok(pending)) = &arriving.pending else {
        panic!("the pending list was read: {:?}", arriving.pending);
    };
    assert!(
        pending
            .iter()
            .all(|item| item.subject != PendingSubjectName::File),
        "an arriving file is no pending work: {pending:?}"
    );

    a.database.set_uploads_paused(false).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !a.database.outbox_queue().await.unwrap().uploads.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("A uploads the cover");
    // The cover is in storage now; B's next fetch of it, here the cover's
    // own read, ends its arrival.
    assert_eq!(
        b.read_cover_image_blob(&release.id).await.unwrap(),
        Some(b"cover".to_vec())
    );
    wait_for_status(
        &devices,
        "the cover's arrival ending on B",
        |snapshot| matches!(&snapshot.arrivals, Some(Ok(arrivals)) if arrivals.is_empty()),
    )
    .await;
    a.close().await.unwrap();
    b.close().await.unwrap();
}
