use super::*;

/// Next button must advance to track 2 and produce audio matching the XLD reference.
///
/// Tests the manual Next path (not auto-advance). After pressing Next, the captured
/// audio from track 2 must match the XLD-split reference.
#[tokio::test]
async fn test_cue_ape_next_track() {
    use bae_audio::decode_audio;

    let mut fixture = CueApeTestFixture::new()
        .await
        .unwrap_or_else(|e| panic!("failed to set up CUE/APE playback fixture: {e}"));

    let track1_id = fixture.track_ids[0].clone();
    let track2_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(track1_id.clone());
    // Drain track 1's play stream; the Next below mints a fresh one for track 2.
    let _track1_play = fixture.next_capture_stream().await;

    // Wait for track 1 to start
    bae_playback::test_capture::wait_until_playing(&mut fixture.progress_rx, &track1_id).await;

    // Press Next — track 2's stream replaces track 1's.
    fixture.playback_handle.next();
    let mut captured = fixture.next_capture_stream().await;

    // Wait for track 2 to start, and play it out: its decode stats come at its end.
    bae_playback::test_capture::wait_until_playing(&mut fixture.progress_rx, &track2_id).await;
    bae_playback::test_capture::play_to_track_end(&fixture.playback_handle, &fixture.audio_clock)
        .await;

    // Decode XLD reference
    let fixture_dir = bae_import::test_fixtures::cue_ape_dir();
    let reference_data =
        bae_util::test_files::read_fixture(fixture_dir.join("02 Test Artist - Track Two.flac"));
    let reference =
        decode_audio(bae_audio::test_samples::filled_sparse_buffer(&reference_data), None, None)
            .expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

    // Align and compare
    let snippet_len = 500 * channels;
    // APE frames are ~73728 samples (~1.7s). FFmpeg's seek lands at a frame
    // boundary, so the captured audio may start up to one frame before the
    // track's CUE start time. Search within 2s for alignment.
    let max_alignment = sample_rate as usize * channels * 2;

    let track2_sample_count = bae_util::test_channels::next_matching(
        &mut fixture.progress_rx,
        bae_util::test_channels::HANG_GUARD,
        |event| match event {
            PlaybackProgress::DecodeStats { track_id, samples_decoded, .. }
                if track_id == track2_id =>
            {
                Some(samples_decoded as usize)
            }
            _ => None,
        },
    )
    .await
    .expect("Track 2 emits its decode stats");
    assert!(
        track2_sample_count > snippet_len,
        "Track 2 should produce enough samples to compare: {} (needed more than {})",
        track2_sample_count,
        snippet_len,
    );
    assert!(
        reference_f32.len() >= track2_sample_count,
        "Track 2 XLD reference should cover decoded track: {} (needed {})",
        reference_f32.len(),
        track2_sample_count,
    );

    let captured_snapshot = captured.wait_for_samples(track2_sample_count).await;
    assert!(
        captured_snapshot.len() >= track2_sample_count,
        "Not enough captured samples after Next (60s): {} (needed {})",
        captured_snapshot.len(),
        track2_sample_count,
    );

    assert_captured_matches_reference(
        &captured_snapshot,
        &reference_f32,
        channels,
        sample_rate,
        max_alignment,
        track2_sample_count,
        0.01,
        "track 2 after Next",
    );
}

