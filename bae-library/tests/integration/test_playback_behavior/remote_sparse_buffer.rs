use crate::test_playback_behavior::fixtures::*;
use crate::test_playback_behavior::support::*;

/// Playback over uploaded files with neither originals nor cached audio present.
struct RemoteMultiWindowPlayback {
    playback_handle: bae_playback::PlaybackHandle,
    progress_rx: bae_playback::PlaybackProgressFeed,
    track_ids: Vec<String>,
    cloud: Arc<coven::MemoryStorage>,
    /// Where the album's FLAC is stored.
    flac_object: coven::ObjectPath,
    library: TestLibrary,
    _capture_stream_rx: bae_playback::test_capture::CaptureStreamRx,
    /// The capture device's audio time, which only the test moves.
    audio_clock: AudioClock,
    _temp_dir: TempDir,
}

impl RemoteMultiWindowPlayback {
    async fn new(name: &str) -> Self {
        Self::import(name).await.expect("set up the remote multi-window fixture")
    }

    /// Import the multi-window album into a library on memory storage,
    /// upload it, and drop every local copy, so playback reads storage.
    async fn import(name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        bae_util::test_logs::install();
        let temp_dir = TempDir::new()?;
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;
        let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
        let (library, cloud) = bae_library::test_libraries::storage_library(
            temp_dir.path(),
            fakes.providers(),
            bae_util::http::Http::for_test(),
        )
        .await;
        bae_library::test_libraries::connect_storage(library.manager(), &cloud).await;
        generate_multi_window_cue_flac_files(&album_dir);
        let release_key = fakes.seed_discogs_test_release(create_multi_window_cue_album());
        let import_handle = library.start_discogs_import().await;
        let import_id = uuid::Uuid::new_v4().to_string();
        import_handle
            .send_command(ImportCommand {
                candidate_key: bae_import::CandidateKey::from(name.as_bytes().to_vec()),
                destination: ImportDestination::Remote { pin: false },
                ..bae_import::test_imports::folder_import(
                    &import_id,
                    album_dir.clone(),
                    bae_import::test_imports::discogs_release(release_key),
                )
            })
            .await?;
        let (release_id, _) =
            wait_for_import_complete(&mut import_handle.subscribe_import(import_id)).await;
        library.manager().drain_uploads_expecting_work().await?;
        let files = library.manager().get_files_for_release(&release_id).await?;
        let flac = files
            .iter()
            .find(|file| file.original_filename.ends_with(".flac"))
            .expect("the album has its FLAC");
        let flac_ref = library.manager().release_file_ref_for_test(&flac.id).await?;
        let coven::FilePlace::Cloud { location, .. } = flac_ref.place() else {
            panic!("the uploaded FLAC is stored");
        };
        let flac_object = coven::ObjectPath::file(
            library.store().store_id(),
            location.uploader(),
            location.object(),
        );
        for file in files {
            library.manager().evict_file_for_test(file.file).await?;
        }
        std::fs::remove_dir_all(album_dir)?;
        let track_ids = library
            .manager()
            .get_tracks_for_release(&release_id)
            .await?
            .into_iter()
            .map(|track| track.id)
            .collect();
        let (playback_handle, capture_stream_rx, audio_clock) =
            start_capture_service(&library).await;
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids,
            cloud,
            flac_object,
            library,
            _capture_stream_rx: capture_stream_rx,
            audio_clock,
            _temp_dir: temp_dir,
        })
    }

    async fn play_and_wait(&mut self, track_id: &str) {
        play_and_wait_on(&self.playback_handle, &mut self.progress_rx, track_id).await;
    }
}

impl PlaybackFixture for RemoteMultiWindowPlayback {
    async fn close(self) {
        self.playback_handle.shutdown().await;
        self.library.manager().close().await.expect("close the test library");
    }
}

