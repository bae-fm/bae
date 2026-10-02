/// One album of a library fixture, credited to `artists`, with no cover.
fn fixture_album(title: &str, artists: &[&str], tracks: &[&str]) -> crate::library::FixtureAlbum {
    crate::library::FixtureAlbum {
        title: title.to_string(),
        artists: artists.iter().map(|artist| artist.to_string()).collect(),
        tracks: tracks.iter().map(|track| track.to_string()).collect(),
        cover: None,
    }
}

/// The library fixture holding `albums` and nothing else.
fn fixture_of_albums(albums: Vec<crate::library::FixtureAlbum>) -> crate::library::LibraryFixture {
    crate::library::LibraryFixture {
        albums,
        watched_folders: Vec::new(),
        playing: None,
    }
}

/// A fixture's albums land as the grid lists them: added in the fixture's
/// order, so newest first is that order reversed, each with its artist and
/// its tracks in order.
#[tokio::test]
async fn fixture_albums_land_in_the_order_they_were_added() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let fixture = fixture_of_albums(vec![
        fixture_album("First", &["Artist A"], &["One"]),
        fixture_album("Second", &["Artist A"], &["One"]),
        fixture_album("Third", &["Artist B"], &["Opening", "Closing"]),
    ]);

    manager.write_fixture(&fixture).await.unwrap();

    let newest_first = manager
        .get_albums(&[crate::db::AlbumSortCriterion {
            field: crate::db::AlbumSortField::DateAdded,
            direction: crate::db::SortDirection::Descending,
        }])
        .await
        .unwrap();
    let titles: Vec<&str> = newest_first.iter().map(|album| album.title.as_str()).collect();
    assert_eq!(titles, ["Third", "Second", "First"]);
    assert_eq!(newest_first[1].artist_id, newest_first[2].artist_id);

    let third = &newest_first[0];
    let artists = manager.get_artists_for_album(&third.id).await.unwrap();
    assert_eq!(artists.len(), 1);
    assert_eq!(artists[0].name, "Artist B");
    let release_id = third.primary_release_id.clone().unwrap();
    let tracks = manager.get_tracks_for_release(&release_id).await.unwrap();
    let track_titles: Vec<&str> = tracks.iter().map(|track| track.title.as_str()).collect();
    assert_eq!(track_titles, ["Opening", "Closing"]);
}

/// An album credited to two artists is credited to both in order, and the
/// grid grouped by artist lists it under each.
#[tokio::test]
async fn a_fixture_album_credited_to_two_artists_is_listed_under_each() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let fixture = fixture_of_albums(vec![
        fixture_album("Shared Album", &["Artist A", "Artist B"], &["One"]),
        fixture_album("Own Album", &["Artist B"], &["One"]),
    ]);

    manager.write_fixture(&fixture).await.unwrap();

    let albums = manager.get_albums(&[]).await.unwrap();
    let shared = albums
        .iter()
        .find(|album| album.title == "Shared Album")
        .expect("the shared album is in the library");
    let credited: Vec<String> = manager
        .get_artists_for_album(&shared.id)
        .await
        .unwrap()
        .into_iter()
        .map(|artist| artist.name)
        .collect();
    assert_eq!(credited, ["Artist A", "Artist B"]);

    let mut browse = manager.subscribe_album_browse(
        &[],
        true,
        [crate::library::LibraryPageWindow {
            offset: 0,
            limit: 10,
        }]
        .into(),
    );
    let grouped = browse.next().await.into_result().unwrap();
    let rows = &grouped.windows[0].rows;
    let sections: Vec<(String, Vec<String>)> = grouped
        .sections
        .iter()
        .map(|section| {
            let start = section.window.offset as usize;
            let end = start + section.window.limit as usize;
            (
                section.title.clone(),
                rows[start..end]
                    .iter()
                    .map(|album| album.title.clone())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        sections,
        [
            ("Artist A".to_string(), vec!["Shared Album".to_string()]),
            (
                "Artist B".to_string(),
                vec!["Own Album".to_string(), "Shared Album".to_string()]
            ),
        ]
    );
}

/// An album's cover is stored as its release's cover: the image resized as
/// every stored cover is.
#[tokio::test]
async fn a_fixture_albums_cover_is_its_releases_cover() {
    let (manager, temp_dir) = setup_test_manager().await;
    let cover = temp_dir.path().join("cover.png");
    ::image::RgbImage::from_pixel(4, 4, ::image::Rgb([200, 40, 40]))
        .save(&cover)
        .unwrap();
    let mut album = fixture_album("Covered Album", &["Artist A"], &["One"]);
    album.cover = Some(cover.clone());

    manager
        .write_fixture(&fixture_of_albums(vec![album]))
        .await
        .unwrap();

    let albums = manager.get_albums(&[]).await.unwrap();
    let release_id = albums[0].primary_release_id.clone().unwrap();
    let stored = manager
        .read_cover_image_blob(&release_id)
        .await
        .unwrap()
        .expect("the release has a cover");
    assert_eq!(
        stored,
        crate::util::cover::resize_cover(&std::fs::read(&cover).unwrap()).unwrap()
    );
}

/// An album that credits no artist is no album a library holds, and the
/// fixture naming it writes nothing.
#[tokio::test]
async fn a_fixture_album_crediting_no_artist_writes_nothing() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let fixture = fixture_of_albums(vec![
        fixture_album("Credited Album", &["Artist A"], &["One"]),
        fixture_album("Uncredited Album", &[], &["One"]),
    ]);

    let error = manager.write_fixture(&fixture).await.unwrap_err();

    assert!(matches!(
        error,
        crate::library::LibraryFixtureError::NoArtist { ref album } if album == "Uncredited Album"
    ));
    assert!(manager.get_albums(&[]).await.unwrap().is_empty());
}

