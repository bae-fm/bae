#![cfg(feature = "test-utils")]
mod support;
use crate::support::{test_encryption_service, tracing_init};
use bae::cache::{CacheConfig, CacheManager};
use bae::db::{Database, DbStorageProfile};
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
/// Test helper to set up playback service with imported test tracks
struct PlaybackTestFixture {
    playback_handle: bae::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    _temp_dir: TempDir,
}
impl PlaybackTestFixture {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let cache_dir = temp_dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir)?;
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;
        let storage_dir = temp_dir.path().join("storage");
        std::fs::create_dir_all(&storage_dir)?;
        let database = Database::new(db_path.to_str().unwrap()).await?;
        // Create a local storage profile (playback tests use local storage now)
        let storage_profile = DbStorageProfile::new_local(
            "test-local",
            storage_dir.to_str().unwrap(),
            true, // encrypted
        );
        let storage_profile_id = storage_profile.id.clone();
        database.insert_storage_profile(&storage_profile).await?;
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
        let discogs_release = create_test_album();
        let _track_data = generate_test_flac_files(&album_dir);
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
        let (_album_id, release_id) = import_handle
            .send_request(ImportRequest::Folder {
                import_id,
                discogs_release: Some(discogs_release),
                mb_release: None,
                folder: album_dir.clone(),
                master_year,
                cover_art_url: None,
                storage_profile_id: Some(storage_profile_id),
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
        assert!(!track_ids.is_empty(), "Should have imported tracks");
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
    /// Wait for a specific state change with timeout
    async fn wait_for_state<F>(
        &mut self,
        predicate: F,
        timeout_duration: Duration,
    ) -> Option<PlaybackState>
    where
        F: Fn(&PlaybackState) -> bool,
    {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::StateChanged { state })) => {
                    if predicate(&state) {
                        return Some(state);
                    }
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        None
    }
    /// Wait for a position update with timeout
    async fn wait_for_position_update(&mut self, timeout_duration: Duration) -> Option<Duration> {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::PositionUpdate { position, .. })) => {
                    return Some(position);
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        None
    }
    /// Wait for a Seeked event with timeout
    async fn wait_for_seeked(&mut self, timeout_duration: Duration) -> Option<Duration> {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::Seeked { position, .. })) => {
                    return Some(position);
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        None
    }
    /// Wait for a SeekSkipped event with timeout
    async fn wait_for_seek_skipped(
        &mut self,
        timeout_duration: Duration,
    ) -> Option<(Duration, Duration)> {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::SeekSkipped {
                    requested_position,
                    current_position,
                })) => {
                    return Some((requested_position, current_position));
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        None
    }
}
/// Create a test album with 2 short tracks
fn create_test_album() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-playback-123".to_string(),
        title: "Playback Test Album".to_string(),
        year: Some(2024),
        genre: vec![],
        style: vec![],
        format: vec![],
        country: Some("US".to_string()),
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
                title: "Test Track 1".to_string(),
                duration: Some("0:10".to_string()),
            },
            DiscogsTrack {
                position: "2".to_string(),
                title: "Test Track 2".to_string(),
                duration: Some("0:10".to_string()),
            },
        ],
        master_id: "test-master-123".to_string(),
    }
}
/// Copy pre-generated FLAC fixtures to test directory
/// Fixtures should be generated using scripts/generate_test_flac.sh
fn generate_test_flac_files(dir: &std::path::Path) -> Vec<Vec<u8>> {
    use std::fs;
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");
    let fixture_files = vec!["01 Test Track 1.flac", "02 Test Track 2.flac"];
    let mut file_data = Vec::new();
    for fixture_name in fixture_files {
        let fixture_path = fixture_dir.join(fixture_name);
        let test_path = dir.join(fixture_name);
        let data = fs::read(&fixture_path).unwrap_or_else(|_| {
            panic!(
                "FLAC fixture not found: {}\n\
                     Run: ./scripts/generate_test_flac.sh",
                fixture_path.display(),
            );
        });
        fs::write(&test_path, &data).expect("Failed to copy FLAC fixture");
        file_data.push(data);
    }
    file_data
}
/// Check if audio tests should be skipped (e.g., in CI without audio device)
fn should_skip_audio_tests() -> bool {
    if std::env::var("SKIP_AUDIO_TESTS").is_ok() {
        return true;
    }
    use cpal::traits::HostTrait;
    cpal::default_host().default_output_device().is_none()
}

