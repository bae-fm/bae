use super::*;

fn sink_with(total: Option<u64>, done: u64, errors: u32) -> LoudnessProgressSink {
    let event_tx = crate::import::ImportEventBus::new(16, crate::import::CandidateRuntime::default());
    LoudnessProgressSink {
        state: None,
        error: None,
        total_frames: total,
        done_frames: done,
        frames_done_before: 0,
        scan_total_frames: total,
        decode_error_count: errors,
        discarded_packet_count: 0,
        frames_since_emit: 0,
        progress: LoudnessProgress::new(&event_tx, "test", "release-1", "import-1"),
    }
}

#[test]
fn measured_frames_control_progress_value_and_determinacy() {
    let event_tx = crate::import::ImportEventBus::new(16, crate::import::CandidateRuntime::default());
    let mut rx = event_tx.subscribe();
    let emit = |total_frames, done_frames, frames_done_before, scan_total_frames| {
        LoudnessProgressSink {
            state: None,
            error: None,
            total_frames,
            done_frames,
            frames_done_before,
            scan_total_frames,
            decode_error_count: 0,
            discarded_packet_count: 0,
            frames_since_emit: 0,
            progress: LoudnessProgress::new(&event_tx, "test", "release-1", "import-1"),
        }
        .emit();
    };

    emit(Some(900), 450, 100, Some(1_000));
    emit(None, 44_100, 100, None);

    let mut percents = Vec::new();
    while let Ok(event) = rx.try_recv() {
        match event {
            crate::import::handle::ImportEvent::ImportProgress {
                progress: crate::import::types::ImportProgress::Progress { percent, phase, .. },
                ..
            } => {
                assert_eq!(phase, crate::import::types::ImportPhase::MeasuringLoudness);
                percents.extend(percent);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    // The determinate scan is 55% through; the indeterminate one has no
    // percent to report, so the row's percent stands where it was.
    assert_eq!(percents, vec![55]);
}

/// The broken signature: a gross frame shortfall (a truncated body under a
/// valid header) or any fatal decode error. A full-window decode, including
/// one that discarded an invalid packet, is not broken.
#[test]
fn broken_reason_flags_shortfall_and_errors_not_clean() {
    assert!(
        sink_with(Some(1000), 1000, 0).broken_reason().is_none(),
        "a full-window decode is clean"
    );
    assert!(
        sink_with(Some(1000), 970, 0).broken_reason().is_none(),
        "a 3% shortfall is boundary rounding, within slack"
    );
    assert!(
        sink_with(Some(1000), 100, 0).broken_reason().is_some(),
        "a 90% shortfall is a truncated body"
    );
    assert!(
        sink_with(Some(1000), 1000, 1).broken_reason().is_some(),
        "a fatal decode error is broken even with the full window"
    );
    assert!(
        sink_with(None, 0, 0).broken_reason().is_none(),
        "with no expected count, a shortfall can't be asserted"
    );

    let mut complete_with_discard = sink_with(Some(1000), 1000, 0);
    crate::audio_codec::DecodedSink::add_discarded_packet_count(&mut complete_with_discard, 1);
    crate::audio_codec::DecodedSink::add_discarded_packet_count(&mut complete_with_discard, 1);
    assert_eq!(complete_with_discard.discarded_packet_count, 2);
    assert!(
        complete_with_discard.broken_reason().is_none(),
        "a discarded packet is recoverable when frame coverage is complete"
    );

    let mut unknown_with_discard = sink_with(None, 0, 0);
    crate::audio_codec::DecodedSink::add_discarded_packet_count(&mut unknown_with_discard, 1);
    assert!(
        unknown_with_discard.broken_reason().is_some(),
        "a discarded packet needs an expected frame count to prove recovery"
    );
}

/// End-to-end over the real decoder: a window that falls in a truncated
/// FLAC's missing tail is flagged broken (the decode errors out or produces
/// far too few frames), while the same window of the intact fixture decodes
/// clean. Rides the same decode-error count + frame count the import
/// verify uses.
#[test]
fn truncated_flac_decode_is_flagged_broken_intact_is_not() {
    crate::audio_codec::init();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/cue_flac/Test Album.flac"
    );
    let clean = std::fs::read(path).expect("read fixture");
    let sr = 44100u64;
    // A one-second window deep in the file (25s, brown noise near the end).
    let start = 25 * sr;
    let end = 26 * sr;
    let total = end - start;

    // Pre-filled buffers: the decode never waits on a fill, so the test
    // exercises the window/verify logic, not streaming.
    let buffer_of = |bytes: &[u8]| {
        let buffer = create_sparse_buffer(bytes.len() as u64);
        buffer.append_at(0, bytes);
        buffer
    };
    let never = || Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut sink = sink_with(Some(total), 0, 0);
    let ok = crate::audio_codec::decode_audio_to_verifying_sink(
        buffer_of(&clean),
        Some(start),
        Some(end),
        &mut sink,
        never(),
    );
    assert!(ok.is_ok(), "intact decode should succeed: {ok:?}");
    assert!(
        sink.broken_reason().is_none(),
        "intact fixture window is not broken (decoded {} of {total})",
        sink.done_frames
    );

    // Truncated to 60% of its bytes: the 25s window is past the surviving
    // audio, so the decode errors out or yields far too few frames.
    let truncated = &clean[..clean.len() * 6 / 10];
    let mut sink = sink_with(Some(total), 0, 0);
    let res = crate::audio_codec::decode_audio_to_verifying_sink(
        buffer_of(truncated),
        Some(start),
        Some(end),
        &mut sink,
        never(),
    );
    assert!(
        res.is_err() || sink.broken_reason().is_some(),
        "truncated fixture window must be flagged broken (decode {res:?}, decoded {} of {total})",
        sink.done_frames,
    );
}

// ── measure_loudness ───────────────────────────────────────────

fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}

fn audio_format(track_id: &str, id: &str) -> crate::db::DbAudioFormat {
    crate::db::DbAudioFormat::new(
        track_id,
        crate::util::content_type::ContentType::Flac,
        44_100,
        Some(16),
        2,
        id.to_string(),
        now(),
    )
}

fn whole_file_main_segment(format_id: &str, file_id: &str) -> crate::db::DbAudioSegment {
    crate::db::DbAudioSegment {
        id: format!("seg-{format_id}"),
        audio_format_id: format_id.to_string(),
        segment_index: 0,
        role: crate::db::DbAudioSegmentRole::Main,
        file_id: file_id.to_string(),
        start_sample: 0,
        end_sample: None,
        start_byte: None,
        end_byte: None,
        created_at: now(),
    }
}

fn standalone_track(track_id: &str, path: &std::path::Path) -> TrackFile {
    TrackFile {
        db_track: crate::db::DbTrack::new_test("release-id", track_id, "Track Title", Some(1)),
        audio: crate::import::TrackAudio::Standalone {
            file_path: path.to_path_buf(),
            source_audio: crate::import::folder_scanner::ScannedAudio {
                content_type: crate::util::content_type::ContentType::Flac,
                duration_ms: 1_000,
                format: crate::album_detail::AudioFormat {
                    codec: "FLAC".to_string(),
                    sample_rate_hz: 44_100,
                    bits_per_sample: Some(16),
                    bitrate_kbps: None,
                    channels: 2,
                },
            },
        },
    }
}

/// The whole-percent moves one loudness pass reported, in order.
fn loudness_percents(
    rx: &mut tokio::sync::broadcast::Receiver<crate::import::handle::ImportEvent>,
) -> Vec<u8> {
    let mut percents = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let crate::import::handle::ImportEvent::ImportProgress {
            progress: crate::import::types::ImportProgress::Progress { percent, phase, .. },
            ..
        } = event
        {
            assert_eq!(phase, crate::import::types::ImportPhase::MeasuringLoudness);
            percents.extend(percent);
        }
    }
    percents
}

