#![cfg(feature = "test-utils")]
//! Integration tests for ALAC and AAC `.m4a` support.
//!
//! Exercises the probe -> scanner -> import -> playback pipeline on real
//! ALAC/AAC bytes (generated once with Homebrew ffmpeg, checked into
//! `test-fixtures/alac/`). Nothing is mocked: every assertion runs through
//! the same code paths production uses.
use bae_test_support as support;
use support::start_test_import;

use bae_core::audio_codec::{decode_audio, probe_audio_from_path};
use bae_core::cue_flac::parse_cue_sheet;
use bae_core::db::Database;
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::discid::compute_discid_from_categorized;
use bae_core::import::folder_scanner::{
    collect_release_candidate_files_with_scope, scan_for_candidates_with_callback, ScanItem,
    StoredCandidateEdits,
};
use bae_core::import::{ImportCommand, MetadataSeed, MetadataSource, StorageMode};
use bae_core::library::LibraryManager;
use bae_core::util::content_type::ContentType;
use coven::StoreDir;
use std::path::{Path, PathBuf};
use support::{seed_discogs_test_release, test_config, tracing_init, wait_for_import_complete};
use tempfile::TempDir;
use tracing::info;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-fixtures")
        .join("alac")
}

fn make_discogs_release(id: &str, title: &str, tracks: &[&str]) -> DiscogsRelease {
    DiscogsRelease {
        id: id.to_string(),
        title: title.to_string(),
        year: Some(2024),
        format: vec![],
        country: None,
        label: vec![],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            id: "discogs-artist-1".to_string(),
            name: "Artist Name".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: tracks
            .iter()
            .enumerate()
            .map(|(i, title)| DiscogsTrack {
                type_: "track".to_string(),
                position: format!("{}", i + 1),
                title: (*title).to_string(),
                duration: Some("0:02".to_string()),
                artists: vec![],
                extraartists: None,
            })
            .collect(),
        master_id: None,
    }
}

async fn import_single_m4a_fixture(
    fixture_name: &str,
    temp_root: &Path,
) -> (LibraryManager, String) {
    let album_dir = temp_root.join("album");
    let db_dir = temp_root.join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");

    let fixture_path = fixture_dir().join(fixture_name);
    let dest = album_dir.join(fixture_name);
    std::fs::copy(&fixture_path, &dest).expect("copy fixture");

    let db_file = db_dir.join("test.db");
    let database = Database::new_test(
        db_file.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .expect("database");
    let library_dir = StoreDir::new(db_dir.clone());
    let config_handle = test_config(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );

    let discogs_release = make_discogs_release("test-m4a", "Album Title", &["Track One"]);
    let release_id_key = seed_discogs_test_release(discogs_release);

    let import_handle =
        start_test_import(tokio::runtime::Handle::current(), library_manager.clone()).await;
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_seed: MetadataSeed::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
            },
            user_edit: None,
        })
        .await
        .expect("send command");

    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _) = wait_for_import_complete(&mut progress_rx).await;
    (library_manager, release_id)
}

// ─────────────────────────────── Probe tests ────────────────────────────────

/// Probe on an ALAC `.m4a` fixture must return `ContentType::Alac` with the
/// real sample rate / channel count / bit depth.
///
/// The `bits_per_sample == Some(16)` assertion exercises the ALAC-0 deep
/// probe: if FFmpeg's codecpar reports 0 (as it does for some ALAC streams
/// before the decoder has parsed the magic cookie), the fallback must decode
/// a frame to recover the real 16. If this lands as `None` or `Some(0)`, the
/// fallback is broken.
#[test]
fn probe_alac_returns_content_type_alac() {
    let path = fixture_dir().join("silence-alac.m4a");
    let probe = probe_audio_from_path(path.to_str().unwrap()).expect("probe ALAC fixture");

    assert_eq!(probe.content_type, ContentType::Alac);
    assert_eq!(probe.sample_rate, 44100);
    assert_eq!(probe.channels, 2);
    assert_eq!(
        probe.bits_per_sample,
        Some(16),
        "ALAC bit depth must be 16. If this is None/Some(0), the \
         ALAC-0 deep probe fallback is broken."
    );
}

