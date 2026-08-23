/// The at-rest key the remote fixture encrypts under. Fixed, so the cipher each
/// cloned test rebuilds decrypts the blob the template uploaded once.
const REMOTE_FIXTURE_MASTER_KEY: [u8; 32] = [17u8; 32];

/// The multi-window CUE album imported remote-unpinned and uploaded once, for
/// every remote test to clone — the import (decode-verify + loudness over a
/// ~30 MiB FLAC) and the encrypt-and-upload of that blob are the expensive
/// half of this fixture, and neither depends on which test runs next.
///
/// The template owns the `InMemoryCloudHome` holding the encrypted blob. Its
/// clones share one backing store ("separate devices reading the same cloud
/// bucket"), so handing each test a clone gives it the same bucket with no
/// re-upload.
///
/// Nothing but the SQLite store is cloned, because nothing but the SQLite store
/// exists: the import original is deleted after the upload and a remote-unpinned
/// blob keeps no local copy, so the audio lives *only* in the cloud home. A
/// cloned test therefore has nothing on disk to resolve against and every read it
/// makes must go through a real ranged cloud fetch — the property these tests
/// exist to exercise.
struct RemoteMultiWindowTemplate {
    dir: TempDir,
    cloud: coven::InMemoryCloudHome,
    track_ids: Vec<String>,
}

static REMOTE_MULTI_WINDOW_TEMPLATE: std::sync::LazyLock<RemoteMultiWindowTemplate> =
    std::sync::LazyLock::new(|| {
        // A dedicated runtime, not a calling test's: the import and upload must
        // finish and every task/connection they spawned must be torn down
        // (dropping the runtime blocks until they are) before the template
        // directory is safe to copy — coven opens SQLite in WAL mode, so a copy
        // taken while a connection is still live could catch an unmerged -wal file.
        let rt = tokio::runtime::Runtime::new()
            .expect("build the remote multi-window template import's runtime");
        let template = rt.block_on(async {
            build_remote_multi_window_template()
                .await
                .expect("import and upload the remote multi-window template release")
        });
        drop(rt);
        template
    });

