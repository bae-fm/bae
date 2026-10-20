use super::*;

#[test]
fn a_cover_another_device_has_not_uploaded_arrives_quietly_until_it_lands() {
    crate::sync::test_devices::run_two_device_test(check_cover_arrival);
}

/// Wait until B's sync status satisfies `condition`, or fail with the last
/// status B published.
async fn wait_for_status(
    manager: &LibraryManager,
    what: &str,
    condition: impl Fn(&SyncStatusSnapshot) -> bool,
) -> SyncStatusSnapshot {
    let mut values = manager.subscribe_sync_status_values();
    let waited = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        values.wait_for(|snapshot| condition(snapshot)),
    )
    .await;
    match waited {
        Ok(snapshot) => snapshot.expect("the sync status stays open").clone(),
        Err(_) => panic!(
            "{what} never happened; the last status was {:?}",
            manager.subscribe_sync_status_values().borrow().clone()
        ),
    }
}

/// Run passes on both devices until B holds the release's cover row.
async fn wait_for_cover_row(devices: &crate::sync::test_devices::TwoDevices, release_id: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while devices
            .b()
            .manager()
            .get_library_image(release_id, &LibraryImageType::Cover)
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
    wait_for_cover_row(&devices, &release.id).await;
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
        b,
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
        b,
        "the cover's arrival ending on B",
        |snapshot| matches!(&snapshot.arrivals, Some(Ok(arrivals)) if arrivals.is_empty()),
    )
    .await;
    a.close().await.unwrap();
    b.close().await.unwrap();
}

#[test]
fn retrying_now_ends_a_download_waiting_for_its_upload() {
    crate::sync::test_devices::run_two_device_test(check_retry_now_ends_the_wait);
}

/// B's download of a cover A hasn't uploaded backs off longer each time it
/// fails, and a backoff after storage answered is one a network report
/// leaves alone. After A uploads, the person's retry ends B's wait at once
/// instead of after that backoff.
async fn check_retry_now_ends_the_wait() {
    let devices = crate::sync::test_devices::TwoDevices::new().await;
    devices.pause().await;
    let a = devices.a().manager();
    a.database.set_uploads_paused(true).await.unwrap();
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
    let release = create_test_release(&album.id);
    insert_release(a, &release).await;
    store_test_cover(a, &release.id, b"cover").await;
    let b = devices.b().manager();
    b.start().unwrap();
    devices.resume().await;
    wait_for_cover_row(&devices, &release.id).await;
    wait_for_status(
        b,
        "the cover arriving from A",
        |snapshot| matches!(&snapshot.arrivals, Some(Ok(arrivals)) if !arrivals.is_empty()),
    )
    .await;
    // Each retry fails again while A holds the cover, doubling B's backoff
    // past what this test waits.
    for _ in 0..7 {
        b.retry_now();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    a.database.set_uploads_paused(false).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !a.database.outbox_queue().await.unwrap().uploads.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("A uploads the cover");
    b.network_changed();
    b.retry_now();
    let mut values = b.subscribe_sync_status_values();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        values.wait_for(
            |snapshot| matches!(&snapshot.arrivals, Some(Ok(arrivals)) if arrivals.is_empty()),
        ),
    )
    .await
    .expect("retrying now fetches the cover")
    .unwrap();
    a.close().await.unwrap();
    b.close().await.unwrap();
}