/// Probe on an AAC `.m4a` fixture must return `ContentType::Aac`. AAC is a
/// perceptual codec and does not carry a meaningful `bits_per_raw_sample`, so
/// we do not assert on bit depth here (the codec round-trips through f32
/// output regardless of source bit depth).
#[test]
fn probe_aac_returns_content_type_aac() {
    let path = fixture_dir().join("silence-aac.m4a");
    let probe = probe_audio_from_path(path.to_str().unwrap()).expect("probe AAC fixture");

    assert_eq!(probe.content_type, ContentType::Aac);
    assert_eq!(probe.sample_rate, 44100);
    assert_eq!(probe.channels, 2);
}

// ─────────────────────────────── Import tests ───────────────────────────────

/// Importing a standalone ALAC `.m4a` must land with `ContentType::Alac` in
/// the DB and resolve to one full-file playback segment. Per-track M4A files
/// don't have CUE boundaries, so the whole file is one track.
#[tokio::test]
async fn import_standalone_alac_m4a() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let (library_manager, release_id) =
        import_single_m4a_fixture("silence-alac.m4a", temp_root.path()).await;

    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 1, "standalone .m4a produces one track");

    let track = &tracks[0];
    let af = library_manager
        .get_audio_format_by_track_id(&track.id)
        .await
        .expect("get audio format")
        .expect("track has audio format");

    assert_eq!(af.content_type, ContentType::Alac);
    assert_eq!(af.sample_rate, 44100);
    assert_eq!(af.channels, 2);
    assert_eq!(af.bits_per_sample, Some(16));

    let resolved = library_manager
        .resolve_track_audio(&track.id)
        .await
        .expect("resolve track audio");
    assert_eq!(resolved.segments.len(), 1);
    let segment = &resolved.segments[0];

    assert_eq!(
        segment.span.start_sample, 0,
        "standalone track starts at sample 0"
    );
    assert_eq!(
        segment.span.end_sample, None,
        "standalone track runs to EOF (no end sample)"
    );
}

/// Importing a standalone AAC `.m4a` must land with `ContentType::Aac` and
/// the same full-file segment shape as ALAC.
#[tokio::test]
async fn import_standalone_aac_m4a() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let (library_manager, release_id) =
        import_single_m4a_fixture("silence-aac.m4a", temp_root.path()).await;

    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 1);

    let track = &tracks[0];
    let af = library_manager
        .get_audio_format_by_track_id(&track.id)
        .await
        .expect("get audio format")
        .expect("track has audio format");

    assert_eq!(af.content_type, ContentType::Aac);
    assert_eq!(af.sample_rate, 44100);
    assert_eq!(af.channels, 2);

    let resolved = library_manager
        .resolve_track_audio(&track.id)
        .await
        .expect("resolve track audio");

    assert_eq!(resolved.segments.len(), 1);
    assert_eq!(resolved.segments[0].span.start_sample, 0);
    assert_eq!(resolved.segments[0].span.end_sample, None);
}

// ─────────────────────────────── Scanner test ───────────────────────────────

/// The folder scanner must recognize a CUE + ALAC `.m4a` pair, produce one
/// release with three tracks, and tag it with the `"CUE+ALAC"` format label
/// derived from `ContentType::Alac.display_name()`.
#[test]
fn scanner_recognizes_cue_alac_pair() {
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    std::fs::create_dir_all(&album_dir).expect("album dir");

    let fix = fixture_dir();
    std::fs::copy(fix.join("cue-alac.m4a"), album_dir.join("cue-alac.m4a")).expect("copy m4a");
    std::fs::copy(fix.join("cue-alac.cue"), album_dir.join("cue-alac.cue")).expect("copy cue");

    let mut candidates = Vec::new();
    scan_for_candidates_with_callback(album_dir.clone(), &StoredCandidateEdits::none(), |item| {
        match item {
            ScanItem::Valid(c) => candidates.push(c),
            ScanItem::Invalid(c) => {
                panic!(
                    "ALAC fixture must scan as a valid release, got invalid: {}",
                    c.reason
                )
            }
            ScanItem::Discovered(_) | ScanItem::Decided { .. } => {}
        }
    })
    .expect("scan folder");

    assert_eq!(candidates.len(), 1, "one release in the folder");
    let candidate = &candidates[0];

    let bound = candidate.files.bound_sheets();
    assert_eq!(bound.len(), 1, "the sheet binds to the .m4a it names");
    assert_eq!(
        bound[0].sheet.tracks.len(),
        3,
        "three tracks in the CUE sheet"
    );
    assert_eq!(candidate.files.format_label, "CUE+ALAC");
}