/// While the person waits on a track whose file is read from storage, its
/// load waits on storage, under the track's id, and none of the track is
/// held yet; once the track plays, it waits on nothing, and its bytes go on
/// arriving until all of them are held.
#[tokio::test(flavor = "multi_thread")]
async fn a_load_from_storage_waits_on_storage() {
    with_fixture(RemoteMultiWindowPlayback::new("remote-load-wait"), async |playback| {
        let track = playback.track_ids[0].clone();
        // Hold the FLAC's body a byte past its header, so the load waits on it.
        let (reached, observed) = tokio::sync::oneshot::channel();
        let (resume, resumed) = tokio::sync::oneshot::channel();
        playback.cloud.hold_body_at(playback.flac_object.clone(), 12, reached, resumed).await;
        let mut waits = playback.playback_handle.subscribe_storage_waits();
        let mut buffered = playback.playback_handle.follow_track_buffered();

        playback.playback_handle.play(track.clone());
        observed.await.expect("the load reads the FLAC from storage");
        let waiting = tokio::time::timeout(HANG_GUARD, async {
            loop {
                if let Some(waiting) = waits.waiting() {
                    return waiting;
                }
                waits.changed().await.expect("playback runs");
            }
        })
        .await
        .expect("the waited load waits on storage");
        assert_eq!(waiting, track);
        let held = buffered.next().await.expect("playback runs").expect("a track plays");
        assert_eq!(held.track_id, track);
        assert_eq!(held.ranges, vec![], "none of the held read's bytes are here");

        resume.send(()).expect("the held read waits");
        wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track: playing, .. } if playing.track_id == track),
    )
    .await
    .expect("the track plays once its bytes arrive");
        assert_eq!(waits.waiting(), None, "a playing track waits on no storage");
        wait_for_whole_track(&mut buffered, &track).await;
    })
    .await;
}

/// Remote-cloud port of `switching_tracks_within_a_multi_window_file_keeps_streaming`:
/// switching to the middle track needs bytes fetched by an actual ranged
/// cloud read, not a local-disk read.
#[tokio::test(flavor = "multi_thread")]
async fn switching_tracks_within_a_multi_window_file_over_remote_cloud() {
    with_fixture(RemoteMultiWindowPlayback::new("multi-window-remote-switch"), async |playback| {
        let last_track = playback.track_ids[2].clone();
        playback.play_and_wait(&last_track).await;

        let middle_track = playback.track_ids[1].clone();
        playback.playback_handle.play(middle_track.clone());
        let playing = wait_for_state_on(
            &mut playback.progress_rx,
            |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == middle_track),
        )
        .await;
        assert!(
            playing.is_some(),
            "the middle track reaches Playing after the switch over a real ranged cloud read"
        );
    })
    .await;
}

/// Remote-cloud port of `auto_advance_crosses_gaplessly_within_a_multi_window_file`:
/// the gapless crossing's preload decodes bytes fetched over a real ranged
/// cloud read.
#[tokio::test(flavor = "multi_thread")]
async fn auto_advance_crosses_gaplessly_over_remote_cloud() {
    with_fixture(RemoteMultiWindowPlayback::new("multi-window-remote-gapless"), async |playback| {
        let first = playback.track_ids[0].clone();
        let second = playback.track_ids[1].clone();
        playback.play_and_wait(&first).await;

        playback.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(54)));
        let outcome = observe_boundary(
            &playback.playback_handle,
            &playback.audio_clock,
            &mut playback.progress_rx,
            &first,
            &second,
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

        wait_for_position_advance(
            &playback.playback_handle,
            &playback.audio_clock,
            &mut playback.progress_rx,
        )
        .await
        .expect("track 2's position must advance after the crossing over a real ranged cloud read");
    })
    .await;
}

/// Remote-cloud port of `manual_next_into_a_pregap_track_rebuilds_and_keeps_streaming`.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_pregap_track_over_remote_cloud() {
    with_fixture(RemoteMultiWindowPlayback::new("multi-window-remote-next-pregap"), async |playback| {
        let [first, second] = [playback.track_ids[0].clone(), playback.track_ids[1].clone()];
        playback.play_and_wait(&first).await;

        playback.playback_handle.next();
        wait_for_state_on(
            &mut playback.progress_rx,
            |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == second),
        )
        .await
        .expect(
            "the second track reaches Playing after a manual Next over a real ranged cloud read",
        );

        wait_for_track_position_where(&playback.playback_handle, &playback.audio_clock, &mut playback.progress_rx, &second, |ms| ms > 0).await.expect(
        "the rebuilt track's position advances — its bytes are fetched over the real cloud path",
    );
    })
    .await;
}