async fn build_remote_multi_window_template(
) -> Result<RemoteMultiWindowTemplate, Box<dyn std::error::Error>> {
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
    let config_handle = test_config(&library_dir);
    let library_manager = LibraryManager::new(
        database,
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    let cloud = coven::InMemoryCloudHome::new();
    library_manager
        .connect_test_cloud_home(Arc::new(cloud.clone()), remote_fixture_cipher())
        .await?;

    let runtime_handle = tokio::runtime::Handle::current();
    let release_id_key = seed_discogs_test_release(create_multi_window_cue_album());
    generate_multi_window_cue_flac_files(&album_dir);

    let import_ids = SequentialIdProvider::new("multi-window-remote-template");
    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone()).await;
    let import_id = import_ids.new_id();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "multi-window-remote-template".to_string(),
            folder: album_dir.clone(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Remote,
            pin: false,
            identity_choice: IdentityChoice::Release {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    let mut import_rx = import_handle.subscribe_import(import_id);
    let (release_id, _album_id) = wait_for_import_complete(&mut import_rx).await;

    // Run the upload so the encrypted blob lands in the cloud and the outbox
    // clears — after this the track resolves cloud-only.
    while matches!(
        library_manager.drain_uploads_for_test().await?,
        coven::DrainOutcome::Drained { uploaded, .. } if uploaded > 0
    ) {}

    // Delete the import original so file resolution can't fall back to it. With
    // the blob unpinned there is now no copy of the audio anywhere but the cloud
    // home, which is what forces every read through a ranged cloud fetch.
    std::fs::remove_dir_all(&album_dir)?;

    let tracks = library_manager.get_tracks_for_release(&release_id).await?;
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
    assert_eq!(track_ids.len(), 3, "the CUE album imports 3 tracks");

    Ok(RemoteMultiWindowTemplate {
        dir: temp_dir,
        cloud,
        track_ids,
    })
}

fn remote_fixture_cipher() -> bae_core::sync::CloudCipher {
    bae_core::sync::CloudCipher::Encrypted(coven::EncryptionService::from_key(
        REMOTE_FIXTURE_MASTER_KEY,
    ))
}

/// Clone the remote template into a fresh `TempDir`, returning it with a handle
/// on the shared cloud bucket and the template's track ids. Blocking (file I/O,
/// and dereferencing the template runs its one-time import on its own runtime),
/// so callers run it on a blocking thread. Mirrors `clone_multi_window_library`.
fn clone_remote_multi_window_library() -> (TempDir, coven::InMemoryCloudHome, Vec<String>) {
    let template = &*REMOTE_MULTI_WINDOW_TEMPLATE;
    let fresh = clone_template_library(template.dir.path());
    (fresh, template.cloud.clone(), template.track_ids.clone())
}

/// Import and upload a remote multi-window library into a bucket nothing else
/// touches, then clone it the way `clone_remote_multi_window_library` clones the
/// shared one. Blocking (an import plus file I/O), so callers run it on a
/// blocking thread.
///
/// Its own bucket, rather than a clone of the shared template's, because that one
/// serves every remote test at once: read counters taken against it would include
/// whatever the others happened to be fetching.
///
/// The import runs on its own runtime, which is dropped before the directory is
/// read — the same teardown `REMOTE_MULTI_WINDOW_TEMPLATE` performs. The result
/// is still a *clone* rather than the template directory itself: coven's
/// store-open guard is process-wide and keyed by directory, and the template's
/// own handle still holds its entry, so opening that same path again is refused
/// ("store is already open"). A clone is a different path, which is exactly why
/// every other fixture here opens one.
fn build_private_remote_multi_window_library() -> (TempDir, coven::InMemoryCloudHome, Vec<String>) {
    let rt = tokio::runtime::Runtime::new()
        .expect("build the private remote multi-window import's runtime");
    let template = rt.block_on(async {
        build_remote_multi_window_template()
            .await
            .expect("import and upload a private remote multi-window release")
    });
    drop(rt);
    let fresh = clone_template_library(template.dir.path());
    (fresh, template.cloud.clone(), template.track_ids.clone())
}

/// The multi-window CUE album imported remote-unpinned against a
/// `InMemoryCloudHome`, with the local originals deleted so every read resolves
/// through an actual ranged cloud fetch.
struct RemoteMultiWindowPlayback {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    /// The bucket this library reads through, for a test that counts what a read
    /// actually cost. Shared with every other clone of the same template, so a
    /// test asserting on its counters needs a template of its own — see
    /// `seek_over_remote_cloud_costs_chunks_not_the_whole_blob`.
    cloud: coven::InMemoryCloudHome,
    _capture_stream_rx: CaptureStreamRx,
    _temp_dir: TempDir,
}

impl RemoteMultiWindowPlayback {
    /// Build against a fresh clone of the shared remote template. `name` only
    /// labels the clone in test output — the import and upload run once per
    /// process, in `REMOTE_MULTI_WINDOW_TEMPLATE`.
    async fn new(name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let (temp_dir, cloud, track_ids) =
            tokio::task::spawn_blocking(clone_remote_multi_window_library).await?;
        tracing::debug!("remote multi-window clone ready for {name}");
        Self::over(temp_dir, cloud, track_ids).await
    }

    /// Build a playback service over an already-cloned remote library and the
    /// bucket its blob lives in. Split out so a test that counts cloud reads can
    /// supply a template it owns alone, instead of the process-wide one every
    /// other remote test is reading concurrently.
    async fn over(
        temp_dir: TempDir,
        cloud: coven::InMemoryCloudHome,
        track_ids: Vec<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let db_path = temp_dir.path().join("test.db");
        let database = Database::new_test(
            db_path.to_str().expect("db path is valid UTF-8"),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await?;
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

        // Reconnect the template's bucket: the cloned rows name the blob, but the
        // live cloud home is an in-process object no directory copy can carry.
        library_manager
            .connect_test_cloud_home(Arc::new(cloud.clone()), remote_fixture_cipher())
            .await?;

        let (playback_handle, capture_stream_rx) =
            start_capture_service(library_manager, runtime_handle);
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids,
            cloud,
            _capture_stream_rx: capture_stream_rx,
            _temp_dir: temp_dir,
        })
    }

    /// Play `track_id` and wait for its Playing state.
    async fn play_and_wait(&mut self, track_id: &str) {
        play_and_wait_on(&self.playback_handle, &mut self.progress_rx, track_id).await;
    }
}

/// Remote-cloud port of `switching_tracks_within_a_multi_window_file_keeps_streaming`:
/// switching to the middle track needs bytes fetched by an actual ranged
/// cloud read, not a local-disk read.
#[tokio::test(flavor = "multi_thread")]
async fn switching_tracks_within_a_multi_window_file_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-switch")
        .await
        .expect("set up the remote multi-window fixture");

    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    let middle_track = playback.track_ids[1].clone();
    playback.playback_handle.play(middle_track.clone());
    let playing = wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == middle_track),
        Duration::from_secs(30),
    )
    .await;
    assert!(
        playing.is_some(),
        "the middle track reaches Playing after the switch over a real ranged cloud read"
    );
}

