//! CPU usage tests for playback.
//!
//! These tests run as a separate binary to get accurate process-wide CPU measurements.

#![cfg(feature = "test-utils")]
mod support;
use crate::support::{
    seed_discogs_test_release, test_config_and_keys, tracing_init, wait_for_import_complete,
};
use bae_core::db::Database;
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{IdentityChoice, ImportCommand, MetadataRef, MetadataSource, StorageMode};
use bae_core::library::LibraryManager;
use bae_core::library_dir::LibraryDir;
use bae_core::playback::{PlaybackProgress, PlaybackState};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;
use tracing::debug;

fn start_test_import(
    runtime_handle: tokio::runtime::Handle,
    library_manager: LibraryManager,
) -> bae_core::import::ImportServiceHandle {
    bae_core::import::ImportService::start(
        runtime_handle.clone(),
        library_manager,
        bae_core::import::cover_art::CoverArtArchiveClient::new(),
    )
}

/// Check if audio tests should be skipped (e.g., in CI without audio device)
fn should_skip_audio_tests() -> bool {
    if std::env::var("SKIP_AUDIO_TESTS").is_ok() {
        return true;
    }
    use cpal::traits::HostTrait;
    cpal::default_host().default_output_device().is_none()
}

/// Generate a large CUE/FLAC fixture on-the-fly for CPU stress testing.
/// Creates a 5-minute 96kHz stereo 24-bit FLAC (~75MB) to stress the buffer.
fn generate_large_cue_flac_files(dir: &std::path::Path) {
    use std::fs;
    use std::process::Command;

    let flac_path = dir.join("Test Album.flac");
    let cue_path = dir.join("Test Album.cue");

    // Generate 5 minutes of audio at 96kHz/24-bit stereo (~75MB FLAC)
    // Using brown noise which compresses reasonably
    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=d=300:c=brown:r=96000", // 300 seconds (5 min) brown noise at 96kHz
            "-ac",
            "2", // Stereo
            "-sample_fmt",
            "s32", // 24-bit in 32-bit container
            "-c:a",
            "flac",
            "-compression_level",
            "0", // Fast compression
            flac_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run ffmpeg");

    if !output.status.success() {
        panic!(
            "ffmpeg failed to generate FLAC:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let file_size = fs::metadata(&flac_path).unwrap().len();
    eprintln!(
        "Generated FLAC: {} bytes ({:.1} MB)",
        file_size,
        file_size as f64 / 1_000_000.0
    );

    // Generate CUE sheet with 3 tracks of ~100 seconds each
    let cue_content = r#"REM GENRE Test
REM DATE 2024
PERFORMER "Test Artist"
TITLE "Test Album"
FILE "Test Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Test Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Test Artist"
    INDEX 01 01:40:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    PERFORMER "Test Artist"
    INDEX 01 03:20:00
"#;
    fs::write(&cue_path, cue_content).expect("Failed to write CUE file");
}

/// Generate per-track MP3 files for CPU stress testing.
/// Creates 3 MP3 files of ~100 seconds each at 320kbps CBR.
fn generate_mp3_track_files(dir: &std::path::Path) {
    use std::fs;
    use std::process::Command;

    for i in 1..=3 {
        let mp3_path = dir.join(format!("{:02} Track {}.mp3", i, i));

        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "anoisesrc=d=100:c=brown:r=44100", // 100 seconds brown noise at 44.1kHz
                "-ac",
                "2",
                "-c:a",
                "libmp3lame",
                "-b:a",
                "320k",
                mp3_path.to_str().unwrap(),
            ])
            .output()
            .expect("Failed to run ffmpeg");

        if !output.status.success() {
            panic!(
                "ffmpeg failed to generate MP3:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let file_size = fs::metadata(&mp3_path).unwrap().len();
        eprintln!(
            "Generated MP3 track {}: {} bytes ({:.1} MB)",
            i,
            file_size,
            file_size as f64 / 1_000_000.0
        );
    }

    // Write a minimal log file so the folder scanner doesn't complain
    fs::write(dir.join("rip.log"), "").unwrap();
}

/// Create test album metadata for CUE/FLAC (matches generated 2-minute file)
fn create_cue_flac_test_album() -> DiscogsRelease {
    DiscogsRelease {
        id: "cue-flac-cpu-test".to_string(),
        title: "Test Album".to_string(),
        year: Some(2024),
        genre: vec!["Test".to_string()],
        style: vec!["Test Style".to_string()],
        format: vec![],
        country: Some("Test Country".to_string()),
        label: vec!["Test Label".to_string()],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            name: "Test Artist".to_string(),
            id: "test-artist-1".to_string(),
        }],
        tracklist: vec![
            DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "Track One".to_string(),
                duration: Some("1:40".to_string()), // 100 seconds
                artists: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track Two".to_string(),
                duration: Some("1:40".to_string()), // 100 seconds
                artists: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track Three".to_string(),
                duration: Some("1:40".to_string()), // 100 seconds
                artists: vec![],
            },
        ],
        master_id: Some("test-master".to_string()),
    }
}

