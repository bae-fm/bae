//! The library fixture a debug build seeds: its albums, credits and covers, its
//! import candidates in their states, and the album it plays.

use super::*;
use crate::manager::tests::*;

/// One album of a library fixture, credited to `artists`, with no cover.
fn fixture_album(title: &str, artists: &[&str], tracks: &[&str]) -> crate::FixtureAlbum {
    crate::FixtureAlbum {
        title: title.to_string(),
        artists: artists.iter().map(|artist| artist.to_string()).collect(),
        tracks: tracks.iter().map(|track| track.to_string()).collect(),
        cover: None,
    }
}

/// The library fixture holding `albums` and nothing else.
fn fixture_of_albums(albums: Vec<crate::FixtureAlbum>) -> crate::LibraryFixture {
    crate::LibraryFixture { albums, watched_folders: Vec::new(), playing: None }
}

/// A fixture's albums land as the grid lists them: added in the fixture's
/// order, so newest first is that order reversed, each with its artist and
/// its tracks in order.
#[tokio::test]
async fn fixture_albums_land_in_the_order_they_were_added() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let fixture = fixture_of_albums(vec![
        fixture_album("First", &["Artist A"], &["One"]),
        fixture_album("Second", &["Artist A"], &["One"]),
        fixture_album("Third", &["Artist B"], &["Opening", "Closing"]),
    ]);

    manager.write_fixture(&fixture).await.unwrap();

    let newest_first = views(manager)
        .get_albums(&[bae_views::AlbumSortCriterion {
            field: bae_views::AlbumSortField::DateAdded,
            direction: bae_store::SortDirection::Descending,
        }])
        .await
        .unwrap();
    let titles: Vec<&str> = newest_first.iter().map(|album| album.title.as_str()).collect();
    assert_eq!(titles, ["Third", "Second", "First"]);
    let second_artists = manager.get_artists_for_album(&newest_first[1].id).await.unwrap();
    let first_artists = manager.get_artists_for_album(&newest_first[2].id).await.unwrap();
    assert_eq!(second_artists[0].artist.id, first_artists[0].artist.id);

    let third = &newest_first[0];
    let artists = manager.get_artists_for_album(&third.id).await.unwrap();
    assert_eq!(artists.len(), 1);
    assert_eq!(artists[0].artist.name, "Artist B");
    let release_id = manager.records.get_releases_for_album(&third.id).await.unwrap()[0].id.clone();
    let tracks = manager.records.get_tracks_for_release(&release_id).await.unwrap();
    let track_titles: Vec<&str> = tracks.iter().map(|track| track.title.as_str()).collect();
    assert_eq!(track_titles, ["Opening", "Closing"]);
}

/// An album credited to two artists is credited to both in order, and the
/// grid grouped by artist lists it under each.
#[tokio::test]
async fn a_fixture_album_credited_to_two_artists_is_listed_under_each() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let fixture = fixture_of_albums(vec![
        fixture_album("Shared Album", &["Artist A", "Artist B"], &["One"]),
        fixture_album("Own Album", &["Artist B"], &["One"]),
    ]);

    manager.write_fixture(&fixture).await.unwrap();

    let albums = views(manager).get_albums(&[]).await.unwrap();
    let shared = albums
        .iter()
        .find(|album| album.title == "Shared Album")
        .expect("the shared album is in the library");
    let credited: Vec<String> = manager
        .get_artists_for_album(&shared.id)
        .await
        .unwrap()
        .into_iter()
        .map(|artist| artist.artist.name)
        .collect();
    assert_eq!(credited, ["Artist A", "Artist B"]);

    let mut browse = views(manager).subscribe_album_browse(
        &[],
        true,
        [bae_store::LibraryPageWindow { offset: 0, limit: 10 }].into(),
    );
    let grouped = browse.next().await.expect("the query stays open").result.unwrap();
    let rows = &grouped.windows[0].rows;
    let sections: Vec<(String, Vec<String>)> = grouped
        .sections
        .iter()
        .map(|section| {
            let start = section.window.offset as usize;
            let end = start + section.window.limit as usize;
            (
                section.title.clone(),
                rows[start..end].iter().map(|row| row.album.title.clone()).collect(),
            )
        })
        .collect();
    assert_eq!(
        sections,
        [
            ("Artist A".to_string(), vec!["Shared Album".to_string()]),
            ("Artist B".to_string(), vec!["Own Album".to_string(), "Shared Album".to_string()]),
        ]
    );
}

