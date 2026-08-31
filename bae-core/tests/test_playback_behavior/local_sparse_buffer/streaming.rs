/// One ~30 MiB noise FLAC shared by every multi-window test. Encoded once —
/// per-test encoding would dominate the suite's wall time. White noise
/// compresses poorly, so 3 minutes of it stays near raw PCM size.
static MULTI_WINDOW_FLAC: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
    use rand::{Rng, SeedableRng};

    bae_core::audio_codec::init();

    const SAMPLE_RATE: u32 = 44_100;
    const TRACK_SECONDS: u32 = 60;
    const TRACKS: u32 = 3;

    let frames = (SAMPLE_RATE * TRACK_SECONDS * TRACKS) as usize;
    // The pregap region (0:58-1:00, track 2's INDEX 00 gap) is silence, like a
    // real rip's pregap. Silence compresses to a few KB, so the pregap becomes
    // a tiny segment whose decode must read past its own end byte (raw-FLAC
    // frame parsing needs lookahead) -- the fill has to serve a reader at its
    // read-ahead ceiling or the preload decoder deadlocks.
    let pregap_samples = (SAMPLE_RATE * 58) as usize * 2..(SAMPLE_RATE * 60) as usize * 2;
    let mut rng = rand::rngs::StdRng::seed_from_u64(0x0bae);
    let samples: Vec<i32> = (0..frames * 2)
        .map(|i| {
            if pregap_samples.contains(&i) {
                0
            } else {
                (rng.random::<i16>() as i32) << 16
            }
        })
        .collect();
    let flac = bae_core::audio_codec::encode_i32(
        bae_core::audio_codec::EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &samples,
        SAMPLE_RATE,
        2,
    )
    .expect("encode the multi-window noise FLAC");
    assert!(
        flac.len() > 24 * 1024 * 1024,
        "the fixture must span several 4 MiB fetch windows, got {} bytes",
        flac.len()
    );
    flac
});

/// A 3-track CUE album over one FLAC large enough that the streaming reader's
/// fetch windows (4 MiB each) cover only slices of it — playing one track
/// buffers the file head (container probe) and a window at that track's byte
/// offset, leaving the other tracks' regions unfetched.
///
/// The boundaries are deliberately mixed: track 2 has a 2-second pregap
/// (INDEX 00 at 0:58), so a manual Next into it takes the pregap-skip rebuild
/// path; track 3 has none, so a manual Next into it promotes the preloaded
/// stream. Raw in-track timelines by construction: track 1 spans 0:00–0:58
/// (ends at track 2's INDEX 00), track 2 spans 0:58–2:00 (62 s including its
/// pregap), track 3 spans 2:00–3:00.
fn generate_multi_window_cue_flac_files(dir: &std::path::Path) {
    std::fs::write(
        dir.join("Multi Window Album.flac"),
        MULTI_WINDOW_FLAC.as_slice(),
    )
    .expect("write the multi-window FLAC");

    let cue = "\
PERFORMER \"Test Artist\"
TITLE \"Multi Window Album\"
FILE \"Multi Window Album.flac\" WAVE
  TRACK 01 AUDIO
    TITLE \"Multi Window One\"
    PERFORMER \"Test Artist\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Multi Window Two\"
    PERFORMER \"Test Artist\"
    INDEX 00 00:58:00
    INDEX 01 01:00:00
  TRACK 03 AUDIO
    TITLE \"Multi Window Three\"
    PERFORMER \"Test Artist\"
    INDEX 01 02:00:00
";
    std::fs::write(dir.join("Multi Window Album.cue"), cue).expect("write the multi-window CUE");
}

fn create_multi_window_cue_album() -> DiscogsRelease {
    let track = |position: &str, title: &str| DiscogsTrack {
        type_: "track".to_string(),
        position: position.to_string(),
        title: title.to_string(),
        duration: Some("1:00".to_string()),
        artists: vec![],
        extraartists: None,
        sub_tracks: vec![],
    };
    DiscogsRelease {
        id: "multi-window-cue-release".to_string(),
        title: "Multi Window Album".to_string(),
        year: Some(2024),
        format: vec![],
        country: Some("US".to_string()),
        label: vec!["Test Label".to_string()],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            name: "Test Artist".to_string(),
            id: "test-artist-1".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: vec![
            track("1", "Multi Window One"),
            track("2", "Multi Window Two"),
            track("3", "Multi Window Three"),
        ],
        master_id: Some("multi-window-cue-master".to_string()),
    }
}

