async fn restore_test_library() -> RestoreTestLibrary {
    let (library_manager, album_dir, temp_dir) = support::setup_test_library_with_album_dir().await;
    let runtime_handle = tokio::runtime::Handle::current();
    let _ = generate_test_flac_files(&album_dir);
    let release_id_key = seed_discogs_test_release(library_manager.providers(), create_test_album());
    support::import_folder_and_wait(&library_manager, album_dir, release_id_key).await;
    let releases = library_manager
        .get_releases_for_album(&library_manager.get_albums(&[]).await.unwrap()[0].id)
        .await
        .unwrap();
    let tracks = library_manager
        .get_tracks_for_release(&releases[0].id)
        .await
        .unwrap();
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
    assert!(!track_ids.is_empty(), "the test album imported some tracks");
    RestoreTestLibrary {
        library_manager,
        runtime_handle,
        track_ids,
        _temp_dir: temp_dir,
    }
}

/// A library with no imports, for the empty-library no-op paths. Returns the
/// manager, runtime handle, and temp dir (kept alive for the DB file).
async fn empty_test_library() -> (LibraryManager, tokio::runtime::Handle, TempDir) {
    tracing_init();
    let temp_dir = TempDir::new().unwrap();
    let (library_manager, _database) = open_test_library(temp_dir.path()).await;
    (library_manager, tokio::runtime::Handle::current(), temp_dir)
}

/// Import a second, distinct-content release (the CUE/FLAC album) into `lib`'s
/// library so a test can have a current track in one release and a
/// preloaded/queued track in another — needed to delete one without the other.
/// Returns the new release's id and track ids; the source `TempDir` is returned
/// so the caller keeps it alive.
async fn import_second_release(lib: &RestoreTestLibrary) -> (String, Vec<String>, TempDir) {
    let source = TempDir::new().unwrap();
    generate_cue_flac_files(source.path());
    let release_key = seed_discogs_test_release(
        lib.library_manager.providers(),
        create_cue_flac_test_album(),
    );
    let import_handle =
        start_test_import(lib.runtime_handle.clone(), lib.library_manager.clone()).await;
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            candidate_key: "second".to_string(),
            ..support::folder_import(
                &import_id,
                source.path().to_path_buf(),
                support::discogs_release(release_key),
            )
        })
        .await
        .unwrap();
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;
    let tracks = lib
        .library_manager
        .get_tracks_for_release(&release_id)
        .await
        .unwrap()
        .iter()
        .map(|t| t.id.clone())
        .collect();
    (release_id, tracks, source)
}

/// A restart takes up the queue where it stood: the lane's shuffled order,
/// Up Next, and the track playing, resumed paused. The order lives in the
/// queue tables, so nothing shuffles it again.
#[tokio::test]
async fn test_restart_resumes_the_stored_queue_in_its_order() {
    let lib = restore_test_library().await;
    let release_id = lib
        .library_manager
        .get_track_display(&lib.track_ids[0])
        .await
        .expect("the imported track")
        .release_id;

    let (handle, _capture_rx) =
        start_capture_service(lib.library_manager.clone(), lib.runtime_handle.clone()).await;
    let mut progress_rx = handle.subscribe_progress();
    handle.play_release(release_id, None, true);
    let playing = wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_secs(20),
    )
    .await
    .expect("the shuffled release plays");
    handle.add_to_queue(vec![lib.track_ids[0].clone()]);
    handle.pause();
    let before = handle.queue_value().await.expect("queue value");
    let order = handle.queue_upcoming().await.expect("queue upcoming");
    assert!(before.context.as_ref().is_some_and(|context| context.shuffled));
    handle.shutdown().await;

    let (handle, _capture_rx) =
        start_capture_service(lib.library_manager.clone(), lib.runtime_handle).await;
    let mut progress_rx = handle.subscribe_progress();
    let playing_id = playing.track_id().expect("a playing track").to_string();
    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Paused { track, .. } if track.track_id == playing_id),
        Duration::from_secs(20),
    )
    .await
    .expect("the playing track resumes paused");
    assert_eq!(
        handle.queue_value().await.expect("queue value"),
        before,
        "the restarted queue is the one that was playing"
    );
    assert_eq!(
        handle.queue_upcoming().await.expect("queue upcoming"),
        order,
        "in its order"
    );

    handle.stop();
}

