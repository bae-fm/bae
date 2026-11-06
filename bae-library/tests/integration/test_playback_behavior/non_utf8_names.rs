use crate::test_playback_behavior::fixtures::*;
use crate::test_playback_behavior::support::*;
use std::os::unix::ffi::OsStrExt;

/// A track whose name isn't UTF-8 plays: playback opens the original by the
/// path the import recorded.
#[tokio::test]
async fn a_track_whose_name_is_not_utf8_plays() {
    bae_util::test_logs::install();
    let mut album = create_test_album();
    album.id = "non-utf8-name".to_string();
    album.tracklist.truncate(1);
    let (library, imported) = imported_release_setup(
        album,
        "non-utf8-name",
        uuid::Uuid::new_v4().to_string(),
        |album_dir| write_first_test_track(album_dir, b"01 Caf\xE9.flac"),
    )
    .await
    .expect("the album imports");
    let [track_id] = imported.track_ids.as_slice() else {
        panic!("one track: {:?}", imported.track_ids);
    };

    assert_plays(&library, track_id).await;
}

/// A track in a folder whose name isn't UTF-8 plays: the path the import
/// recorded holds the folder's own bytes.
#[tokio::test]
async fn a_track_in_a_folder_whose_name_is_not_utf8_plays() {
    bae_util::test_logs::install();
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let (library, temp_dir) = bae_library::test_libraries::setup_test_library_with(
        fakes.providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let album_dir = temp_dir.path().join(std::ffi::OsStr::from_bytes(b"Caf\xE9"));
    std::fs::create_dir_all(&album_dir).unwrap();
    let mut album = create_test_album();
    album.id = "non-utf8-folder".to_string();
    album.tracklist.truncate(1);
    let release_key = fakes.seed_discogs_test_release(album);
    write_first_test_track(&album_dir, b"01 Track.flac");

    let release_id = library.import_folder(album_dir, release_key).await;
    let tracks = library.get_tracks_for_release(&release_id).await.unwrap();
    let [track] = tracks.as_slice() else {
        panic!("one track: {tracks:?}");
    };

    assert_plays(&library, &track.id).await;
}

/// Write the first test track's FLAC into `album_dir` under `name`.
fn write_first_test_track(album_dir: &std::path::Path, name: &[u8]) {
    let fixture = bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac");
    std::fs::write(
        album_dir.join(std::ffi::OsStr::from_bytes(name)),
        bae_import::test_fixtures::read_fixture(&fixture),
    )
    .unwrap();
}

/// Play `track_id` on a capture device and wait until it plays and its audio
/// plays on.
async fn assert_plays(library: &bae_library::test_libraries::TestLibrary, track_id: &str) {
    let (playback_handle, _capture_stream_rx, audio_clock) =
        bae_library::test_libraries::start_manual_playback(library).await;
    let mut progress_rx = playback_handle.subscribe_progress();

    playback_handle.play(track_id.to_string());

    wait_for_state_on(
        &mut progress_rx,
        |state| matches!(state, PlaybackState::Playing { track, .. } if track.track_id == track_id),
    )
    .await
    .expect("the track plays");
    assert!(
        wait_for_position_advance(&playback_handle, &audio_clock, &mut progress_rx).await.is_some(),
        "its audio plays on"
    );
}
