#![cfg(feature = "test-utils")]
//! Integration tests for the import service.
//!
//! Tests the real ImportService through its public handle API:
//! scan → import. All through real infrastructure.

mod support;

use bae_core::db::{Database, LibraryImageType};
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{
    CoverSelection, IdentityChoice, ImportCommand, ImportService, MetadataRef, MetadataSource,
    PressingEdit, ReleaseUserEdit, ScanEvent, StorageMode, TrackUserEdit,
};
use bae_core::library::LibraryManager;
use bae_core::library_dir::LibraryDir;
use bae_core::musicbrainz::{
    ExternalUrls, MbArtistCredit, MbArtistRef, MbMedium, MbRecording, MbReleaseGroupRef,
    MbReleaseResponse, MbTrack,
};
use serial_test::serial;
use std::fs;
use std::path::Path;
use support::seed_discogs_test_release;
use tempfile::TempDir;

// ── Helpers ─────────────────────────────────────────────────────────────────

struct ImportFixture {
    db: Database,
    library_dir: LibraryDir,
    handle: bae_core::import::ImportServiceHandle,
    _temp: TempDir,
}

impl ImportFixture {
    async fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let db_dir = temp.path().join("db");
        fs::create_dir_all(&db_dir).unwrap();

        let db = Database::new_test(
            db_dir.join("test.db").to_str().unwrap(),
            std::sync::Arc::new(bae_core::clock::SystemClock),
        )
        .await
        .unwrap();
        let library_dir = LibraryDir::new(db_dir.clone());
        let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
        let library_manager = LibraryManager::new(
            db.clone(),
            library_dir.clone(),
            config_handle,
            key_service,
            std::sync::Arc::new(bae_core::clock::SystemClock),
            std::sync::Arc::new(bae_core::id_provider::UuidProvider),
            tokio::runtime::Handle::current(),
            None,
        );

        let handle = ImportService::start(
            tokio::runtime::Handle::current(),
            library_manager,
            bae_core::import::cover_art::CoverArtArchiveClient::new(),
        );

        Self {
            db,
            library_dir,
            handle,
            _temp: temp,
        }
    }

    fn temp_path(&self) -> &Path {
        self._temp.path()
    }
}

fn generate_album_files(dir: &Path, filenames: &[&str]) {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/flac/01 Test Track 1.flac");
    let flac = fs::read(&fixture).expect("FLAC fixture missing");
    for name in filenames {
        fs::write(dir.join(name), &flac).unwrap();
    }
}

/// Per-track tag values for `generate_tagged_album_files` — covers the
/// fields the file-tag projection reads at import time. `track_artist`
/// empty rolls up to the album artist in the editor's convention.
struct TaggedTrack<'a> {
    filename: &'a str,
    title: &'a str,
    track_number: u32,
}

/// Drop tagged FLAC files into `dir`, sharing the same album-level
/// metadata across every file. The FLAC fixture itself carries no
/// tags; lofty rewrites a Vorbis-comment block on top.
fn generate_tagged_album_files(
    dir: &Path,
    album_title: &str,
    album_artist: &str,
    year: Option<u16>,
    tracks: &[TaggedTrack<'_>],
) {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::items::Timestamp;
    use lofty::tag::{Tag, TagType};

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/flac/01 Test Track 1.flac");
    let flac = fs::read(&fixture).expect("FLAC fixture missing");

    for t in tracks {
        let dest = dir.join(t.filename);
        fs::write(&dest, &flac).unwrap();
        let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(t.title.to_string());
        tag.set_artist(album_artist.to_string());
        tag.set_album(album_title.to_string());
        tag.insert_text(ItemKey::AlbumArtist, album_artist.to_string());
        if let Some(y) = year {
            tag.set_date(Timestamp {
                year: y,
                month: None,
                day: None,
                hour: None,
                minute: None,
                second: None,
            });
        }
        tag.set_track(t.track_number);
        tagged.insert_tag(tag);
        tagged
            .save_to_path(&dest, WriteOptions::default())
            .expect("save tags");
    }
}

/// A minimal valid JPEG (SOI/APP0/DQT/SOF0/EOI) used as embedded cover
/// data — the import never decodes it, it only stores the bytes.
const EMBEDDED_JPEG: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9,
];

/// Like `generate_tagged_album_files`, but also embeds `EMBEDDED_JPEG` as
/// a front cover in every file — a tagged rip whose only artwork is
/// embedded in the audio.
fn generate_tagged_album_files_with_embedded_cover(
    dir: &Path,
    album_title: &str,
    album_artist: &str,
    tracks: &[TaggedTrack<'_>],
) {
    use lofty::config::WriteOptions;
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/flac/01 Test Track 1.flac");
    let flac = fs::read(&fixture).expect("FLAC fixture missing");

    for t in tracks {
        let dest = dir.join(t.filename);
        fs::write(&dest, &flac).unwrap();
        let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(t.title.to_string());
        tag.set_artist(album_artist.to_string());
        tag.set_album(album_title.to_string());
        tag.insert_text(ItemKey::AlbumArtist, album_artist.to_string());
        tag.set_track(t.track_number);
        tag.push_picture(
            Picture::unchecked(EMBEDDED_JPEG.to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Jpeg)
                .build(),
        );
        tagged.insert_tag(tag);
        tagged
            .save_to_path(&dest, WriteOptions::default())
            .expect("save tags");
    }
}

fn generate_album_with_cover(dir: &Path, filenames: &[&str]) {
    generate_album_files(dir, filenames);
    let scans = dir.join("scans");
    fs::create_dir_all(&scans).unwrap();
    let jpeg: Vec<u8> = vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06,
        0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B,
        0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
        0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31,
        0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF,
        0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xD9,
    ];
    fs::write(scans.join("back.jpg"), &jpeg).unwrap();
}