/// [`measure_loudness`] under the labels every test in this module uses —
/// none of them varies the candidate key, release id, or import id, and none
/// asserts on one.
async fn measure(
    event_tx: &crate::import::handle::ImportEventBus,
    audio_formats: &mut [crate::db::DbAudioFormat],
    audio_segments: &[crate::db::DbAudioSegment],
    file_ids: &HashMap<PathBuf, String>,
    source_file_sizes: &HashMap<PathBuf, u64>,
    tracks_to_files: &[TrackFile],
) -> LoudnessResult {
    measure_loudness(
        event_tx,
        audio_formats,
        audio_segments,
        file_ids,
        source_file_sizes,
        tracks_to_files,
        "cand",
        "release-1",
        "import-1",
    )
    .await
    .unwrap()
}

fn cue_flac_fixture(name: &str) -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/cue_flac"
    ))
    .join(name)
}

/// [`measure_loudness`] over one whole-file track at `path` whose validated
/// size is `size`, returning the pass's error.
async fn measure_failure(path: &std::path::Path, size: u64) -> crate::import::ImportError {
    let event_tx =
        crate::import::ImportEventBus::new(16, crate::import::CandidateRuntime::default());
    let mut audio_formats = vec![audio_format("track-0", "af-0")];
    let audio_segments = vec![whole_file_main_segment("af-0", "file-0")];
    let file_ids = HashMap::from([(path.to_path_buf(), "file-0".to_string())]);
    let source_file_sizes = HashMap::from([(path.to_path_buf(), size)]);
    let tracks = vec![standalone_track("track-0", path)];

    let result = measure_loudness(
        &event_tx,
        &mut audio_formats,
        &audio_segments,
        &file_ids,
        &source_file_sizes,
        &tracks,
        "cand",
        "release-1",
        "import-1",
    )
    .await;
    assert!(audio_formats[0].track_loudness_lufs.is_none());
    match result {
        Err(error) => error,
        Ok(result) => panic!(
            "an unreadable source must fail the pass, not report {:?} broken",
            result.broken
        ),
    }
}