/// The multi-window CUE album, imported once (decode-verify + loudness over
/// ~30 MiB) into a template library directory that outlives every test. Each
/// `MultiWindowPlayback` clones the template's small DB/library files into
/// its own `TempDir` instead of re-importing, so porting many behaviors onto
/// this fixture doesn't multiply the import cost by the number of tests.
///
/// The audio file itself is never cloned: `local_blob_refs` stores an
/// absolute path, so a clone's DB rows point straight back at the template's
/// stable `album/` directory, which is held alive for the process's lifetime.
struct MultiWindowTemplate {
    dir: TempDir,
    track_ids: Vec<String>,
}

static MULTI_WINDOW_TEMPLATE: std::sync::LazyLock<MultiWindowTemplate> =
    std::sync::LazyLock::new(|| {
        // A dedicated runtime, not the calling test's: the import must finish
        // and every task/connection it spawned must be torn down (dropping
        // the runtime blocks until they are) before the template directory
        // is safe to copy — coven opens SQLite in WAL mode, so a copy taken
        // while a connection is still live could catch an unmerged -wal file.
        let rt = tokio::runtime::Runtime::new().expect("build the template import's runtime");
        let template = rt.block_on(async {
            let import_ids = SequentialIdProvider::new("multi-window-template");
            let setup = imported_release_setup(
                create_multi_window_cue_album(),
                "multi-window-template",
                import_ids.new_id(),
                generate_multi_window_cue_flac_files,
                |_| Ok(()),
            )
            .await
            .expect("import the multi-window template release");
            assert_eq!(setup.track_ids.len(), 3, "the CUE album imports 3 tracks");
            MultiWindowTemplate {
                dir: setup.temp_dir,
                track_ids: setup.track_ids,
            }
        });
        drop(rt);
        template
    });

/// Recursively copy `src` into `dst` (which must not yet exist).
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).expect("create cloned directory");
    for entry in std::fs::read_dir(src).expect("read directory to clone") {
        let entry = entry.expect("directory entry to clone");
        let dest = dst.join(entry.file_name());
        let file_type = entry.file_type().expect("entry file type");
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), &dest).expect("clone file");
        }
    }
}

/// SQLite's WAL-mode sidecar files: present only while a connection has
/// uncommitted WAL data, and reclaimed by SQLite itself around a connection's
/// close. A file matching one of these suffixes disappearing between
/// `read_dir` and `copy` is that reclaim, not a real race on the template's
/// stable content — its absence at clone time means "fully checkpointed,"
/// exactly the state a fresh open of the (always-present) main `.db` file
/// needs.
fn is_sqlite_wal_sidecar(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name.ends_with("-wal") || name.ends_with("-shm") || name.ends_with("-journal")
}

/// Clone a template library's DB/library files into a fresh `TempDir`, skipping
/// the `album/` directory — the imported audio stays at the template's own
/// stable path, which the cloned DB's `local_blob_refs` rows (absolute paths)
/// already point at — and skipping SQLite's WAL sidecars, reclaimed around a
/// connection's close. Blocking (file I/O only); callers run it on a blocking
/// thread. Shared by every import-once template fixture so the WAL-race
/// handling lives in one place.
fn clone_template_library(template_dir: &std::path::Path) -> TempDir {
    let fresh = TempDir::new().expect("fresh cloned library dir");
    for entry in std::fs::read_dir(template_dir).expect("read template dir") {
        let entry = entry.expect("template dir entry");
        if entry.file_name() == "album" {
            continue;
        }
        let dest = fresh.path().join(entry.file_name());
        let file_type = entry.file_type().expect("template entry file type");
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest);
        } else if let Err(e) = std::fs::copy(entry.path(), &dest) {
            if e.kind() == std::io::ErrorKind::NotFound && is_sqlite_wal_sidecar(&entry.file_name())
            {
                debug!(
                    "template clone: {:?} was reclaimed before copy (fully checkpointed); skipping",
                    entry.file_name()
                );
            } else {
                panic!("clone template file {:?}: {e}", entry.path());
            }
        }
    }
    fresh
}

