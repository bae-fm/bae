async fn restore_test_library() -> RestoreTestLibrary {
    tracing_init();
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).unwrap();
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let database_arc = Arc::new(database.clone());
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    let runtime_handle = tokio::runtime::Handle::current();
    let _ = generate_test_flac_files(&album_dir);
    let discogs_release = create_test_album();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone()).await;
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let _ = wait_for_import_complete(&mut progress_rx).await;
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
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database,
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
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
    let release_key = seed_discogs_test_release(create_cue_flac_test_album());
    let import_handle =
        start_test_import(lib.runtime_handle.clone(), lib.library_manager.clone()).await;
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "second".to_string(),
            folder: source.path().to_path_buf(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_key, MetadataSource::Discogs),
            },
            user_edit: None,
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

/// A resume cache whose context release is gone (its `get_track_ids` is empty)
/// restores the manual lane only — the context drops, the surviving manual
/// tracks and current track stay. The restored queue re-persists with no
/// context, which is what we read back.
#[tokio::test]
async fn test_restore_drops_deleted_context_keeps_manual() {
    let lib = restore_test_library().await;
    let track_id = lib.track_ids[0].clone();

    // The context points at a release id that no longer exists, so its
    // `get_track_ids` returns empty (the deleted-release signal). The manual lane
    // and current track are a real, surviving track.
    let state = bae_core::db::DbPlaybackState {
        context: Some(bae_core::db::DbPlaybackContext {
            source: RELEASE_THAT_WAS_DELETED.to_string(),
            shuffled: false,
        }),
        manual: format!("[{:?}]", track_id),
        repeat: "off".to_string(),
        current_track_id: Some(track_id.clone()),
        position_ms: Some(0),
        volume: 0.8,
        is_muted: false,
    };
    lib.library_manager
        .save_playback_state(&state)
        .await
        .unwrap();

    let (handle, _capture_rx) =
        start_capture_service(lib.library_manager.clone(), lib.runtime_handle);
    let mut progress_rx = handle.subscribe_progress();

    // Restore committed once the surviving manual track surfaces as the
    // restored (Paused) current track.
    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Paused { track_info, .. } if track_info.track_id == track_id),
        Duration::from_secs(20),
    )
    .await
    .expect("the surviving manual track should restore as current");

    // Force the restored queue to re-persist, then read it back: the dropped
    // context is gone and the manual track survived.
    handle.set_repeat_mode(bae_core::playback::RepeatMode::Track);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut row = None;
    while Instant::now() < deadline {
        if let bae_core::db::LoadedPlaybackState::Present(loaded) =
            lib.library_manager.load_playback_state().await.unwrap()
        {
            if loaded.repeat == "track" {
                row = Some(loaded);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let row = row.expect("the restored, re-persisted row");
    assert!(
        row.context.is_none(),
        "the deleted context release is dropped on restore"
    );
    assert_eq!(
        row.manual,
        format!("[{:?}]", track_id),
        "the surviving manual lane restored"
    );

    handle.stop();
}

/// A corrupt resume cache (here, manual lane that is not valid JSON) is discarded
/// by the boundary parse: the service starts with an empty queue and never
/// panics. No playback state surfaces because no current track restored.
#[tokio::test]
async fn test_restore_corrupt_row_starts_fresh() {
    let lib = restore_test_library().await;
    let track_id = lib.track_ids[0].clone();

    let state = bae_core::db::DbPlaybackState {
        context: None,
        manual: "not valid json".to_string(),
        repeat: "off".to_string(),
        current_track_id: Some(track_id),
        position_ms: Some(0),
        volume: 0.8,
        is_muted: false,
    };
    lib.library_manager
        .save_playback_state(&state)
        .await
        .unwrap();

    let (handle, _capture_rx) = start_capture_service(lib.library_manager, lib.runtime_handle);
    let mut progress_rx = handle.subscribe_progress();

    // A discarded resume cache restores nothing: no playback state may surface.
    let restored = wait_for_state_on(&mut progress_rx, |_| true, Duration::from_millis(500)).await;
    assert!(
        restored.is_none(),
        "a discarded resume cache leaves nothing playing (fresh start), got {restored:?}"
    );
    let queue = handle.queue_projection().await.expect("queue projection");
    assert!(
        queue.manual.is_empty() && queue.context.is_none(),
        "a discarded resume cache starts with an empty queue"
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
    fixture.playback_handle.preview_play(preview_path);
    let mut saw_playing = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PreviewStateChanged(
                bae_core::playback::PreviewState::Playing { .. },
            ))) => {
                saw_playing = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(saw_playing, "preview should reach Playing state");

    // Pause — wait for Paused state.
    fixture.playback_handle.preview_toggle_pause();
    let mut saw_paused = false;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PreviewStateChanged(
                bae_core::playback::PreviewState::Paused { .. },
            ))) => {
                saw_paused = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(saw_paused, "preview should reach Paused state");

    // Seek to the middle of the file while paused. No ticks fire in the
    // paused state, so the only way the NSView learns the new position is
    // the explicit emit in handle_preview_seek.
    fixture.playback_handle.preview_seek_by_ratio(0.5);

    let mut saw_position_update: Option<u64> = None;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PreviewPositionUpdate { position_ms, .. })) => {
                saw_position_update = Some(position_ms);
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }

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
    cloud: Arc<coven::InMemoryCloudHome>,
    track_ids: Vec<String>,
    _capture_stream_rx: CaptureStreamRx,
    _temp_dir: TempDir,
}