/// Remote-cloud port of `auto_advance_crosses_gaplessly_within_a_multi_window_file`:
/// the gapless crossing's preload decodes bytes fetched over a real ranged
/// cloud read.
#[tokio::test(flavor = "multi_thread")]
async fn auto_advance_crosses_gaplessly_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-gapless")
        .await
        .expect("set up the remote multi-window fixture");
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();
    playback.play_and_wait(&first).await;

    playback.playback_handle.seek(Duration::from_secs(54));
    let outcome = observe_boundary(
        &mut playback.progress_rx,
        &first,
        &second,
        Duration::from_secs(30),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "playback crosses into the second track over a real ranged cloud read"
    );
    assert!(
        outcome.decode_stats_for_finishing && !outcome.completed_for_finishing,
        "the boundary is gapless: decode stats without TrackCompleted"
    );

    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("track 2's position must advance after the crossing over a real ranged cloud read");
}

/// Remote-cloud port of `manual_next_into_a_pregap_track_rebuilds_and_keeps_streaming`.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_pregap_track_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-next-pregap")
        .await
        .expect("set up the remote multi-window fixture");
    let [first, second] = [playback.track_ids[0].clone(), playback.track_ids[1].clone()];
    playback.play_and_wait(&first).await;

    playback.playback_handle.next();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
        Duration::from_secs(30),
    )
    .await
    .expect("the second track reaches Playing after a manual Next over a real ranged cloud read");

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(3)).await;
    assert!(
        position > 0,
        "the rebuilt track's position advances — its bytes are fetched over the real cloud path"
    );
}

/// Remote-cloud port of `manual_next_into_a_clean_track_promotes_the_preload`.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_clean_track_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-next-preload")
        .await
        .expect("set up the remote multi-window fixture");
    let [second, third] = [playback.track_ids[1].clone(), playback.track_ids[2].clone()];
    playback.play_and_wait(&second).await;

    playback.playback_handle.next();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
        Duration::from_secs(30),
    )
    .await
    .expect("the third track reaches Playing after a manual Next over a real ranged cloud read");

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(3)).await;
    assert!(
        position > 0,
        "the promoted track's position advances — its bytes are fetched over the real cloud path"
    );
}

/// Remote-cloud seek port: seeking within the last track over a real ranged
/// cloud read must still land near the target and keep streaming.
#[tokio::test(flavor = "multi_thread")]
async fn seek_within_the_last_track_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-seek")
        .await
        .expect("set up the remote multi-window fixture");
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    let target = Duration::from_secs(20);
    playback.playback_handle.seek(target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(30))
        .await
        .expect("a seek within the last track emits Seeked over a real ranged cloud read");
    assert!(
        Duration::from_millis(landed).abs_diff(target) < Duration::from_secs(2),
        "should land near {target:?}, got {landed}ms"
    );

    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must keep flowing after the seek over a real ranged cloud read");
}

/// No single ranged read may exceed this. The fill asks for one 4 MiB window at a
/// time and coven serves the chunks covering it, so the largest read is a window
/// plus chunk alignment; the slack here is for that alignment, not for a second
/// window. This is the assertion that states the property — *a range costs its own
/// bytes* — and it holds however many windows the fill happens to pull.
const SEEK_LARGEST_RANGE_BOUND: u64 = 5 * 1024 * 1024;

/// Everything the seek's ranged reads move, together, must stay under this — half
/// the fixture's 31,922,637-byte blob. Loose on purpose: the fill keeps reading
/// ahead while the seek settles, so the total lands on a whole number of 4 MiB
/// windows (measured at one or two, 4,195,328 or 8,521,760 bytes, depending on
/// how far read-ahead got before the counter was read). The point it pins is only
/// that a seek never drags the file across, so it is bounded well above that
/// race and well below the blob.
const SEEK_RANGE_BYTES_BOUND: u64 = 16 * 1024 * 1024;

/// Coarse backstop on the seek itself. Serving a range used to mean downloading,
/// decrypting and hashing the whole blob — seconds per read, several reads to a
/// seek — so this is far above a chunked seek and far below the old cost.
const SEEK_COST_BACKSTOP: Duration = Duration::from_secs(15);