fn clone_multi_window_library() -> (TempDir, Vec<String>) {
    let template = &*MULTI_WINDOW_TEMPLATE;
    let fresh = clone_template_library(template.dir.path());
    (fresh, template.track_ids.clone())
}

/// An imported multi-window CUE album with a real-time-paced playback service
/// over it, for tests that drive switches, advances, and boundaries against a
/// sparsely buffered file.
struct MultiWindowPlayback {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
    _capture_stream_rx: CaptureStreamRx,
    _temp_dir: TempDir,
}

impl MultiWindowPlayback {
    /// Build against a fresh clone of the shared multi-window template. `name`
    /// only labels the clone in test failure output — the import itself runs
    /// once per process, in `MULTI_WINDOW_TEMPLATE`.
    async fn new(name: &str) -> Self {
        let (temp_dir, track_ids) = tokio::task::spawn_blocking(clone_multi_window_library)
            .await
            .expect("clone the multi-window template library");
        tracing::debug!("multi-window clone ready for {name}");

        let db_path = temp_dir.path().join("test.db");
        let database = Database::new_test(
            db_path.to_str().expect("db path is valid UTF-8"),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .expect("open the cloned multi-window database");
        let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
        let config_handle = test_config(&library_dir);
        let runtime_handle = tokio::runtime::Handle::current();
        let library_manager = LibraryManager::new(
            database,
            config_handle,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            runtime_handle.clone(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );

        let (playback_handle, capture_stream_rx) =
            start_capture_service(library_manager.clone(), runtime_handle.clone());
        let progress_rx = playback_handle.subscribe_progress();
        Self {
            playback_handle,
            progress_rx,
            track_ids,
            library_manager,
            runtime_handle,
            _capture_stream_rx: capture_stream_rx,
            _temp_dir: temp_dir,
        }
    }

    /// Play `track_id` and wait for its Playing state.
    async fn play_and_wait(&mut self, track_id: &str) {
        play_and_wait_on(&self.playback_handle, &mut self.progress_rx, track_id).await;
    }
}

/// Switching tracks within a single multi-window file must keep the shared
/// file buffer streaming. Playing the last track buffers the file head and a
/// window at that track's offset; the middle track's bytes are in neither, so
/// reaching Playing after the switch requires the buffer's fill task to still
/// be fetching on demand. A fill task killed by the switch leaves the decoder
/// waiting forever on bytes that never arrive: no Playing, position pinned at
/// the start.
#[tokio::test(flavor = "multi_thread")]
async fn switching_tracks_within_a_multi_window_file_keeps_streaming() {
    let mut playback = MultiWindowPlayback::new("multi-window-switch").await;

    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    let middle_track = playback.track_ids[1].clone();
    playback.playback_handle.play(middle_track.clone());
    let playing = wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == middle_track),
        Duration::from_secs(20),
    )
    .await;
    assert!(
        playing.is_some(),
        "the middle track reaches Playing after the switch — its bytes must be fetched on demand"
    );
}

/// Natural track ends within a multi-window file cross gaplessly, twice in a
/// row. Each crossing proves the preload actually decoded the next track's
/// bytes — regions no earlier fetch buffered, so the shared buffer's fill task
/// must serve the preload decoder's demand while the current track plays. The
/// seeks that pull each boundary closer land in unbuffered regions too, so
/// mid-track sparse seeking is exercised on the way.
#[tokio::test(flavor = "multi_thread")]
async fn auto_advance_crosses_gaplessly_within_a_multi_window_file() {
    let mut playback = MultiWindowPlayback::new("multi-window-gapless").await;
    let [first, second, third] = [
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    ];

    playback.play_and_wait(&first).await;

    // Track 1's raw timeline is 58 s (it ends at track 2's INDEX 00); seek near
    // the end so the boundary arrives in a few real-time seconds.
    playback.playback_handle.seek(Duration::from_secs(54));
    let outcome = observe_boundary(
        &mut playback.progress_rx,
        &first,
        &second,
        Duration::from_secs(25),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "playback crosses into the second track (through its pregap)"
    );
    assert!(
        outcome.decode_stats_for_finishing && !outcome.completed_for_finishing,
        "the first boundary is gapless: decode stats without TrackCompleted"
    );

    // The crossing events fire at the boundary whether or not audio flows, so
    // require the position to actually advance into track 2 before seeking --
    // a preload decoder starved at the pregap boundary pins it in place.
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect(
            "track 2's position must advance after the crossing -- its decoder must keep \
             producing samples past the pregap boundary",
        );

    // Track 2's raw timeline is 62 s (2 s pregap + 60 s).
    playback.playback_handle.seek(Duration::from_secs(58));
    let outcome = observe_boundary(
        &mut playback.progress_rx,
        &second,
        &third,
        Duration::from_secs(25),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "playback crosses into the third track"
    );
    assert!(
        outcome.decode_stats_for_finishing && !outcome.completed_for_finishing,
        "the second boundary is gapless: the crossing handler re-preloaded track 3"
    );

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(1)).await;
    assert!(
        position > 0,
        "the third track's position advances — samples are flowing after two crossings"
    );
}

