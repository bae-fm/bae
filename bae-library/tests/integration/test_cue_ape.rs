//! Tests for CUE/APE format handling.
//!
//! CUE/APE albums have multiple tracks in a single APE file. The import must:
//! - Parse the CUE sheet to find track boundaries
//! - Record per-track timing (track_start_ms / track_end_ms)
//! - Store correct per-track durations (NOT the full file duration)
//! - Enable playback of individual tracks via full-file decode with seek/stop

mod multi_disc;
mod single_disc;

use bae_audio::test_samples::{assert_captured_matches_reference, samples_as_f32};
use bae_catalogs::discogs::models::DiscogsRelease;
use bae_import::test_imports::wait_for_import_complete;
use bae_import::{ImportCommand, ImportDestination};
use bae_library::LibraryManager;
use bae_playback::PlaybackProgress;
use bae_types::track_time::TrackTime;
use bae_util::content_type::ContentType;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;
use tracing::info;

fn copy_cue_ape_fixture(dir: &Path) {
    use std::fs;
    let fixture_dir = bae_import::test_fixtures::cue_ape_dir();
    let ape_data = fs::read(fixture_dir.join("Test Album.ape"))
        .unwrap_or_else(|_| panic!("CUE/APE fixture not found at {:?}", fixture_dir));
    let cue_data = fs::read(fixture_dir.join("Test Album.cue"))
        .unwrap_or_else(|_| panic!("CUE/APE fixture not found at {:?}", fixture_dir));
    fs::write(dir.join("Test Album.ape"), &ape_data).expect("write ape");
    fs::write(dir.join("Test Album.cue"), &cue_data).expect("write cue");
    info!("Copied CUE/APE fixture: {} bytes APE", ape_data.len());
}

fn create_test_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        master_id: Some("test-master".to_string()),
        ..bae_catalogs::test_releases::discogs_test_release(
            "test-cue-ape",
            "Test Album",
            &[("Track One", "0:30"), ("Track Two", "0:30"), ("Track Three", "0:30")],
        )
    }
}

/// Ground truth durations derived from CUE INDEX 01 timings.
/// CUE frames are 1/75th of a second.
///
/// Track boundaries (INDEX 01):
///   1: 00:00:00 (0ms)
///   2: 00:30:00 (30000ms)
///   3: 01:00:00 (60000ms)
/// Full file: 90000ms
const EXPECTED_DURATIONS_MS: [u64; 3] = [
    30000, // Track 1: 0 -> 30000
    30000, // Track 2: 30000 -> 60000
    30000, // Track 3: 60000 -> 90000
];

const FULL_FILE_DURATION_MS: u64 = 90000;

/// Import the 3-track CUE/APE fixture into a fresh library, returning the
/// manager, the imported release id, and the temp root that owns both the
/// album folder and the database.
async fn import_cue_ape_fixture() -> (bae_library::test_libraries::TestLibrary, String, TempDir) {
    bae_util::test_logs::install();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    copy_cue_ape_fixture(&album_dir);

    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let library =
        bae_library::test_libraries::open_test_library_with(&db_dir, fakes.providers()).await;
    let release_id_key = fakes.seed_discogs_test_release(create_test_discogs_release());
    let release_id = library.import_folder(album_dir, release_id_key).await;
    (library, release_id, temp_root)
}

struct CueApeTestFixture {
    playback_handle: bae_playback::PlaybackHandle,
    progress_rx: bae_playback::PlaybackProgressFeed,
    track_ids: Vec<String>,
    capture_stream_rx: bae_playback::test_capture::CaptureStreamRx,
    _temp_dir: TempDir,
}

impl CueApeTestFixture {
    /// The 3-track CUE/APE album, imported and playing through a full-speed
    /// capture sink.
    async fn with_capture() -> Result<Self, Box<dyn std::error::Error>> {
        let (library, imported) = bae_library::test_libraries::imported_release_setup(
            create_test_discogs_release(),
            "test",
            uuid::Uuid::new_v4().to_string(),
            copy_cue_ape_fixture,
        )
        .await?;
        assert_eq!(imported.track_ids.len(), 3, "Should have 3 tracks from CUE/APE");
        let (playback_handle, capture_stream_rx) =
            bae_library::test_libraries::start_capture_playback(
                &library,
                bae_playback::test_capture::TestAudioDevice::Capture,
            )
            .await;
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids: imported.track_ids,
            capture_stream_rx,
            _temp_dir: imported.temp_dir,
        })
    }

    async fn next_capture_stream(&mut self) -> bae_playback::CapturedStream {
        bae_playback::test_capture::next_capture_stream(&mut self.capture_stream_rx).await
    }
}
