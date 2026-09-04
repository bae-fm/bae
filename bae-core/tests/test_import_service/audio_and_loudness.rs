/// 3. Local folder import: album, release, tracks all in DB, files stay in place.
#[tokio::test]
async fn local_folder_import() {
    support::tracing_init();

    let release = discogs_release("Test Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.clone(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    // Verify release in DB: local (not remote), with its files registered as
    // coven external refs at their in-place import location.
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

    // Verify tracks
    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].title, "Track One");
    assert_eq!(tracks[1].title, "Track Two");
    assert_eq!(tracks[2].title, "Track Three");

    // Verify files
    let files = f.db.get_files_for_release(&release_id).await.unwrap();
    assert_eq!(files.len(), 3);

    // Original files still in place (local)
    assert!(album_dir.join("01 Track One.flac").exists());
    assert!(album_dir.join("02 Track Two.flac").exists());
    assert!(album_dir.join("03 Track Three.flac").exists());

}

#[tokio::test]
async fn import_progress_names_every_operation_before_loudness() {
    use bae_core::import::{ImportEvent, ImportPhase, ImportProgress, ImportStep, PrepareStep};

    support::tracing_init();

    let f = ImportFixture::new().await;
    let mut events = f.handle.subscribe_events();

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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::FileTags),
            user_edit: None,
        })
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
            ImportProgress::Complete { import_id: completed, .. } if completed == import_id => {
                break;
            }
            ImportProgress::Failed { error, .. } => panic!("import failed: {error}"),
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