/// Remote-cloud port of `manual_next_into_a_clean_track_promotes_the_preload`.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_clean_track_over_remote_cloud() {
    with_fixture(RemoteMultiWindowPlayback::new("multi-window-remote-next-preload"), async |playback| {
        let [second, third] = [playback.track_ids[1].clone(), playback.track_ids[2].clone()];
        playback.play_and_wait(&second).await;

        playback.playback_handle.next();
        wait_for_state_on(
            &mut playback.progress_rx,
            |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == third),
        )
        .await
        .expect(
            "the third track reaches Playing after a manual Next over a real ranged cloud read",
        );

        wait_for_track_position_where(&playback.playback_handle, &playback.audio_clock, &mut playback.progress_rx, &third, |ms| ms > 0).await.expect(
        "the promoted track's position advances — its bytes are fetched over the real cloud path",
    );
    })
    .await;
}

/// Remote-cloud seek port: seeking within the last track over a real ranged
/// cloud read must still land near the target and keep streaming.
#[tokio::test(flavor = "multi_thread")]
async fn seek_within_the_last_track_over_remote_cloud() {
    with_fixture(RemoteMultiWindowPlayback::new("multi-window-remote-seek"), async |playback| {
        let last_track = playback.track_ids[2].clone();
        playback.play_and_wait(&last_track).await;

        let target = Duration::from_secs(20);
        playback.playback_handle.seek(TrackTime::from_duration(target));
        let landed = wait_for_seeked_on(&mut playback.progress_rx)
            .await
            .expect("a seek within the last track emits Seeked over a real ranged cloud read");
        assert!(
            Duration::from_millis(landed).abs_diff(target) < Duration::from_secs(2),
            "should land near {target:?}, got {landed}ms"
        );

        wait_for_position_advance(
            &playback.playback_handle,
            &playback.audio_clock,
            &mut playback.progress_rx,
        )
        .await
        .expect("audio must keep flowing after the seek over a real ranged cloud read");
    })
    .await;
}

/// No single ranged read may exceed this. The fill asks for one 4 MiB window at a
/// time and coven serves the chunks covering it, so the largest read is a window
/// plus chunk alignment; the slack here is for that alignment, not for a second
/// window. This is the assertion that states the property — *a range costs its own
/// bytes* — and it holds however many windows the fill happens to pull.
const SEEK_LARGEST_RANGE_BOUND: u64 = 5 * 1024 * 1024;

/// Everything the seek's ranged reads move, together, must stay under this — half
/// the fixture's 31,922,637-byte file. Loose on purpose: the fill keeps reading
/// ahead while the seek settles, so the total lands on a whole number of 4 MiB
/// windows (measured at one or two, 4,195,328 or 8,521,760 bytes, depending on
/// how far read-ahead got before the counter was read). The point it pins is only
/// that a seek never drags the file across, so it is bounded well above that
/// race and well below the file.
const SEEK_RANGE_BYTES_BOUND: u64 = 16 * 1024 * 1024;

