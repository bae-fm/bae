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
//! The gate combines CPU-*seconds* spent per second of audio played with that
//! cost expressed as a multiple of a bare decode of the same file. Measuring
//! CPU-time (via getrusage) rather than CPU-percent over a wall-clock window
//! excludes wall time stolen by a noisy neighbour. The decode multiple
//! normalizes host speed, while the absolute factor prevents CPU-frequency
//! scaling from turning a cheap, intermittently scheduled playback workload
//! into a false regression against a full-speed decode.
//!
//! Its own test binary so the process-wide getrusage reading isn't polluted by
//! other crates' tests; `#[serial]` keeps the two cases from overlapping within
//! it, so neither needs the `--test-threads=1` flag the suite once required.

#![cfg(feature = "test-utils")]
use bae_core::db::Database;
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{ImportCommand, MetadataProvenance, MetadataSource, StorageMode};
use bae_core::library::LibraryManager;
use bae_core::playback::{PlaybackProgress, PlaybackState};
use bae_test_support as support;
use coven::StoreDir;
use serial_test::serial;
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::start_test_import;
use support::{seed_discogs_test_release, test_config, tracing_init, wait_for_import_complete};
use tempfile::TempDir;
use tokio::time::timeout;

/// Let playback reach steady state before measuring: the decoder fills the ring
/// buffer in an initial burst, which isn't representative of steady playback.
const WARMUP: Duration = Duration::from_secs(2);

/// Length of the steady-state measurement window. Playback is cheap (a fraction
/// of a percent of a core), so the CPU sample this yields is small in absolute
/// terms and a short window measures mostly jitter. Long enough that the sample
/// is dominated by real work; short enough to keep the test quick. Both tracks
/// are 30s, comfortably longer than WARMUP + WINDOW, so playback never ends
/// mid-measure.
const WINDOW: Duration = Duration::from_secs(8);

/// How much CPU the decode baseline must accumulate before it is taken as
/// settled. The baseline divides the playback figure, so noise in it lands
/// straight in the result — and a single decode of a short file is a few tens of
/// milliseconds of CPU, small enough that scheduler and timer granularity
/// dominate it (measured: a single-pass baseline swung ~20% run to run, and the
/// ratio built on it swung 60%).
///
/// Decoding is not real-time bound, so the baseline simply decodes the file again
/// until it has this much CPU to divide by. That self-calibrates: a slow machine
/// reaches the target in fewer passes, a fast one in more, and both end up with a
/// sample large enough to be stable. One second of accumulated CPU brought the
/// baseline's run-to-run spread down to ~1% (FLAC) / ~5% (MP3), which moves the
/// remaining ratio noise entirely into the numerator (the fixed real-time
/// window), where it belongs.
const BASELINE_MIN_CPU: Duration = Duration::from_millis(1000);

/// Ceiling on how much CPU steady-state playback may spend per second of audio,
/// as a multiple of what it costs *this machine* to simply decode that same
/// second of audio.
///
/// The numerator (playback) and denominator (a bare decode of the same file,
/// in-process, same build, moments earlier) usually move together with host
/// speed. CPU-frequency scaling can separate them because playback wakes
/// intermittently while the baseline decodes continuously; the absolute limit
/// below distinguishes that measurement effect from expensive playback.
///
/// What it measures is the work playback does *on top of* the decode it cannot
/// avoid: the drain, the gain, the position ticks, the ring-buffer fill. A bare
/// decode is 1x by definition, and healthy playback sits several times above that
/// — most of the ratio is that per-buffer overhead. The regressions this catches
/// are the ones that inflate that dominant term: a spin, a per-buffer allocation,
/// a resampler, a tick fan-out. A spin sends it to 20x+; a doubling of the
/// overhead term roughly doubles the ratio.
///
/// It does *not* sensitively catch a re-decode. Playback already pays one decode,
/// so decoding twice adds only one more decode's worth to a numerator that is
/// already several decodes wide — the ratio moves by about 1, inside the noise.
/// The old absolute ceiling could not catch that either; nothing measuring a
/// fraction-of-a-percent-of-a-core cost over a short window can resolve a delta
/// that small. This guards the multiple-x jump, not a few-percent drift.
///
/// Calibrated, not guessed. Measured on release: idle, CUE/FLAC 2.8–5.1x and MP3
/// 5.1–6.8x. Under CPU contention the numerator's system-time share gets noisy
/// and the readings drift up — the worst across dozens of bursty-load runs was
/// MP3 at ~10.1x, FLAC ~7.6x. That ~2x peak-to-median spread is the real floor on
/// what this can resolve, and it is why the ceiling is loose: 15.0 clears the
/// worst observed reading by ~1.5x, while still identifying the relative half of
/// a 3x-and-up explosion from a spin or an egregious per-buffer regression.
const MAX_DECODE_MULTIPLE: f64 = 15.0;