/// Seeking a preview while paused must emit a PreviewPositionUpdate even
/// though no position ticks fire in the paused state. Without this explicit
/// emission, the progress NSView stays stuck at the old position until the
/// user resumes.
///
/// Regression test for the preview dual-sink fix: if someone removes the
/// inline emit in handle_preview_seek, this test fails.
#[tokio::test]
async fn test_preview_seek_while_paused_emits_position_update() {
    let mut fixture = PlaybackTestFixture::new().await;

    let preview_path = fixture
        .album_dir
        .join("01 Test Track 1.flac")
        .to_string_lossy()
        .into_owned();

    // Start preview — wait for it to reach Playing state.
    fixture
        .playback_handle
        .preview_play(bae_core::playback::PreviewTarget::whole_file(preview_path));
    let saw_playing =
        support::next_matching(&mut fixture.progress_rx, Duration::from_secs(5), |event| {
            matches!(
                event,
                PlaybackProgress::PreviewStateChanged(
                    bae_core::playback::PreviewState::Playing { .. }
                )
            )
            .then_some(())
        })
        .await;
    assert!(saw_playing.is_some(), "preview should reach Playing state");

    // Pause — wait for Paused state.
    fixture.playback_handle.preview_toggle_pause();
    let saw_paused =
        support::next_matching(&mut fixture.progress_rx, Duration::from_secs(3), |event| {
            matches!(
                event,
                PlaybackProgress::PreviewStateChanged(
                    bae_core::playback::PreviewState::Paused { .. }
                )
            )
            .then_some(())
        })
        .await;
    assert!(saw_paused.is_some(), "preview should reach Paused state");

    // Seek to the middle of the file while paused. No ticks fire in the
    // paused state, so the only way the NSView learns the new position is
    // the explicit emit in handle_preview_seek.
    fixture.playback_handle.preview_seek_by_ratio(0.5);

    let saw_position_update = support::next_matching(
        &mut fixture.progress_rx,
        Duration::from_secs(3),
        |event| match event {
            PlaybackProgress::PreviewPositionUpdate { position_ms, .. } => Some(position_ms),
            _ => None,
        },
    )
    .await;

    assert!(
        saw_position_update.is_some(),
        "seek-while-paused should emit PreviewPositionUpdate"
    );

    fixture.playback_handle.preview_stop();
}

/// A cloud-only remote track whose data lives only in the cloud. The reader
/// streams in the background after `play_track` has already built the stream,
/// so a cloud failure here is genuinely mid-flight — not a prepare-time error.
struct CloudOnlyPlaybackFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    cloud: Arc<coven::MemoryStorage>,
    track_ids: Vec<String>,
    _capture_stream_rx: support::CaptureStreamRx,
    _temp_dir: TempDir,
}

impl CloudOnlyPlaybackFixture {
    /// Import the FLAC fixtures as remote-unpinned, run the upload to put the
    /// encrypted files in the (mock) cloud and clear the outbox, then delete the
    /// import originals so resolution lands on the cloud reader. The mock cloud
    /// is returned so the test can arm a read failure before playing.
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;
        let (library_manager, _, cloud) = support::storage_library(temp_dir.path(), bae_core::providers::Providers::offline()).await;
        support::connect_storage(&library_manager, &cloud).await;

        let runtime_handle = tokio::runtime::Handle::current();
        let discogs_release = create_test_album();
        let release_id_key = seed_discogs_test_release(library_manager.providers(), discogs_release);
        generate_test_flac_files(&album_dir);