/// A seek over remote storage costs the chunks its range touches, not the
/// whole file. coven seals a file per chunk and authenticates each chunk on its
/// own, so a ranged read fetches only the chunks covering it: seeking near the end
/// of the last track moves 4,195,328 bytes — the fill's one 4 MiB window and a
/// kilobyte of probe — out of a 31,922,637-byte file.
///
/// Before chunked ranges, opening a remote stream downloaded, decrypted and hashed
/// the entire file before it could serve a byte. The byte count is what pins the
/// shape here, because it is exact where a stopwatch is load-dependent, and it is
/// two-sided: see the assertions.
///
/// This test owns its bucket. Every other remote test clones the shared template
/// and reads one `MemoryStorage` between them, so counters taken there would
/// include their traffic.
#[tokio::test(flavor = "multi_thread")]
async fn seek_over_remote_cloud_costs_chunks_not_the_whole_file() {
    with_fixture(RemoteMultiWindowPlayback::new("remote-seek-ranges"), async |playback| {
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
        playback.playback_handle.seek(TrackTime::from_duration(target));
        let landed = wait_for_seeked_on(&mut playback.progress_rx)
            .await
            .expect("the seek lands over a chunked ranged read");
        assert!(
            Duration::from_millis(landed).abs_diff(target) < Duration::from_secs(2),
            "the seek should land near {target:?}, got {landed}ms"
        );
        wait_for_position_advance(
            &playback.playback_handle,
            &playback.audio_clock,
            &mut playback.progress_rx,
        )
        .await
        .expect("audio must flow from the completed chunked-range seek");

        // Ranged reads are the measurement, not `exact_full_read_count`: that counter
        // is bucket-wide and the sync loop reads whole objects (commits, snapshots)
        // throughout, so it climbs by the hundreds here for reasons that have nothing
        // to do with playback.
        //
        // Three checks, catching different regressions. That any ranged read happened
        // at all is what says the file was served by range: were the whole-object
        // download back, the audio would arrive as one full read and this list would
        // be empty. The largest single read is the property itself — a range costs its
        // own bytes — and is independent of how many windows read-ahead pulled. The
        // total is the coarse backstop that no seek drags the file across.
        let all_reads = playback.cloud.reads().await;
        let reads = &all_reads[first_read..];
        let range_bytes: u64 = reads.iter().map(|(_, _, length)| *length).sum();
        let largest = reads.iter().map(|(_, _, length)| *length).max();
        let largest = largest.expect(
            "the seek served no ranged read at all -- the file was fetched whole, not by range",
        );
        assert!(
            largest <= SEEK_LARGEST_RANGE_BOUND,
            "the seek's largest single ranged read was {largest} bytes (bound \
         {SEEK_LARGEST_RANGE_BOUND}); a chunked range must cost the chunks it covers, not the \
         31,922,637-byte file. All reads: {reads:?}",
        );
        assert!(
            range_bytes < SEEK_RANGE_BYTES_BOUND,
            "the seek fetched {range_bytes} bytes from the cloud across {} reads (bound \
         {SEEK_RANGE_BYTES_BOUND}); a seek must not drag the whole file across",
            reads.len(),
        );
    })
    .await;
}

/// A relaunch that resumes the last track 50 s in reads its file from where
/// that decode starts, not from the track's start: the fresh buffer's first
/// window is the one the decode reads first. The track starts at 2:00 of the
/// file, about 21 MB in, and the resumed point is about 9 MB past that, more
/// than two fill windows away, so no read the resumed decode needs lies near
/// the track's start.
#[tokio::test(flavor = "multi_thread")]
async fn a_resume_at_a_position_reads_from_there_not_the_track_start() {
    with_fixture(RemoteMultiWindowPlayback::new("remote-resume-start"), async |playback| {
        let last_track = playback.track_ids[2].clone();
        let resolved =
            bae_records::ResolvedTrackAudio::resolve(playback.library.records(), &last_track)
                .await
                .expect("the last track's audio");
        let track_start = resolved.segments[0].span.start_byte.expect("a CUE track's start byte");
        playback.play_and_wait(&last_track).await;
        playback.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(50)));
        wait_for_seeked_on(&mut playback.progress_rx)
            .await
            .expect("the seek before shutdown lands");
        playback.playback_handle.shutdown().await;
        // What the first session read stays on this device; the relaunch reads
        // storage only for what it no longer holds.
        let flac = playback
            .library
            .manager()
            .release_file_ref_for_test(&resolved.segments[0].file_id)
            .await
            .expect("the FLAC's coven file");
        playback.library.manager().evict_file_for_test(flac.id()).await.expect("evict the FLAC");
        let first_read = playback.cloud.reads().await.len();

        let (handle, _capture_rx, audio_clock) = start_capture_service(&playback.library).await;
        let mut progress_rx = handle.subscribe_progress();
        wait_for_state_on(
            &mut progress_rx,
            |s| matches!(s, PlaybackState::Paused { track, .. } if track.track_id == last_track),
        )
        .await
        .expect("the relaunch restores the last track paused");
        handle.resume();
        wait_for_position_advance(&handle, &audio_clock, &mut progress_rx)
            .await
            .expect("the resumed track plays on from its position");
        handle.shutdown().await;

        let reads = playback.cloud.reads().await;
        let near_track_start = reads[first_read..]
            .iter()
            .filter(|(object, _, _)| *object == playback.flac_object)
            .filter(|(_, offset, _)| {
                (track_start.saturating_sub(64 * 1024)..track_start + 2 * 1024 * 1024)
                    .contains(offset)
            })
            .collect::<Vec<_>>();
        assert!(
            near_track_start.is_empty(),
            "the resume read near the track's start byte {track_start}: {near_track_start:?}"
        );
    })
    .await;
}