/// An album's cover is stored as its release's cover: the image resized as
/// every stored cover is.
#[tokio::test]
async fn a_fixture_albums_cover_is_its_releases_cover() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let cover = temp_dir.path().join("cover.png");
    ::image::RgbImage::from_pixel(4, 4, ::image::Rgb([200, 40, 40])).save(&cover).unwrap();
    let mut album = fixture_album("Covered Album", &["Artist A"], &["One"]);
    album.cover = Some(cover.clone());

    manager.write_fixture(&fixture_of_albums(vec![album])).await.unwrap();

    let albums = views(manager).get_albums(&[]).await.unwrap();
    let release_id =
        manager.records.get_releases_for_album(&albums[0].id).await.unwrap()[0].id.clone();
    let stored = manager
        .read_cover_image_bytes(&release_id)
        .await
        .unwrap()
        .expect("the release has a cover");
    assert_eq!(
        stored,
        bae_util::stored_image::stored_jpeg(&std::fs::read(&cover).unwrap()).unwrap()
    );
}

/// An album that credits no artist is no album a library holds, and the
/// fixture naming it writes nothing.
#[tokio::test]
async fn a_fixture_album_crediting_no_artist_writes_nothing() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let fixture = fixture_of_albums(vec![
        fixture_album("Credited Album", &["Artist A"], &["One"]),
        fixture_album("Uncredited Album", &[], &["One"]),
    ]);

    let error = manager.write_fixture(&fixture).await.unwrap_err();

    assert!(matches!(
        error,
        crate::LibraryFixtureError::NoArtist { ref album } if album == "Uncredited Album"
    ));
    assert!(views(manager).get_albums(&[]).await.unwrap().is_empty());
}

/// A fixture candidate in `state`, its folder named `folder`, of two tracks.
fn fixture_candidate(folder: &str, state: crate::FixtureCandidateState) -> crate::FixtureCandidate {
    crate::FixtureCandidate {
        folder: folder.to_string(),
        tracks: vec!["01 Track.flac".to_string(), "02 Track.flac".to_string()],
        state,
    }
}

/// Found's rows the import list reads, by display path, newest first.
async fn found_rows(manager: &LibraryManager) -> bae_import::ImportListProjection {
    let tables = bae_import::tables::ImportTables::new(manager.store.clone());
    let list = tables.open_import_list(bae_import::ImportListView::default()).await.unwrap();
    tables
        .load_import_list(bae_import::ImportListRequest {
            list,
            windows: std::iter::once(bae_store::LibraryPageWindow { offset: 0, limit: 50 })
                .collect(),
        })
        .await
        .unwrap()
}

