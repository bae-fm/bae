// Pregap tests. The CUE/FLAC fixture's track 2 has a 2s pregap (INDEX 00 at
// 8s, INDEX 01 at 10s). Play and Next skip it, so the track's first position
// is 0 or later; auto-advance plays it, so its first position is negative.

#[tokio::test]
async fn test_direct_play_skips_pregap() {
    let mut fixture = CueFlacTestFixture::new(support::TestAudioDevice::RealtimeCapture)
        .await
        .expect("set up CUE/FLAC realtime capture fixture");
    let pregapped_track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(pregapped_track_id.clone());
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| {
            matches!(s, PlaybackState::Playing { track, .. }
                if track.track_id == pregapped_track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("the pregapped track should start playing");

    let first_position =
        first_position_of(&mut fixture.progress_rx, &pregapped_track_id).await;
    assert!(
        first_position >= 0,
        "direct play should skip the 2s pregap and start the track at 0; \
         got {first_position}ms (a played pregap starts below zero)",
    );
    wait_for_track_position_where(&mut fixture.progress_rx, &pregapped_track_id, |ms| ms > 600)
        .await
        .expect("the position should climb into the track after the skipped pregap");
}

#[tokio::test]
async fn test_next_button_skips_pregap() {
    let mut fixture = CueFlacTestFixture::new(support::TestAudioDevice::RealtimeCapture)
        .await
        .expect("set up CUE/FLAC realtime capture fixture");
    let first_track_id = fixture.track_ids[0].clone();
    let pregapped_track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(first_track_id.clone());
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_secs(5),
    )
    .await
    .expect("the first track should start playing");

    // Next is a direct selection: it skips the incoming track's pregap.
    fixture.playback_handle.next();
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| {
            matches!(s, PlaybackState::Playing { track, .. }
                if track.track_id == pregapped_track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("Next should switch to the pregapped track");

    let first_position =
        first_position_of(&mut fixture.progress_rx, &pregapped_track_id).await;
    assert!(
        first_position >= 0,
        "Next should skip the 2s pregap and start the track at 0; \
         got {first_position}ms (a played pregap starts below zero)",
    );
    wait_for_track_position_where(&mut fixture.progress_rx, &pregapped_track_id, |ms| ms > 600)
        .await
        .expect("the position should climb into the track after the skipped pregap");
}

#[tokio::test]
async fn test_auto_advance_plays_pregap() {
    let mut fixture = CueFlacTestFixture::new(support::TestAudioDevice::RealtimeCapture)
        .await
        .expect("set up CUE/FLAC realtime capture fixture");
    let first_track_id = fixture.track_ids[0].clone();
    let pregapped_track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(first_track_id.clone());
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| {
            matches!(s, PlaybackState::Playing { track, .. }
                if track.track_id == first_track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("the first track should start playing");

    // Track 1 runs 0–8s; seek near its end so it crosses into track 2's pregap.
    fixture.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(7)));
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| {
            matches!(s, PlaybackState::Playing { track, .. }
                if track.track_id == pregapped_track_id)
        },
        Duration::from_secs(10),
    )
    .await
    .expect("playback should auto-advance into the pregapped track");

    let first_position =
        first_position_of(&mut fixture.progress_rx, &pregapped_track_id).await;
    assert!(
        first_position < 0,
        "auto-advance should play the pregap with a negative countdown; \
         the track's first position was {first_position}ms",
    );

    wait_for_track_position_where(&mut fixture.progress_rx, &pregapped_track_id, |ms| ms > 600)
        .await
        .expect("once the pregap passes, position should climb into the track");
}

/// Seeking 3 s into CUE/FLAC track 2, which starts mid-album after a 2 s
/// pregap, plays audio from 3 s into the XLD reference, which starts at INDEX
/// 01: a seek counts from the track's start, as the player shows it.
#[tokio::test]
async fn test_cue_flac_seek() {
    use bae_core::audio_codec::decode_audio;

    // Real-time capture: at full speed the decoder can finish track 2 and move
    // to the next track before the seek lands, leaving nothing after the seek.
    let mut fixture = CueFlacTestFixture::new(support::TestAudioDevice::RealtimeCapture)
        .await
        .expect("set up CUE/FLAC realtime capture fixture");

    let track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(track_id.clone());
    // The seek below starts a new capture stream.
    let _play_stream = fixture.next_capture_stream().await;

    let started =
        support::next_matching(&mut fixture.progress_rx, Duration::from_secs(5), |event| {
            matches!(
                event,
                PlaybackProgress::StateChanged {
                    state: PlaybackState::Playing { .. }
                }
            )
            .then_some(())
        })
        .await;
    assert!(started.is_some(), "Playback should start");

    fixture.playback_handle.seek(TrackTime::from_millis(3_000));
    let captured = fixture.next_capture_stream().await;

    support::wait_for_seek(&mut fixture.progress_rx, &track_id).await;

    let fixture_dir = bae_test_support::fixture_dir!("cue_flac");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two (White Noise).flac"))
            .expect("read reference");
    let reference =
        decode_audio(buffer_from(&reference_data), None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

    // One second of samples.
    let target_samples = sample_rate as usize * channels;
    let captured_snapshot =
        bae_core::playback::wait_for_samples(&captured, target_samples, Duration::from_secs(60))
            .await;

    assert!(
        !captured_snapshot.is_empty(),
        "No samples captured after seek",
    );

    // Search the whole reference for the match.
    let snippet_len = 200 * channels;
    let step = 100 * channels;

    let mut best_sad: f64 = f64::MAX;
    let mut best_ref_offset: usize = 0;

    let search_end = reference_f32.len().saturating_sub(snippet_len);
    for ref_offset in (0..search_end).step_by(step) {
        let mut sad: f64 = 0.0;
        for i in 0..snippet_len.min(captured_snapshot.len()) {
            sad += (captured_snapshot[i] as f64 - reference_f32[ref_offset + i] as f64).abs();
            if sad > best_sad {
                break;
            }
        }
        if sad < best_sad {
            best_sad = sad;
            best_ref_offset = ref_offset;
        }
    }

    let ref_time_ms = best_ref_offset as f64 / channels as f64 / sample_rate as f64 * 1000.0;
    let avg_diff = best_sad / snippet_len as f64;

    assert!(
        (ref_time_ms - 3_000.0).abs() < 20.0,
        "the seek to 3 s into the track played audio from {ref_time_ms:.1}ms into it",
    );

    // This only checks where the audio came from; test_cue_flac.rs checks exact samples.
    assert!(
        avg_diff < 0.5,
        "Post-seek audio average difference too high ({:.4}), audio may be from wrong position.\n\
         Best alignment at {:.1}ms in reference.",
        avg_diff,
        ref_time_ms,
    );

    debug!(
        "Post-seek CUE/FLAC audio aligned at {:.1}ms in reference (avg_diff {:.4}).",
        ref_time_ms, avg_diff,
    );
}

/// A seek counts from the track's start (INDEX 01), as the player shows it: on
/// CUE/FLAC track 2, after its 2 s pregap, a seek to 4 s shows 4 s and plays on
/// from there.
#[tokio::test]
async fn seek_lands_at_the_track_time_it_names() {
    let mut fixture = CueFlacTestFixture::new(support::TestAudioDevice::RealtimeCapture)
        .await
        .expect("set up CUE/FLAC realtime capture fixture");
    let track_id = fixture.track_ids[1].clone();
    play_and_wait_on(&fixture.playback_handle, &mut fixture.progress_rx, &track_id).await;

    fixture.playback_handle.seek(TrackTime::from_millis(4_000));
    let seeked =
        support::next_matching(&mut fixture.progress_rx, PLAY_START_BACKSTOP, |event| {
            match event {
                PlaybackProgress::Seeked {
                    position_ms,
                    track_id: tid,
                    ..
                } if tid == track_id => Some(position_ms),
                _ => None,
            }
        })
        .await
        .expect("the seek reports where it landed");
    assert_eq!(seeked, 4_000, "the seek shows the track time it named");
    wait_for_track_position_where(&mut fixture.progress_rx, &track_id, |ms| {
        (4_000..5_000).contains(&ms)
    })
    .await
    .expect("playback goes on from 4 s into the track");
}

/// Direct play of CUE/FLAC track 2 skips its 2s pregap: the captured audio
/// matches the XLD reference from INDEX 01.
#[tokio::test]
async fn test_direct_play_skips_pregap_cue_flac() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = CueFlacTestFixture::new(support::TestAudioDevice::Capture)
        .await
        .expect("set up CUE/FLAC capture fixture");

    let track_id = fixture.track_ids[1].clone();

    // The capture device takes audio faster than real time, so the track can
    // finish before its Playing state is reported; the audio it captures is
    // what shows where playback started.
    fixture.playback_handle.play(track_id);
    let captured = fixture.next_capture_stream().await;

    // XLD splits at INDEX 01, so the reference has no pregap.
    let fixture_dir = bae_test_support::fixture_dir!("cue_flac");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two (White Noise).flac"))
            .expect("read reference");
    let reference =
        decode_audio(buffer_from(&reference_data), None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

    // Two seconds of samples.
    let target_samples = sample_rate as usize * channels * 2;
    let captured_snapshot =
        bae_core::playback::wait_for_samples(&captured, target_samples, Duration::from_secs(60))
            .await;

    // Had the pregap played, the start of the reference would not be found
    // within the first tenth of a second.
    let snippet_len = 500 * channels;
    let max_alignment = sample_rate as usize * channels / 10;

    assert!(
        captured_snapshot.len() > max_alignment + snippet_len,
        "Not enough captured samples: {}",
        captured_snapshot.len(),
    );

    let mut best_max_diff: f32 = f32::MAX;
    let mut best_offset: usize = 0;
    for offset in 0..max_alignment.min(captured_snapshot.len().saturating_sub(snippet_len)) {
        let mut max_diff: f32 = 0.0;
        for i in 0..snippet_len.min(reference_f32.len()) {
            let diff = (captured_snapshot[offset + i] - reference_f32[i]).abs();
            max_diff = max_diff.max(diff);
            if max_diff > best_max_diff {
                break;
            }
        }
        if max_diff < best_max_diff {
            best_max_diff = max_diff;
            best_offset = offset;
        }
    }

    let offset_ms = best_offset as f64 / channels as f64 / sample_rate as f64 * 1000.0;

    assert!(
        best_max_diff < 0.01,
        "Direct play did not skip pregap: captured audio doesn't match reference at INDEX 01.\n\
         Best offset {:.1}ms, max sample diff {:.6}",
        offset_ms,
        best_max_diff,
    );

    let compare_count = (sample_rate as usize * channels)
        .min(captured_snapshot.len() - best_offset)
        .min(reference_f32.len());

    for i in 0..compare_count {
        let diff = (captured_snapshot[best_offset + i] - reference_f32[i]).abs();
        assert!(
            diff < 0.01,
            "AUDIO MISMATCH at index {} ({:.1}ms): pregap may not be properly skipped",
            i,
            i as f64 / channels as f64 / sample_rate as f64 * 1000.0,
        );
    }

    debug!(
        "Direct play correctly skips pregap ({} samples match after INDEX 01, offset {:.1}ms).",
        compare_count, offset_ms,
    );
}

/// A three-track CUE album over one FLAC of tones written here. Track 2 has a
/// 2.5 s pregap of silence (INDEX 00 at 0:05, INDEX 01 188 CD frames later),
/// track 3 has none. In the file: track 1 spans 0:00–0:05, track 2 0:05–0:17.5
/// (10 s after its pregap), track 3 0:17.5–0:28. Times past 0:07 are ~7 ms
/// later than written, since 2.5 s is 187.5 CD frames.
fn generate_back_album_files(dir: &std::path::Path) {
    const SAMPLE_RATE: u32 = 44_100;
    const FILE_SECONDS: f64 = 28.0;
    // INDEX 00 at 00:05:00 to INDEX 01 at 00:07:38, in 1/75 s CD frames.
    let cd_frame = |frames: u32| (frames as u64 * SAMPLE_RATE as u64 / 75) as usize;
    let pregap = cd_frame(5 * 75)..cd_frame(7 * 75 + 38);
    let frames = (FILE_SECONDS * SAMPLE_RATE as f64) as usize;
    let samples: Vec<i32> = (0..frames)
        .flat_map(|frame| {
            let level = if pregap.contains(&frame) {
                0.0
            } else {
                let t = frame as f64 / SAMPLE_RATE as f64;
                0.25 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()
            };
            let sample = ((level * i16::MAX as f64) as i32) << 16;
            [sample, sample]
        })
        .collect();
    bae_core::audio_codec::init();
    let flac = bae_core::audio_codec::encode_i32(
        bae_core::audio_codec::EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        &samples,
        SAMPLE_RATE,
        2,
    )
    .expect("encode the Back album FLAC");
    std::fs::write(dir.join("Back Album.flac"), flac).expect("write the Back album FLAC");

    let cue = "\
PERFORMER \"Test Artist\"
TITLE \"Back Album\"
FILE \"Back Album.flac\" WAVE
  TRACK 01 AUDIO
    TITLE \"Back One\"
    PERFORMER \"Test Artist\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Back Two\"
    PERFORMER \"Test Artist\"
    INDEX 00 00:05:00
    INDEX 01 00:07:38
  TRACK 03 AUDIO
    TITLE \"Back Three\"
    PERFORMER \"Test Artist\"
    INDEX 01 00:17:38
";
    std::fs::write(dir.join("Back Album.cue"), cue).expect("write the Back album CUE");
}

fn create_back_album() -> DiscogsRelease {
    DiscogsRelease {
        artists: vec![support::discogs_artist("test-artist-1", "Test Artist")],
        master_id: Some("back-album-master".to_string()),
        ..support::discogs_test_release(
            "back-album-release",
            "Back Album",
            &[
                ("Back One", "0:05"),
                ("Back Two", "0:10"),
                ("Back Three", "0:10"),
            ],
        )
    }
}

/// Back on track 2, whose 2.5 s pregap a restart skips: the restarted track
/// starts 2.5 s into its audio, but Back 1 s later still counts 1 s and goes
/// to track 1.
#[tokio::test]
async fn back_twice_on_a_pregapped_track_reaches_the_previous_track() {
    let mut fixture = CueFlacTestFixture::import(
        support::TestAudioDevice::RealtimeCapture,
        create_back_album(),
        generate_back_album_files,
    )
    .await
    .expect("set up the Back album fixture");
    let first_track_id = fixture.track_ids[0].clone();
    let pregapped_track_id = fixture.track_ids[1].clone();
    assert_back_twice_reaches_previous(&mut fixture, &pregapped_track_id, &first_track_id).await;
}

/// Back on track 3, which has no pregap, works the same way.
#[tokio::test]
async fn back_twice_on_a_track_without_pregap_reaches_the_previous_track() {
    let mut fixture = CueFlacTestFixture::import(
        support::TestAudioDevice::RealtimeCapture,
        create_back_album(),
        generate_back_album_files,
    )
    .await
    .expect("set up the Back album fixture");
    let pregapped_track_id = fixture.track_ids[1].clone();
    let plain_track_id = fixture.track_ids[2].clone();
    assert_back_twice_reaches_previous(&mut fixture, &plain_track_id, &pregapped_track_id).await;
}

/// Play the 10 s `track_id`, press Back 5 s in (it restarts), then press Back
/// again 1 s into the restarted track (it plays `previous_track_id`).
async fn assert_back_twice_reaches_previous(
    fixture: &mut CueFlacTestFixture,
    track_id: &str,
    previous_track_id: &str,
) {
    play_and_wait_on(&fixture.playback_handle, &mut fixture.progress_rx, track_id).await;
    fixture.playback_handle.seek_by_ratio(0.5);
    wait_for_track_position_where(&mut fixture.progress_rx, track_id, |ms| ms >= 5_000)
        .await
        .expect("the seek lands halfway into the track and plays on");

    fixture.playback_handle.previous();
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == track_id),
        PLAY_START_BACKSTOP,
    )
    .await
    .expect("Back 5 s into the track restarts it");
    // Updates the stream queued before the restart report 5 s or more, so one
    // under 3 s is the restarted track's.
    let restarted_at =
        wait_for_track_position_where(&mut fixture.progress_rx, track_id, |ms| ms < 3_000)
            .await
            .expect("the restarted track reports a position near its start");
    assert!(
        restarted_at >= 0,
        "a restart skips the pregap and starts the track at 0; got {restarted_at}ms",
    );
    wait_for_track_position_where(&mut fixture.progress_rx, track_id, |ms| {
        (1_000..3_000).contains(&ms)
    })
    .await
    .expect("the restarted track plays on to 1 s");

    fixture.playback_handle.previous();
    let landed = wait_for_state_on(
        &mut fixture.progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        PLAY_START_BACKSTOP,
    )
    .await
    .expect("Back plays a track");
    let PlaybackState::Playing { track: landed, .. } = landed else {
        unreachable!("the wait matched only Playing");
    };
    assert_eq!(
        landed.track_id, previous_track_id,
        "Back 1 s into the restarted track should play the previous track, not restart \
         {track_id} again",
    );
}