/// Absolute ceiling on steady-state playback: five percent of one core.
///
/// The relative gate can read high when a host runs the full-speed decode
/// baseline at a higher CPU frequency than the intermittently scheduled
/// real-time playback threads. That is not a playback regression while the
/// measured playback work remains below this ceiling. Conversely, a slow host
/// may cross this absolute ceiling while its decode multiple remains healthy.
/// Playback fails the gate only when it crosses both limits.
const MAX_REALTIME_FACTOR: f64 = 0.05;

fn playback_cpu_exceeds_limits(decode_multiple: f64, realtime_factor: f64) -> bool {
    decode_multiple >= MAX_DECODE_MULTIPLE && realtime_factor >= MAX_REALTIME_FACTOR
}

/// Run the `ffmpeg` command-line tool to generate a fixture, failing loudly
/// with an actionable message if it is missing.
///
/// This is the `ffmpeg` *binary* — a hard dependency of these tests, distinct
/// from the FFmpeg *libraries* bae-core links against. It must be on PATH:
/// `scripts/setup-ffmpeg.sh` fetches it into `bae-ffmpeg/dist/bin` (put that on
/// PATH), or install it system-wide (`brew install ffmpeg` on macOS, your
/// package manager on Linux).
fn run_ffmpeg(args: &[&str], fixture: &str) {
    use std::process::Command;

    let output = Command::new("ffmpeg")
        .args(args)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "could not run the `ffmpeg` command-line tool needed to generate the {fixture} \
                 fixture: {e}\nPut `ffmpeg` on PATH: run scripts/setup-ffmpeg.sh (it fetches \
                 ffmpeg into bae-ffmpeg/dist/bin — add that to PATH), or install it system-wide \
                 (`brew install ffmpeg` on macOS, your package manager on Linux)."
            )
        });

    assert!(
        output.status.success(),
        "ffmpeg failed to generate the {fixture} fixture:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
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

    let flac_path = dir.join("Test Album.flac");
    let cue_path = dir.join("Test Album.cue");

    run_ffmpeg(
        &[
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
        ],
        "CUE/FLAC",
    );

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

    for i in 1..=3 {
        let mp3_path = dir.join(format!("{:02} Track {}.mp3", i, i));

        run_ffmpeg(
            &[
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
            ],
            "MP3",
        );
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
        format: vec![],
        country: Some("Test Country".to_string()),
        label: vec!["Test Label".to_string()],
        covers: vec![],
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
                sub_tracks: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track Two".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track Three".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
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
        format: vec![],
        country: Some("Test Country".to_string()),
        label: vec!["Test Label".to_string()],
        covers: vec![],
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
                sub_tracks: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track 2".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track 3".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
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
    /// The album's source audio file, kept so the test can decode it directly and
    /// establish this machine's cost per second of audio — the baseline the
    /// playback measurement is expressed against.
    source_file: std::path::PathBuf,
    _temp_dir: TempDir,
}

impl PlaybackTestFixture {
    async fn new(
        discogs_release: DiscogsRelease,
        generate_files: impl FnOnce(&std::path::Path),
        expected_tracks: usize,
        source_file: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;

        let database = Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await?;
        let database_arc = Arc::new(database.clone());
        let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
        let config_handle = test_config(&library_dir);
        let library_manager = LibraryManager::new(
            (*database_arc).clone(),
            config_handle,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );
        let runtime_handle = tokio::runtime::Handle::current();

        generate_files(&album_dir);

        let release_id_key = seed_discogs_test_release(discogs_release);
        let import_handle =
            start_test_import(runtime_handle.clone(), library_manager.clone()).await;

        let import_id = uuid::Uuid::new_v4().to_string();

        import_handle
            .send_command(ImportCommand {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir.clone(),
                scope: bae_core::import::ReleaseFileScope::Recursive,
                selected_cover: None,
                storage_mode: StorageMode::Local,
                pin: false,
                metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                    source: MetadataSource::Discogs,
                    release_id: release_id_key,
                    partners: vec![],
                }),
                user_edit: None,
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let mut progress_rx = import_handle.subscribe_import(import_id);
        let (_release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;

        let albums = library_manager.get_albums(&[]).await?;
        assert!(!albums.is_empty(), "Should have imported album");
        let releases = library_manager
            .get_releases_for_album(&albums[0].id)
            .await?;
        assert!(!releases.is_empty(), "Should have imported release");
        let tracks = library_manager
            .get_tracks_for_release(&releases[0].id)
            .await?;
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        assert_eq!(
            track_ids.len(),
            expected_tracks,
            "Should have {} tracks",
            expected_tracks
        );

        // Real-time probe sink: drives the real decode + drain at real time and
        // discards the samples, so playback CPU is measured with no device.
        let playback_handle = library_manager.start_playback_service_with_audio_device(
            runtime_handle,
            100,
            true,
            Box::new(bae_core::playback::RealtimeProbeDevice),
        );
        let progress_rx = playback_handle.subscribe_progress();

        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids,
            source_file: album_dir.join(source_file),
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

/// This machine's cost to decode one second of this album's audio: CPU-seconds
/// spent per audio-second in a bare `decode_audio` of the source file, with no
/// playback pipeline around it.
///
/// The baseline the playback measurement is divided by. It moves with everything
/// that makes the absolute playback figure unstable across hosts — core speed,
/// memory bandwidth, the codec's own cost, the build profile — and with nothing
/// in the playback path, which is what leaves the ratio measuring only playback's
/// own overhead.
///
/// The file is read into memory before the CPU clock starts, so the I/O to fetch
/// it is not charged to the decode.
fn measure_decode_baseline_factor(fixture: &PlaybackTestFixture, label: &str) -> f64 {
    let bytes = std::fs::read(&fixture.source_file).unwrap_or_else(|e| {
        panic!(
            "{label}: read the source file to measure the decode baseline ({}): {e}",
            fixture.source_file.display()
        )
    });

    let cpu_before = get_process_cpu_time();
    let mut audio_seconds = 0.0f64;
    let mut passes = 0u32;
    let cpu_time = loop {
        let decoded = bae_core::audio_codec::decode_audio(buffer_from(&bytes), None, None)
            .unwrap_or_else(|e| panic!("{label}: decode the source file for the baseline: {e}"));
        let frames = decoded.samples.len() as f64 / decoded.channels as f64;
        audio_seconds += frames / decoded.sample_rate as f64;
        passes += 1;

        let elapsed = get_process_cpu_time().saturating_sub(cpu_before);
        if elapsed >= BASELINE_MIN_CPU {
            break elapsed;
        }
    };

    assert!(
        audio_seconds > 1.0,
        "{label}: the decode baseline needs a meaningful span of audio, got {audio_seconds:.3}s"
    );

    let factor = cpu_time.as_secs_f64() / audio_seconds;
    eprintln!(
        "{label}: decode baseline {factor:.5} CPU-s per audio-s \
         (cpu {cpu_time:?} to decode {audio_seconds:.1}s of audio over {passes} pass(es))"
    );
    factor
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

/// Measure the decode baseline, then steady-state playback, and reject a run
/// only when playback crosses both the host-relative and absolute CPU limits.
///
/// The baseline is taken first and on the same thread, so it cannot be charged
/// with any of the playback pipeline's CPU (playback has not started yet).
async fn assert_playback_efficient(fixture: &mut PlaybackTestFixture, label: &str) {
    let decode_factor = measure_decode_baseline_factor(fixture, label);
    let playback_factor = measure_playback_realtime_factor(fixture, label).await;

    let multiple = playback_factor / decode_factor;
    eprintln!(
        "{label}: playback costs {multiple:.2}x a bare decode of the same audio \
         (relative max {MAX_DECODE_MULTIPLE:.1}x; absolute max {:.1}% of one core)",
        MAX_REALTIME_FACTOR * 100.0,
    );
    assert!(
        !playback_cpu_exceeds_limits(multiple, playback_factor),
        "{label}: steady-state playback crossed both CPU limits: {multiple:.2}x the CPU a \
         bare decode of the same audio costs on this machine (max {MAX_DECODE_MULTIPLE:.1}x), \
         and {:.1}% of one core (max {:.1}%). Playback {playback_factor:.4} CPU-s per audio-s \
         against a {decode_factor:.5} decode baseline. Look for a re-decode, per-buffer \
         allocation, or resampler/tick regression in the playback path.",
        playback_factor * 100.0,
        MAX_REALTIME_FACTOR * 100.0,
    );
}

#[test]
fn playback_cpu_gate_requires_both_limits() {
    assert!(!playback_cpu_exceeds_limits(14.0, 0.02));
    assert!(!playback_cpu_exceeds_limits(16.0, 0.04));
    assert!(!playback_cpu_exceeds_limits(14.0, 0.06));
    assert!(playback_cpu_exceeds_limits(16.0, 0.06));
}

/// CUE/FLAC steady-state playback stays cheap (one big FLAC, sliced into tracks).
#[tokio::test]
#[serial]
async fn test_playback_cpu_cue_flac() {
    let mut fixture = PlaybackTestFixture::new(
        create_cue_flac_test_album(),
        generate_cue_flac_files,
        3,
        "Test Album.flac",
    )
    .await
    .expect("set up CUE/FLAC playback fixture");

    assert_playback_efficient(&mut fixture, "CUE/FLAC").await;
}

/// MP3 per-track steady-state playback stays cheap.
#[tokio::test]
#[serial]
async fn test_playback_cpu_mp3() {
    let mut fixture = PlaybackTestFixture::new(
        create_mp3_test_album(),
        generate_mp3_track_files,
        3,
        "01 Track 1.mp3",
    )
    .await
    .expect("set up MP3 playback fixture");

    assert_playback_efficient(&mut fixture, "MP3").await;
}

/// A sparse buffer pre-filled with the whole byte slice, so a decode exercises
/// the window logic without waiting on a fill.
fn buffer_from(bytes: &[u8]) -> bae_core::playback::SharedSparseBuffer {
    let buffer = bae_core::playback::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}
