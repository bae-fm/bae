/// Opening an album while sync runs still shows the album.
///
/// A running sync cycle keeps coven's reader busy and writes rows the album
/// detail's query reads the whole time, and no rate of either may stop the
/// query from reaching its first value: a subscription that keeps restarting
/// its read delivers neither a value nor an error, which reads on screen as a
/// spinner that never resolves. The release detail and the storage page read
/// the same way, through the same `reconfigurable_live_query_events`.
///
/// The load here is sync-shaped without a sync loop: reads keeping coven's
/// reader connection busy so the query's read waits its turn behind them, and
/// a steady stream of writes to a row the query reads, a release of the album
/// entering and leaving a storage transition. Both are paced by something
/// other than the CPU — the reads by the connection thread, the writes by a
/// timer — so this runtime stays able to schedule the deadline below, and a
/// regression fails on it instead of hanging.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn album_detail_delivers_while_sync_shaped_load_runs() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let mut releases = Vec::new();
    for _ in 0..25 {
        let release = create_test_release(&album.id);
        insert_release(&manager, &release).await;
        releases.push(release.id);
    }

    let mut load = Vec::new();
    for _ in 0..8 {
        let database = manager.database.clone();
        let album_id = album.id.clone();
        load.push(tokio::spawn(async move {
            loop {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "this is load on the database; each read's answer is not the point"
                )]
                let _ = database.get_releases_for_album(&album_id).await;
            }
        }));
    }
    let database = manager.database.clone();
    let transitioning = releases[0].clone();
    load.push(tokio::spawn(async move {
        loop {
            database
                .begin_release_storage_action(&transitioning, bae_db::StoredTransition::Unpin)
                .await
                .unwrap();
            database.end_release_storage_action(&transitioning).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }));

    // Playback on no hardware: this test never plays anything.
    let services = manager.start_services().await.expect("start the services");

    let values = services.subscribe_album_detail();
    values.set(Some(album.id.clone())).unwrap();
    let delivered = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match values.next().await {
                Ok(snapshot) if snapshot.id.is_none() => continue,
                other => return other,
            }
        }
    })
    .await;

    // Dropping `services` joins the playback and import threads, so it happens
    // on a blocking thread, and before the assertions so a failure reports
    // rather than hanging the test binary on the panic path.
    for task in load {
        task.abort();
    }
    drop(values);
    tokio::task::spawn_blocking(move || drop(services)).await.expect("tear the application down");

    let detail = delivered
        .expect("the album detail arrives while sync-shaped load runs")
        .expect("the album detail resolves")
        .value
        .expect("the album is present");
    assert_eq!(detail.album.id, album.id);
}

/// A detail view that moves to another album moves its one read: the next
/// value answers the new id, and clearing the id reads nothing.
#[tokio::test]
async fn album_detail_moves_between_albums_on_one_subscription() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let mut albums = Vec::new();
    for title in ["Album One", "Album Two"] {
        let mut album = create_test_album();
        album.title = title.to_string();
        manager.database.insert_album(&album).await.unwrap();
        insert_release(&manager, &create_test_release(&album.id)).await;
        albums.push(album);
    }
    let services = manager.start_services().await.unwrap();
    let values = services.subscribe_album_detail();
    let next_for = |id: Option<String>| {
        let values = &values;
        async move {
            loop {
                let snapshot =
                    tokio::time::timeout(std::time::Duration::from_secs(5), values.next())
                        .await
                        .expect("the album detail delivers")
                        .expect("the album detail resolves");
                if snapshot.id == id {
                    return snapshot;
                }
            }
        }
    };

    values.set(Some(albums[0].id.clone())).unwrap();
    let first = next_for(Some(albums[0].id.clone())).await;
    assert_eq!(first.value.expect("present").album.title, "Album One");

    values.set(Some(albums[1].id.clone())).unwrap();
    let second = next_for(Some(albums[1].id.clone())).await;
    assert_eq!(second.value.expect("present").album.title, "Album Two");

    values.set(None).unwrap();
    assert!(next_for(None).await.value.is_none());
}

/// A release's storage choices follow the cloud home within its one read:
/// a local release offers none until storage is set up, then offers its
/// upload, with nothing asked of the subscription in between.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn release_detail_offers_storage_once_a_cloud_home_is_set_up() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("local-source"),
        "Test Album",
        &[("a.flac", b"aaa")],
    )
    .await;
    let services = manager.library.start_services().await.unwrap();
    let values = services.subscribe_release_detail();
    values.set(Some(release.id.clone())).unwrap();
    let actions_until = |accept: fn(&[bae_types::release_storage::ReleaseStorageAction]) -> bool| {
        let values = &values;
        async move {
            tokio::time::timeout(std::time::Duration::from_secs(20), async {
                loop {
                    let snapshot = values.next().await.expect("the release detail resolves");
                    if let Some(detail) = snapshot.value {
                        if accept(&detail.summary.storage_actions) {
                            return detail.summary.storage_actions;
                        }
                    }
                }
            })
            .await
            .expect("the release detail shows the expected storage choices")
        }
    };
    actions_until(|actions| actions.is_empty()).await;
    connect_test_cloud(&manager).await;
    assert_eq!(
        actions_until(|actions| !actions.is_empty()).await,
        vec![bae_types::release_storage::ReleaseStorageAction::Upload]
    );
}