/// The OS error kind a `SourceRead` import error carries.
fn source_read_kind(error: &crate::import::ImportError) -> Option<std::io::ErrorKind> {
    match error {
        crate::import::ImportError::SourceRead { error, .. } => match error.as_ref() {
            crate::playback::PlaybackError::Io { source, .. } => Some(source.kind()),
            _ => None,
        },
        _ => None,
    }
}

/// A source that disappears after validation fails the pass with the
/// read's own error — not a "decode failed: Invalid data found" broken
/// track, which would claim the audio is bad.
#[tokio::test]
async fn measure_loudness_fails_on_a_source_that_cannot_be_opened() {
    crate::audio_codec::init();
    let missing = PathBuf::from("/nonexistent/track.flac");

    let error = measure_failure(&missing, 1).await;

    assert_eq!(
        source_read_kind(&error),
        Some(std::io::ErrorKind::NotFound),
        "{error}"
    );
    let text = error.to_string();
    assert!(text.contains("/nonexistent/track.flac"), "{text}");
    assert!(!text.contains("Invalid data found"), "{text}");
    assert!(!text.contains("decode verification"), "{text}");
}

/// A source whose read fails partway (here: shorter than the size the scan
/// validated, so the read runs out) fails the pass with that read's error,
/// even though the bytes that did arrive are valid FLAC.
#[tokio::test]
async fn measure_loudness_fails_on_a_source_whose_read_fails() {
    crate::audio_codec::init();
    let fixture = std::fs::read(cue_flac_fixture(
        "03 Test Artist - Track Three (Brown Noise).flac",
    ))
    .expect("fixture");
    let temp = tempfile::Builder::new()
        .suffix(".flac")
        .tempfile()
        .expect("temp file");
    std::fs::write(temp.path(), &fixture[..fixture.len() / 2]).expect("short copy");

    let error = measure_failure(temp.path(), fixture.len() as u64).await;

    assert_eq!(
        source_read_kind(&error),
        Some(std::io::ErrorKind::UnexpectedEof),
        "{error}"
    );
    assert!(
        !error.to_string().contains("Invalid data found"),
        "{error}"
    );
}

/// A track whose audio format has no segments is skipped (nothing to
/// decode) and stays unmeasured.
#[tokio::test]
async fn measure_loudness_skips_track_with_no_segments() {
    let event_tx = crate::import::ImportEventBus::new(16, crate::import::CandidateRuntime::default());
    let mut audio_formats = vec![audio_format("track-0", "af-0")];
    let audio_segments: Vec<crate::db::DbAudioSegment> = Vec::new();
    let file_ids = HashMap::new();
    let source_file_sizes = HashMap::new();
    let tracks = vec![standalone_track("track-0", &PathBuf::from("/unused.flac"))];

    let result = measure(
        &event_tx,
        &mut audio_formats,
        &audio_segments,
        &file_ids,
        &source_file_sizes,
        &tracks,
    )
    .await;

    assert!(result.album_loudness_lufs.is_none());
    assert!(audio_formats[0].track_loudness_lufs.is_none());
}