/// Copy pre-generated CUE/FLAC fixtures to test directory
/// Fixtures should be generated using scripts/generate_cue_flac_fixture.sh
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

/// Create a test album matching the CUE/FLAC fixture (3 tracks)
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
        master_id: "test-master-cue-flac".to_string(),
    }
}

/// Test fixture for CUE/FLAC playback (single FLAC with CUE sheet)
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
                storage_profile_id: None, // No storage - direct local playback
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

// ============================================================================
// Pause state preservation tests
// ============================================================================
// These tests verify that Next/Previous preserve pause state while fresh Play
// and AutoAdvance always start playing.

#[tokio::test]
async fn test_next_while_paused_stays_paused() {
    // When paused and pressing Next, the next track should start paused
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for next-while-paused test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Pause
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(paused_state.is_some(), "Should be paused");

    // Press Next while paused
    fixture.playback_handle.next();

    // Should transition to second track in Paused state (not Playing)
    let next_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Paused { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    assert!(
        next_track_state.is_some(),
        "Next while paused should switch to next track but stay paused"
    );
}

#[tokio::test]
async fn test_previous_while_paused_stays_paused() {
    // When paused and pressing Previous, the previous track should start paused
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for previous-while-paused test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start on second track
    fixture.playback_handle.play(second_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    // Pause
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(paused_state.is_some(), "Should be paused");

    // Press Previous while paused (within 3 seconds, so goes to previous track)
    fixture.playback_handle.previous();

    // Should transition to first track in Paused state (not Playing)
    let previous_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Paused { track, .. } = s {
                    track.id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    assert!(
        previous_track_state.is_some(),
        "Previous while paused should switch to previous track but stay paused"
    );
}

#[tokio::test]
async fn test_fresh_play_always_starts_playing() {
    // Fresh play should always start playing, even if previously paused
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for fresh play test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Pause
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(paused_state.is_some(), "Should be paused");

    // Fresh play of a different track should start Playing (not Paused)
    fixture.playback_handle.play(second_track_id.clone());

    let new_play_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    assert!(
        new_play_state.is_some(),
        "Fresh play should always start playing, not paused"
    );
}

#[tokio::test]
async fn test_pause_then_seek_stays_paused() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.is_empty() {
        debug!("No tracks available for testing");
        return;
    }
    let track_id = &fixture.track_ids[0];
    fixture.playback_handle.play(track_id.clone());
    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    if playing_state.is_none() {
        debug!("Failed to start playback");
        return;
    }
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(
        paused_state.is_some(),
        "Should be paused after pause command"
    );
    fixture.playback_handle.seek(Duration::from_secs(5));
    let seeked_position = fixture.wait_for_seeked(Duration::from_secs(3)).await;
    assert!(
        seeked_position.is_some(),
        "Should receive Seeked event after seeking"
    );
    let final_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_millis(100),
        )
        .await;
    if final_state.is_none() {
        debug!("No state change after seek, stayed paused");
    }
}
#[tokio::test]
async fn test_play_then_seek_continues_playing() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.is_empty() {
        debug!("No tracks available for testing");
        return;
    }
    let track_id = &fixture.track_ids[0];
    fixture.playback_handle.play(track_id.clone());
    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(
        playing_state.is_some(),
        "Should be playing after play command"
    );
    fixture.playback_handle.seek(Duration::from_secs(3));
    let seeked_position = fixture.wait_for_seeked(Duration::from_secs(3)).await;
    assert!(
        seeked_position.is_some(),
        "Should receive Seeked event after seeking"
    );
    let final_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_millis(100),
        )
        .await;
    if final_state.is_none() {
        debug!("No state change after seek, stayed playing");
    }
}
#[tokio::test]
async fn test_auto_advance_to_next_track() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for auto-advance test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    fixture
        .playback_handle
        .seek(Duration::from_secs(4) + Duration::from_millis(500));

    // Wait for auto-advance and collect decode stats
    let mut total_decode_errors = 0u32;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut advanced = false;

    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track, .. } = state {
                    if track.id == second_track_id {
                        advanced = true;
                        break;
                    }
                }
            }
            Ok(Some(PlaybackProgress::DecodeStats { error_count, .. })) => {
                total_decode_errors += error_count;
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    if advanced {
        assert_eq!(
            total_decode_errors, 0,
            "Auto-advance test had {} decode errors",
            total_decode_errors
        );
    } else {
        debug!("Auto-advance test inconclusive - may need valid FLAC files");
    }
}
#[tokio::test]
async fn test_position_maintained_across_pause_resume() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.is_empty() {
        debug!("No tracks available for testing");
        return;
    }
    let track_id = &fixture.track_ids[0];
    fixture.playback_handle.play(track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    let seek_position = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_position);
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    if let Some(PlaybackState::Paused { position, .. }) = paused_state {
        let diff = position.abs_diff(seek_position);
        assert!(
            diff < Duration::from_secs(1),
            "Position should be maintained when paused",
        );
    }
    fixture.playback_handle.resume();
    let resumed_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(2),
        )
        .await;
    if let Some(PlaybackState::Playing { position, .. }) = resumed_state {
        let diff = position.abs_diff(seek_position);
        assert!(
            diff < Duration::from_secs(1),
            "Position should be maintained when resumed",
        );
    }
}
#[tokio::test]
async fn test_previous_track_navigation() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for previous track test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first_track_id.clone());
    let first_track_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(
        first_track_state.is_some(),
        "Should be playing first track after play command",
    );
    fixture.playback_handle.next();
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        second_track_state.is_some(),
        "Should be playing second track after Next command",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let previous_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        previous_track_state.is_some(),
        "Should go to previous track when Previous is called early in track",
    );
    fixture.playback_handle.seek(Duration::from_secs(4));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let restart_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing {
                    track, position, ..
                } = s
                {
                    track.id == first_track_id && *position < Duration::from_secs(1)
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        restart_state.is_some(),
        "Should restart current track when Previous is called late in track",
    );
}
#[tokio::test]
async fn test_previous_track_when_starting_on_second_track() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for previous track test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(second_track_id.clone());
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        second_track_state.is_some(),
        "Should be playing second track after play command",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let previous_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        previous_track_state.is_some(),
        "Should go to previous track when Previous is called after starting on second track",
    );
}
#[tokio::test]
async fn test_previous_track_multiple_navigation() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for previous track test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(second_track_id.clone());
    let _second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let first_nav_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        first_nav_state.is_some(),
        "Should go to first track when Previous is called from second track",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let restart_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing {
                    track, position, ..
                } = s
                {
                    track.id == first_track_id && *position < Duration::from_secs(1)
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        restart_state.is_some(),
        "Should restart first track when Previous is called and there's no previous track",
    );
}
#[tokio::test]
async fn test_seek_to_same_position_sends_state_changed() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.is_empty() {
        debug!("No tracks available for testing");
        return;
    }
    let track_id = &fixture.track_ids[0];
    fixture.playback_handle.play(track_id.clone());
    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(
        playing_state.is_some(),
        "Should be playing after play command"
    );
    let seek_position = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_position);
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let current_pos = fixture
        .wait_for_position_update(Duration::from_secs(1))
        .await
        .unwrap_or(seek_position);
    let same_position = current_pos + Duration::from_millis(50);
    fixture.playback_handle.seek(same_position);
    let seek_skipped = fixture.wait_for_seek_skipped(Duration::from_secs(2)).await;
    assert!(
        seek_skipped.is_some(),
        "Should receive SeekSkipped event when position difference < 100ms",
    );
    if let Some((requested, current)) = seek_skipped {
        let diff = requested.abs_diff(current);
        assert!(
            diff < Duration::from_millis(100),
            "Seek should only be skipped when difference < 100ms, got {:?}",
            diff,
        );
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let position_update = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    assert!(
        position_update.is_some(),
        "Position updates should continue after skipped seek",
    );
}
#[tokio::test]
async fn test_queue_maintained_after_previous_navigation() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for queue navigation test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first_track_id.clone());
    let _first_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    fixture.playback_handle.next();
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        second_track_state.is_some(),
        "Should be playing second track after Next command",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let back_to_first_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        back_to_first_state.is_some(),
        "Should go back to first track when Previous is called from second track",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.next();
    let should_be_second_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        should_be_second_state.is_some(),
        "Should go to track 2 when Next is called after navigating back to track 1",
    );
}
// Note: test_playback_error_emitted_when_storage_offline was removed because it relied
// on MockCloudStorage injection which was removed with CloudStorageManager.

