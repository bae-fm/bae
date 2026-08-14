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
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
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
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
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
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
                },
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

/// With every track length known, the loudness pass emits a continuous
/// `fraction` (0 → 1) as it scans, ticking ~0.1s of audio at a time rather than
/// once per track, so the import UI bar advances during a track's measure span.
#[tokio::test]
async fn loudness_pass_emits_within_track_progress() {
    use bae_core::import::ImportEvent;

    support::tracing_init();

    let release = discogs_release("Loudness Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    // Subscribe to the full import event stream before the import runs; the
    // 1024-slot broadcast buffer holds every tick until we drain it below.
    let mut event_rx = f.handle.subscribe_events();

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
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let _ = support::wait_for_import_complete(&mut progress_rx).await;

    // Drain the buffered events; keep the loudness ticks for our candidate in
    // arrival order.
    let mut ticks: Vec<(u32, u32, Option<f32>)> = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        if let ImportEvent::ImportLoudnessProgress {
            candidate_key,
            tracks_done,
            tracks_total,
            fraction,
        } = event
        {
            if candidate_key == "test" {
                ticks.push((tracks_done, tracks_total, fraction));
            }
        }
    }

    // Three tracks measured ~0.1s at a time → many ticks, not one per track, so
    // the bar creeps within each track instead of stepping. `fraction` is the
    // overall scan progress: monotonic, starting at 0 and reaching exactly 1.0 so
    // the bar always completes; total is constant; the last track-count is N/N.
    assert!(
        ticks.len() > 4,
        "within-track measurement emits many ticks, not one per track: {} ticks",
        ticks.len()
    );
    assert!(
        ticks.iter().all(|(_, total, _)| *total == 3),
        "every tick reports total=3: {ticks:?}"
    );
    let fractions: Vec<f32> = ticks
        .iter()
        .map(|(_, _, fraction)| {
            fraction.expect("generated tracks provide determinate frame totals")
        })
        .collect();
    assert!(
        fractions.windows(2).all(|w| w[1] >= w[0]),
        "fraction is monotonic non-decreasing: {fractions:?}"
    );
    assert_eq!(fractions.first().copied(), Some(0.0), "starts at 0");
    assert_eq!(fractions.last().copied(), Some(1.0), "reaches exactly 1.0");
    assert_eq!(
        ticks.last().map(|(d, t, _)| (*d, *t)),
        Some((3, 3)),
        "final tick labels the last track N/N"
    );
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
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
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

/// 5. Two sequential imports both succeed and produce separate albums.
#[tokio::test]
async fn two_sequential_imports() {
    support::tracing_init();

    let titles = ["First Album", "Second Album"];
    let mut release_keys = vec![];
    for title in &titles {
        let release = discogs_release(title, &["Track"]);
        release_keys.push(seed_discogs_test_release(release));
    }
    let f = ImportFixture::new().await;

    let mut release_ids = vec![];
    for (i, title) in titles.iter().enumerate() {
        let _ = title;
        let album_dir = f.temp_path().join(format!("album{}", i + 1));
        fs::create_dir_all(&album_dir).unwrap();
        // Distinct filename per album so the two imports carry different content
        // hashes. The content hash is the relative path + size of each file, and
        // re-importing the same content overwrites the prior release, so reusing
        // one name would make the second import delete the first.
        let track_name = format!("01 Track {}.flac", i + 1);
        generate_album_files(&album_dir, &[track_name.as_str()]);

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
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_keys[i].clone(), MetadataSource::Discogs),
                },
                user_edit: None,
            })
            .await
            .unwrap();

        let mut progress_rx = f.handle.subscribe_import(import_id);
        let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;
        release_ids.push(release_id);
    }

    // Both releases exist in DB
    let release1 =
        f.db.find_release_by_id(&release_ids[0])
            .await
            .unwrap()
            .unwrap();
    let release2 =
        f.db.find_release_by_id(&release_ids[1])
            .await
            .unwrap()
            .unwrap();

    // Different albums
    assert_ne!(release1.album_id, release2.album_id);

    let album1 =
        f.db.find_album_by_id(&release1.album_id)
            .await
            .unwrap()
            .unwrap();
    let album2 =
        f.db.find_album_by_id(&release2.album_id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(album1.title, "First Album");
    assert_eq!(album2.title, "Second Album");
}

