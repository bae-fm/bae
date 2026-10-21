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

/// A release with a cover on device A, whose uploads are paused, so the
/// cover's row syncs while its file stays on A. Sync is paused on both
/// devices; the caller resumes it.
async fn held_cover_on_a(devices: &crate::sync::test_devices::TwoDevices) -> (DbAlbum, DbRelease) {
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
    (album, release)
}

/// Wait until A's upload queue is empty: the cover is in storage.
async fn uploaded_on_a(devices: &crate::sync::test_devices::TwoDevices) {
    let a = devices.a().manager();
    a.database.set_uploads_paused(false).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !a.database.outbox_queue().await.unwrap().uploads.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("A uploads the cover");
}

/// Device A attaches a release and its cover while its uploads are paused.
/// Device B receives the rows and tries to fetch the cover, which storage
/// doesn't hold yet: the cover arrives from A, named by A's device name and
/// placed on its release, and is no pending work at all. Once A uploads it,
/// B's next download fetches it and the arrival is gone.
async fn check_cover_arrival() {
    let devices = crate::sync::test_devices::TwoDevices::new().await;
    let a = devices.a().manager();
    let (album, release) = held_cover_on_a(&devices).await;
    let b = devices.b().manager();
    b.start().unwrap();
    devices.resume().await;
    // B reads the cover as soon as its row arrives, so the arrival doesn't
    // wait for B's own download to come round.
    wait_for_cover_row(&devices, &release.id).await;
    let read = b.read_cover_image_bytes(&release.id).await;
    assert!(
        matches!(
            &read,
            Err(LibraryError::FileRead { error, .. })
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

    uploaded_on_a(&devices).await;
    // The cover is in storage now; B's next fetch of it, here the cover's
    // own read, ends its arrival.
    assert_eq!(
        b.read_cover_image_bytes(&release.id).await.unwrap(),
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
    let a = devices.a().manager();
    let (_album, release) = held_cover_on_a(&devices).await;
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
    uploaded_on_a(&devices).await;
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

#[test]
fn a_cover_still_arriving_reads_once_its_file_lands() {
    crate::sync::test_devices::run_two_device_test(check_image_arrival);
}

/// The image read the UI makes: a cover another device hasn't uploaded is
/// arriving, never a failure, and names its file; waiting for that file ends
/// once B fetches it after A uploads, and the read then has the bytes.
async fn check_image_arrival() {
    let devices = crate::sync::test_devices::TwoDevices::new().await;
    let (_album, release) = held_cover_on_a(&devices).await;
    let b = devices.b().manager();
    devices.resume().await;
    wait_for_cover_row(&devices, &release.id).await;
    let image = b
        .cover_ref(&release.id)
        .await
        .unwrap()
        .expect("the cover row");
    let Some(ImageRead::Arriving(file)) = b.read_image(&image).await.unwrap() else {
        panic!("the cover is still arriving from A");
    };
    let a_device = devices
        .a()
        .database()
        .members()
        .await
        .unwrap()
        .into_iter()
        .flat_map(|member| member.devices)
        .find(|device| device.is_self)
        .expect("A's own device")
        .id;
    assert_eq!(file.uploader, a_device);
    let arrived = tokio::spawn({
        let b = b.clone();
        async move { b.wait_for_arrival(file).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        !arrived.is_finished(),
        "the file has not arrived while A holds it"
    );
    uploaded_on_a(&devices).await;
    b.retry_now();
    tokio::time::timeout(std::time::Duration::from_secs(10), arrived)
        .await
        .expect("the wait ends once B fetches the cover")
        .unwrap()
        .unwrap();
    assert_eq!(
        b.read_image(&image).await.unwrap(),
        Some(ImageRead::Bytes(b"cover".to_vec()))
    );
    devices.a().manager().close().await.unwrap();
    b.close().await.unwrap();
}