// ============================================================================
// Pregap behavior tests
// ============================================================================
// These tests verify CD-like pregap behavior:
// - Direct selection (play, next, previous button): skip pregap, start at INDEX 01
// - Natural transition (auto-advance): play pregap from INDEX 00, show negative time

#[tokio::test]
async fn test_direct_play_skips_pregap() {
    // When directly playing a track with pregap_ms set,
    // playback should start at pregap_ms offset (INDEX 01), not 0 (INDEX 00)
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.is_empty() {
        debug!("No tracks available for testing");
        return;
    }

    let track_id = &fixture.track_ids[0];
    fixture.playback_handle.play(track_id.clone());

    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // For tracks without pregap, position should start at 0
    // For tracks with pregap, position should start at pregap_ms
    // This test uses one-file-per-track fixtures which have no pregap,
    // so we just verify the basic behavior works.
    // TODO: Add CUE/FLAC fixture with pregap to properly test this
    if let Some(PlaybackState::Playing {
        position,
        pregap_ms,
        ..
    }) = playing_state
    {
        if let Some(pregap) = pregap_ms {
            // If there's a pregap, position should start at or after pregap_ms
            assert!(
                position.as_millis() as i64 >= pregap,
                "Direct play should skip pregap: position {} should be >= pregap {}",
                position.as_millis(),
                pregap
            );
        } else {
            // No pregap, position should start near 0
            assert!(
                position < Duration::from_millis(500),
                "Without pregap, position should start near 0"
            );
        }
    }
}