fn discogs_release(title: &str, tracks: &[&str]) -> DiscogsRelease {
    DiscogsRelease {
        id: format!("test-{}", title.to_lowercase().replace(' ', "-")),
        title: title.to_string(),
        year: Some(2024),
        genre: vec![],
        style: vec![],
        format: vec![],
        country: None,
        label: vec![],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![],
        tracklist: tracks
            .iter()
            .enumerate()
            .map(|(i, t)| DiscogsTrack {
                type_: "track".to_string(),
                position: format!("{}", i + 1),
                title: t.to_string(),
                duration: Some("3:00".to_string()),
                artists: vec![],
            })
            .collect(),
        master_id: None,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

/// 2. Folder scan produces correct candidates from a multi-album directory.
#[tokio::test]
#[serial]
async fn folder_scan_produces_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album1 = collection.join("Artist - First Album");
    let album2 = collection.join("Artist - Second Album");
    fs::create_dir_all(&album1).unwrap();
    fs::create_dir_all(&album2).unwrap();
    generate_album_files(&album1, &["01 Track.flac", "02 Track.flac"]);
    generate_album_files(&album2, &["01 Track.flac"]);

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .unwrap();

    let mut candidates = vec![];
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(100), scan_rx.recv()).await {
            Ok(Some(ScanEvent::FolderCandidate(c))) => {
                candidates.push(c);
            }
            Ok(Some(ScanEvent::Finished)) => break,
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {
                if tokio::time::Instant::now() > deadline {
                    panic!("Scan did not finish within 5s");
                }
            }
        }
    }

    // `Collection/` has no disc-indicator subdirs, so it's a navigation
    // container. Each album inside it is its own candidate.
    assert_eq!(candidates.len(), 2, "each album should be a candidate");
    let names: std::collections::BTreeSet<_> = candidates.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains("Artist - First Album"));
    assert!(names.contains("Artist - Second Album"));

    println!("Folder scan produced 2 independent candidates");
}

/// The watcher reconciles a folder against the candidates it last emitted: a new
/// release folder appears as a candidate, and a deleted one emits
/// `CandidateRemoved`. Driven by `scan_watched_folders` re-triggers (plus the
/// watcher's own debounced FS reconciles) over a fixed window, so it doesn't
/// hinge on filesystem-event timing — both paths reconcile to the same on-disk
/// truth, so `contains` assertions hold regardless of how many fire.
#[tokio::test]
#[serial]
async fn watcher_reconciles_added_and_removed_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album1 = collection.join("Artist - First Album");
    fs::create_dir_all(&album1).unwrap();
    generate_album_files(&album1, &["01 Track.flac"]);
    let album1_key = album1.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .unwrap();

    let first = collect_scan_events(&mut scan_rx).await;
    assert!(
        first.added.contains(&album1_key),
        "initial scan should surface the first album"
    );
    assert!(first.removed.is_empty());

    // A new release folder appears on disk; re-scan surfaces it.
    let album2 = collection.join("Artist - Second Album");
    fs::create_dir_all(&album2).unwrap();
    generate_album_files(&album2, &["01 Track.flac"]);
    let album2_key = album2.to_string_lossy().into_owned();
    f.handle.scan_watched_folders().unwrap();

    let second = collect_scan_events(&mut scan_rx).await;
    assert!(
        second.added.contains(&album2_key),
        "the newly-added folder should surface as a candidate"
    );

    // The first release folder is deleted; re-scan removes its candidate.
    fs::remove_dir_all(&album1).unwrap();
    f.handle.scan_watched_folders().unwrap();

    let third = collect_scan_events(&mut scan_rx).await;
    assert!(
        third.removed.contains(&album1_key),
        "the deleted folder's candidate should be removed"
    );
}

struct ScanBatch {
    added: Vec<String>,
    removed: Vec<String>,
}

/// Drain scan events over a fixed window, collecting candidate paths added and
/// candidate keys removed. A fixed window (rather than stopping at the first
/// `Finished`) absorbs the watcher's debounced FS reconcile landing alongside an
/// explicit re-scan.
async fn collect_scan_events(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
) -> ScanBatch {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await {
            Ok(Some(ScanEvent::FolderCandidate(c))) => {
                added.push(c.path.to_string_lossy().into_owned());
            }
            Ok(Some(ScanEvent::CandidateRemoved { candidate_key })) => {
                removed.push(candidate_key);
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }
    }
    ScanBatch { added, removed }
}