        let import_handle =
            start_test_import(runtime_handle.clone(), library_manager.clone()).await;
        let import_id = uuid::Uuid::new_v4().to_string();
        import_handle
            .send_command(ImportCommand {
                destination: ImportDestination::Remote { pin: false },
                ..support::folder_import(
                    &import_id,
                    album_dir.clone(),
                    support::discogs_release(release_id_key),
                )
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let mut import_rx = import_handle.subscribe_import(import_id);
        let (release_id, _album_id) = wait_for_import_complete(&mut import_rx).await;

        // Run the upload so the encrypted files land in the cloud and the outbox
        // clears — after this the track resolves cloud-only (no local copy, no
        // pending upload).
        while matches!(
            library_manager.drain_uploads_for_test().await?,
            coven::DrainOutcome::Drained { uploaded, .. } if uploaded > 0
        ) {}

        for file in library_manager.get_files_for_release(&release_id).await? {
            let reference = library_manager.release_file_ref_for_test(&file.id).await?;
            library_manager.evict_file_for_test(&reference).await?;
        }

        // Delete the import originals so file resolution can't fall back to them.
        std::fs::remove_dir_all(&album_dir)?;

        let tracks = library_manager.get_tracks_for_release(&release_id).await?;
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        assert!(!track_ids.is_empty(), "Should have imported tracks");

        let (playback_handle, capture_stream_rx) =
            start_capture_service(library_manager, runtime_handle).await;
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            cloud,
            track_ids,
            _capture_stream_rx: capture_stream_rx,
            _temp_dir: temp_dir,
        })
    }
}

/// A mid-flight cloud read failure must drive playback to Stopped, not leave the
/// UI frozen in a not-yet-started "Playing" state. The reader fails in the
/// background after `play_track` built the stream; the service's progress
/// self-subscription turns the resulting PlaybackError into a stop. The terminal
/// state must be Stopped — never Playing/Loading — because the error path tears
/// playback down rather than leaving a frozen position bar.
#[tokio::test]
async fn mid_flight_cloud_read_failure_ends_in_stopped() {
    let mut fixture = CloudOnlyPlaybackFixture::new()
        .await
        .expect("set up cloud-only playback fixture");
    assert!(
        !fixture.track_ids.is_empty(),
        "fixture must import at least one playable track"
    );

    fixture.cloud.set_online(false);

    let track_id = fixture.track_ids[0].clone();
    fixture.playback_handle.play(track_id);

    // Collect the whole StateChanged sequence up to Stopped. The read fails
    // before the decoder's ready signal, so Playing must never appear: the
    // ready-gated emission holds Playing until audio flows, and the error path
    // tears playback down to Stopped instead. Both are exercised here.
    let states = collect_states_on(
        &mut fixture.progress_rx,
        |s| matches!(s, PlaybackState::Stopped),
        Duration::from_secs(8),
    )
    .await;
    assert!(
        matches!(states.last(), Some(PlaybackState::Stopped)),
        "a mid-flight cloud read failure must end in Stopped, got {states:?}"
    );
    assert!(
        !states
            .iter()
            .any(|s| matches!(s, PlaybackState::Playing { .. })),
        "Playing must never be emitted for a track whose audio never became ready, got {states:?}"
    );

    // And it must not bounce back into Playing afterwards: nothing the service
    // had queued by the time it stopped restarts it.
    let events = settled_events_on(&fixture.playback_handle, &mut fixture.progress_rx).await;
    let resurfaced = states_in(&events);
    assert!(
        resurfaced
            .iter()
            .all(|s| matches!(s, PlaybackState::Stopped)),
        "playback must stay stopped after the failure, got {resurfaced:?}"
    );
    let state = fixture.playback_handle.subscribe_values().borrow().state.clone();
    assert!(
        matches!(state, PlaybackState::Stopped),
        "playback must stay stopped after the failure, not flip back to Playing, got {state:?}"
    );
}