// ─────────────────────────────── Import CUE+ALAC ────────────────────────────

/// End-to-end CUE+ALAC import: three tracks, each with one playback segment
/// carrying the CUE-derived sample boundaries (3s = 132300 samples at 44100 Hz).
/// The last track has no `end_sample` because it runs to EOF.
#[tokio::test]
async fn import_cue_alac_pair() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");

    let fix = fixture_dir();
    std::fs::copy(fix.join("cue-alac.m4a"), album_dir.join("cue-alac.m4a")).expect("copy m4a");
    std::fs::copy(fix.join("cue-alac.cue"), album_dir.join("cue-alac.cue")).expect("copy cue");

    let db_file = db_dir.join("test.db");
    let database = Database::new_test(
        db_file.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .expect("database");
    let library_dir = StoreDir::new(db_dir.clone());
    let config_handle = test_config(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );

    let discogs_release = make_discogs_release(
        "test-cue-alac",
        "Album Title",
        &["Track One", "Track Two", "Track Three"],
    );
    let release_id_key = seed_discogs_test_release(discogs_release);

    let import_handle =
        start_test_import(tokio::runtime::Handle::current(), library_manager.clone()).await;
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_seed: MetadataSeed::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
            },
            user_edit: None,
        })
        .await
        .expect("send command");
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _) = wait_for_import_complete(&mut progress_rx).await;

    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3);

    // CUE INDEX 01 times: 00:00:00, 00:03:00, 00:06:00.
    // One CUE frame = 1/75s, so boundaries in samples at 44100 Hz are:
    //   track 1: 0
    //   track 2: 3s * 44100 = 132300
    //   track 3: 6s * 44100 = 264600
    const START_SAMPLES: [u64; 3] = [0, 132_300, 264_600];
    const END_SAMPLES: [Option<u64>; 3] = [Some(132_300), Some(264_600), None];

    for (i, track) in tracks.iter().enumerate() {
        let af = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio format")
            .unwrap_or_else(|| panic!("track {} has no audio format", i + 1));

        assert_eq!(
            af.content_type,
            ContentType::Alac,
            "track {} content type",
            i + 1
        );

        let resolved = library_manager
            .resolve_track_audio(&track.id)
            .await
            .expect("resolve track audio");
        assert_eq!(resolved.segments.len(), 1);
        let segment = &resolved.segments[0];

        assert_eq!(
            segment.span.start_sample,
            START_SAMPLES[i],
            "track {} start_sample",
            i + 1
        );
        assert_eq!(
            segment.span.end_sample,
            END_SAMPLES[i],
            "track {} end_sample",
            i + 1
        );
    }
}

// ─────────────────────────────── Disc ID test ───────────────────────────────