/// A seek over a remote cloud home costs the chunks its range touches, not the
/// whole blob. coven seals a blob per chunk and authenticates each chunk on its
/// own, so a ranged read fetches only the chunks covering it: seeking near the end
/// of the last track moves 4,195,328 bytes — the fill's one 4 MiB window and a
/// kilobyte of probe — out of a 31,922,637-byte blob.
///
/// Before chunked ranges, opening a remote stream downloaded, decrypted and hashed
/// the entire blob before it could serve a byte. The byte count is what pins the
/// shape here, because it is exact where a stopwatch is load-dependent, and it is
/// two-sided: see the assertions. The elapsed bound is only a coarse backstop.
///
/// This test owns its bucket. Every other remote test clones the shared template
/// and reads one `InMemoryCloudHome` between them, so counters taken there would
/// include their traffic.
#[tokio::test(flavor = "multi_thread")]
async fn seek_over_remote_cloud_costs_chunks_not_the_whole_blob() {
    let (dir, cloud, track_ids) =
        tokio::task::spawn_blocking(build_private_remote_multi_window_library)
            .await
            .expect("build a private remote multi-window library");
    let mut playback = RemoteMultiWindowPlayback::over(dir, cloud, track_ids)
        .await
        .expect("set up playback over the private remote fixture");

    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    // Whatever starting the track cost is already paid; measure the seek alone.
    playback.cloud.clear_exact_range_reads();

    // Near the end of the last track (it runs 2:00–3:00 of the file, so this
    // lands ~10 s from the end). Deliberately not mid-track: with a track ceiling
    // set the fill reads ahead to the end of the *current track*, which for a
    // seek to 0:20 would pull the remaining ~40 s — read-ahead the seek did not
    // need, swamping the thing being measured. Landing near the end leaves only a
    // short tail to read ahead into, so what the counter sees is the seek's own
    // window.
    let target = Duration::from_secs(50);
    let started = Instant::now();
    playback.playback_handle.seek(target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, SEEK_COST_BACKSTOP)
        .await
        .expect("the seek must land within the backstop over a chunked ranged read");
    assert!(
        Duration::from_millis(landed).abs_diff(target) < Duration::from_secs(2),
        "the seek should land near {target:?}, got {landed}ms"
    );
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must flow from the completed chunked-range seek");
    let elapsed = started.elapsed();

    // Ranged reads are the measurement, not `exact_full_read_count`: that counter
    // is bucket-wide and the sync loop reads whole objects (commits, snapshots)
    // throughout, so it climbs by the hundreds here for reasons that have nothing
    // to do with playback.
    //
    // Three checks, catching different regressions. That any ranged read happened
    // at all is what says the blob was served by range: were the whole-object
    // download back, the audio would arrive as one full read and this list would
    // be empty. The largest single read is the property itself — a range costs its
    // own bytes — and is independent of how many windows read-ahead pulled. The
    // total is the coarse backstop that no seek drags the file across.
    let reads = playback.cloud.exact_range_reads();
    let range_bytes = playback.cloud.exact_range_read_bytes();
    let largest = reads.iter().map(|(start, end)| end - start).max();
    let largest = largest.expect(
        "the seek served no ranged read at all -- the blob was fetched whole, not by range",
    );
    assert!(
        largest <= SEEK_LARGEST_RANGE_BOUND,
        "the seek's largest single ranged read was {largest} bytes (bound \
         {SEEK_LARGEST_RANGE_BOUND}); a chunked range must cost the chunks it covers, not the \
         31,922,637-byte blob. All reads: {reads:?}",
    );
    assert!(
        range_bytes < SEEK_RANGE_BYTES_BOUND,
        "the seek fetched {range_bytes} bytes from the cloud across {} reads (bound \
         {SEEK_RANGE_BYTES_BOUND}); a seek must not drag the whole blob across",
        reads.len(),
    );
    assert!(
        elapsed < SEEK_COST_BACKSTOP,
        "the seek took {elapsed:?}, past the {SEEK_COST_BACKSTOP:?} backstop"
    );
}

/// A sparse buffer pre-filled with the whole byte slice, so a decode exercises
/// the window logic without waiting on a fill.
fn buffer_from(bytes: &[u8]) -> bae_core::playback::SharedSparseBuffer {
    let buffer = bae_core::playback::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}
