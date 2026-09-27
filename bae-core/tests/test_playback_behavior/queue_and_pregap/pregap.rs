// Pregap tests. The CUE/FLAC fixture's track 2 has a 2s pregap (INDEX 00 at
// 8s, INDEX 01 at 10s). Play and Next skip it, so the position is positive
// partway in; auto-advance plays it, so the position is still negative.

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
            matches!(s, PlaybackState::Playing { track_info, .. }
                if track_info.track_id == pregapped_track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("the pregapped track should start playing");

    let position_ms = position_after(&mut fixture.progress_rx, Duration::from_millis(1200)).await;
    assert!(
        position_ms > 600,
        "direct play should skip the 2s pregap and let position climb from 0; \
         got {position_ms}ms ~1.2s in (a played pregap would keep it pinned at 0)",
    );
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
            matches!(s, PlaybackState::Playing { track_info, .. }
                if track_info.track_id == pregapped_track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("Next should switch to the pregapped track");

    let position_ms = position_after(&mut fixture.progress_rx, Duration::from_millis(1200)).await;
    assert!(
        position_ms > 600,
        "Next should skip the 2s pregap and let position climb from 0; \
         got {position_ms}ms ~1.2s in (a played pregap would keep it pinned at 0)",
    );
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
            matches!(s, PlaybackState::Playing { track_info, .. }
                if track_info.track_id == first_track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("the first track should start playing");

    // Track 1 runs 0–8s; seek near its end so it crosses into track 2's pregap.
    fixture.playback_handle.seek(Duration::from_secs(7));
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| {
            matches!(s, PlaybackState::Playing { track_info, .. }
                if track_info.track_id == pregapped_track_id)
        },
        Duration::from_secs(10),
    )
    .await
    .expect("playback should auto-advance into the pregapped track");

    // ~1s in, the pregap is still playing, so the position is below zero.
    let during_pregap = position_after(&mut fixture.progress_rx, Duration::from_millis(1000)).await;
    assert!(
        during_pregap < 0,
        "auto-advance should play the pregap with a negative countdown; \
         got {during_pregap}ms ~1s in",
    );

    let after_pregap = position_after(&mut fixture.progress_rx, Duration::from_millis(2500)).await;
    assert!(
        after_pregap > 600,
        "once the pregap passes, position should climb into the track; got {after_pregap}ms",
    );
}

/// Seeking within CUE/FLAC track 2, which starts mid-album, plays audio from the
/// matching place in the XLD reference.
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

    fixture.playback_handle.seek(Duration::from_secs(5));
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

    // Seek positions count from INDEX 00, so 5s lands 3s into the reference,
    // which starts at INDEX 01. Search the whole reference for the match.
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
        ref_time_ms > 0.0,
        "Seek appears to have gone to the beginning of the track instead of 5s in",
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