/// The MusicBrainz disc ID for a CUE+ALAC pair is deterministic for a fixed
/// set of track offsets and lead-out. This test pins the computed value — if
/// the math or the underlying `discid` crate changes, we want to know.
///
/// Expected value was captured from the first successful run of this test on
/// the shared fixture (`cue-alac.cue` with tracks at 0s/3s/6s, 9s total
/// duration reported by FFmpeg).
#[test]
fn cue_alac_disc_id_is_stable() {
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    std::fs::create_dir_all(&album_dir).expect("album dir");

    let fix = fixture_dir();
    std::fs::copy(fix.join("cue-alac.m4a"), album_dir.join("cue-alac.m4a")).expect("copy m4a");
    std::fs::copy(fix.join("cue-alac.cue"), album_dir.join("cue-alac.cue")).expect("copy cue");

    let categorized = collect_release_candidate_files_with_scope(
        &album_dir,
        bae_core::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan album dir");
    let track_count = categorized.track_count();
    let disc_id = compute_discid_from_categorized(&categorized);

    assert_eq!(track_count, 3, "three tracks in the CUE sheet");
    let computed = disc_id.expect("CUE+ALAC pair must produce a disc ID");
    assert_eq!(
        computed.disc_id.len(),
        28,
        "MusicBrainz disc IDs are 28-char base64 strings"
    );
    assert_eq!(
        computed.disc_id, EXPECTED_CUE_ALAC_DISC_ID,
        "disc ID drifted — CUE parsing, probe duration, or disc ID math changed",
    );
    assert!(
        computed.source_file.ends_with(".cue"),
        "the sheet it was carved from rides with it, got {:?}",
        computed.source_file
    );
}

/// Pinned disc ID for the `cue-alac.cue` + `cue-alac.m4a` fixture pair.
/// Captured from the first successful run of `cue_alac_disc_id_is_stable`.
/// Three tracks at 0s/3s/6s with a 9s total duration yield this disc ID
/// deterministically; a drift means something upstream of the `discid` crate
/// changed.
const EXPECTED_CUE_ALAC_DISC_ID: &str = "RGsIqQtOzlx_LdxgsE0wIvFi8h4-";

// Cue sheet sanity: parse our fixture and make sure it has the shape we
// expect. Keeps the disc ID test honest — if the CUE parser silently
// misreads the fixture, both tests would drift together without this.
#[test]
fn cue_alac_cue_sheet_has_three_tracks() {
    let sheet = parse_cue_sheet(&fixture_dir().join("cue-alac.cue")).expect("parse cue");
    assert_eq!(sheet.tracks.len(), 3);
    // INDEX 01 is in CUE frames (1/75s). The three tracks start at 0, 3s, 6s.
    assert_eq!(sheet.tracks[0].start_cue_frames, 0);
    assert_eq!(sheet.tracks[1].start_cue_frames, 3 * 75);
    assert_eq!(sheet.tracks[2].start_cue_frames, 6 * 75);
}

// ─────────────────────────────── Decode smoke ───────────────────────────────

/// Decoding the 2-second ALAC silence fixture through the production decoder
/// must produce the expected sample count (2s * 44100 Hz * 2 ch = 176400
/// samples) and match the decoder's output format. The source ALAC bit depth is
/// asserted by the probe/import tests above; `decode_audio` emits 32-bit PCM
/// after sample conversion. Silence means every sample must be 0, so we assert
/// that too, which is a useful sanity check that we haven't accidentally
/// decoded noise.
#[test]
fn decode_alac_fixture_smoke() {
    let bytes = std::fs::read(fixture_dir().join("silence-alac.m4a")).expect("read alac");
    let decoded = decode_audio(buffer_from(&bytes), None, None).expect("decode alac");

    assert_eq!(decoded.sample_rate, 44100);
    assert_eq!(decoded.channels, 2);

    let expected_samples = 44_100usize * 2 * 2;
    assert_eq!(
        decoded.samples.len(),
        expected_samples,
        "2s stereo @ 44100 Hz should decode to {expected_samples} interleaved samples",
    );

    // Silence: every sample must be exactly 0.
    let non_zero = decoded.samples.iter().filter(|&&s| s != 0).count();
    assert_eq!(
        non_zero, 0,
        "silence fixture must decode to all-zero samples; found {non_zero} non-zero"
    );

    info!(
        "Decoded {} samples from silence-alac.m4a ({} Hz, {} ch)",
        decoded.samples.len(),
        decoded.sample_rate,
        decoded.channels,
    );
}

/// A sparse buffer pre-filled with the whole byte slice, so a decode exercises
/// the window logic without waiting on a fill.
fn buffer_from(bytes: &[u8]) -> bae_core::playback::SharedSparseBuffer {
    let buffer = bae_core::playback::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}