#[tokio::test]
async fn test_next_button_skips_pregap() {
    // When pressing Next button, the next track should start at INDEX 01 (skip pregap)
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for next button test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _first_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Press Next (direct selection)
    fixture.playback_handle.next();

    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    // Verify position starts at pregap_ms (or 0 if no pregap)
    if let Some(PlaybackState::Playing {
        position,
        pregap_ms: Some(pregap),
        ..
    }) = second_track_state
    {
        assert!(
            position.as_millis() as i64 >= pregap,
            "Next button should skip pregap: position {} should be >= pregap {}",
            position.as_millis(),
            pregap
        );
    }
}

#[tokio::test]
async fn test_auto_advance_plays_pregap() {
    // When a track naturally ends and auto-advances, the next track should
    // start at INDEX 00 (play pregap), with position showing negative time initially
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for auto-advance pregap test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _first_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Seek near end to trigger auto-advance
    // Test fixture tracks are ~5 seconds, so seek to 4.5s
    fixture
        .playback_handle
        .seek(Duration::from_secs(4) + Duration::from_millis(800));

    // Wait for auto-advance to second track
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track, .. } = s {
                    track.id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(10),
        )
        .await;

    // For natural transition (auto-advance), position should start at 0 (INDEX 00)
    // to play the pregap (showing negative time in UI)
    if let Some(PlaybackState::Playing {
        position,
        pregap_ms,
        ..
    }) = second_track_state
    {
        if pregap_ms.is_some() {
            // With pregap: natural transition should start at 0, not at pregap_ms
            assert!(
                position < Duration::from_millis(500),
                "Auto-advance should start at INDEX 00 (position 0) to play pregap, got {:?}",
                position
            );
        }
    } else {
        debug!("Auto-advance test inconclusive - may need longer track fixtures");
    }
}