/// 3. Unmanaged folder import: album, release, tracks all in DB, files stay in place.
#[tokio::test]
#[serial]
async fn unmanaged_folder_import() {
    support::tracing_init();

    let release = discogs_release("Test Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    // Subscribe to library events BEFORE import
    let mut event_rx = f.handle.library_manager.subscribe_events();

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(
        &album_dir,
        &[
            "01 Track One.flac",
            "02 Track Two.flac",
            "03 Track Three.flac",
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
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
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    // Verify release in DB: unmanaged (not managed), with this device's
    // unmanaged-source row recording the in-place import path.
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert!(!release.managed);
    let source =
        f.db.get_release_unmanaged_source(&release_id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(source.path, album_dir.to_str().unwrap());

    // Verify tracks
    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].title, "Track One");
    assert_eq!(tracks[1].title, "Track Two");
    assert_eq!(tracks[2].title, "Track Three");

    // Verify files
    let files = f.db.get_files_for_release(&release_id).await.unwrap();
    assert_eq!(files.len(), 3);

    // Original files still in place (unmanaged)
    assert!(album_dir.join("01 Track One.flac").exists());
    assert!(album_dir.join("02 Track Two.flac").exists());
    assert!(album_dir.join("03 Track Three.flac").exists());

    // Verify library events were emitted
    let mut events = Vec::new();
    while let Ok(Ok(event)) =
        tokio::time::timeout(std::time::Duration::from_millis(200), event_rx.recv()).await
    {
        events.push(event);
    }

    let album_added = events
        .iter()
        .filter(|e| matches!(e, bae_core::library::LibraryEvent::AlbumAdded { .. }))
        .count();
    assert_eq!(album_added, 1, "expected one AlbumAdded event after import");
}

/// Managed { pin: false } import enqueues the cloud_outbox upload BEFORE
/// emitting `ImportProgress::Complete`, so a consumer that treats Complete
/// as "done" can rely on the cloud upload being queued. A regression would
/// re-introduce the race where a fast consumer (navigating away, tearing
/// the import handle down) acted on Complete before the outbox row landed.
#[tokio::test]
#[serial]
async fn managed_unpin_complete_fires_after_outbox_enqueue() {
    support::tracing_init();

    let release = discogs_release("Order Test", &["Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Managed,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let _ = support::wait_for_import_complete(&mut progress_rx).await;

    // The moment Complete is observed, the outbox MUST already have the
    // entry — no polling, no sleep. If this fails, the Complete-before-outbox
    // race is back.
    let uploads = f.db.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(
        uploads.len(),
        1,
        "outbox should already hold the upload by the time Complete fires",
    );
}

/// 4. Import produces correct audio format records.
#[tokio::test]
#[serial]
async fn import_produces_audio_format_records() {
    support::tracing_init();

    let release = discogs_release("Format Album", &["Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 1);

    // Check audio format was recorded for the track
    let format =
        f.db.find_audio_format_by_track_id(&tracks[0].id)
            .await
            .unwrap();
    assert!(format.is_some(), "should have audio format record");
    let format = format.unwrap();
    assert_eq!(format.content_type.as_str(), "audio/flac");

    println!(
        "Audio format records verified: {}",
        format.content_type.as_str()
    );
}

/// Interleaved-stereo 1 kHz sine at `amplitude` (fraction of full scale).
///
/// When `spikes` is set, a single full-scale sample is injected every 0.1 s.
/// These raise the measured true peak to ~1.0 without moving the integrated
/// loudness (they're far too sparse to form a gating block), which is exactly
/// the high-crest-factor signal needed to make the playback peak clamp engage:
/// a quiet track that nonetheless can't be boosted past full scale. A pure sine
/// can never demonstrate the clamp, because its peak and RMS scale together.
///
/// Samples are scaled to the 16-bit range because the FLAC encoder writes S16.
/// A small deterministic dither keeps the stream from compressing below the
/// import's FLAC truncation check (file must be >= 10% of raw PCM); the dither
/// sits ~60 dB down, moving neither the loudness nor the peak.
fn sine(amplitude: f64, sample_rate: u32, secs: f64, spikes: bool) -> Vec<i32> {
    use std::f64::consts::PI;
    let n = (sample_rate as f64 * secs) as usize;
    let spike_period = (sample_rate as f64 * 0.1) as usize;
    let full_scale = i16::MAX as i32;
    let mut rng: u32 = 0x1234_5678;
    let mut dither = || {
        rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((rng >> 16) as i32 & 0x3F) - 32
    };
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let s = if spikes && i % spike_period == 0 {
            full_scale
        } else {
            let t = i as f64 / sample_rate as f64;
            ((2.0 * PI * 1000.0 * t).sin() * amplitude * i16::MAX as f64) as i32 + dither()
        };
        out.push(s);
        out.push(s);
    }
    out
}

/// Write a synthetic 16-bit stereo FLAC of `samples` to `path`.
fn write_flac(path: &Path, samples: &[i32], sample_rate: u32) {
    let bytes = bae_core::audio_codec::encode_to_flac(
        samples,
        sample_rate,
        2,
        16,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .expect("encode synthetic FLAC");
    fs::write(path, bytes).unwrap();
}

/// Loudness normalization, end to end: import two tracks of deliberately
/// different loudness, confirm the stored per-track measurements differ, and
/// confirm the gain each track derives at playback reflects its own loudness
/// (the quieter track is boosted more) with the peak clamp engaging on a quiet
/// track that nonetheless peaks near full scale.
///
/// This is the load-bearing test for the feature: it exercises the real import
/// measurement path (`ImportService` → `measure_loudness`) and the real playback
/// derivation (`ResolvedTrackAudio::replay_gain_linear`), not reconstructions.
#[tokio::test]
#[serial]
async fn loudness_measured_at_import_drives_playback_gain() {
    use bae_core::config::ReplayGainMode;

    support::tracing_init();

    let release = discogs_release("Loudness Album", &["Quiet Track", "Loud Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let sr = 44_100;
    // Quiet track: a low-amplitude sine (so it wants a boost toward the target)
    // with sparse full-scale spikes that push its true peak to ~1.0 — the
    // crest factor that makes the playback clamp engage. Loud track: a steady
    // half-scale sine, clearly louder and pulled down toward the target.
    write_flac(
        &album_dir.join("01 Quiet Track.flac"),
        &sine(0.03, sr, 4.0, true),
        sr,
    );
    write_flac(
        &album_dir.join("02 Loud Track.flac"),
        &sine(0.5, sr, 4.0, false),
        sr,
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    // ── Stored measurements differ and reflect the two tracks' loudness ──
    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 2);
    let quiet = tracks.iter().find(|t| t.title == "Quiet Track").unwrap();
    let loud = tracks.iter().find(|t| t.title == "Loud Track").unwrap();

    let quiet_fmt =
        f.db.find_audio_format_by_track_id(&quiet.id)
            .await
            .unwrap()
            .unwrap();
    let loud_fmt =
        f.db.find_audio_format_by_track_id(&loud.id)
            .await
            .unwrap()
            .unwrap();

    let quiet_lufs = quiet_fmt
        .track_loudness_lufs
        .expect("quiet track measured a loudness");
    let loud_lufs = loud_fmt
        .track_loudness_lufs
        .expect("loud track measured a loudness");
    assert!(
        loud_lufs > quiet_lufs + 10.0,
        "loud track ({loud_lufs} LUFS) should be clearly louder than quiet ({quiet_lufs} LUFS)"
    );
    // The quiet track's late full-scale burst pushes its peak near 1.0; the
    // steady half-scale loud track peaks near 0.5.
    let quiet_peak = quiet_fmt
        .track_peak_linear
        .expect("quiet track measured a peak");
    let loud_peak = loud_fmt
        .track_peak_linear
        .expect("loud track measured a peak");
    assert!(
        quiet_peak > 0.9,
        "quiet track's burst should peak near full scale: {quiet_peak}"
    );
    assert!(
        loud_peak < 0.7,
        "loud track's steady sine should peak near 0.5: {loud_peak}"
    );

    // Album loudness was written to the release row from the combined meters.
    // EBU R128 album loudness is the gated integration over both tracks, so the
    // louder track dominates: the album sits in [quiet, loud] and lands near the
    // loud track, never below the quiet one.
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    let album_lufs = release
        .album_loudness_lufs
        .expect("album loudness measured");
    assert!(
        album_lufs.is_finite() && album_lufs >= quiet_lufs && album_lufs <= loud_lufs + 0.01,
        "album loudness {album_lufs} should fall within [{quiet_lufs}, {loud_lufs}]"
    );
    let album_peak = release.album_peak_linear.expect("album peak measured");
    assert!(
        (album_peak - quiet_peak).abs() < 1e-6,
        "album peak {album_peak} should be the max of the tracks' peaks ({quiet_peak})"
    );

    // ── Playback gain reflects each track's own loudness (mode = Track) ──
    let quiet_audio = f
        .handle
        .library_manager
        .resolve_track_audio(&quiet.id)
        .await
        .unwrap();
    let loud_audio = f
        .handle
        .library_manager
        .resolve_track_audio(&loud.id)
        .await
        .unwrap();

    let quiet_gain = quiet_audio.replay_gain_linear(ReplayGainMode::Track);
    let loud_gain = loud_audio.replay_gain_linear(ReplayGainMode::Track);

    // Off is always unity, regardless of measurements.
    assert_eq!(quiet_audio.replay_gain_linear(ReplayGainMode::Off), 1.0);

    // The quieter track wants more boost; the louder track is attenuated toward
    // the target. So the quiet track's gain exceeds the loud track's.
    assert!(
        quiet_gain > loud_gain,
        "quiet track gain {quiet_gain} should exceed loud track gain {loud_gain}"
    );
    // The loud track at ~-9 LUFS is pulled DOWN toward -18 (gain < 1, no clamp).
    assert!(
        loud_gain < 1.0,
        "loud track should be attenuated toward the target: {loud_gain}"
    );

    // ── Peak clamp engages for the quiet, near-full-scale track ──
    // Unclamped, the quiet track's gain would be 10^((-18 - L)/20), a large
    // boost. With a peak ~1.0 the clamp caps it at ~1/peak ≈ 1.0, so the applied
    // gain must equal the clamp, NOT the (much larger) loudness-only gain.
    let unclamped = 10f64.powf((-18.0 - quiet_lufs) / 20.0) as f32;
    let clamp = (1.0 / quiet_peak) as f32;
    assert!(
        unclamped > clamp + 0.5,
        "test setup: quiet track's unclamped gain {unclamped} must exceed its clamp {clamp} so the clamp is observable"
    );
    assert!(
        (quiet_gain - clamp).abs() < 0.05,
        "quiet track's applied gain {quiet_gain} should be the peak clamp {clamp}, not the unclamped {unclamped}"
    );
}

/// 5. Two sequential imports both succeed and produce separate albums.
#[tokio::test]
#[serial]
async fn two_sequential_imports() {
    support::tracing_init();

    let titles = ["First Album", "Second Album"];
    let mut release_keys = vec![];
    for title in &titles {
        let release = discogs_release(title, &["Track"]);
        release_keys.push(seed_discogs_test_release(release));
    }
    let f = ImportFixture::new().await;

    let mut release_ids = vec![];
    for (i, title) in titles.iter().enumerate() {
        let _ = title;
        let album_dir = f.temp_path().join(format!("album{}", i + 1));
        fs::create_dir_all(&album_dir).unwrap();
        // Distinct filename per album so the two imports carry different content
        // hashes. The content hash is the relative path + size of each file, and
        // re-importing the same content overwrites the prior release, so reusing
        // one name would make the second import delete the first.
        let track_name = format!("01 Track {}.flac", i + 1);
        generate_album_files(&album_dir, &[track_name.as_str()]);

        let import_id = uuid::Uuid::new_v4().to_string();
        f.handle
            .send_command(ImportCommand::Folder {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir,
                selected_cover: None,
                storage_mode: StorageMode::Unmanaged,
                pin: false,
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_keys[i].clone(), MetadataSource::Discogs),
                },
                user_edit: None,
            })
            .unwrap();

        let mut progress_rx = f.handle.subscribe_import(import_id);
        let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;
        release_ids.push(release_id);
    }

    // Both releases exist in DB
    let release1 =
        f.db.find_release_by_id(&release_ids[0])
            .await
            .unwrap()
            .unwrap();
    let release2 =
        f.db.find_release_by_id(&release_ids[1])
            .await
            .unwrap()
            .unwrap();

    // Different albums
    assert_ne!(release1.album_id, release2.album_id);

    let album1 =
        f.db.find_album_by_id(&release1.album_id)
            .await
            .unwrap()
            .unwrap();
    let album2 =
        f.db.find_album_by_id(&release2.album_id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(album1.title, "First Album");
    assert_eq!(album2.title, "Second Album");

    println!("Two sequential imports verified");
}

/// 6. Import with local cover art: library_images row and image file on disk.
#[tokio::test]
#[serial]
async fn import_with_cover_art() {
    support::tracing_init();

    let release = discogs_release("Cover Album", &["Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_with_cover(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: Some(CoverSelection::Local("scans/back.jpg".to_string())),
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    // Cover image row in DB
    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap();
    assert!(cover.is_some(), "should have cover image in DB");
    let cover = cover.unwrap();
    assert_eq!(cover.source, "local");

    // Cover image file on disk
    let cover_path = bae_core::storage::local::image_path(&f.library_dir, &release_id);
    assert!(
        cover_path.exists(),
        "cover image file should exist at {:?}",
        cover_path
    );
    let cover_bytes = fs::read(&cover_path).unwrap();
    assert!(!cover_bytes.is_empty(), "cover file should not be empty");

    println!(
        "Import with cover art verified: DB row + {} bytes on disk",
        cover_bytes.len()
    );
}

// ── identity-choice + user-edit at commit ──────────────────────────────────

/// A Discogs release with rich pressing fields, used to assert that Exact
/// preserves them and Approximate clears them.
fn discogs_release_rich(title: &str, master_id: &str, tracks: &[&str]) -> DiscogsRelease {
    DiscogsRelease {
        id: format!("test-{}", title.to_lowercase().replace(' ', "-")),
        title: title.to_string(),
        year: Some(1996),
        genre: vec![],
        style: vec![],
        format: vec!["CD".to_string()],
        country: Some("US".to_string()),
        label: vec!["Label Name".to_string()],
        cover_image: None,
        thumb: None,
        catno: Some("CAT-001".to_string()),
        artists: vec![],
        tracklist: tracks
            .iter()
            .enumerate()
            .map(|(i, t)| DiscogsTrack {
                type_: "track".to_string(),
                position: format!("{}", i + 1),
                title: t.to_string(),
                duration: Some("3:00".to_string()),
                artists: vec![],
            })
            .collect(),
        master_id: Some(master_id.to_string()),
    }
}

/// Exact import: identity row carries `source_release_id`; pressing fields
/// (year, format, label, catalog number, country) seed from the picked
/// release.
#[tokio::test]
#[serial]
async fn exact_import_writes_release_id_and_pressing_fields() {
    support::tracing_init();

    let release = discogs_release_rich("Album Title", "master-exact", &["Track One"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key.clone(), MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.pressing.year, Some(1996));
    assert_eq!(release.pressing.format.as_deref(), Some("CD"));
    assert_eq!(release.pressing.label.as_deref(), Some("Label Name"));
    assert_eq!(release.pressing.catalog_number.as_deref(), Some("CAT-001"));
    assert_eq!(release.pressing.country.as_deref(), Some("US"));

    // metadata_source columns point at the picked release.
    assert_eq!(
        release.metadata_source_release_id.as_deref(),
        Some(release_id_key.as_str())
    );

    let identities = f.db.get_release_identities(&release.id).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::Discogs);
    assert_eq!(identities[0].source_group_id, "master-exact");
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some(release_id_key.as_str())
    );
}

/// Approximate import: identity row's `source_release_id` is NULL; pressing
/// fields stay NULL on the release row even though the source supplied them.
/// Album-group-stable fields (title, tracks) still seed.
#[tokio::test]
#[serial]
async fn approximate_import_nulls_release_id_and_clears_pressing() {
    support::tracing_init();

    let release = discogs_release_rich("Album Title", "master-approx", &["Track One"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(release_id_key.clone(), MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    // Pressing fields cleared by the Approximate seed.
    assert!(
        release.pressing.year.is_none(),
        "year should be NULL, got {:?}",
        release.pressing.year
    );
    assert!(
        release.pressing.format.is_none(),
        "format should be NULL, got {:?}",
        release.pressing.format
    );
    assert!(
        release.pressing.label.is_none(),
        "label should be NULL, got {:?}",
        release.pressing.label
    );
    assert!(
        release.pressing.catalog_number.is_none(),
        "catalog_number should be NULL, got {:?}",
        release.pressing.catalog_number
    );
    assert!(
        release.pressing.country.is_none(),
        "country should be NULL, got {:?}",
        release.pressing.country
    );

    // metadata_source columns still point at the picked release — a later
    // re-projection needs them to replay the seed.
    assert_eq!(
        release.metadata_source_release_id.as_deref(),
        Some(release_id_key.as_str())
    );

    // Identity row: group ID present, release ID NULL.
    let identities = f.db.get_release_identities(&release.id).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source_group_id, "master-approx");
    assert!(
        identities[0].source_release_id.is_none(),
        "Approximate must NULL source_release_id, got {:?}",
        identities[0].source_release_id
    );

    // Album-group-stable fields still came from the source.
    let album =
        f.db.find_album_by_id(&release.album_id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(album.title, "Album Title");
    let tracks = f.db.get_tracks_for_release(&release.id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Track One");
}

/// User-edit overlay applies on top of the Approximate seed: the user
/// can fill country (cleared by Approximate) and the committed value
/// reflects the edit.
#[tokio::test]
#[serial]
async fn approximate_import_with_user_edit_overlay() {
    support::tracing_init();

    let release = discogs_release_rich("Album Title", "master-edit", &["Track One"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let edit = ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_names: vec!["Edited Artist".to_string()],
        pressing: PressingEdit {
            // User typed JP — we expect this to land on the release row.
            country: Some("JP".to_string()),
            ..PressingEdit::blank()
        },
        tracks: vec![TrackUserEdit {
            title: "Edited Track".to_string(),
            side: 1,
            track_number: Some(1),
            artist_names: vec![],
        }],
    };

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(release_id_key.clone(), MetadataSource::Discogs),
            },
            user_edit: Some(edit),
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    // Other pressing fields stay NULL — user didn't fill them and
    // Approximate cleared the seed.
    assert!(release.pressing.year.is_none());
    assert!(release.pressing.format.is_none());

    let album =
        f.db.find_album_by_id(&release.album_id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(album.title, "Edited Title");

    let tracks = f.db.get_tracks_for_release(&release.id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Edited Track");

    // Identity row still NULL release_id — user_edit doesn't affect identity.
    let identities = f.db.get_release_identities(&release.id).await.unwrap();
    assert_eq!(identities.len(), 1);
    assert!(identities[0].source_release_id.is_none());
}

// ── cross-source identity rows ──────────────────────────────────────────────
//
// When MB url-rels link to a Discogs release with a master id (or vice
// versa), the mapper emits two `release_identities` rows. The user's
// identity choice applies to BOTH rows: Exact keeps the per-source
// release IDs on each, Approximate clears both.

/// Seed a Discogs release the MB-rooted import path will resolve via
/// MB url-rels. Returns the Discogs release id.
fn seed_discogs_for_xref(release_id: &str, master_id: &str, title: &str) -> String {
    let discogs_release = DiscogsRelease {
        id: release_id.to_string(),
        title: title.to_string(),
        year: Some(1996),
        genre: vec![],
        style: vec![],
        format: vec!["CD".to_string()],
        country: Some("US".to_string()),
        label: vec!["Label Name".to_string()],
        cover_image: None,
        thumb: None,
        catno: Some("CAT-001".to_string()),
        artists: vec![DiscogsArtist {
            id: "discogs-artist-1".to_string(),
            name: "Artist Name".to_string(),
        }],
        tracklist: vec![DiscogsTrack {
            type_: "track".to_string(),
            position: "1".to_string(),
            title: "Track One".to_string(),
            duration: Some("3:00".to_string()),
            artists: vec![],
        }],
        master_id: Some(master_id.to_string()),
    };
    bae_core::discogs::client::seed_master_cache(master_id, Some(1996), "{}".to_string());
    bae_core::discogs::client::seed_release_cache(release_id, (discogs_release, "{}".to_string()));
    release_id.to_string()
}

/// Seed an MB release whose url-rels carry a Discogs release URL.
/// Returns the MB release id.
fn seed_mb_with_discogs_xref(
    mb_release_id: &str,
    mb_group_id: &str,
    discogs_release_id: &str,
    title: &str,
) -> String {
    let response = MbReleaseResponse {
        id: mb_release_id.to_string(),
        title: title.to_string(),
        date: Some("1996".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(MbReleaseGroupRef {
            id: mb_group_id.to_string(),
            first_release_date: None,
            relations: None,
        }),
        label_info: vec![],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: vec![MbTrack {
                position: Some(1),
                number: Some("1".to_string()),
                title: None,
                length: None,
                recording: Some(MbRecording {
                    title: Some("Track One".to_string()),
                }),
                artist_credit: vec![],
            }],
        }],
        relations: vec![],
    };
    let external_urls = ExternalUrls {
        discogs_master_url: None,
        discogs_release_url: Some(format!(
            "https://www.discogs.com/release/{}",
            discogs_release_id
        )),
    };
    bae_core::musicbrainz::seed_release_cache(
        mb_release_id,
        (response, external_urls, "{}".to_string()),
    );
    bae_core::musicbrainz::seed_release_group_json_cache(mb_group_id, "{}".to_string());
    mb_release_id.to_string()
}

/// MB-rooted Exact import with a Discogs cross-link writes two identity
/// rows, both carrying their per-source `source_release_id`.
#[tokio::test]
#[serial]
async fn cross_source_exact_writes_both_release_ids() {
    support::tracing_init();

    let discogs_id = seed_discogs_for_xref("90000001", "xref-d-master-exact", "Album Title");
    // MB needs to know about the Discogs URL → release id mapping for
    // the `fetch_mb_xref` path; this test goes the other direction
    // (MB → Discogs via url-rels), but seeding both directions costs
    // nothing and keeps the cache from racing on a stale `None`.
    bae_core::musicbrainz::seed_discogs_url_lookup(&discogs_id, None);
    let mb_id = seed_mb_with_discogs_xref(
        "xref-mb-rel-exact",
        "xref-mb-group-exact",
        &discogs_id,
        "Album Title",
    );

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(mb_id.clone(), MetadataSource::MusicBrainz),
            },
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert_eq!(identities.len(), 2, "expected MB + Discogs identity rows");

    let mb = identities
        .iter()
        .find(|i| i.source == MetadataSource::MusicBrainz)
        .expect("MB identity row missing");
    assert_eq!(mb.source_group_id, "xref-mb-group-exact");
    assert_eq!(mb.source_release_id.as_deref(), Some(mb_id.as_str()));

    let discogs = identities
        .iter()
        .find(|i| i.source == MetadataSource::Discogs)
        .expect("Discogs identity row missing");
    assert_eq!(discogs.source_group_id, "xref-d-master-exact");
    assert_eq!(
        discogs.source_release_id.as_deref(),
        Some(discogs_id.as_str())
    );
}

/// MB-rooted Approximate import with a Discogs cross-link still writes
/// two identity rows, but both have `source_release_id = NULL` — the
/// user's "I don't claim a specific pressing" applies across sources.
#[tokio::test]
#[serial]
async fn cross_source_approximate_nulls_both_release_ids() {
    support::tracing_init();

    let discogs_id = seed_discogs_for_xref("90000002", "xref-d-master-approx", "Album Title");
    bae_core::musicbrainz::seed_discogs_url_lookup(&discogs_id, None);
    let mb_id = seed_mb_with_discogs_xref(
        "xref-mb-rel-approx",
        "xref-mb-group-approx",
        &discogs_id,
        "Album Title",
    );

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(mb_id.clone(), MetadataSource::MusicBrainz),
            },
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert_eq!(identities.len(), 2, "expected MB + Discogs identity rows");

    // Group ids survive — the claim is at the group level for each source.
    let mb = identities
        .iter()
        .find(|i| i.source == MetadataSource::MusicBrainz)
        .expect("MB identity row missing");
    assert_eq!(mb.source_group_id, "xref-mb-group-approx");
    assert!(
        mb.source_release_id.is_none(),
        "MB row release_id should be NULL for Approximate, got {:?}",
        mb.source_release_id,
    );

    let discogs = identities
        .iter()
        .find(|i| i.source == MetadataSource::Discogs)
        .expect("Discogs identity row missing");
    assert_eq!(discogs.source_group_id, "xref-d-master-approx");
    assert!(
        discogs.source_release_id.is_none(),
        "Discogs row release_id should be NULL for Approximate, got {:?}",
        discogs.source_release_id,
    );
}

/// Discogs-rooted Approximate import. MB has a back-link to this
/// Discogs release; the Discogs mapper emits both rows. Approximate
/// clears `source_release_id` on both.
#[tokio::test]
#[serial]
async fn cross_source_discogs_rooted_approximate_nulls_both_release_ids() {
    support::tracing_init();

    let discogs_id = seed_discogs_for_xref("90000003", "xref-drooted-d-master", "Album Title");
    // The Discogs commit path reaches MB via the URL-lookup cache.
    let mb_release_id = "xref-drooted-mb-rel".to_string();
    let mb_group_id = "xref-drooted-mb-group".to_string();
    bae_core::musicbrainz::seed_discogs_url_lookup(&discogs_id, Some(mb_release_id.clone()));
    bae_core::musicbrainz::seed_release_cache(
        &mb_release_id,
        (
            MbReleaseResponse {
                id: mb_release_id.clone(),
                title: "Album Title".to_string(),
                date: None,
                country: None,
                barcode: None,
                artist_credit: vec![],
                release_group: Some(MbReleaseGroupRef {
                    id: mb_group_id.clone(),
                    first_release_date: None,
                    relations: None,
                }),
                label_info: vec![],
                media: vec![],
                relations: vec![],
            },
            ExternalUrls {
                discogs_master_url: None,
                discogs_release_url: None,
            },
            "{}".to_string(),
        ),
    );
    bae_core::musicbrainz::seed_release_group_json_cache(&mb_group_id, "{}".to_string());

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(discogs_id.clone(), MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert_eq!(identities.len(), 2, "expected Discogs + MB identity rows");

    let discogs = identities
        .iter()
        .find(|i| i.source == MetadataSource::Discogs)
        .expect("Discogs identity row missing");
    assert_eq!(discogs.source_group_id, "xref-drooted-d-master");
    assert!(discogs.source_release_id.is_none());

    let mb = identities
        .iter()
        .find(|i| i.source == MetadataSource::MusicBrainz)
        .expect("MB identity row missing");
    assert_eq!(mb.source_group_id, mb_group_id);
    assert!(mb.source_release_id.is_none());
}

// ── "Add as Unknown" ────────────────────────────────────────────────────────

/// Unknown commit reads embedded tags, writes zero `release_identities`
/// rows, sets `metadata_source = file_tags`, leaves
/// `metadata_source_release_id = NULL`, and seeds the album / tracks
/// from what's on disk. No external source consulted.
#[tokio::test]
#[serial]
async fn unknown_import_seeds_from_file_tags_and_writes_no_identity() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album From Tags",
        "Artist From Tags",
        Some(2003),
        &[
            TaggedTrack {
                filename: "01.flac",
                title: "Track One",
                track_number: 1,
            },
            TaggedTrack {
                filename: "02.flac",
                title: "Track Two",
                track_number: 2,
            },
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(
        release.metadata_source,
        bae_core::db::ReleaseMetadataSource::FileTags,
    );
    assert!(
        release.metadata_source_release_id.is_none(),
        "Unknown imports must leave metadata_source_release_id NULL, got {:?}",
        release.metadata_source_release_id,
    );
    assert_eq!(release.pressing.year, Some(2003));
    assert_eq!(release.pressing.format.as_deref(), Some("FLAC"));

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert!(
        identities.is_empty(),
        "Unknown imports must write zero identity rows, got {identities:?}",
    );

    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Album From Tags");

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title, "Track One");
    assert_eq!(tracks[1].title, "Track Two");
}

/// A tagged rip whose only artwork is embedded in the audio (no folder
/// image, no remote selection) gets that embedded picture as its cover.
#[tokio::test]
#[serial]
async fn unknown_import_seeds_embedded_cover_when_no_folder_image() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files_with_embedded_cover(
        &album_dir,
        "Embedded Cover Album",
        "Artist",
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("embedded cover should be written when no folder/remote image exists");
    assert_eq!(
        cover.source, "embedded",
        "cover must be sourced from the embedded picture"
    );

    let cover_path = bae_core::storage::local::image_path(&f.library_dir, &release_id);
    let bytes = fs::read(&cover_path).expect("cover file on disk");
    assert_eq!(
        bytes, EMBEDDED_JPEG,
        "on-disk bytes are the embedded picture"
    );
}

/// A folder image outranks the embedded picture: when both exist, the
/// folder artwork is the cover and the embedded picture is ignored.
#[tokio::test]
#[serial]
async fn unknown_import_folder_image_wins_over_embedded_cover() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files_with_embedded_cover(
        &album_dir,
        "Both Covers Album",
        "Artist",
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );
    // A folder image alongside the embedded-cover audio. No explicit
    // selection — the auto-pick must still prefer this folder image.
    let scans = album_dir.join("scans");
    fs::create_dir_all(&scans).unwrap();
    fs::write(scans.join("cover.jpg"), EMBEDDED_JPEG).unwrap();

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("a cover should be written");
    assert_eq!(
        cover.source, "local",
        "the folder image must win over the embedded picture, got source {:?}",
        cover.source
    );
}

/// Unknown imports never deduplicate against existing releases — even
/// when an identified release with the same album title is already in
/// the library, an Unknown import lands on a fresh album.
#[tokio::test]
#[serial]
async fn unknown_import_always_creates_a_fresh_album() {
    support::tracing_init();

    // First import: identified, lands on its own album.
    let release = discogs_release_rich("Album Title", "master-existing", &["Track One"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let identified_dir = f.temp_path().join("identified");
    fs::create_dir_all(&identified_dir).unwrap();
    generate_album_files(&identified_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "identified".to_string(),
            folder: identified_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (_, identified_album_id) = support::wait_for_import_complete(&mut rx).await;

    // Second import: Unknown, same album title in tags. Must NOT
    // attach to the identified album.
    let unknown_dir = f.temp_path().join("unknown");
    fs::create_dir_all(&unknown_dir).unwrap();
    generate_tagged_album_files(
        &unknown_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01.flac",
            title: "Track One",
            track_number: 1,
        }],
    );

    let import_id2 = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id2.clone(),
            candidate_key: "unknown".to_string(),
            folder: unknown_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();
    let mut rx2 = f.handle.subscribe_import(import_id2);
    let (_, unknown_album_id) = support::wait_for_import_complete(&mut rx2).await;

    assert_ne!(
        identified_album_id, unknown_album_id,
        "Unknown import must land on a fresh album",
    );
}

/// User-edit overlay applies on top of the file-tag seed: the user
/// can override album title, artist, year, pressing fields, and track
/// titles via the editor before commit. Persisted metadata reflects
/// the edits.
#[tokio::test]
#[serial]
async fn unknown_import_with_user_edit_overlay() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Wrong Album Title",
        "Wrong Artist",
        Some(1999),
        &[TaggedTrack {
            filename: "01.flac",
            title: "Wrong Track Title",
            track_number: 1,
        }],
    );

    let edit = ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_names: vec!["Edited Artist".to_string()],
        pressing: PressingEdit {
            year: Some(2010),
            format: Some("FLAC".to_string()),
            label: Some("Edited Label".to_string()),
            catalog_number: Some("EDIT-1".to_string()),
            country: Some("JP".to_string()),
            barcode: Some("4943674000000".to_string()),
        },
        tracks: vec![TrackUserEdit {
            title: "Edited Track Title".to_string(),
            side: 1,
            track_number: Some(1),
            artist_names: vec![],
        }],
    };

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: Some(edit),
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(release.pressing.year, Some(2010));
    assert_eq!(release.pressing.format.as_deref(), Some("FLAC"));
    assert_eq!(release.pressing.label.as_deref(), Some("Edited Label"));
    assert_eq!(release.pressing.catalog_number.as_deref(), Some("EDIT-1"));
    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert_eq!(release.pressing.barcode.as_deref(), Some("4943674000000"));
    assert_eq!(
        release.metadata_source,
        bae_core::db::ReleaseMetadataSource::FileTags,
    );
    assert!(release.metadata_source_release_id.is_none());

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert!(
        identities.is_empty(),
        "user_edit must not introduce identity rows for Unknown",
    );

    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(album.title, "Edited Title");

    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Edited Track Title");
}

/// Unknown commit of a rip with no usable album-level tags seeds the
/// album title from the containing folder name rather than failing —
/// the permissive file-tag projection never hard-fails on a missing
/// ALBUM tag (the editable confirmation form gates a blank title before
/// save). The artist falls back to empty for the user to fill.
#[tokio::test]
#[serial]
async fn unknown_import_with_no_tags_seeds_title_from_folder_name() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("Mystery Rip");
    fs::create_dir_all(&album_dir).unwrap();
    // The fixture FLAC carries no Vorbis comments — no ALBUM/ARTIST tag
    // for the projection to read, so the folder name is the album title.
    generate_album_files(&album_dir, &["01.flac", "02.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode: StorageMode::Unmanaged,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(
        album.title, "Mystery Rip",
        "untagged rip takes the folder name as its album title",
    );
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(
        release.metadata_source,
        bae_core::db::ReleaseMetadataSource::FileTags,
    );
    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 2, "both untagged files import as tracks");
}