/// Import the two-disc CUE/APE release: one CUE sheet and APE image per disc,
/// each folder's sheet naming `CDImage.ape`, read as the release's two discs.
/// Returns the library, the folder holding its files, and the release's id.
async fn import_multi_disc_cue_ape(
    destination: ImportDestination,
) -> (bae_library::test_libraries::TestLibrary, bae_util::test_files::TestFolder, String) {
    bae_util::test_logs::install();
    let temp_root = bae_util::test_files::TestFolder::new();
    let album_dir = temp_root.path().join("album");
    let cd1_dir = album_dir.join("CD1");
    let cd2_dir = album_dir.join("CD2");
    let db_dir = temp_root.path().join("db");
    temp_root.create_folder(&cd1_dir);
    temp_root.create_folder(&cd2_dir);
    temp_root.create_folder(&db_dir);

    // Reuse the 90s / 3-track CUE+APE fixture, once per disc, renamed to
    // `CDImage.*` so both discs share the same bare filename (a common
    // multi-disc rip layout).
    let fixture_dir = bae_import::test_fixtures::cue_ape_dir();
    let ape_bytes = bae_util::test_files::read_fixture(fixture_dir.join("Test Album.ape"));
    let cue_body = "PERFORMER \"Test Artist\"\n\
                    TITLE \"Disc Title\"\n\
                    FILE \"CDImage.ape\" WAVE\n  \
                    TRACK 01 AUDIO\n    TITLE \"Track One\"\n    INDEX 01 00:00:00\n  \
                    TRACK 02 AUDIO\n    TITLE \"Track Two\"\n    INDEX 01 00:30:00\n  \
                    TRACK 03 AUDIO\n    TITLE \"Track Three\"\n    INDEX 01 01:00:00\n";
    for dir in [&cd1_dir, &cd2_dir] {
        temp_root.write(dir.join("CDImage.ape"), &ape_bytes);
        temp_root.write(dir.join("CDImage.cue"), cue_body);
    }

    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let (library, storage) = bae_library::test_libraries::storage_library(
        &db_dir,
        fakes.providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    if destination.goes_to_cloud() {
        bae_library::test_libraries::connect_storage(library.manager(), library.store(), &storage)
            .await;
    }

    // Discogs multi-disc tracklist: positions "1-1".."1-3", "2-1".."2-3",
    // read as the release's two discs.
    let discogs_release = DiscogsRelease {
        country: None,
        labels: vec![],
        // Each position carries its disc, which the shared builder's flat 1..n
        // numbering cannot express, so this tracklist is built here.
        tracklist: (1..=2)
            .flat_map(|disc| {
                (1..=3).map(move |n| {
                    bae_catalogs::test_releases::discogs_track(
                        &format!("{disc}-{n}"),
                        &format!("Disc {disc} Track {n}"),
                        "0:30",
                    )
                })
            })
            .collect(),
        ..bae_catalogs::test_releases::discogs_test_release(
            "test-multi-disc-cue-ape",
            "Multi-Disc Album",
            &[],
        )
    };
    let release_id_key = fakes.seed_discogs_test_release(discogs_release);
    let import_handle = library.start_discogs_import().await;

    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    import_handle
        .send_command(ImportCommand {
            destination,
            ..bae_import::test_imports::folder_import(
                &import_id,
                album_dir,
                bae_import::test_imports::discogs_release(release_id_key),
            )
        })
        .await
        .expect("send command");

    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _) =
        wait_for_multi_disc_cue_ape_import_ready(library.manager(), destination, &mut progress_rx)
            .await;
    (library, temp_root, release_id)
}

/// Import a two-disc CUE/APE release to the given destination and assert
/// each track's main audio segment resolves to its own disc's APE file
/// (identified by the relative path `CD{N}/CDImage.ape`).
///
/// This exercises the bare-filename collision regression in every code path:
/// bytes-never-copied (Local), bytes-uploaded-and-pinned (Remote + pin),
/// and bytes-uploaded-cloud-only (Remote, no pin).
async fn assert_multi_disc_cue_ape_per_disc_mapping(destination: ImportDestination) {
    let (library, _temp_root, release_id) = import_multi_disc_cue_ape(destination).await;
    let tracks = library.records().get_tracks_for_release(&release_id).await.expect("get tracks");
    assert_eq!(tracks.len(), 6, "should have 6 tracks (2 discs × 3 tracks)");

    // `original_filename` in release_files must be the full relative path
    // (`CD1/CDImage.ape`), not the bare filename — otherwise the two discs'
    // APE rows would collide on the same `"CDImage.ape"` string.
    let files = library.manager().get_files_for_release(&release_id).await.expect("get files");
    let mut ape_filenames: Vec<String> = files
        .iter()
        .filter(|f| f.original_filename.ends_with(".ape"))
        .map(|f| f.original_filename.clone())
        .collect();
    ape_filenames.sort();
    assert_eq!(
        ape_filenames,
        vec!["CD1/CDImage.ape".to_string(), "CD2/CDImage.ape".to_string()],
        "release_files must store relative paths for both discs, not bare filenames"
    );

    // Each track's main audio segment must point to its own disc's APE.
    let filename_by_id: std::collections::HashMap<String, String> =
        files.iter().map(|f| (f.id.clone(), f.original_filename.clone())).collect();

    let mediums = library.records().get_release_mediums(&release_id).await.expect("mediums");
    let [disc_one, disc_two] = [&mediums[0].id, &mediums[1].id];
    for track in &tracks {
        let resolved = bae_records::ResolvedTrackAudio::resolve(library.records(), &track.id)
            .await
            .expect("resolve track audio");
        let segment = resolved
            .segments
            .iter()
            .find(|segment| segment.role == bae_records::DbAudioSegmentRole::Main)
            .unwrap_or_else(|| panic!("track {} has no main audio segment", track.title));
        let filename = filename_by_id
            .get(&segment.file_id)
            .unwrap_or_else(|| panic!("file_id {} not in release_files", segment.file_id));

        let expected = match track.medium_id.as_str() {
            medium if medium == disc_one => "CD1/CDImage.ape",
            medium if medium == disc_two => "CD2/CDImage.ape",
            other => panic!("unexpected medium {other:?} for track {}", track.title),
        };
        assert_eq!(
            filename, expected,
            "track '{}' (medium {:?}) should point at {expected}, got {filename}",
            track.title, track.medium_id,
        );
    }
}

async fn wait_for_multi_disc_cue_ape_import_ready(
    library_manager: &LibraryManager,
    destination: ImportDestination,
    progress_rx: &mut bae_import::ImportProgressFeed,
) -> (String, String) {
    match destination {
        ImportDestination::ThisDevice => wait_for_import_complete(progress_rx).await,
        ImportDestination::Cloud => {
            let (release_id, album_id) = wait_for_import_complete(progress_rx).await;
            library_manager
                .drain_uploads_expecting_work()
                .await
                .expect("upload every imported file");
            let files = library_manager
                .get_files_for_release(&release_id)
                .await
                .expect("get release files");
            assert_eq!(
                files.len(),
                4,
                "remote multi-disc CUE/APE import keeps both audio files and both CUE sheets"
            );
            (release_id, album_id)
        }
    }
}