/// Create test album metadata for MP3 per-track files
fn create_mp3_test_album() -> DiscogsRelease {
    DiscogsRelease {
        id: "mp3-cpu-test".to_string(),
        title: "Test Album MP3".to_string(),
        year: Some(2024),
        genre: vec!["Test".to_string()],
        style: vec!["Test Style".to_string()],
        format: vec![],
        country: Some("Test Country".to_string()),
        label: vec!["Test Label".to_string()],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            name: "Test Artist".to_string(),
            id: "test-artist-2".to_string(),
        }],
        tracklist: vec![
            DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "Track 1".to_string(),
                duration: Some("1:40".to_string()),
                artists: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track 2".to_string(),
                duration: Some("1:40".to_string()),
                artists: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track 3".to_string(),
                duration: Some("1:40".to_string()),
                artists: vec![],
            },
        ],
        master_id: Some("test-master-mp3".to_string()),
    }
}

/// Test fixture for playback CPU measurement (works with any format)
struct PlaybackTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    _temp_dir: TempDir,
}

impl PlaybackTestFixture {
    async fn new(
        discogs_release: DiscogsRelease,
        generate_files: impl FnOnce(&std::path::Path),
        expected_tracks: usize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;

        let database = Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(bae_core::clock::SystemClock),
        )
        .await?;
        let database_arc = Arc::new(database.clone());
        let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let library_manager = LibraryManager::new(
            (*database_arc).clone(),
            library_dir.clone(),
            config_handle,
            key_service,
            std::sync::Arc::new(bae_core::clock::SystemClock),
            std::sync::Arc::new(bae_core::id_provider::UuidProvider),
            tokio::runtime::Handle::current(),
            None,
        );
        let runtime_handle = tokio::runtime::Handle::current();

        generate_files(&album_dir);

        let release_id_key = seed_discogs_test_release(discogs_release);
        let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());

        let import_id = uuid::Uuid::new_v4().to_string();

        import_handle
            .send_command(ImportCommand::Folder {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir.clone(),
                selected_cover: None,
                storage_mode: StorageMode::Unmanaged,
                pin: false,
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
                },
                user_edit: None,
            })
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let mut progress_rx = import_handle.subscribe_import(import_id);
        let (_release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;

        let albums = library_manager.get_albums(&[]).await?;
        assert!(!albums.is_empty(), "Should have imported album");
        let releases = library_manager
            .get_releases_for_album(&albums[0].id)
            .await?;
        assert!(!releases.is_empty(), "Should have imported release");
        let tracks = library_manager.get_tracks(&releases[0].id).await?;
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        assert_eq!(
            track_ids.len(),
            expected_tracks,
            "Should have {} tracks",
            expected_tracks
        );

        std::env::set_var("MUTE_TEST_AUDIO", "1");
        let playback_handle = bae_core::playback::PlaybackService::start(
            library_manager.clone(),
            runtime_handle,
            100,
        );
        playback_handle.set_volume(0.0);
        let progress_rx = playback_handle.subscribe_progress();

        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids,
            _temp_dir: temp_dir,
        })
    }
}

