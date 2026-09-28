/// A folder import stores a local release and its tracks and leaves the files
/// where they were.
#[tokio::test]
async fn local_folder_import() {
    support::tracing_init();

    let f = ImportFixture::new().await;

    let release = discogs_release("Test Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(f.library_manager.providers(), release);

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(
        &album_dir,
        &[
            "01 Track One.flac",
            "02 Track Two.flac",
            "03 Track Three.flac",
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(support::folder_import(
            &import_id,
            album_dir.clone(),
            support::discogs_release(release_id_key),
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    // The release's files resolve to the folder they were imported from.
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert!(!release.remote);
    let files = f.db.get_files_for_release(&release_id).await.unwrap();
    let local_path = f
        .library_manager
        .file_local_path(&files[0].id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(local_path.parent().unwrap(), album_dir);

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].title, "Track One");
    assert_eq!(tracks[1].title, "Track Two");
    assert_eq!(tracks[2].title, "Track Three");

    let files = f.db.get_files_for_release(&release_id).await.unwrap();
    assert_eq!(files.len(), 3);

    assert!(album_dir.join("01 Track One.flac").exists());
    assert!(album_dir.join("02 Track Two.flac").exists());
    assert!(album_dir.join("03 Track Three.flac").exists());
}

#[tokio::test]
async fn import_progress_names_every_operation_before_loudness() {
    use bae_core::import::{ImportEvent, ImportPhase, ImportProgress, ImportStep, PrepareStep};

    support::tracing_init();

    let f = ImportFixture::new().await;
    let mut events = f.handle.every_event_for_test();

    let album_dir = f.temp_path().join("album");
    let expected_candidate_key = album_dir.to_string_lossy().into_owned();
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album Title",
        "Artist Name",
        Some(2024),
        &[TaggedTrack {
            filename: "01 Track Title.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );

    let import_id = f.ids.new_id();
    f.handle
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            MetadataProvenance::FileMetadata,
        ))
        .await
        .unwrap();

    let mut steps = Vec::new();
    loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(20), events.recv())
            .await
            .expect("import progress arrives")
            .expect("import event stream remains open");
        let ImportEvent::ImportProgress {
            candidate_key,
            progress,
        } = event
        else {
            continue;
        };
        if candidate_key != expected_candidate_key {
            continue;
        }
        let step = match progress {
            ImportProgress::Preparing { step, .. } => Some(ImportStep::Preparing(step)),
            ImportProgress::Progress { phase, .. } => Some(ImportStep::Running(phase)),
            ImportProgress::Complete {
                import_id: completed,
                ..
            } if completed == import_id => {
                break;
            }
            ImportProgress::Failed { error, .. } => panic!("import failed: {error}"),
            ImportProgress::Cancelled { .. } => panic!("nothing cancelled the import"),
            ImportProgress::RemoteUploadQueued { .. } => None,
            ImportProgress::Complete { .. } => None,
        };
        if let Some(step) = step {
            if steps.last() != Some(&step) {
                let reached_finalizing = step == ImportStep::Running(ImportPhase::Finalizing);
                steps.push(step);
                if reached_finalizing {
                    break;
                }
            }
        }
    }

    assert_eq!(
        steps,
        vec![
            ImportStep::Preparing(PrepareStep::ValidatingSourceFiles),
            ImportStep::Running(ImportPhase::ReadingFiles),
            ImportStep::Running(ImportPhase::MeasuringLoudness),
            ImportStep::Running(ImportPhase::Finalizing),
        ]
    );
}

#[tokio::test]
async fn import_produces_audio_format_records() {
    support::tracing_init();

    let f = ImportFixture::new().await;

    let release = discogs_release("Format Album", &["Track"]);
    let release_id_key = seed_discogs_test_release(f.library_manager.providers(), release);

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            support::discogs_release(release_id_key),
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 1);

    let format =
        f.db.find_audio_format_by_track_id(&tracks[0].id)
            .await
            .unwrap();
    assert!(format.is_some(), "should have audio format record");
    let format = format.unwrap();
    assert_eq!(format.content_type.as_str(), "audio/flac");
}

#[tokio::test]
async fn exact_metadata_import_stores_dsd_audio_format() {
    support::tracing_init();

    for (index, fixture_name, import_name) in [
        (1, "placeholder-dsd.dsf", "01 Track.dsf"),
        (2, "placeholder-dsd.dff", "01 Track.dff"),
    ] {
        let release = discogs_release(&format!("DSD Format Album {index}"), &["Track"]);
        let f = ImportFixture::new().await;
        let release_id_key = seed_discogs_test_release(f.library_manager.providers(), release);

        let album_dir = f.temp_path().join("album");
        fs::create_dir_all(&album_dir).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("test-fixtures")
                .join("audio-format")
                .join(fixture_name),
            album_dir.join(import_name),
        )
        .unwrap();

        let import_id = uuid::Uuid::new_v4().to_string();
        f.handle
            .send_command(support::folder_import(
                &import_id,
                album_dir,
                support::discogs_release(release_id_key),
            ))
            .await
            .unwrap();

        let mut progress_rx = f.handle.subscribe_import(import_id);
        let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

        let files = f.db.get_files_for_release(&release_id).await.unwrap();
        assert_eq!(files.len(), 1, "{fixture_name}");
        assert_eq!(
            files[0].content_type,
            bae_core::util::content_type::ContentType::Dsd,
            "{fixture_name}"
        );

        let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
        assert_eq!(tracks.len(), 1, "{fixture_name}");
        let format =
            f.db.find_audio_format_by_track_id(&tracks[0].id)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            format.content_type,
            bae_core::util::content_type::ContentType::Dsd,
            "{fixture_name}"
        );
    }
}

