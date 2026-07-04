//! Steady-state playback CPU regression test.
//!
//! Guards against a change in the playback core that turns the few-percent CPU
//! steady-state playback normally uses into tens of percent of a core — a
//! re-decode, a per-buffer allocation, a resampler or tick-fan-out regression,
//! a spin. Not aimed at catching a runaway busy-loop (that shows up as 100%+
//! and any threshold catches it); aimed at the subtler multiple-x jump.
//!
//! It drives the real pipeline through `RealtimeProbeOutput`, which pulls and
//! discards samples at real time (the same per-buffer work the cpal sink does:
//! pull, gain, ~20 Hz position ticks, decoder fill/park) without an audio
//! device. Real-time pacing matters: a full-speed decode is an order of
//! magnitude cheaper and misses where the cost actually is.
//!
//! The metric is the realtime factor — CPU-*seconds* spent per second of audio
//! played, i.e. the fraction of one core. Measuring CPU-time (via getrusage),
//! not CPU-percent over a wall-clock window, is what keeps it stable on shared
//! CI runners: a noisy neighbour steals wall-clock but not the work done.
//!
//! Its own test binary so the process-wide getrusage reading isn't polluted by
//! other crates' tests; `#[serial]` keeps the two cases from overlapping within
//! it, so neither needs the `--test-threads=1` flag the suite once required.

#![cfg(feature = "test-utils")]
mod support;
use crate::support::{
    seed_discogs_test_release, test_config_and_keys, tracing_init, wait_for_import_complete,
};
use bae_core::db::Database;
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{IdentityChoice, ImportCommand, MetadataRef, MetadataSource, StorageMode};
use bae_core::library::LibraryManager;
use bae_core::playback::{PlaybackProgress, PlaybackState};
use coven::LibraryDir;
use serial_test::serial;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;

/// Let playback reach steady state before measuring: the decoder fills the ring
/// buffer in an initial burst, which isn't representative of steady playback.
const WARMUP: Duration = Duration::from_secs(2);

/// Length of the steady-state measurement window. Long enough to average out
/// per-tick jitter; short enough to keep the test quick. Both tracks are 30s,
/// comfortably longer than WARMUP + WINDOW, so playback never ends mid-measure.
const WINDOW: Duration = Duration::from_secs(4);

/// Ceiling on the realtime factor — CPU-seconds spent per second of audio
/// played, i.e. the fraction of one core steady-state playback uses.
///
/// Healthy FLAC/MP3 playback is a few percent of a core. The regression this
/// guards against is a multiple-x jump (a re-decode, a per-buffer allocation, a
/// resampler or tick regression) into tens of percent. The ceiling sits well
/// above healthy-plus-runner-noise and well below that regression band, so it
/// never flakes and still catches the jump.
///
/// The absolute factor is hardware-dependent (a slower runner spends more CPU
/// per audio-second); the regression *ratio* is not. Calibrated for the runner
/// this test lands on — recalibrate if that runner changes.
const MAX_REALTIME_FACTOR: f64 = 0.20;

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

/// Generate a CUE/FLAC fixture: one 90s 96kHz/24-bit stereo FLAC split into
/// three 30s tracks by the CUE sheet — a high-resolution vinyl-rip format.
/// Validated against a real 96kHz/24-bit FLAC: brown noise at this rate measures
/// within ~20% of real music. At this rate the content-independent per-buffer
/// work (drain, gain, ticks) dominates the cheap decode, so the synthetic
/// fixture tracks real playback closely. Brown noise so the decoder does real
/// work every frame (silence decodes trivially).
fn generate_cue_flac_files(dir: &std::path::Path) {
    use std::fs;
    use std::process::Command;

    let flac_path = dir.join("Test Album.flac");
    let cue_path = dir.join("Test Album.cue");

    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=d=90:c=brown:r=96000", // 90s (3x30s) brown noise at 96kHz
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

    // Three 30s tracks; track one is INDEX 00:00:00..00:30:00.
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
    INDEX 01 00:30:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    PERFORMER "Test Artist"
    INDEX 01 01:00:00
"#;
    fs::write(&cue_path, cue_content).expect("Failed to write CUE file");
}

/// Generate per-track MP3 files: three 30s 44.1kHz stereo tracks at 320kbps CBR.
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
                "anoisesrc=d=30:c=brown:r=44100", // 30s brown noise at 44.1kHz
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
    }

    // Minimal log file so the folder scanner doesn't complain.
    fs::write(dir.join("rip.log"), "").unwrap();
}

/// Metadata for the CUE/FLAC album (three 30s tracks).
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
        extraartists: Some(vec![]),
        tracklist: vec![
            DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "Track One".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track Two".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track Three".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
        ],
        master_id: Some("test-master".to_string()),
    }
}

/// Metadata for the MP3 per-track album (three 30s tracks).
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
        extraartists: Some(vec![]),
        tracklist: vec![
            DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "Track 1".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track 2".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track 3".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
        ],
        master_id: Some("test-master-mp3".to_string()),
    }
}