#[tokio::test]
async fn reimport_cover_download_failure_preserves_prior_release() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("cover-failure-reimport");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01 Track Title.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );

    let (prior_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await
    .expect("initial import succeeds");
    assert_release_has_external_ref(&f, &prior_release_id).await;

    let result = import_folder(
        &f,
        &album_dir,
        Some(CoverSelection::Remote(
            "http://127.0.0.1:9/cover.jpg".to_string(),
            MetadataSource::MusicBrainz,
        )),
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await;

    assert!(result.is_err(), "cover download should fail");
    assert_release_has_external_ref(&f, &prior_release_id).await;
}

#[tokio::test]
async fn reimport_decode_verification_failure_preserves_prior_release() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.set_decode_verification(false);

    let album_dir = f.temp_path().join("decode-failure-reimport");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01 Track Title.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );
    truncate_flac_body(&album_dir.join("01 Track Title.flac"));

    let (prior_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await
    .expect("initial import succeeds while decode verification is disabled");
    assert_release_has_external_ref(&f, &prior_release_id).await;

    f.set_decode_verification(true);
    let result = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await;

    let error = result.expect_err("decode verification should fail");
    assert!(
        error.contains("decode verification failed"),
        "unexpected error: {error}"
    );
    assert_release_has_external_ref(&f, &prior_release_id).await;
}

#[tokio::test]
async fn successful_reimport_replaces_prior_release_once() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;

    let album_dir = f.temp_path().join("successful-reimport");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01 Track Title.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );

    let (prior_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Remote,
        IdentityChoice::Unknown,
    )
    .await
    .expect("initial remote import queues upload");
    let upload_count = f
        .library_manager
        .drain_uploads_expecting_work()
        .await
        .unwrap();
    assert_eq!(
        upload_count, 1,
        "initial remote import should upload one file"
    );
    let prior_release =
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .expect("prior release exists after upload");
    assert!(prior_release.remote, "prior release should be remote");
    let content_hash = prior_release.content_hash.clone().unwrap();

    let (replacement_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await
    .expect("re-import succeeds");

    assert!(
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .is_none(),
        "prior release should be replaced"
    );
    let release_ids =
        f.db.release_ids_for_content_hash(&content_hash)
            .await
            .unwrap();
    assert_eq!(
        release_ids,
        vec![replacement_release_id],
        "one release should carry the re-imported content hash"
    );
    assert_eq!(
        f.db.queued_delete_count_for_test().await.unwrap(),
        1,
        "replacing the prior remote release should queue its cloud blob for deletion"
    );
}

/// The remote-transition rollback: the mirror of the local unit test
/// `failed_import_before_finalize_leaves_only_import_audit_row`, but one stage
/// later. The release is finalized (status Importing), then the cloud
/// transition fails and `run_import` calls `fail_import_and_delete_release`. A
/// Remote import with no sync provider connected fails at exactly that point
/// (`coven_make_remote` returns `SyncNotReady`) — the honest injection for a
/// post-finalize transition failure, since the upload itself is deferred to the
/// drain and never runs synchronously. The rollback must delete the
/// just-finalized release and its album, mark the import Failed with its release
/// link cleared, and leave a pre-existing release untouched.
#[tokio::test]
async fn remote_transition_failure_rolls_back_finalized_release() {
    support::tracing_init();
    // No cloud/sync connected, so the make-Remote transition fails.
    let f = ImportFixture::new().await;

    // A prior local release already in the library; the failed remote import
    // below must not touch it.
    let prior_dir = f.temp_path().join("prior");
    fs::create_dir_all(&prior_dir).unwrap();
    generate_tagged_album_files(
        &prior_dir,
        "Prior Album",
        "Prior Artist",
        None,
        &[TaggedTrack {
            filename: "01 Prior Track.flac",
            title: "Prior Track",
            track_number: 1,
        }],
    );
    let (prior_release_id, _) = import_folder(
        &f,
        &prior_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await
    .expect("prior local import succeeds");

    // The remote import: finalize commits the release (status Importing), then
    // coven_make_remote fails because sync was never connected.
    let album_dir = f.temp_path().join("remote");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Remote Album",
        "Remote Artist",
        None,
        &[TaggedTrack {
            filename: "01 Remote Track.flac",
            title: "Remote Track",
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
            storage_mode: StorageMode::Remote,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id.clone());
    let error = support::try_wait_for_import_complete(&mut progress_rx)
        .await
        .expect_err("remote transition without a sync provider fails");
    assert!(error.contains("cloud upload"), "unexpected error: {error}");

    // The rollback deleted the finalized remote release, its album, and the
    // artist row that finalize inserted for it; only the prior release, album,
    // and artist remain. The remote import's artist is referenced by nothing
    // else, so leaving it behind would orphan a row on every failed remote
    // import.
    let (release_count, album_count, artist_count) =
        f.db.library_row_counts_for_test().await.unwrap();
    assert_eq!(release_count, 1, "only the prior release remains");
    assert_eq!(album_count, 1, "only the prior album remains");
    assert_eq!(
        artist_count, 1,
        "only the prior artist remains; the rolled-back import's artist row is gone",
    );
    assert!(
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .is_some(),
        "the prior release is untouched by the failed remote import",
    );
}