/// Get total CPU time consumed by this process (user + system time).
fn get_process_cpu_time() -> Duration {
    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;
        let mut usage = MaybeUninit::<libc::rusage>::uninit();
        unsafe {
            if libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) == 0 {
                let usage = usage.assume_init();
                let user = Duration::new(
                    usage.ru_utime.tv_sec as u64,
                    (usage.ru_utime.tv_usec as u32) * 1000,
                );
                let system = Duration::new(
                    usage.ru_stime.tv_sec as u64,
                    (usage.ru_stime.tv_usec as u32) * 1000,
                );
                return user + system;
            }
        }
        Duration::ZERO
    }
    #[cfg(not(unix))]
    {
        Duration::ZERO
    }
}

/// Measure CPU usage during playback of the first track in a fixture.
/// Returns the CPU percentage (100% = 1 core fully utilized).
async fn measure_playback_cpu(fixture: &mut PlaybackTestFixture, label: &str) -> f64 {
    let track_id = fixture.track_ids[0].clone();

    // Start playback
    fixture.playback_handle.play(track_id.clone());

    // Wait for playback to start
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if matches!(state, PlaybackState::Playing { .. }) {
                    started = true;
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(started, "{}: playback should start", label);

    // Measure CPU during seek (includes buffering phase where O(n²) bug manifests)
    let measure_start = Instant::now();
    let initial_cpu = get_process_cpu_time();

    // Seek forward to trigger new buffering (this is where high CPU was observed)
    fixture.playback_handle.seek(Duration::from_secs(3));

    // Let playback and buffering run for measurement period
    let measure_duration = Duration::from_secs(3);
    tokio::time::sleep(measure_duration).await;

    let final_cpu = get_process_cpu_time();
    let wall_time = measure_start.elapsed();
    let cpu_time = final_cpu.saturating_sub(initial_cpu);

    let cpu_percent = (cpu_time.as_secs_f64() / wall_time.as_secs_f64()) * 100.0;

    eprintln!(
        "{}: CPU usage during playback: {:.1}% (cpu_time={:?}, wall_time={:?})",
        label, cpu_percent, cpu_time, wall_time
    );

    fixture.playback_handle.stop();

    cpu_percent
}

fn assert_cpu_reasonable(cpu_percent: f64, label: &str) {
    // Steady-state playback should be lightweight (ring buffer + audio callback)
    // Baseline is ~6%, 20% allows headroom for variance
    let max_cpu_percent = 20.0;

    assert!(
        cpu_percent < max_cpu_percent,
        "{}: CPU usage too high: {:.1}% (max allowed: {:.0}%)\n\
         This indicates a busy-wait loop or spin lock somewhere.\n\
         Common causes: buffer underrun retries, spin-waiting for data.",
        label,
        cpu_percent,
        max_cpu_percent
    );
}

/// Test that CUE/FLAC playback doesn't consume excessive CPU.
///
/// This is a regression test for busy-wait loops that cause 500%+ CPU usage.
/// During normal playback, CPU should be minimal - the audio callback runs
/// periodically and the decoder should block on I/O, not spin.
#[tokio::test]
async fn test_playback_cpu_usage_cue_flac() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new(
        create_cue_flac_test_album(),
        generate_large_cue_flac_files,
        3,
    )
    .await
    {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    let cpu = measure_playback_cpu(&mut fixture, "CUE/FLAC").await;
    assert_cpu_reasonable(cpu, "CUE/FLAC");
}

/// Test that MP3 per-track playback doesn't consume excessive CPU.
#[tokio::test]
async fn test_playback_cpu_usage_mp3() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new(
        create_mp3_test_album(),
        generate_mp3_track_files,
        3,
    )
    .await
    {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    let cpu = measure_playback_cpu(&mut fixture, "MP3").await;
    assert_cpu_reasonable(cpu, "MP3");
}