/// With every track length known, the loudness pass reports its percent
/// several times within each track, rising to 100.
#[tokio::test]
async fn loudness_pass_emits_within_track_progress() {
    let f = ImportFixture::new().await;

    use bae_core::import::ImportEvent;
    use bae_core::import::{ImportPhase, ImportProgress};

    support::tracing_init();

    let release = discogs_release("Loudness Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(f.library_manager.providers(), release);

    // Read from before the import starts, so no percent is missed.
    let mut event_rx = f.handle.every_event_for_test();

    let album_dir = f.temp_path().join("album");
    let expected_candidate_key = album_dir.to_string_lossy().into_owned();
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(
        &album_dir,
        &[
            "01 Track One.flac",
            "02 Track Two.flac",
            "03 Track Three.flac",
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            support::discogs_release(release_id_key),
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let _ = support::wait_for_import_complete(&mut progress_rx).await;

    let mut percents: Vec<u8> = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        if let ImportEvent::ImportProgress {
            candidate_key,
            progress: ImportProgress::Progress { percent, phase, .. },
        } = event
        {
            if candidate_key == expected_candidate_key && phase == ImportPhase::MeasuringLoudness {
                percents.extend(percent);
            }
        }
    }

    // More reports than tracks means the percent moves within a track.
    assert!(
        percents.len() > 4,
        "within-track measurement moves the percent more than once per track: {percents:?}",
    );
    assert!(
        percents.windows(2).all(|w| w[1] >= w[0]),
        "the percent is monotonic non-decreasing: {percents:?}"
    );
    assert_eq!(percents.last().copied(), Some(100), "reaches exactly 100");
}

/// Interleaved-stereo 1 kHz sine at `amplitude` (fraction of full scale).
///
/// `spikes` adds one full-scale sample every 0.1 s: too sparse to move the
/// loudness, but it raises the peak to ~1.0, so a quiet track still cannot be
/// boosted past full scale. A pure sine cannot show the playback peak clamp,
/// because its peak and loudness scale together.
fn sine(amplitude: f64, sample_rate: u32, secs: f64, spikes: bool) -> Vec<i32> {
    use std::f64::consts::PI;
    let n = (sample_rate as f64 * secs) as usize;
    let spike_period = (sample_rate as f64 * 0.1) as usize;
    let full_scale = i32::MAX;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let s = if spikes && i % spike_period == 0 {
            full_scale
        } else {
            let t = i as f64 / sample_rate as f64;
            ((2.0 * PI * 1000.0 * t).sin() * amplitude * i32::MAX as f64) as i32
        };
        out.push(s);
        out.push(s);
    }
    out
}

/// Write a synthetic 16-bit stereo FLAC of `samples` to `path`.
fn write_flac(path: &Path, samples: &[i32], sample_rate: u32) {
    let bytes = bae_core::audio_codec::encode_i32(
        bae_core::audio_codec::EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        samples,
        sample_rate,
        2,
    )
    .expect("encode synthetic FLAC");
    fs::write(path, bytes).unwrap();
}

/// Two tracks of different loudness are measured at import, and at playback the
/// quieter one gets more gain, capped by the peak clamp because it peaks near
/// full scale. Runs the real import measurement and the real playback gain.
#[tokio::test]
async fn loudness_measured_at_import_drives_playback_gain() {
    let f = ImportFixture::new().await;

    use bae_core::config::ReplayGainMode;

    support::tracing_init();

    let release = discogs_release("Loudness Album", &["Quiet Track", "Loud Track"]);
    let release_id_key = seed_discogs_test_release(f.library_manager.providers(), release);

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let sr = 44_100;
    // Quiet track: a low sine with full-scale spikes, so it wants a boost but
    // peaks near full scale. Loud track: a steady half-scale sine.
    write_flac(
        &album_dir.join("01 Quiet Track.flac"),
        &sine(0.03, sr, 4.0, true),
        sr,
    );
    write_flac(
        &album_dir.join("02 Loud Track.flac"),
        &sine(0.5, sr, 4.0, false),
        sr,
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            support::discogs_release(release_id_key),
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    // ── Stored measurements ──
    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 2);
    let quiet = tracks.iter().find(|t| t.title == "Quiet Track").unwrap();
    let loud = tracks.iter().find(|t| t.title == "Loud Track").unwrap();

    let quiet_fmt =
        f.db.find_audio_format_by_track_id(&quiet.id)
            .await
            .unwrap()
            .unwrap();
    let loud_fmt =
        f.db.find_audio_format_by_track_id(&loud.id)
            .await
            .unwrap()
            .unwrap();

    let quiet_lufs = quiet_fmt
        .track_loudness_lufs
        .expect("quiet track measured a loudness");
    let loud_lufs = loud_fmt
        .track_loudness_lufs
        .expect("loud track measured a loudness");
    assert!(
        loud_lufs > quiet_lufs + 10.0,
        "loud track ({loud_lufs} LUFS) should be clearly louder than quiet ({quiet_lufs} LUFS)"
    );
    // The quiet track's spikes put its peak near 1.0; the loud track's near 0.5.
    let quiet_peak = quiet_fmt
        .track_peak_linear
        .expect("quiet track measured a peak");
    let loud_peak = loud_fmt
        .track_peak_linear
        .expect("loud track measured a peak");
    assert!(
        quiet_peak > 0.9,
        "quiet track's burst should peak near full scale: {quiet_peak}"
    );
    assert!(
        loud_peak < 0.7,
        "loud track's steady sine should peak near 0.5: {loud_peak}"
    );

    // Album loudness is measured over both tracks together, so it falls between
    // the two track loudnesses.
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    let album_lufs = release
        .album_loudness_lufs
        .expect("album loudness measured");
    assert!(
        album_lufs.is_finite() && album_lufs >= quiet_lufs && album_lufs <= loud_lufs + 0.01,
        "album loudness {album_lufs} should fall within [{quiet_lufs}, {loud_lufs}]"
    );
    let album_peak = release.album_peak_linear.expect("album peak measured");
    assert!(
        (album_peak - quiet_peak).abs() < 1e-6,
        "album peak {album_peak} should be the max of the tracks' peaks ({quiet_peak})"
    );

    // ── Playback gain per track ──
    let quiet_audio = f
        .library_manager
        .resolve_track_audio(&quiet.id)
        .await
        .unwrap();
    let loud_audio = f
        .library_manager
        .resolve_track_audio(&loud.id)
        .await
        .unwrap();

    let quiet_gain = quiet_audio.replay_gain_linear(ReplayGainMode::Track);
    let loud_gain = loud_audio.replay_gain_linear(ReplayGainMode::Track);

    assert_eq!(quiet_audio.replay_gain_linear(ReplayGainMode::Off), 1.0);

    assert!(
        quiet_gain > loud_gain,
        "quiet track gain {quiet_gain} should exceed loud track gain {loud_gain}"
    );
    // The loud track is above the -18 LUFS target, so it is turned down.
    assert!(
        loud_gain < 1.0,
        "loud track should be attenuated toward the target: {loud_gain}"
    );

    // ── Peak clamp ──
    // The quiet track's gain is capped at 1/peak, well below the boost its
    // loudness alone asks for.
    let unclamped = 10f64.powf((-18.0 - quiet_lufs) / 20.0) as f32;
    let clamp = (1.0 / quiet_peak) as f32;
    assert!(
        unclamped > clamp + 0.5,
        "test setup: quiet track's unclamped gain {unclamped} must exceed its clamp {clamp} so the clamp is observable"
    );
    assert!(
        (quiet_gain - clamp).abs() < 0.05,
        "quiet track's applied gain {quiet_gain} should be the peak clamp {clamp}, not the unclamped {unclamped}"
    );
}

/// The candidate row shows `ImportProgress::Progress`'s percent, so the
/// loudness pass, an import's longest phase, reports each whole-percent move
/// rather than leaving the bar at 0 until it ends.
#[tokio::test]
async fn loudness_pass_advances_the_candidate_rows_percent() {
    let f = ImportFixture::new().await;

    use bae_core::import::{ImportEvent, ImportPhase, ImportProgress};

    support::tracing_init();

    let release = discogs_release("Loudness Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(f.library_manager.providers(), release);

    let mut event_rx = f.handle.every_event_for_test();

    let album_dir = f.temp_path().join("album");
    let expected_candidate_key = album_dir.to_string_lossy().into_owned();
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(
        &album_dir,
        &[
            "01 Track One.flac",
            "02 Track Two.flac",
            "03 Track Three.flac",
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(support::folder_import(
            &import_id,
            album_dir,
            support::discogs_release(release_id_key),
        ))
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let _ = support::wait_for_import_complete(&mut progress_rx).await;

    let mut percents = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        let ImportEvent::ImportProgress {
            candidate_key,
            progress:
                ImportProgress::Progress {
                    percent,
                    phase: ImportPhase::MeasuringLoudness,
                    ..
                },
        } = event
        else {
            continue;
        };
        if candidate_key == expected_candidate_key {
            percents.push(percent);
        }
    }

    assert!(
        percents.len() > 2,
        "the pass reports its scan while it runs, not one percent for the whole phase: {percents:?}"
    );
    assert!(
        percents.windows(2).all(|w| w[1] > w[0]),
        "each report is a whole-percent move forward: {percents:?}"
    );
    assert_eq!(
        percents.first().copied(),
        Some(Some(0)),
        "a known frame denominator opens at zero"
    );
    assert_eq!(
        percents.last().copied(),
        Some(Some(100)),
        "the bar reaches the end of the phase"
    );
}