/// A manual Next into a track WITH a pregap can't use the preloaded stream
/// (it was decoded from INDEX 00; a manual skip starts at INDEX 01), so it
/// discards the preload and rebuilds through play_track. The rebuild re-reads
/// the same file — its shared buffer and fill task must survive the discard
/// for the skip to ever produce audio.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_pregap_track_rebuilds_and_keeps_streaming() {
    let mut playback = MultiWindowPlayback::new("multi-window-next-pregap").await;
    let [first, second] = [playback.track_ids[0].clone(), playback.track_ids[1].clone()];

    playback.play_and_wait(&first).await;

    playback.playback_handle.next();
    let states = collect_states_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
        Duration::from_secs(20),
    )
    .await;
    assert!(
        matches!(&states.last(), Some(PlaybackState::Playing { track_info, .. }) if track_info.track_id == second),
        "the second track reaches Playing after a manual Next, got {states:?}"
    );
    assert!(
        states
            .iter()
            .any(|s| matches!(s, PlaybackState::Loading { track_id, .. } if *track_id == second)),
        "a pregap skip rebuilds through play_track, which surfaces a Loading arc"
    );

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(1)).await;
    assert!(
        position > 0,
        "the rebuilt track's position advances — its bytes are fetched on demand"
    );
}

/// A manual Next into a track WITHOUT a pregap promotes the preloaded stream:
/// no rebuild, so no Loading arc — Playing lands directly. The promoted
/// decoder then streams bytes from an unbuffered region of the shared file,
/// which only works while the buffer's fill task is alive.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_clean_track_promotes_the_preload() {
    let mut playback = MultiWindowPlayback::new("multi-window-next-preload").await;
    let [second, third] = [playback.track_ids[1].clone(), playback.track_ids[2].clone()];

    playback.play_and_wait(&second).await;

    playback.playback_handle.next();
    let states = collect_states_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
        Duration::from_secs(20),
    )
    .await;
    assert!(
        matches!(&states.last(), Some(PlaybackState::Playing { track_info, .. }) if track_info.track_id == third),
        "the third track reaches Playing after a manual Next, got {states:?}"
    );
    assert!(
        !states
            .iter()
            .any(|s| matches!(s, PlaybackState::Loading { .. })),
        "a pregap-free Next promotes the preloaded stream — no Loading arc, got {states:?}"
    );

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(1)).await;
    assert!(
        position > 0,
        "the promoted track's position advances — its bytes are fetched on demand"
    );
}