/// 4. Import produces correct audio format records.
#[tokio::test]
async fn import_produces_audio_format_records() {
    support::tracing_init();

    let release = discogs_release("Format Album", &["Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 1);

    // Check audio format was recorded for the track
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
        let release_id_key = seed_discogs_test_release(release);
        let f = ImportFixture::new().await;

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
            .send_command(ImportCommand {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir,
                scope: bae_core::import::ReleaseFileScope::Recursive,
                selected_cover: None,
                storage_mode: StorageMode::Local,
                pin: false,
                metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                    source: MetadataSource::Discogs,
                release_id: release_id_key,
                    partners: vec![],
                }),
                user_edit: None,
            })
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

/// With every track length known, the loudness pass reports a continuous
/// percent (0 → 100) as it scans, moving ~0.1s of audio at a time rather than
/// once per track, so the import UI bar advances during a track's measure span.
#[tokio::test]
async fn loudness_pass_emits_within_track_progress() {
    use bae_core::import::ImportEvent;
    use bae_core::import::{ImportPhase, ImportProgress};

    support::tracing_init();

    let release = discogs_release("Loudness Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    // Subscribe to the full import event stream before the import runs; the
    // 1024-slot broadcast buffer holds every tick until we drain it below.
    let mut event_rx = f.handle.subscribe_events();

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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let _ = support::wait_for_import_complete(&mut progress_rx).await;

    // Drain the buffered events; keep our candidate's loudness percents in
    // arrival order.
    let mut percents: Vec<u8> = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        if let ImportEvent::ImportProgress {
            candidate_key,
            progress: ImportProgress::Progress { percent, phase, .. },
        } = event
        {
            if candidate_key == expected_candidate_key
                && phase == ImportPhase::MeasuringLoudness
            {
                percents.extend(percent);
            }
        }
    }

    // Three tracks measured ~0.1s at a time → far more moves than one per
    // track, so the bar creeps within each track instead of stepping. The
    // percent is the overall scan: monotonic, and reaching 100 so the bar always
    // completes.
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
/// When `spikes` is set, a single full-scale sample is injected every 0.1 s.
/// These raise the measured true peak to ~1.0 without moving the integrated
/// loudness (they're far too sparse to form a gating block), which is exactly
/// the high-crest-factor signal needed to make the playback peak clamp engage:
/// a quiet track that nonetheless can't be boosted past full scale. A pure sine
/// can never demonstrate the clamp, because its peak and RMS scale together.
///
/// Samples are full-range i32 PCM; the FLAC encoder writes their high 16 bits.
/// A small deterministic dither keeps the stream from compressing below the
/// import's FLAC truncation check (file must be >= 10% of raw PCM); the dither
/// sits ~60 dB down, moving neither the loudness nor the peak.
fn sine(amplitude: f64, sample_rate: u32, secs: f64, spikes: bool) -> Vec<i32> {
    use std::f64::consts::PI;
    let n = (sample_rate as f64 * secs) as usize;
    let spike_period = (sample_rate as f64 * 0.1) as usize;
    let full_scale = i32::MAX;
    let mut rng: u32 = 0x1234_5678;
    let mut dither = || {
        rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (((rng >> 16) as i32 & 0x3F) - 32) << 16
    };
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let s = if spikes && i % spike_period == 0 {
            full_scale
        } else {
            let t = i as f64 / sample_rate as f64;
            ((2.0 * PI * 1000.0 * t).sin() * amplitude * i32::MAX as f64) as i32 + dither()
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

/// Loudness normalization, end to end: import two tracks of deliberately
/// different loudness, confirm the stored per-track measurements differ, and
/// confirm the gain each track derives at playback reflects its own loudness
/// (the quieter track is boosted more) with the peak clamp engaging on a quiet
/// track that nonetheless peaks near full scale.
///
/// This is the load-bearing test for the feature: it exercises the real import
/// measurement path (`ImportService` → `measure_loudness`) and the real playback
/// derivation (`ResolvedTrackAudio::replay_gain_linear`), not reconstructions.
#[tokio::test]
async fn loudness_measured_at_import_drives_playback_gain() {
    use bae_core::config::ReplayGainMode;

    support::tracing_init();

    let release = discogs_release("Loudness Album", &["Quiet Track", "Loud Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let sr = 44_100;
    // Quiet track: a low-amplitude sine (so it wants a boost toward the target)
    // with sparse full-scale spikes that push its true peak to ~1.0 — the
    // crest factor that makes the playback clamp engage. Loud track: a steady
    // half-scale sine, clearly louder and pulled down toward the target.
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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    // ── Stored measurements differ and reflect the two tracks' loudness ──
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
    // The quiet track's late full-scale burst pushes its peak near 1.0; the
    // steady half-scale loud track peaks near 0.5.
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

    // Album loudness was written to the release row from the combined meters.
    // EBU R128 album loudness is the gated integration over both tracks, so the
    // louder track dominates: the album sits in [quiet, loud] and lands near the
    // loud track, never below the quiet one.
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

    // ── Playback gain reflects each track's own loudness (mode = Track) ──
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

    // Off is always unity, regardless of measurements.
    assert_eq!(quiet_audio.replay_gain_linear(ReplayGainMode::Off), 1.0);

    // The quieter track wants more boost; the louder track is attenuated toward
    // the target. So the quiet track's gain exceeds the loud track's.
    assert!(
        quiet_gain > loud_gain,
        "quiet track gain {quiet_gain} should exceed loud track gain {loud_gain}"
    );
    // The loud track at ~-9 LUFS is pulled DOWN toward -18 (gain < 1, no clamp).
    assert!(
        loud_gain < 1.0,
        "loud track should be attenuated toward the target: {loud_gain}"
    );

    // ── Peak clamp engages for the quiet, near-full-scale track ──
    // Unclamped, the quiet track's gain would be 10^((-18 - L)/20), a large
    // boost. With a peak ~1.0 the clamp caps it at ~1/peak ≈ 1.0, so the applied
    // gain must equal the clamp, NOT the (much larger) loudness-only gain.
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

/// The loudness pass is the longest phase of an import, and the candidate row
/// renders `ImportProgress::Progress`'s percent. So the pass reports its scan
/// on that channel too — coarsely, on whole-percent moves — and the row's bar
/// advances instead of sitting at 0 until the phase ends.
#[tokio::test]
async fn loudness_pass_advances_the_candidate_rows_percent() {
    use bae_core::import::{ImportEvent, ImportPhase, ImportProgress};

    support::tracing_init();

    let release = discogs_release("Loudness Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let mut event_rx = f.handle.subscribe_events();

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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
                partners: vec![],
            }),
            user_edit: None,
        })
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