impl CloudOnlyPlaybackFixture {
    /// Import the FLAC fixtures as remote-unpinned, run the upload to put the
    /// encrypted blobs in the (mock) cloud and clear the outbox, then delete the
    /// import originals so resolution lands on the cloud reader. The mock cloud
    /// is returned so the test can arm a read failure before playing.
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;
        let database = Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await?;
        let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let library_manager = LibraryManager::new(
            database,
            config_handle,
            key_service,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );
        let master_key = [11u8; 32];
        let cloud = Arc::new(coven::InMemoryCloudHome::new());
        library_manager
            .connect_test_cloud_home(
                cloud.clone(),
                bae_core::sync::CloudCipher::Encrypted(coven::EncryptionService::from_key(
                    master_key,
                )),
            )
            .await?;

        let runtime_handle = tokio::runtime::Handle::current();
        let discogs_release = create_test_album();
        let release_id_key = seed_discogs_test_release(discogs_release);
        generate_test_flac_files(&album_dir);

        let import_handle =
            start_test_import(runtime_handle.clone(), library_manager.clone()).await;
        let import_id = uuid::Uuid::new_v4().to_string();
        import_handle
            .send_command(ImportCommand {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir.clone(),
                scope: bae_core::import::ReleaseFileScope::Recursive,
                selected_cover: None,
                storage_mode: StorageMode::Remote,
                pin: false,
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
                },
                user_edit: None,
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let mut import_rx = import_handle.subscribe_import(import_id);
        let (release_id, _album_id) = wait_for_import_complete(&mut import_rx).await;

        // Run the upload so the encrypted blobs land in the cloud and the outbox
        // clears — after this the track resolves cloud-only (no local copy, no
        // pending upload).
        while matches!(
            library_manager.drain_uploads_for_test().await?,
            coven::DrainOutcome::Drained { uploaded, .. } if uploaded > 0
        ) {}

        // Delete the import originals so file resolution can't fall back to them.
        std::fs::remove_dir_all(&album_dir)?;

        let tracks = library_manager.get_tracks_for_release(&release_id).await?;
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        assert!(!track_ids.is_empty(), "Should have imported tracks");

        let (playback_handle, capture_stream_rx) =
            start_capture_service(library_manager, runtime_handle);
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

    // Every release-file object leaves the cloud, so the background reader's
    // fetch 404s and it cancels the buffer and emits PlaybackError. Arming the
    // legacy range-read failure no longer covers this: coven reads an exact
    // object by its locator, on a path that hook does not sit on.
    let objects: Vec<String> = fixture
        .cloud
        .keys()
        .into_iter()
        .filter(|key| key.starts_with("release_files/"))
        .collect();
    assert!(
        !objects.is_empty(),
        "the fixture uploaded its release files"
    );
    for key in objects {
        fixture.cloud.remove(&key);
    }

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

    // And it must not bounce back into Playing afterwards.
    let resurfaced = wait_for_state_on(
        &mut fixture.progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_millis(300),
    )
    .await;
    assert!(
        resurfaced.is_none(),
        "playback must stay stopped after the failure, not flip back to Playing"
    );
}
