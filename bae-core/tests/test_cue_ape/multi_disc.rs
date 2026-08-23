/// Next button must advance to track 2 and produce audio matching the XLD reference.
///
/// Tests the manual Next path (not auto-advance). After pressing Next, the captured
/// audio from track 2 must match the XLD-split reference.
#[tokio::test]
async fn test_cue_ape_next_track() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = CueApeTestFixture::with_capture()
        .await
        .unwrap_or_else(|e| panic!("failed to set up CUE/APE playback fixture: {e}"));

    let track1_id = fixture.track_ids[0].clone();
    let track2_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(track1_id.clone());
    // Drain track 1's play stream; the Next below mints a fresh one for track 2.
    let _track1_play = fixture.next_capture_stream().await;

    // Wait for track 1 to start
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = &state {
                    if track_info.track_id == track1_id {
                        started = true;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(started, "Track 1 should start playing");

    // Press Next — track 2's stream replaces track 1's.
    fixture.playback_handle.next();
    let captured = fixture.next_capture_stream().await;

    // Wait for track 2 to start
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut track2_started = false;
    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = &state {
                    if track_info.track_id == track2_id {
                        track2_started = true;
                        break;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(track2_started, "Track 2 should start after Next");

    // Decode XLD reference
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two.flac")).expect("read reference");
    let reference =
        decode_audio(buffer_from(&reference_data), None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

    // Align and compare
    let snippet_len = 500 * channels;
    // APE frames are ~73728 samples (~1.7s). FFmpeg's seek lands at a frame
    // boundary, so the captured audio may start up to one frame before the
    // track's CUE start time. Search within 2s for alignment.
    let max_alignment = sample_rate as usize * channels * 2;

    let track2_sample_count = timeout(Duration::from_secs(10), async {
        loop {
            match fixture.progress_rx.recv().await {
                Some(PlaybackProgress::DecodeStats {
                    track_id,
                    samples_decoded,
                    ..
                }) if track_id == track2_id => break samples_decoded as usize,
                Some(_) => continue,
                None => panic!("Track 2 should emit decode stats"),
            }
        }
    })
    .await
    .expect("Track 2 should emit decode stats within 10s");
    assert!(
        track2_sample_count > snippet_len,
        "Track 2 should produce enough samples to compare: {} (needed more than {})",
        track2_sample_count,
        snippet_len,
    );
    assert!(
        reference_f32.len() >= track2_sample_count,
        "Track 2 XLD reference should cover decoded track: {} (needed {})",
        reference_f32.len(),
        track2_sample_count,
    );

    let captured_snapshot = bae_core::playback::wait_for_samples(
        &captured,
        track2_sample_count,
        Duration::from_secs(60),
    )
    .await;
    assert!(
        captured_snapshot.len() >= track2_sample_count,
        "Not enough captured samples after Next (60s): {} (needed {})",
        captured_snapshot.len(),
        track2_sample_count,
    );

    assert_captured_matches_reference(
        &captured_snapshot,
        &reference_f32,
        channels,
        sample_rate,
        max_alignment,
        track2_sample_count,
        0.01,
        "track 2 after Next",
    );
}

/// Import a two-disc CUE/APE release under the given storage mode and assert
/// each track's main audio segment resolves to its own disc's APE file
/// (identified by the relative path `CD{N}/CDImage.ape`).
///
/// This exercises the bare-filename collision regression in every code path:
/// bytes-never-copied (Local), bytes-uploaded-and-pinned (Remote + pin),
/// and bytes-uploaded-cloud-only (Remote, no pin). Pin is an orthogonal coven
/// cache choice, so it rides alongside the `StorageMode` as its own argument.
async fn assert_multi_disc_cue_ape_per_disc_mapping(storage_mode: StorageMode, pin: bool) {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let cd1_dir = album_dir.join("CD1");
    let cd2_dir = album_dir.join("CD2");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&cd1_dir).expect("cd1 dir");
    std::fs::create_dir_all(&cd2_dir).expect("cd2 dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");

    // Reuse the 90s / 3-track CUE+APE fixture, once per disc, renamed to
    // `CDImage.*` so both discs share the same bare filename (a common
    // multi-disc rip layout).
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");
    let ape_bytes = std::fs::read(fixture_dir.join("Test Album.ape")).expect("read ape fixture");
    let cue_body = "PERFORMER \"Test Artist\"\n\
                    TITLE \"Disc Title\"\n\
                    FILE \"CDImage.ape\" WAVE\n  \
                    TRACK 01 AUDIO\n    TITLE \"Track One\"\n    INDEX 01 00:00:00\n  \
                    TRACK 02 AUDIO\n    TITLE \"Track Two\"\n    INDEX 01 00:30:00\n  \
                    TRACK 03 AUDIO\n    TITLE \"Track Three\"\n    INDEX 01 01:00:00\n";
    for dir in [&cd1_dir, &cd2_dir] {
        std::fs::write(dir.join("CDImage.ape"), &ape_bytes).expect("write ape");
        std::fs::write(dir.join("CDImage.cue"), cue_body).expect("write cue");
    }

    let library_dir = StoreDir::new(db_dir.clone());
    let config_handle = test_config(&library_dir);
    let library_manager = LibraryManager::open(
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        None,
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    )
    .expect("open library manager");
    if storage_mode == StorageMode::Remote {
        library_manager
            .connect_test_cloud_home(
                Arc::new(coven::InMemoryCloudHome::new()),
                CloudCipher::Encrypted(EncryptionService::from_key([7u8; 32])),
            )
            .await
            .expect("connect in-memory cloud home");
        assert!(
            library_manager.is_sync_ready(),
            "remote import test cloud should start sync"
        );
    }

    // Discogs multi-disc tracklist: positions "1-1".."1-3", "2-1".."2-3".
    // `parse_side_from_position` maps these to side=1 and side=2.
    let discogs_release = DiscogsRelease {
        id: "test-multi-disc-cue-ape".to_string(),
        title: "Multi-Disc Album".to_string(),
        year: Some(2024),
        format: vec![],
        country: None,
        label: vec![],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            id: "discogs-artist-1".to_string(),
            name: "Artist Name".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: (1..=2)
            .flat_map(|disc| {
                (1..=3).map(move |n| DiscogsTrack {
                    type_: "track".to_string(),
                    position: format!("{}-{}", disc, n),
                    title: format!("Disc {} Track {}", disc, n),
                    duration: Some("0:30".to_string()),
                    artists: vec![],
                    extraartists: None,
                })
            })
            .collect(),
        master_id: None,
    };
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle =
        start_test_import(tokio::runtime::Handle::current(), library_manager.clone()).await;

    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode,
            pin,
            identity_choice: IdentityChoice::Release {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .expect("send command");

    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _) =
        wait_for_multi_disc_cue_ape_import_ready(&library_manager, storage_mode, &mut progress_rx)
            .await;

    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 6, "should have 6 tracks (2 discs × 3 tracks)");

    // `original_filename` in release_files must be the full relative path
    // (`CD1/CDImage.ape`), not the bare filename — otherwise the two discs'
    // APE rows would collide on the same `"CDImage.ape"` string.
    let files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("get files");
    let mut ape_filenames: Vec<String> = files
        .iter()
        .filter(|f| f.original_filename.ends_with(".ape"))
        .map(|f| f.original_filename.clone())
        .collect();
    ape_filenames.sort();
    assert_eq!(
        ape_filenames,
        vec!["CD1/CDImage.ape".to_string(), "CD2/CDImage.ape".to_string()],
        "release_files must store relative paths for both discs, not bare filenames"
    );

    // Each track's main audio segment must point to its own disc's APE.
    let filename_by_id: std::collections::HashMap<String, String> = files
        .iter()
        .map(|f| (f.id.clone(), f.original_filename.clone()))
        .collect();

    for track in &tracks {
        let resolved = library_manager
            .resolve_track_audio(&track.id)
            .await
            .expect("resolve track audio");
        let segment = resolved
            .segments
            .iter()
            .find(|segment| segment.role == bae_core::db::DbAudioSegmentRole::Main)
            .unwrap_or_else(|| panic!("track {} has no main audio segment", track.title));
        let filename = filename_by_id
            .get(&segment.file_id)
            .unwrap_or_else(|| panic!("file_id {} not in release_files", segment.file_id));

        let expected = match track.side {
            1 => "CD1/CDImage.ape",
            2 => "CD2/CDImage.ape",
            other => panic!("unexpected side {other} for track {}", track.title),
        };
        assert_eq!(
            filename, expected,
            "track '{}' (side {}) should point at {expected}, got {filename}",
            track.title, track.side,
        );
    }
}

async fn wait_for_multi_disc_cue_ape_import_ready(
    library_manager: &LibraryManager,
    storage_mode: StorageMode,
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> (String, String) {
    match storage_mode {
        StorageMode::Local => wait_for_import_complete(progress_rx).await,
        StorageMode::Remote => {
            let (release_id, album_id) = wait_for_remote_upload_queued(progress_rx).await;
            // The connected store may still be completing its initial snapshot.
            // Bound a stuck transition without imposing a latency guarantee on
            // that cycle plus the import's publication.
            timeout(Duration::from_secs(60), async {
                loop {
                    let release = library_manager
                        .get_release_by_id(&release_id)
                        .await
                        .expect("find imported release")
                        .expect("imported release exists");
                    let pending = library_manager
                        .count_pending_uploads_for_release(&release_id)
                        .await
                        .expect("count pending release uploads");
                    if release.remote && pending == 0 {
                        break;
                    }
                    sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("remote multi-disc CUE/APE import uploads all files");
            let files = library_manager
                .get_files_for_release(&release_id)
                .await
                .expect("get release files");
            assert_eq!(
                files.len(),
                4,
                "remote multi-disc CUE/APE import keeps both audio files and both CUE sheets"
            );
            (release_id, album_id)
        }
    }
}

async fn wait_for_remote_upload_queued(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> (String, String) {
    while let Some(progress) = progress_rx.recv().await {
        match progress {
            bae_core::import::ImportProgress::RemoteUploadQueued { id, album_id, .. } => {
                return (id, album_id)
            }
            bae_core::import::ImportProgress::Failed { error, .. } => {
                panic!("Import failed: {}", error);
            }
            _ => {}
        }
    }
    panic!("Progress channel closed without remote upload being queued");
}

/// Regression: multi-disc CUE/APE imports must link each track to its OWN disc's
/// audio file, not collapse every track onto the last disc's file.
///
/// Bug: `file_ids` was keyed by bare filename (`CDImage.ape`), so CD1's entry
/// was overwritten by CD2's. Every `audio_formats.file_id` ended up pointing at
/// CD2's APE, so playing any disc 1 track decoded disc 2's audio.
///
/// The same structural bug lived in all three byte-placement strategies, so we
/// cover each explicitly to prevent a regression on any one path.
#[tokio::test]
async fn test_multi_disc_cue_ape_local() {
    assert_multi_disc_cue_ape_per_disc_mapping(StorageMode::Local, false).await;
}

#[tokio::test]
async fn test_multi_disc_cue_ape_remote_pin() {
    assert_multi_disc_cue_ape_per_disc_mapping(StorageMode::Remote, true).await;
}

#[tokio::test]
async fn test_multi_disc_cue_ape_remote_unpin() {
    assert_multi_disc_cue_ape_per_disc_mapping(StorageMode::Remote, false).await;
}

/// A sparse buffer pre-filled with the whole byte slice, so a decode exercises
/// the window logic without waiting on a fill.
fn buffer_from(bytes: &[u8]) -> bae_core::playback::SharedSparseBuffer {
    let buffer = bae_core::playback::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}