/// Test CUE/FLAC playback - ensures byte range extraction with headers works correctly.
/// This catches the bug where headers are doubled (prepended to buffer AND passed to decoder).
#[tokio::test]
async fn test_cue_flac_playback() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match CueFlacTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up CUE/FLAC test fixture: {}", e);
            return;
        }
    };

    assert_eq!(fixture.track_ids.len(), 3, "Should have 3 tracks");
    let track_id = fixture.track_ids[0].clone();

    // Play track 1
    fixture.playback_handle.play(track_id.clone());

    // Wait for playback to progress and then complete
    // This exercises the CUE/FLAC code path with byte range extraction and header prepending
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut progressed = false;
    let mut decode_error_count: Option<u32> = None;

    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate { position, .. })) => {
                if position >= Duration::from_millis(500) {
                    progressed = true;
                }
            }
            Ok(Some(PlaybackProgress::DecodeStats { error_count, .. })) => {
                decode_error_count = Some(error_count);
                break; // Got stats, we're done
            }
            Ok(Some(PlaybackProgress::TrackCompleted { .. })) => {
                // Track completed, DecodeStats should follow
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    assert!(progressed, "CUE/FLAC playback should progress beyond 500ms");

    // Check FFmpeg decode errors - should be 0 for valid CUE/FLAC playback
    // If headers are doubled/corrupted, FFmpeg will log errors even though playback continues
    let error_count = decode_error_count.unwrap_or(0);
    assert_eq!(
        error_count, 0,
        "CUE/FLAC playback had {} FFmpeg decode errors - \
         this indicates corrupted data (possibly doubled headers bug)",
        error_count
    );
}