/// Every seek test elsewhere in this suite runs on a single-window fixture
/// that is fully buffered the moment it opens — seeking has never been
/// exercised against a sparse, multi-range buffer. Playing the last track
/// buffers only the file head and this track's own region (the other
/// tracks' regions, including everywhere earlier in the track's own raw
/// timeline that a backward seek can reach, are untouched); seeking backward
/// and then forward within it runs the real seek machinery — decoder
/// rebuild, byte-seek, demand publication, window fetch — over holes in the
/// buffer, not a fully-populated one.
#[tokio::test(flavor = "multi_thread")]
async fn seek_within_the_last_track_lands_and_keeps_streaming_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-sparse").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    // Seek backward toward the start of the track's own raw timeline — into a
    // region the initial fetch (which lands at the track's start byte) may
    // already partly cover, but well behind wherever playback has since moved.
    let backward_target = Duration::from_secs(5);
    playback.playback_handle.seek(backward_target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("a backward seek within the last track emits Seeked");
    assert!(
        Duration::from_millis(landed).abs_diff(backward_target) < Duration::from_secs(2),
        "backward seek should land near {backward_target:?}, got {landed}ms"
    );
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must keep flowing after the backward seek");

    // Seek forward, well past the backward target and into territory the fill
    // has not fetched — the fill's demand-driven window fetch must catch up.
    let forward_target = Duration::from_secs(40);
    playback.playback_handle.seek(forward_target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("a forward seek within the last track emits Seeked");
    assert!(
        Duration::from_millis(landed).abs_diff(forward_target) < Duration::from_secs(2),
        "forward seek should land near {forward_target:?}, got {landed}ms"
    );
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must keep flowing after the forward seek");
}

/// A seek issued the instant Playing arrives — before the fill has had any
/// settle time to buffer ahead of the startup window — must still land and
/// produce audio rather than deadlock waiting on bytes the sparse buffer
/// hasn't fetched yet.
#[tokio::test(flavor = "multi_thread")]
async fn seek_immediately_after_playing_lands_and_plays_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-immediate").await;
    let last_track = playback.track_ids[2].clone();

    playback.playback_handle.play(last_track.clone());
    let playing = wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == last_track),
        Duration::from_secs(20),
    )
    .await;
    assert!(playing.is_some(), "the last track reaches Playing");

    // Seek immediately — no settle time for the fill to get ahead.
    let target = Duration::from_secs(20);
    playback.playback_handle.seek(target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("a seek issued right after Playing still emits Seeked");
    assert!(
        Duration::from_millis(landed).abs_diff(target) < Duration::from_secs(2),
        "should land near {target:?}, got {landed}ms"
    );
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must play after an immediate seek");
}

/// A seek into an unbuffered region surfaces a buffering state before it
/// confirms: the seek target shows as `Loading { resolved: Some(..) }` (the
/// metadata is already known — this is a seek, not a fresh play), and `Seeked`
/// follows only once the demanded window lands and the ready-watcher fires. A
/// fully-buffered file already has the window, so this arc only appears over a
/// sparse buffer.
#[tokio::test(flavor = "multi_thread")]
async fn seek_into_an_unbuffered_region_emits_resolved_loading_before_seeked() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-loading").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    // Seek forward into territory the demand-driven fill has not fetched yet.
    playback.playback_handle.seek(Duration::from_secs(40));

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut saw_loading = false;
    let mut saw_seeked = false;
    while Instant::now() < deadline && !saw_seeked {
        match timeout(Duration::from_millis(200), playback.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged {
                state:
                    PlaybackState::Loading {
                        track_id,
                        resolved: Some(_),
                    },
            })) if track_id == last_track => {
                saw_loading = true;
            }
            Ok(Some(PlaybackProgress::Seeked { track_id, .. })) if track_id == last_track => {
                assert!(
                    saw_loading,
                    "a resolved Loading must arrive before Seeked on a seek into an unbuffered region"
                );
                saw_seeked = true;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(saw_loading, "the seek must emit a resolved Loading state");
    assert!(
        saw_seeked,
        "the seek must emit Seeked once the window lands"
    );
}

// ============================================================================
// Port matrix: behaviors that involve more than decode, run against the real
// sparse multi-window fixture instead of only the fully-buffered single-window
// one. Each ports an existing single-window test's assertion; see that test's
// name in this section's doc comments for the source. The single-window
// originals stay — they are the fast decode-correctness/XLD-alignment
// coverage — this only adds the streaming-relevant coverage they never had.
// ============================================================================

/// Multi-window port of `seek_past_end_of_track_signals_rather_than_hanging`:
/// a seek well past the last track's end must still SIGNAL (`Seeked` or
/// `PlaybackError`), not freeze — over the sparse buffer, the target is past
/// everything the fill has fetched, so the end-of-stream resolution has to
/// happen without the byte-fetch machinery ever landing more data.
#[tokio::test(flavor = "multi_thread")]
async fn seek_past_end_of_track_signals_rather_than_hanging_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-past-end").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    // Track 3's raw timeline is 60s; 600s is far past the end.
    playback.playback_handle.seek(Duration::from_secs(600));

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut signaled = false;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), playback.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::Seeked { .. }))
            | Ok(Some(PlaybackProgress::PlaybackError { .. })) => {
                signaled = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(
        signaled,
        "a seek past the end must signal (Seeked or PlaybackError), not freeze silently, \
         even when the target is well past everything the sparse buffer has fetched"
    );
}