/// Imports an album and starts playback through the real-time probe sink (no
/// audio device). Works with any format.
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
            std::sync::Arc::new(coven::SystemClock),
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
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            tokio::runtime::Handle::current(),
        );
        let runtime_handle = tokio::runtime::Handle::current();

        generate_files(&album_dir);

        let release_id_key = seed_discogs_test_release(discogs_release);
        let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());

        let import_id = uuid::Uuid::new_v4().to_string();

        import_handle
            .send_command(ImportCommand {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir.clone(),
                selected_cover: None,
                storage_mode: StorageMode::Local,
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

        // Real-time probe sink: drives the real decode + drain at real time and
        // discards the samples, so playback CPU is measured with no device.
        let playback_handle = bae_core::playback::PlaybackService::start_with_output(
            library_manager.clone(),
            runtime_handle,
            100,
            Box::new(bae_core::playback::RealtimeProbeOutput::new()),
        );
        let progress_rx = playback_handle.subscribe_progress();

        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids,
            _temp_dir: temp_dir,
        })
    }
}

/// Total CPU time consumed by this process (user + system), via getrusage.
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

/// Wait (up to `within`) for playback to report it is playing.
async fn await_playing(fixture: &mut PlaybackTestFixture, within: Duration, label: &str) {
    let deadline = Instant::now() + within;
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            panic!("{label}: playback did not start within {within:?}");
        };
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged {
                state: PlaybackState::Playing { .. },
            })) => return,
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => panic!("{label}: playback did not start within {within:?}"),
        }
    }
}

/// Play track one and return the steady-state realtime factor: CPU-seconds
/// spent per second of audio played. Plays in real time, so the wall-clock
/// measurement window equals the audio played.
async fn measure_playback_realtime_factor(fixture: &mut PlaybackTestFixture, label: &str) -> f64 {
    let track_id = fixture.track_ids[0].clone();
    fixture.playback_handle.play(track_id);

    await_playing(fixture, Duration::from_secs(5), label).await;

    // Skip the initial ring-buffer fill burst, then measure steady state.
    tokio::time::sleep(WARMUP).await;
    let cpu_before = get_process_cpu_time();
    let wall_start = Instant::now();
    tokio::time::sleep(WINDOW).await;
    let cpu_time = get_process_cpu_time().saturating_sub(cpu_before);
    let wall_time = wall_start.elapsed();

    fixture.playback_handle.stop();

    // Liveness: the window must have contained real playback, not a silently
    // stopped or errored stream. Drained after the measurement so it adds no
    // CPU to it.
    let (mut saw_position, mut error) = (false, None);
    while let Ok(progress) = fixture.progress_rx.try_recv() {
        match progress {
            PlaybackProgress::PositionUpdate { .. } => saw_position = true,
            PlaybackProgress::PlaybackError { reason } => error = Some(reason),
            _ => {}
        }
    }
    assert!(error.is_none(), "{label}: playback errored: {error:?}");
    assert!(
        saw_position,
        "{label}: no position updates during the window — playback was not running"
    );

    // Real-time playback: the audio played equals the wall-clock window.
    let factor = cpu_time.as_secs_f64() / wall_time.as_secs_f64();
    eprintln!(
        "{label}: realtime factor {factor:.4} ({:.1}% of one core) \
         (cpu {cpu_time:?} over {wall_time:?} of playback)",
        factor * 100.0
    );
    factor
}

fn assert_playback_efficient(factor: f64, label: &str) {
    assert!(
        factor < MAX_REALTIME_FACTOR,
        "{label}: steady-state playback is too expensive: realtime factor {factor:.4} \
         (max {MAX_REALTIME_FACTOR:.2}) — playback uses {:.0}% of one core. Look for a \
         re-decode, per-buffer allocation, or resampler/tick regression in the playback path.",
        factor * 100.0
    );
}

/// CUE/FLAC steady-state playback stays cheap (one big FLAC, sliced into tracks).
#[tokio::test]
#[serial]
async fn test_playback_cpu_cue_flac() {
    let mut fixture =
        PlaybackTestFixture::new(create_cue_flac_test_album(), generate_cue_flac_files, 3)
            .await
            .expect("set up CUE/FLAC playback fixture");

    let factor = measure_playback_realtime_factor(&mut fixture, "CUE/FLAC").await;
    assert_playback_efficient(factor, "CUE/FLAC");
}

/// MP3 per-track steady-state playback stays cheap.
#[tokio::test]
#[serial]
async fn test_playback_cpu_mp3() {
    let mut fixture =
        PlaybackTestFixture::new(create_mp3_test_album(), generate_mp3_track_files, 3)
            .await
            .expect("set up MP3 playback fixture");

    let factor = measure_playback_realtime_factor(&mut fixture, "MP3").await;
    assert_playback_efficient(factor, "MP3");
}