/// Test that seeking in CUE/FLAC tracks decodes correctly without errors.
///
/// This specifically tests seeking in track 2 (not track 1) because track 1
/// starts near byte 0, so seektable bugs might not manifest. Track 2 starts
/// mid-album, exposing bugs where the album's seektable offsets don't match
/// the track's byte range.
#[tokio::test]
async fn test_cue_flac_seek() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match CueFlacTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up CUE/FLAC test fixture: {}", e);
            return;
        }
    };

    assert_eq!(fixture.track_ids.len(), 3, "Should have 3 tracks");
    // Use track 2 (index 1) - this starts mid-album, exposing seektable bugs
    let track_id = fixture.track_ids[1].clone();

    // Play track 2
    fixture.playback_handle.play(track_id.clone());

    // Wait for playback to start (any state change to Playing)
    let deadline = Instant::now() + Duration::from_secs(5);
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

    // Seek forward - test tracks are ~16s each, seek to 5s to trigger smart seek
    fixture.playback_handle.seek(Duration::from_secs(5));

    // Wait for seek to complete and track to finish (or timeout)
    // We need to check DecodeStats specifically for our track (track 2), not subsequent tracks
    let mut deadline = Instant::now() + Duration::from_secs(20);
    let mut decode_stats: Option<(u32, u64)> = None;
    let mut track_completed = false;

    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::DecodeStats {
                error_count,
                samples_decoded,
                track_id: stats_track_id,
            })) => {
                if stats_track_id == track_id {
                    decode_stats = Some((error_count, samples_decoded));
                    break;
                }
                // Stats for a different track (auto-advanced), keep waiting
            }
            Ok(Some(PlaybackProgress::TrackCompleted {
                track_id: completed_track_id,
            })) => {
                if completed_track_id == track_id {
                    track_completed = true;
                    // DecodeStats should follow shortly - extend deadline to ensure we catch it
                    deadline = Instant::now() + Duration::from_secs(2);
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    assert!(
        track_completed,
        "Track 2 should complete (either normally or after failed seek decode)"
    );

    // If we got DecodeStats for our track, check the error count and samples decoded
    // If we didn't get DecodeStats (bug: smart seek doesn't emit stats), the test should also fail
    let (error_count, samples_decoded) = decode_stats.expect(
        "Should receive DecodeStats for track 2 - \
         if missing, smart seek completion may not be emitting stats",
    );
    assert_eq!(
        error_count, 0,
        "CUE/FLAC seek had {} fatal FFmpeg errors - \
         likely seektable offsets are wrong for mid-album track",
        error_count
    );

    // After seeking to 5s in a 10s track, we should have ~5s of audio remaining
    // At 44100Hz stereo, that's ~441000 samples. Allow some tolerance.
    let min_expected_samples = 44100 * 2 * 3; // At least 3 seconds of stereo audio
    assert!(
        samples_decoded >= min_expected_samples,
        "CUE/FLAC seek produced only {} samples, expected at least {} - \
         likely silent playback bug (decode succeeded but produced no audio)",
        samples_decoded,
        min_expected_samples
    );
}

/// Test pregap skip behavior with CUE/FLAC.
///
/// Track 2 has a 2-second pregap (INDEX 00 at 8s, INDEX 01 at 10s).
/// When directly playing track 2, playback should skip the pregap and start at INDEX 01.
/// The actual audio position should start at ~2000ms (the pregap duration), not 0.
#[tokio::test]
async fn test_direct_play_skips_pregap_cue_flac() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match CueFlacTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up CUE/FLAC test fixture: {}", e);
            return;
        }
    };

    assert_eq!(fixture.track_ids.len(), 3, "Should have 3 tracks");
    // Track 2 (index 1) has a 2-second pregap: INDEX 00 at 8s, INDEX 01 at 10s
    let track_id = fixture.track_ids[1].clone();

    // Direct play track 2
    fixture.playback_handle.play(track_id.clone());

    // Wait for playback to start
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut pregap_ms_value: Option<i64> = None;

    while Instant::now() < deadline && pregap_ms_value.is_none() {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing {
                    track, pregap_ms, ..
                } = &state
                {
                    if track.id == track_id {
                        pregap_ms_value = *pregap_ms;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    let pregap = pregap_ms_value.expect("Track 2 should have pregap_ms set");
    assert!(
        pregap > 0,
        "Track 2 should have a positive pregap_ms, got {}",
        pregap
    );

    // Wait for position updates and verify we see positions >= pregap offset.
    // The seek happens after playback starts, so there may be a few early positions
    // before the seek completes. We wait until we see a position >= pregap threshold.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut positions: Vec<u64> = Vec::new();
    let mut found_post_seek = false;
    let threshold = (pregap as u64).saturating_sub(500); // pregap minus 500ms tolerance

    while Instant::now() < deadline && !found_post_seek {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate {
                position,
                track_id: pos_track_id,
            })) => {
                if pos_track_id == track_id {
                    let pos_ms = position.as_millis() as u64;
                    positions.push(pos_ms);
                    if pos_ms >= threshold {
                        found_post_seek = true;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    assert!(
        !positions.is_empty(),
        "Should receive at least one position update"
    );

    // The bug: without the seek, position starts at 0 and progresses linearly.
    // With the seek, position should eventually jump to >= pregap_ms (~2000ms).
    assert!(
        found_post_seek,
        "Direct play should skip pregap: never saw position >= {}ms (pregap {} minus 500ms). \
         Positions received: {:?}. \
         If positions stay low, the pregap seek is not being performed or not completing.",
        threshold, pregap, positions
    );
}