/// A real, non-silent decode yields per-track loudness/peak and an album
/// aggregate over the measured tracks.
#[tokio::test]
async fn measure_loudness_computes_track_and_album_values() {
    crate::audio_codec::init();
    let event_tx = crate::import::ImportEventBus::new(16, crate::import::CandidateRuntime::default());
    let path = cue_flac_fixture("03 Test Artist - Track Three (Brown Noise).flac");
    let mut audio_formats = vec![audio_format("track-0", "af-0")];
    let audio_segments = vec![whole_file_main_segment("af-0", "file-0")];
    let file_ids = HashMap::from([(path.clone(), "file-0".to_string())]);
    let source_file_sizes =
        HashMap::from([(path.clone(), std::fs::metadata(&path).unwrap().len())]);
    let tracks = vec![standalone_track("track-0", &path)];

    let result = measure(
        &event_tx,
        &mut audio_formats,
        &audio_segments,
        &file_ids,
        &source_file_sizes,
        &tracks,
    )
    .await;

    assert!(
        audio_formats[0].track_loudness_lufs.is_some(),
        "brown-noise track has a measurable loudness"
    );
    assert!(audio_formats[0].track_peak_linear.is_some());
    assert!(
        result.album_loudness_lufs.is_some(),
        "one measured track yields an album aggregate"
    );
    assert!(result.album_peak_linear.is_some());
}

#[tokio::test]
async fn measure_loudness_progress_weights_tracks_by_frames() {
    crate::audio_codec::init();
    let event_tx = crate::import::ImportEventBus::new(32, crate::import::CandidateRuntime::default());
    let mut rx = event_tx.subscribe();
    let path = cue_flac_fixture("03 Test Artist - Track Three (Brown Noise).flac");
    let mut audio_formats = vec![
        audio_format("track-0", "af-0"),
        audio_format("track-1", "af-1"),
    ];
    let mut short = whole_file_main_segment("af-0", "file-0");
    short.end_sample = Some(2_205);
    let mut long = whole_file_main_segment("af-1", "file-0");
    long.end_sample = Some(6_615);
    let audio_segments = vec![short, long];
    let file_ids = HashMap::from([(path.clone(), "file-0".to_string())]);
    let source_file_sizes =
        HashMap::from([(path.clone(), std::fs::metadata(&path).unwrap().len())]);
    let tracks = vec![
        standalone_track("track-0", &path),
        standalone_track("track-1", &path),
    ];

    measure(
        &event_tx,
        &mut audio_formats,
        &audio_segments,
        &file_ids,
        &source_file_sizes,
        &tracks,
    )
    .await;

    // The first track holds 2,205 of the 8,820 measured frames, so the pass
    // stands at 25% when it finishes — a quarter, not the half an
    // equal-track-slices bar would report.
    assert!(
        loudness_percents(&mut rx).contains(&25),
        "the first of two tracks completing is a quarter of the frame work",
    );
}

/// A window shorter than one EBU R128 gated block (400 ms) produces no
/// usable loudness — the same "unmeasured" outcome the code labels silent
/// (`into_result` returns `Ok((_, None))`). The track keeps NULL
/// loudness/peak and contributes nothing to the album. (The available
/// "silence" fixture measures above the −70 LUFS floor over its full length,
/// so a sub-gate window is the deterministic way to reach this branch with a
/// real decode.)
#[tokio::test]
async fn measure_loudness_leaves_ungated_track_unmeasured() {
    crate::audio_codec::init();
    let event_tx = crate::import::ImportEventBus::new(16, crate::import::CandidateRuntime::default());
    let path = cue_flac_fixture("03 Test Artist - Track Three (Brown Noise).flac");
    let mut audio_formats = vec![audio_format("track-0", "af-0")];
    // ~50 ms at 44.1 kHz — far short of a 400 ms gated block.
    let mut segment = whole_file_main_segment("af-0", "file-0");
    segment.end_sample = Some(2_205);
    let audio_segments = vec![segment];
    let file_ids = HashMap::from([(path.clone(), "file-0".to_string())]);
    let source_file_sizes =
        HashMap::from([(path.clone(), std::fs::metadata(&path).unwrap().len())]);
    let tracks = vec![standalone_track("track-0", &path)];

    let result = measure(
        &event_tx,
        &mut audio_formats,
        &audio_segments,
        &file_ids,
        &source_file_sizes,
        &tracks,
    )
    .await;

    assert!(
        audio_formats[0].track_loudness_lufs.is_none(),
        "a sub-gate window has no usable loudness and stays unmeasured"
    );
    assert!(
        result.album_loudness_lufs.is_none(),
        "no measured track means no album loudness"
    );
}