/// Multi-window port of `seek_by_ratio_maps_to_a_proportional_position`:
/// `SeekByRatio`'s position math runs down through the same `seek()` this
/// suite already drives directly, but over the sparse buffer instead of a
/// fully-buffered file.
#[tokio::test(flavor = "multi_thread")]
async fn seek_by_ratio_maps_to_a_proportional_position_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-ratio").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    playback.playback_handle.seek_by_ratio(0.25);
    let quarter = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("Seeked for ratio 0.25");
    playback.playback_handle.seek_by_ratio(0.75);
    let three_quarter = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("Seeked for ratio 0.75");

    assert!(
        quarter > 1_000,
        "ratio 0.25 should land well past the start, got {quarter}ms"
    );
    assert!(
        three_quarter > quarter + 1_000,
        "ratio 0.75 should land clearly later than 0.25 (quarter={quarter}ms three_quarter={three_quarter}ms)"
    );
}

/// Multi-window port of `test_pause_seek_resume_advances_position`: seeking
/// while paused must not auto-play, and resuming afterward must produce
/// audio — over the sparse buffer, the resumed decoder has to fetch the seek
/// target's window on demand rather than reading it out of an
/// already-fully-buffered file.
#[tokio::test(flavor = "multi_thread")]
async fn pause_seek_resume_advances_position_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-pause-seek-resume").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    playback.playback_handle.pause();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Paused { .. }),
        Duration::from_secs(5),
    )
    .await
    .expect("playback should pause");

    let seek_target = Duration::from_secs(20);
    playback.playback_handle.seek(seek_target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("Seeked after seeking while paused");
    assert!(
        Duration::from_millis(landed).abs_diff(seek_target) < Duration::from_secs(2),
        "should land near {seek_target:?}, got {landed}ms"
    );

    let auto_played = wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_millis(500),
    )
    .await;
    assert!(
        auto_played.is_none(),
        "should still be paused after seek, not auto-playing"
    );

    playback.playback_handle.resume();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_secs(5),
    )
    .await
    .expect("resume should start playing");

    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must advance after resuming a paused sparse-buffer seek");
}

/// Multi-window port of `test_direct_play_skips_pregap`: a direct selection
/// of the pregapped track (track 2) must skip its 2s pregap and let position
/// climb from the very start — over the sparse buffer, this is the decoder's
/// first read landing on a window the fill has to fetch fresh, not one
/// already sitting in a fully-buffered file.
#[tokio::test(flavor = "multi_thread")]
async fn direct_play_skips_pregap_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-direct-skip-pregap").await;
    let pregapped_track = playback.track_ids[1].clone();
    playback.play_and_wait(&pregapped_track).await;

    let position_ms = position_after(&mut playback.progress_rx, Duration::from_millis(1200)).await;
    assert!(
        position_ms > 600,
        "direct play should skip the 2s pregap and let position climb from 0; \
         got {position_ms}ms ~1.2s in (a played pregap would keep it pinned at 0)",
    );
}

/// Multi-window port of `test_auto_advance_plays_pregap`'s "position pinned
/// during the pregap" assertion — `auto_advance_crosses_gaplessly_within_a_multi_window_file`
/// already proves the crossing happens and that audio keeps flowing
/// afterward, but never checks that the pregap itself is actually played
/// (position pinned near 0) rather than silently skipped.
#[tokio::test(flavor = "multi_thread")]
async fn auto_advance_plays_pregap_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-auto-advance-pregap").await;
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();
    playback.play_and_wait(&first).await;

    // Track 1 runs 0–58s; seek near the end so the crossing arrives soon.
    playback.playback_handle.seek(Duration::from_secs(56));
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
        Duration::from_secs(25),
    )
    .await
    .expect("playback should auto-advance into the pregapped track");

    // ~1s into track 2 the 2s pregap is still playing: position pinned at 0.
    let during_pregap =
        position_after(&mut playback.progress_rx, Duration::from_millis(1000)).await;
    assert!(
        during_pregap < 600,
        "auto-advance should play the pregap: position stays pinned at 0 across it, \
         got {during_pregap}ms ~1s in (a skipped pregap would already be climbing)",
    );
}