/// Each candidate of a watched folder lands on Found in the state the
/// fixture gives it, read back from the stored rows the list reads, and the
/// folder reads as read through.
#[tokio::test]
async fn fixture_candidates_land_on_found_in_their_states() {
    use crate::FixtureCandidateState as State;
    use bae_import::PendingStanding;
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let tables = bae_import::tables::ImportTables::new(manager.store.clone());
    let root = temp_dir.path().join("watched").to_path_buf();
    let states = [
        ("Not Looked Up", State::NotLookedUp),
        ("Needs You", State::NeedsYou),
        ("Identified", State::Identified),
        ("Unmatched", State::Unmatched),
        ("Lookup Error", State::LookupError),
        ("Error", State::Error),
        ("Import Error", State::ImportError),
    ];
    let fixture = crate::LibraryFixture {
        albums: Vec::new(),
        watched_folders: vec![crate::FixtureWatchedFolder {
            path: root.display().to_string(),
            candidates: states
                .iter()
                .map(|(folder, state)| fixture_candidate(folder, *state))
                .collect(),
        }],
        playing: None,
    };

    manager.write_fixture(&fixture).await.unwrap();

    let projection = found_rows(manager).await;
    let standings: std::collections::BTreeMap<String, Option<PendingStanding>> = projection
        .windows
        .iter()
        .flat_map(|window| &window.rows)
        .filter_map(|item| match item {
            bae_import::ImportListItem::Candidate { row, live, .. } => {
                Some((row.display_path.clone(), live.standing.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        standings,
        std::collections::BTreeMap::from([
            ("Not Looked Up".to_string(), Some(PendingStanding::NotLookedUp)),
            (
                "Needs You".to_string(),
                Some(PendingStanding::NeedsYou { reason: bae_import::NeedsYouReason::NotFound })
            ),
            ("Identified".to_string(), Some(PendingStanding::Identified)),
            ("Unmatched".to_string(), Some(PendingStanding::Unmatched)),
            ("Lookup Error".to_string(), Some(PendingStanding::LookupError)),
            (
                "Error".to_string(),
                Some(PendingStanding::Error {
                    failure: bae_catalogs::InternalFailure {
                        detail: crate::FIXTURE_LOOKUP_FAILURE.to_string(),
                    }
                })
            ),
            ("Import Error".to_string(), Some(PendingStanding::ImportError)),
        ])
    );
    assert_eq!(projection.summary.counts.pending, states.len() as u32);
    assert_eq!(
        projection
            .summary
            .watched_folders
            .iter()
            .map(|folder| folder.path.as_path())
            .collect::<Vec<_>>(),
        [root.as_path()]
    );
    let scans = tables.load_stored_folder_scans().await.unwrap();
    assert_eq!(
        scans
            .iter()
            .map(|scan| {
                let outcome = scan.last_scan.as_ref().map(|scan| scan.outcome.clone());
                (scan.watched_folder_path.clone(), outcome)
            })
            .collect::<Vec<_>>(),
        [(root, Some(bae_import::StoredScanOutcome::Complete))],
        "the fixture's folder stores a finished read"
    );
}

/// An identified candidate leads with the release it is linked to, and a
/// candidate whose import failed says why.
#[tokio::test]
async fn fixture_candidates_carry_what_their_states_name() {
    use crate::FixtureCandidateState as State;
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let fixture = crate::LibraryFixture {
        albums: Vec::new(),
        watched_folders: vec![crate::FixtureWatchedFolder {
            path: temp_dir.path().join("watched").to_string_lossy().into_owned(),
            candidates: vec![
                fixture_candidate("Identified Folder", State::Identified),
                fixture_candidate("Failed Folder", State::ImportError),
            ],
        }],
        playing: None,
    };

    manager.write_fixture(&fixture).await.unwrap();

    let rows: Vec<bae_import::TriageRow> = found_rows(manager)
        .await
        .windows
        .into_iter()
        .flat_map(|window| window.rows)
        .filter_map(|item| match item {
            bae_import::ImportListItem::Candidate { row, .. } => Some(row),
            _ => None,
        })
        .collect();
    let row = |path: &str| {
        rows.iter()
            .find(|row| row.display_path == path)
            .unwrap_or_else(|| panic!("{path} is listed"))
    };
    assert!(
        matches!(
            row("Identified Folder").reading,
            bae_import::triage::TriageReading::Identified { .. }
        ),
        "the row reads as linked to its release"
    );
    assert_eq!(
        row("Identified Folder")
            .metadata_summary
            .as_ref()
            .map(|summary| summary.album_title.as_str()),
        Some("Identified Folder"),
        "the row shows the draft read from the release"
    );
    assert_eq!(
        row("Failed Folder").import_status,
        Some(bae_import::TriageImportStatus::Error {
            failure: bae_import::ImportFailureReason {
                detail: crate::FIXTURE_IMPORT_FAILURE.to_string()
            }
        })
    );
}

/// A disc image in `folder`: ten seconds of silent CD audio as a WAV file, and
/// a sheet carving it into two tracks, the second after an INDEX 00 two
/// seconds before its INDEX 01. Returns the sheet's path.
fn fixture_disc_image(folder: &std::path::Path) -> std::path::PathBuf {
    let samples: u32 = 44_100 * 10;
    let data_size = samples * 4;
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&44_100u32.to_le_bytes());
    wav.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.resize(44 + data_size as usize, 0);
    std::fs::write(folder.join("disc.wav"), wav).unwrap();
    let sheet = folder.join("disc.cue");
    std::fs::write(
        &sheet,
        "PERFORMER \"Artist Name\"\n\
         TITLE \"Album Title\"\n\
         FILE \"disc.wav\" WAVE\n\
         \x20 TRACK 01 AUDIO\n\
         \x20   TITLE \"Opening Track\"\n\
         \x20   INDEX 01 00:00:00\n\
         \x20 TRACK 02 AUDIO\n\
         \x20   TITLE \"Pregap Track\"\n\
         \x20   INDEX 00 00:03:00\n\
         \x20   INDEX 01 00:05:00\n",
    )
    .unwrap();
    sheet
}

/// The fixture of nothing but `playing`.
fn fixture_playing(playing: crate::FixturePlaying) -> crate::LibraryFixture {
    crate::LibraryFixture {
        albums: Vec::new(),
        watched_folders: Vec::new(),
        playing: Some(playing),
    }
}

/// The playing album is laid out over its disc image as an import lays out a
/// CUE image's tracks: each track a window of the image, the second starting
/// with its pregap, which the library reads from the user's own file.
#[tokio::test]
async fn a_fixtures_playing_album_lays_its_tracks_over_the_disc_image() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let sheet = fixture_disc_image(temp_dir.path());

    manager
        .write_fixture(&fixture_playing(crate::FixturePlaying {
            title: "Album Title".to_string(),
            artists: vec!["Artist Name".to_string()],
            cue_sheet: sheet,
            track: 2,
            position_ms: 0,
        }))
        .await
        .unwrap();

    let albums = views(manager).get_albums(&[]).await.unwrap();
    assert_eq!(
        albums.iter().map(|album| album.title.as_str()).collect::<Vec<_>>(),
        ["Album Title"]
    );
    let release_id =
        manager.records.get_releases_for_album(&albums[0].id).await.unwrap()[0].id.clone();
    let tracks = manager.records.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(
        tracks.iter().map(|track| (track.title.as_str(), track.duration_ms)).collect::<Vec<_>>(),
        [("Opening Track", 3_000), ("Pregap Track", 5_000)]
    );
    let pregap =
        bae_records::ResolvedTrackAudio::resolve(&manager.records, &tracks[1].id).await.unwrap();
    assert_eq!(pregap.pregap(), std::time::Duration::from_secs(2));
    assert_eq!(
        pregap
            .segments
            .iter()
            .map(|segment| (
                segment.role.clone(),
                segment.span.start_sample,
                segment.span.end_sample
            ))
            .collect::<Vec<_>>(),
        [
            (bae_records::DbAudioSegmentRole::AudioPregap, 132_300, Some(220_500)),
            (bae_records::DbAudioSegmentRole::Main, 220_500, None),
        ]
    );
    let files = manager.get_files_for_release(&release_id).await.unwrap();
    let image = manager
        .file_local_path(&files[0].id)
        .await
        .unwrap()
        .expect("the image is the user's own file");
    assert_eq!(image, temp_dir.path().join("disc.wav"));
}

/// Playback starts on the fixture's resume row: paused on the track it
/// names, at the start of that track's pregap.
#[tokio::test(flavor = "multi_thread")]
async fn playback_resumes_the_fixtures_playing_album_in_the_pregap() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let sheet = fixture_disc_image(temp_dir.path());
    manager
        .write_fixture(&fixture_playing(crate::FixturePlaying {
            title: "Album Title".to_string(),
            artists: vec!["Artist Name".to_string()],
            cue_sheet: sheet,
            track: 2,
            position_ms: 0,
        }))
        .await
        .unwrap();
    let album_id = views(manager).get_albums(&[]).await.unwrap()[0].id.clone();
    let release_id = manager.records.get_releases_for_album(&album_id).await.unwrap()[0].id.clone();
    let pregap_track =
        manager.records.get_tracks_for_release(&release_id).await.unwrap()[1].id.clone();

    let (device, _capture, _audio_clock) = bae_playback::test_capture::manual_capture_device();
    let playback = test
        .library
        .start_playback_on(device, bae_playback::test_capture::wall_playback_clock(), true)
        .await;
    let values = manager.values_until_closed(manager.subscribe_now_playing());
    let mut positions = playback.subscribe_positions();
    let restored = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let value = values.next().await.expect("the playback values resolve");
            if let Some(bae_playback::PlaybackState::Paused { track, .. }) =
                value.map(|value| value.playback.state)
            {
                return track;
            }
        }
    })
    .await
    .expect("playback resumes the fixture's track");
    assert_eq!(restored.track.track_id, pregap_track);
    tokio::time::timeout(
        bae_util::test_channels::HANG_GUARD,
        positions.wait_for(|positions| {
            positions.library.as_ref().is_some_and(|position| {
                position.track_id == pregap_track && position.position_ms == -2_000
            })
        }),
    )
    .await
    .expect("playback restores the fixture's track at its pregap")
    .unwrap();
    playback.shutdown().await;
}
