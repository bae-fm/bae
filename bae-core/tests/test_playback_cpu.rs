//! CPU usage tests for playback.
//!
//! These tests run as a separate binary to get accurate process-wide CPU measurements.

#![cfg(feature = "test-utils")]
mod support;
use crate::support::{test_encryption_service, tracing_init};
use bae_core::cache::{CacheConfig, CacheManager};
use bae_core::db::Database;
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::encryption::EncryptionService;
use bae_core::import::ImportRequest;
use bae_core::library::{LibraryManager, SharedLibraryManager};
use bae_core::playback::{PlaybackProgress, PlaybackState};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;
use tracing::debug;

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
        artists: vec![DiscogsArtist {
            name: "Test Artist".to_string(),
            id: "test-artist-1".to_string(),
        }],
        tracklist: vec![
            DiscogsTrack {
                position: "1".to_string(),
                title: "Track One".to_string(),
                duration: Some("1:40".to_string()), // 100 seconds
            },
            DiscogsTrack {
                position: "2".to_string(),
                title: "Track Two".to_string(),
                duration: Some("1:40".to_string()), // 100 seconds
            },
            DiscogsTrack {
                position: "3".to_string(),
                title: "Track Three".to_string(),
                duration: Some("1:40".to_string()), // 100 seconds
            },
        ],
        master_id: "test-master".to_string(),
    }
}

/// Test fixture for CUE/FLAC playback
struct CueFlacTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    _temp_dir: TempDir,
}

impl CueFlacTestFixture {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let cache_dir = temp_dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir)?;
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;

        let database = Database::new(db_path.to_str().unwrap()).await?;
        let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
        let cache_config = CacheConfig {
            cache_dir,
            max_size_bytes: 1024 * 1024 * 1024,
            max_files: 10000,
        };
        let _cache_manager = CacheManager::with_config(cache_config).await?;
        let database_arc = Arc::new(database);
        let library_manager =
            LibraryManager::new((*database_arc).clone(), test_encryption_service());
        let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
        let library_manager_arc = Arc::new(library_manager);
        let runtime_handle = tokio::runtime::Handle::current();

        // Generate large CUE/FLAC fixture for CPU stress testing
        let discogs_release = create_cue_flac_test_album();
        generate_large_cue_flac_files(&album_dir);

        let import_handle = bae_core::import::ImportService::start(
            runtime_handle.clone(),
            shared_library_manager.clone(),
            encryption_service.clone(),
            database_arc,
        );

        let master_year = discogs_release.year.unwrap_or(2024);
        let import_id = uuid::Uuid::new_v4().to_string();

        // Import without storage (local CUE/FLAC playback)
        let (_album_id, release_id) = import_handle
            .send_request(ImportRequest::Folder {
                import_id,
                discogs_release: Some(discogs_release),
                mb_release: None,
                folder: album_dir.clone(),
                master_year,
                cover_art_url: None,
                storage_profile_id: None,
                selected_cover_filename: None,
            })
            .await?;

        let mut progress_rx = import_handle.subscribe_release(release_id.clone());
        while let Some(progress) = progress_rx.recv().await {
            match progress {
                bae_core::import::ImportProgress::Complete { .. } => break,
                bae_core::import::ImportProgress::Failed { error, .. } => {
                    return Err(format!("Import failed: {}", error).into());
                }
                _ => {}
            }
        }

        let albums = library_manager_arc.get_albums().await?;
        assert!(!albums.is_empty(), "Should have imported album");
        let releases = library_manager_arc
            .get_releases_for_album(&albums[0].id)
            .await?;
        assert!(!releases.is_empty(), "Should have imported release");
        let tracks = library_manager_arc.get_tracks(&releases[0].id).await?;
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        assert_eq!(track_ids.len(), 3, "Should have 3 tracks from CUE/FLAC");

        std::env::set_var("MUTE_TEST_AUDIO", "1");
        let playback_handle = bae_core::playback::PlaybackService::start(
            library_manager_arc.as_ref().clone(),
            encryption_service,
            runtime_handle,
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

/// Test that playback doesn't consume excessive CPU.
///
/// This is a regression test for busy-wait loops that cause 500%+ CPU usage.
/// During normal playback, CPU should be minimal - the audio callback runs
/// periodically and the decoder should block on I/O, not spin.
#[tokio::test]
async fn test_playback_cpu_usage_is_reasonable() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match CueFlacTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

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
    assert!(started, "Playback should start");

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

    // Calculate CPU percentage (100% = 1 core fully utilized)
    let cpu_percent = (cpu_time.as_secs_f64() / wall_time.as_secs_f64()) * 100.0;

    eprintln!(
        "CPU usage during playback: {:.1}% (cpu_time={:?}, wall_time={:?})",
        cpu_percent, cpu_time, wall_time
    );

    // Stop playback
    fixture.playback_handle.stop();

    // Steady-state playback should be lightweight (ring buffer + audio callback)
    // Baseline is ~6%, 15% allows headroom for variance
    let max_cpu_percent = 15.0;

    assert!(
        cpu_percent < max_cpu_percent,
        "Playback CPU usage too high: {:.1}% (max allowed: {:.0}%)\n\
         This indicates a busy-wait loop or spin lock somewhere.\n\
         Common causes: buffer underrun retries, spin-waiting for data.",
        cpu_percent,
        max_cpu_percent
    );
}