/// A fixture candidate in `state`, its folder named `folder`, of two tracks.
fn fixture_candidate(
    folder: &str,
    state: crate::library::FixtureCandidateState,
) -> crate::library::FixtureCandidate {
    crate::library::FixtureCandidate {
        folder: folder.to_string(),
        tracks: vec!["01 Track.flac".to_string(), "02 Track.flac".to_string()],
        state,
    }
}

/// Found's rows the import list reads, by display path, newest first.
async fn found_rows(manager: &LibraryManager) -> crate::import::ImportListProjection {
    manager
        .load_import_list(crate::import::ImportListRequest {
            windows: std::iter::once(crate::library::LibraryPageWindow {
                offset: 0,
                limit: 50,
            })
            .collect(),
            ..crate::import::ImportListRequest::default()
        })
        .await
        .unwrap()
}

/// Each candidate of a watched folder lands on Found in the state the
/// fixture gives it, read back from the stored rows the list reads, and the
/// folder reads as read through.
#[tokio::test]
async fn fixture_candidates_land_on_found_in_their_states() {
    use crate::import::PendingStanding;
    use crate::library::FixtureCandidateState as State;
    let (manager, temp_dir) = setup_test_manager().await;
    let root = temp_dir.path().join("watched").to_string_lossy().into_owned();
    let states = [
        ("Not Looked Up", State::NotLookedUp),
        ("Needs You", State::NeedsYou),
        ("Identified", State::Identified),
        ("Unmatched", State::Unmatched),
        ("Lookup Error", State::LookupError),
        ("Error", State::Error),
        ("Import Error", State::ImportError),
    ];
    let fixture = crate::library::LibraryFixture {
        albums: Vec::new(),
        watched_folders: vec![crate::library::FixtureWatchedFolder {
            path: root.clone(),
            candidates: states
                .iter()
                .map(|(folder, state)| fixture_candidate(folder, *state))
                .collect(),
        }],
        playing: None,
    };

    manager.write_fixture(&fixture).await.unwrap();

    let projection = found_rows(&manager).await;
    let standings: std::collections::BTreeMap<String, Option<PendingStanding>> = projection
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => {
                Some((row.display_path.clone(), row.action_basis.standing.clone()))
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
                Some(PendingStanding::NeedsYou {
                    reason: crate::import::NeedsYouReason::NotFound
                })
            ),
            ("Identified".to_string(), Some(PendingStanding::Identified)),
            ("Unmatched".to_string(), Some(PendingStanding::Unmatched)),
            ("Lookup Error".to_string(), Some(PendingStanding::LookupError)),
            (
                "Error".to_string(),
                Some(PendingStanding::Error {
                    failure: crate::signals::InternalFailure {
                        detail: crate::library::FIXTURE_LOOKUP_FAILURE.to_string(),
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
            .map(|folder| folder.path.as_str())
            .collect::<Vec<_>>(),
        [root.as_str()]
    );
    let scans = manager.load_folder_scan_progress().await.unwrap();
    assert_eq!(scans.activity, None, "no folder reads as being read");
    assert_eq!(
        scans
            .statuses
            .iter()
            .map(|status| (status.watched_folder_path.clone(), status.status.clone()))
            .collect::<Vec<_>>(),
        [(root, crate::import::FolderScanStatus::Complete)]
    );
}

/// An identified candidate leads with the release it is linked to, and a
/// candidate whose import failed says why.
#[tokio::test]
async fn fixture_candidates_carry_what_their_states_name() {
    use crate::library::FixtureCandidateState as State;
    let (manager, temp_dir) = setup_test_manager().await;
    let fixture = crate::library::LibraryFixture {
        albums: Vec::new(),
        watched_folders: vec![crate::library::FixtureWatchedFolder {
            path: temp_dir.path().join("watched").to_string_lossy().into_owned(),
            candidates: vec![
                fixture_candidate("Identified Folder", State::Identified),
                fixture_candidate("Failed Folder", State::ImportError),
            ],
        }],
        playing: None,
    };

    manager.write_fixture(&fixture).await.unwrap();

    let rows: Vec<crate::import::TriageRow> = found_rows(&manager)
        .await
        .windows
        .into_iter()
        .flat_map(|window| window.items)
        .filter_map(|item| match item {
            crate::import::ImportListItem::Candidate { row, .. } => Some(row),
            _ => None,
        })
        .collect();
    let row = |path: &str| {
        rows.iter()
            .find(|row| row.display_path == path)
            .unwrap_or_else(|| panic!("{path} is listed"))
    };
    assert_eq!(
        row("Identified Folder")
            .matched
            .as_ref()
            .map(|matched| matched.title.as_str()),
        Some("Identified Folder")
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
        Some(crate::import::TriageImportStatus::Error {
            failure: crate::import::ImportFailureReason::Error {
                detail: crate::library::FIXTURE_IMPORT_FAILURE.to_string()
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
fn fixture_playing(playing: crate::library::FixturePlaying) -> crate::library::LibraryFixture {
    crate::library::LibraryFixture {
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
    let (manager, temp_dir) = setup_test_manager().await;
    let sheet = fixture_disc_image(temp_dir.path());

    manager
        .write_fixture(&fixture_playing(crate::library::FixturePlaying {
            title: "Album Title".to_string(),
            artists: vec!["Artist Name".to_string()],
            cue_sheet: sheet,
            track: 2,
            position_ms: 0,
        }))
        .await
        .unwrap();

    let albums = manager.get_albums(&[]).await.unwrap();
    assert_eq!(
        albums.iter().map(|album| album.title.as_str()).collect::<Vec<_>>(),
        ["Album Title"]
    );
    let release_id = albums[0].primary_release_id.clone().unwrap();
    let tracks = manager.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(
        tracks
            .iter()
            .map(|track| (track.title.as_str(), track.duration_ms))
            .collect::<Vec<_>>(),
        [("Opening Track", Some(3_000)), ("Pregap Track", Some(5_000))]
    );
    let pregap = manager.resolve_track_audio(&tracks[1].id).await.unwrap();
    assert_eq!(pregap.pregap_ms, Some(2_000));
    assert_eq!(
        pregap
            .segments
            .iter()
            .map(|segment| (segment.role.clone(), segment.span.start_sample, segment.span.end_sample))
            .collect::<Vec<_>>(),
        [
            (crate::db::DbAudioSegmentRole::AudioPregap, 132_300, Some(220_500)),
            (crate::db::DbAudioSegmentRole::Main, 220_500, None),
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
#[cfg(feature = "test-utils")]
#[tokio::test(flavor = "multi_thread")]
async fn playback_resumes_the_fixtures_playing_album_in_the_pregap() {
    let (manager, temp_dir) = setup_test_manager().await;
    let sheet = fixture_disc_image(temp_dir.path());
    manager
        .write_fixture(&fixture_playing(crate::library::FixturePlaying {
            title: "Album Title".to_string(),
            artists: vec!["Artist Name".to_string()],
            cue_sheet: sheet,
            track: 2,
            position_ms: 0,
        }))
        .await
        .unwrap();
    let release_id = manager.get_albums(&[]).await.unwrap()[0]
        .primary_release_id
        .clone()
        .unwrap();
    let pregap_track = manager.get_tracks_for_release(&release_id).await.unwrap()[1]
        .id
        .clone();

    let (device, _capture) = crate::playback::RealtimeCaptureAudioDevice::new();
    let playback = manager.start_playback_service_with_audio_device(
        tokio::runtime::Handle::current(),
        100,
        true,
        Box::new(device),
    );
    let mut values = playback.subscribe_values();
    let restored = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        values.wait_for(|values| {
            matches!(values.state, crate::playback::PlaybackState::Paused { .. })
                && values.position.as_ref().is_some_and(|position| {
                    position.track_id == pregap_track && position.position_ms == -2_000
                })
        }),
    )
    .await
    .expect("playback resumes the fixture's track")
    .unwrap()
    .clone();
    let crate::playback::PlaybackState::Paused { track, .. } = restored.state else {
        unreachable!("waited for a paused state");
    };
    assert_eq!(track.track_id, pregap_track);
    playback.shutdown().await;
}
