/// Playback over uploaded files with neither originals nor cached audio present.
struct RemoteMultiWindowPlayback {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    cloud: Arc<coven::MemoryStorage>,
    _capture_stream_rx: support::CaptureStreamRx,
    _temp_dir: TempDir,
}

impl RemoteMultiWindowPlayback {
    async fn new(name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;
        let (library_manager, _, cloud) = support::storage_library(temp_dir.path(), bae_core::providers::Providers::offline()).await;
        support::connect_storage(&library_manager, &cloud).await;
        generate_multi_window_cue_flac_files(&album_dir);
        let release_key = seed_discogs_test_release(library_manager.providers(), create_multi_window_cue_album());
        let import_handle = start_test_import(tokio::runtime::Handle::current(), library_manager.clone()).await;
        let import_id = uuid::Uuid::new_v4().to_string();
        import_handle.send_command(ImportCommand {
            candidate_key: name.into(),
            destination: ImportDestination::Remote { pin: false },
            ..support::folder_import(&import_id, album_dir.clone(), support::discogs_release(release_key))
        }).await?;
        let (release_id, _) = wait_for_import_complete(&mut import_handle.subscribe_import(import_id)).await;
        library_manager.drain_uploads_expecting_work().await?;
        for file in library_manager.get_files_for_release(&release_id).await? {
            let reference = library_manager.release_blob_ref_for_test(&file.id).await?;
            library_manager.evict_blob_for_test(&reference).await?;
        }
        std::fs::remove_dir_all(album_dir)?;
        let track_ids = library_manager.get_tracks_for_release(&release_id).await?.into_iter().map(|track| track.id).collect();
        let (playback_handle, capture_stream_rx) = start_capture_service(library_manager, tokio::runtime::Handle::current());
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self { playback_handle, progress_rx, track_ids, cloud, _capture_stream_rx: capture_stream_rx, _temp_dir: temp_dir })
    }

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
        |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == middle_track),
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

    playback.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(54)));
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
        |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == second),
        Duration::from_secs(30),
    )
    .await
    .expect("the second track reaches Playing after a manual Next over a real ranged cloud read");

    wait_for_track_position_where(&mut playback.progress_rx, &second, |ms| ms > 0)
        .await
        .expect(
            "the rebuilt track's position advances — its bytes are fetched over the real cloud path",
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
        |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == third),
        Duration::from_secs(30),
    )
    .await
    .expect("the third track reaches Playing after a manual Next over a real ranged cloud read");

    wait_for_track_position_where(&mut playback.progress_rx, &third, |ms| ms > 0)
        .await
        .expect(
            "the promoted track's position advances — its bytes are fetched over the real cloud path",
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
    playback.playback_handle.seek(TrackTime::from_duration(target));
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
/// and reads one `MemoryStorage` between them, so counters taken there would
/// include their traffic.
#[tokio::test(flavor = "multi_thread")]
async fn seek_over_remote_cloud_costs_chunks_not_the_whole_blob() {
    let mut playback = RemoteMultiWindowPlayback::new("remote-seek-ranges").await
        .expect("set up the private remote fixture");

    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    // Whatever starting the track cost is already paid; measure the seek alone.
    let first_read = playback.cloud.reads().await.len();

    // Near the end of the last track (it runs 2:00–3:00 of the file, so this
    // lands ~10 s from the end). Deliberately not mid-track: with a track ceiling
    // set the fill reads ahead to the end of the *current track*, which for a
    // seek to 0:20 would pull the remaining ~40 s — read-ahead the seek did not
    // need, swamping the thing being measured. Landing near the end leaves only a
    // short tail to read ahead into, so what the counter sees is the seek's own
    // window.
    let target = Duration::from_secs(50);
    let started = Instant::now();
    playback.playback_handle.seek(TrackTime::from_duration(target));
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
    let all_reads = playback.cloud.reads().await;
    let reads = &all_reads[first_read..];
    let range_bytes: u64 = reads.iter().map(|(_, _, length)| *length).sum();
    let largest = reads.iter().map(|(_, _, length)| *length).max();
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
