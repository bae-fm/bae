/// Opening an album while sync runs still shows the album.
///
/// The album detail subscription merges its database query with the config,
/// sync-status and transfer streams, which re-render the same projection with no
/// database change behind them. A running sync cycle publishes on those streams
/// the whole time, and no rate of publication on them may stop the query from
/// reaching its first value: a subscription that keeps restarting its read
/// delivers neither a value nor an error, which reads on screen as a spinner
/// that never resolves. The release detail and the storage page merge their
/// queries the same way, through the same `live_query_events`.
///
/// The load here is sync-shaped without a sync loop: reads keeping coven's
/// reader connection busy so the query's read waits its turn behind them, and a
/// steady stream on one of the three re-render channels. Both are paced by
/// something other than the CPU — the reads by the connection thread, the stream
/// by a timer — so this runtime stays able to schedule the deadline below, and a
/// regression fails on it instead of hanging.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn album_detail_delivers_while_sync_shaped_load_runs() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    for _ in 0..25 {
        let release = create_test_release(&album.id);
        manager.database.insert_release(&release).await.unwrap();
    }

    let mut load = Vec::new();
    for _ in 0..8 {
        let database = manager.database.clone();
        let album_id = album.id.clone();
        load.push(tokio::spawn(async move {
            loop {
                let _ = database.get_releases_for_album(&album_id).await;
            }
        }));
    }
    let transitions = manager.transitions.clone();
    load.push(tokio::spawn(async move {
        loop {
            transitions.republish();
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }));

    let playback = manager.start_playback_service_with_audio_device(
        tokio::runtime::Handle::current(),
        50,
        false,
        // No hardware behind it: this test never plays anything.
        Box::new(crate::playback::audio_output::FailingAudioDevice),
    );
    let import = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .expect("start the import service");
    let services = crate::library::AppServices::new(manager, playback, import);

    let mut values = services
        .subscribe_album_detail_values(&tokio::runtime::Handle::current(), album.id.clone());
    let delivered = tokio::time::timeout(std::time::Duration::from_secs(10), values.recv()).await;

    // Dropping `services` joins the playback and import threads, so it happens
    // on a blocking thread, and before the assertions so a failure reports
    // rather than hanging the test binary on the panic path.
    for task in load {
        task.abort();
    }
    drop(values);
    tokio::task::spawn_blocking(move || drop(services))
        .await
        .expect("tear the application down");

    let detail = delivered
        .expect("the album detail arrives while sync-shaped load runs")
        .expect("the subscription stays open")
        .expect("the album detail resolves")
        .expect("the album is present");
    assert_eq!(detail.album.id, album.id);
}