/// Regression: multi-disc CUE/APE imports must link each track to its OWN disc's
/// audio file, not collapse every track onto the last disc's file.
///
/// Bug: `file_ids` was keyed by bare filename (`CDImage.ape`), so CD1's entry
/// was overwritten by CD2's. Every `audio_formats.file_id` ended up pointing at
/// CD2's APE, so playing any disc 1 track decoded disc 2's audio.
///
/// The same structural bug lived in both byte-placement strategies, so we
/// cover each explicitly to prevent a regression on any one path.
#[tokio::test]
async fn test_multi_disc_cue_ape_local() {
    assert_multi_disc_cue_ape_per_disc_mapping(ImportDestination::ThisDevice).await;
}

#[tokio::test]
async fn test_multi_disc_cue_ape_remote() {
    assert_multi_disc_cue_ape_per_disc_mapping(ImportDestination::Cloud).await;
}

/// The release editor puts a CUE sheet on another disc of the release: the
/// draft shows the sheet's tracks under that disc at once, after the tracks
/// the disc holds, and saving writes the move: the medium the sheet left is
/// gone, the moved tracks number on from the disc's own, and the sheet is the
/// disc its tracks are on.
#[tokio::test]
async fn test_editor_moves_a_sheet_to_a_disc_that_holds_other_tracks() {
    let (library, _temp_root, release_id) =
        import_multi_disc_cue_ape(ImportDestination::ThisDevice).await;
    let manager = library.manager();
    let files = manager.get_files_for_release(&release_id).await.unwrap();
    let sheet_id = |name: &str| {
        files
            .iter()
            .find(|file| file.original_filename == name)
            .unwrap_or_else(|| panic!("the release holds {name}"))
            .id
            .clone()
    };
    let (first, second) = (sheet_id("CD1/CDImage.cue"), sheet_id("CD2/CDImage.cue"));

    let form = manager.open_release_edit(&release_id).await.unwrap();
    let drafts = manager.subscribe_release_edit_draft();
    let disc = bae_types::mediums::MediumSide::first_of;
    let moved = form
        .clone()
        .with_sheet_place(first.clone(), bae_import::folder_scanner::SheetPlace::On(disc(2)))
        .unwrap();
    let put_back = moved
        .clone()
        .with_sheet_place(first.clone(), bae_import::folder_scanner::SheetPlace::On(disc(1)))
        .unwrap();
    assert!(!put_back.changed(), "a sheet picked back onto its own disc is no change");
    drafts.set(Some(moved.clone())).expect("the draft takes the form");
    let draft = drafts.next().await.expect("read the draft").expect("the library holds it");
    let sides = draft.sides.as_deref().expect("the release has tracks");
    let [emptied, side] = sides else { panic!("two headings: {sides:?}") };
    assert_eq!(emptied.side, bae_types::mediums::TrackSide::Disc { disc: 1 });
    assert!(emptied.runs.is_empty(), "the disc the sheet left keeps its heading until Save");
    assert_eq!(side.side, bae_types::mediums::TrackSide::Disc { disc: 2 });
    let runs: Vec<_> = side
        .runs
        .iter()
        .map(|run| match run {
            bae_import::TrackTableRun::Sheet { sheet, rows } => (
                sheet.sheet_id.clone(),
                sheet.assigned.as_ref().map(|choice| choice.side),
                rows.len(),
            ),
            bae_import::TrackTableRun::Track(_) => panic!("a sheet carves each track"),
        })
        .collect();
    assert_eq!(
        runs,
        [(second.clone(), Some(disc(2)), 3), (first.clone(), Some(disc(2)), 3)],
        "the sheet the release kept leads, and the moved sheet follows it on the same disc"
    );
    assert!(draft.changed);

    manager.save_release_edit(moved, bae_library::GonePick::Refuse).await.expect("save the form");

    let mediums = library.records().get_release_mediums(&release_id).await.unwrap();
    assert_eq!(mediums.len(), 1, "the disc the sheet left is gone");
    let tracks = library.records().get_tracks_for_release(&release_id).await.unwrap();
    let places: Vec<_> =
        tracks.iter().map(|track| (track.side.get(), track.track_number.get())).collect();
    assert_eq!(places, [(1, 1), (1, 2), (1, 3), (1, 4), (1, 5), (1, 6)]);
    let reopened = manager.open_release_edit(&release_id).await.unwrap();
    let discs: Vec<_> = reopened
        .opened
        .display
        .tracks
        .iter()
        .map(|track| track.sheet.as_ref().map(|sheet| sheet.place.side))
        .collect();
    assert_eq!(discs, [Some(disc(1)); 6], "each sheet is on the side its tracks are on");
}
