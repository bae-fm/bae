//! CPU usage tests for playback.
//!
//! These tests run as a separate binary to get accurate process-wide CPU measurements.

#![cfg(feature = "test-utils")]
mod support;
use crate::support::{test_encryption_service, tracing_init};
use bae::cache::{CacheConfig, CacheManager};
use bae::db::Database;
use bae::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae::encryption::EncryptionService;
use bae::import::ImportRequest;
use bae::library::{LibraryManager, SharedLibraryManager};
use bae::playback::{PlaybackProgress, PlaybackState};
use bae::torrent::LazyTorrentManager;
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

/// Copy pre-generated CUE/FLAC fixtures to test directory
fn generate_cue_flac_files(dir: &std::path::Path) {
    use std::fs;
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_flac");

    // Copy FLAC file
    let flac_src = fixture_dir.join("Test Album.flac");
    let flac_dst = dir.join("Test Album.flac");
    let flac_data = fs::read(&flac_src).unwrap_or_else(|_| {
        panic!(
            "CUE/FLAC fixture not found: {}\n\
             Run: ./scripts/generate_cue_flac_fixture.sh",
            flac_src.display(),
        );
    });
    fs::write(&flac_dst, &flac_data).expect("Failed to copy FLAC fixture");

    // Copy CUE file
    let cue_src = fixture_dir.join("Test Album.cue");
    let cue_dst = dir.join("Test Album.cue");
    let cue_data = fs::read(&cue_src).unwrap_or_else(|_| {
        panic!(
            "CUE fixture not found: {}\n\
             Run: ./scripts/generate_cue_flac_fixture.sh",
            cue_src.display(),
        );
    });
    fs::write(&cue_dst, &cue_data).expect("Failed to copy CUE fixture");
}

/// Create test album metadata for CUE/FLAC
fn create_cue_flac_test_album() -> DiscogsRelease {
    DiscogsRelease {
        id: "cue-flac-test-release".to_string(),
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
                title: "Track One (Silence)".to_string(),
                duration: Some("0:10".to_string()),
            },
            DiscogsTrack {
                position: "2".to_string(),
                title: "Track Two (White Noise)".to_string(),
                duration: Some("0:10".to_string()),
            },
            DiscogsTrack {
                position: "3".to_string(),
                title: "Track Three (Brown Noise)".to_string(),
                duration: Some("0:10".to_string()),
            },
        ],
        master_id: "test-master".to_string(),
    }
}

/// Test fixture for CUE/FLAC playback
struct CueFlacTestFixture {
    playback_handle: bae::playback::PlaybackHandle,
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
        let encryption_service = EncryptionService::new_with_key(&[0u8; 32]);
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

        // Use CUE/FLAC fixtures
        let discogs_release = create_cue_flac_test_album();
        generate_cue_flac_files(&album_dir);

        let torrent_manager = LazyTorrentManager::new_noop(runtime_handle.clone());
        let import_handle = bae::import::ImportService::start(
            runtime_handle.clone(),
            shared_library_manager.clone(),
            encryption_service.clone(),
            torrent_manager,
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
                bae::import::ImportProgress::Complete { .. } => break,
                bae::import::ImportProgress::Failed { error, .. } => {
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
        let playback_handle = bae::playback::PlaybackService::start(
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

    // Seek forward to trigger seek code path (this is where high CPU was observed)
    fixture.playback_handle.seek(Duration::from_secs(3));

    // Wait a moment for seek to complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Measure CPU during steady-state playback (after startup/seek are done)
    let measure_start = Instant::now();
    let initial_cpu = get_process_cpu_time();
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
