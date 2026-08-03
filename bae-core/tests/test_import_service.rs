#![cfg(feature = "test-utils")]
//! Integration tests for the import service.
//!
//! Tests the real ImportService through its public handle API:
//! scan → import. All through real infrastructure.

use bae_test_support as support;

use bae_core::db::{Database, LibraryImageType};
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{
    CoverSelection, IdentityChoice, ImportCommand, ImportService, MetadataRef, MetadataSource,
    PressingEdit, ReleaseUserEdit, ScanEvent, StorageMode, TrackUserEdit,
};
use bae_core::library::LibraryManager;
use bae_core::musicbrainz::{
    MbArtistCredit, MbArtistRef, MbMedium, MbRecording, MbRelation, MbReleaseGroupRef,
    MbReleaseResponse, MbTrack, MbWork,
};
use bae_core::sync::CloudCipher;
use coven::EncryptionService;
use coven::InMemoryCloudHome;
use coven::StoreDir;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use support::seed_discogs_test_release;
use tempfile::TempDir;

// ── Helpers ─────────────────────────────────────────────────────────────────

struct ImportFixture {
    db: Database,
    handle: bae_core::import::ImportServiceHandle,
    library_manager: LibraryManager,
    config_handle: Arc<bae_core::config::ConfigHandle>,
    ids: Arc<dyn coven::IdProvider>,
    /// The library manager's cover-art client, kept so a test that drives a
    /// lookup can seed it (the hermetic client panics on a live request).
    cover_art: bae_core::import::cover_art::CoverArtArchiveClient,
    _temp: TempDir,
}

impl ImportFixture {
    async fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let db_dir = temp.path().join("db");
        fs::create_dir_all(&db_dir).unwrap();

        let db = Database::new_test(
            db_dir.join("test.db").to_str().unwrap(),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .unwrap();
        let library_dir = StoreDir::new(db_dir.clone());
        let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
        let ids: Arc<dyn coven::IdProvider> = Arc::new(coven::UuidProvider);
        let library_manager = LibraryManager::new(
            db.clone(),
            config_handle.clone(),
            key_service,
            std::sync::Arc::new(coven::SystemClock),
            ids.clone(),
            bae_core::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_core::import::cover_art::CoverArtArchiveClient::hermetic(),
        );

        // The manager's own client, not a second one: the import reads cover
        // answers through the manager, so a test seeding this handle has to be
        // seeding the instance the import will ask.
        let cover_art = library_manager.cover_art_archive_for_test();
        let handle =
            ImportService::start(tokio::runtime::Handle::current(), library_manager.clone())
                .await
                .expect("import service starts");

        Self {
            db,
            handle,
            library_manager,
            config_handle,
            ids,
            cover_art,
            _temp: temp,
        }
    }

    fn temp_path(&self) -> &Path {
        self._temp.path()
    }

    /// No sync loop behind the connection: the remote-import tests drain the
    /// upload queue themselves and assert what that pass moved.
    async fn connect_cloud(&self) {
        self.library_manager
            .connect_test_cloud_home_caller_driven(
                Arc::new(InMemoryCloudHome::new()),
                CloudCipher::Encrypted(EncryptionService::from_key([7u8; 32])),
            )
            .await
            .expect("connect in-memory cloud home");
    }

    fn set_decode_verification(&self, verify: bool) {
        self.config_handle
            .update(|c| c.verify_decode_on_import = verify)
            .expect("set verify_decode_on_import");
    }
}

async fn import_folder(
    f: &ImportFixture,
    album_dir: &Path,
    selected_cover: Option<CoverSelection>,
    storage_mode: StorageMode,
    identity_choice: IdentityChoice,
) -> Result<(String, String), String> {
    let import_id = f.ids.new_id();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.to_path_buf(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover,
            storage_mode,
            pin: false,
            identity_choice,
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    support::try_wait_for_import_complete(&mut progress_rx).await
}

async fn assert_release_has_external_ref(f: &ImportFixture, release_id: &str) {
    assert!(
        f.db.find_release_by_id(release_id).await.unwrap().is_some(),
        "prior release row should remain"
    );
    let files = f.db.get_files_for_release(release_id).await.unwrap();
    assert!(!files.is_empty(), "prior release files should remain");
    assert!(
        f.db.external_blob(&files[0].id).await.unwrap().is_some(),
        "prior local file reference should remain"
    );
}

fn generate_album_files(dir: &Path, filenames: &[&str]) {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/flac/01 Test Track 1.flac");
    let flac = fs::read(&fixture).expect("FLAC fixture missing");
    for name in filenames {
        fs::write(dir.join(name), &flac).unwrap();
    }
}

fn copy_cue_flac_fixture(dir: &Path) {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac");
    for name in ["Test Album.cue", "Test Album.flac"] {
        fs::copy(fixture.join(name), dir.join(name)).expect("copy CUE fixture");
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

/// Embedded cover width/height for the fixture below.
const EMBEDDED_COVER_DIMS: (u32, u32) = (64, 48);

/// A real, decodable JPEG cover (solid color, [`EMBEDDED_COVER_DIMS`]) embedded
/// in test audio. The store path decodes every cover and re-encodes it to a
/// ≤600 JPEG, so fixtures must be genuine images, not hand-written JPEG headers.
fn embedded_cover_jpeg() -> Vec<u8> {
    let (w, h) = EMBEDDED_COVER_DIMS;
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 60, 40]));
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .unwrap();
    buf.into_inner()
}

/// Like `generate_tagged_album_files`, but also embeds a real JPEG cover
/// (`embedded_cover_jpeg`) as the front cover in every file — a tagged rip
/// whose only artwork is embedded in the audio.
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
            Picture::unchecked(embedded_cover_jpeg())
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
    fs::write(scans.join("back.jpg"), embedded_cover_jpeg()).unwrap();
}

/// Like `generate_album_with_cover`, but the folder image is an oversized
/// (1000×1000) PNG — a source that must be downscaled and re-encoded to JPEG at
/// store time. Returns the selection path for the cover.
fn generate_album_with_oversized_cover(dir: &Path, filenames: &[&str]) -> String {
    generate_album_files(dir, filenames);
    let scans = dir.join("scans");
    fs::create_dir_all(&scans).unwrap();
    let img = image::RgbImage::from_pixel(1000, 1000, image::Rgb([20, 160, 90]));
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buf, image::ImageFormat::Png)
        .unwrap();
    fs::write(scans.join("cover.png"), buf.into_inner()).unwrap();
    "scans/cover.png".to_string()
}

/// A unique id for a synthetic discogs release. The discogs release cache is
/// process-global and keyed by id; deriving the id from the title alone made
/// two tests that pick the same title collide on one cache entry, so a
/// concurrent run served whichever seed landed last. Each built release is a
/// distinct release and gets a distinct id.
fn synthetic_release_id(title: &str) -> String {
    format!(
        "test-{}-{}",
        title.to_lowercase().replace(' ', "-"),
        uuid::Uuid::new_v4()
    )
}

fn discogs_release(title: &str, tracks: &[&str]) -> DiscogsRelease {
    DiscogsRelease {
        id: synthetic_release_id(title),
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
            .map(|(i, t)| DiscogsTrack {
                type_: "track".to_string(),
                position: format!("{}", i + 1),
                title: t.to_string(),
                duration: Some("3:00".to_string()),
                artists: vec![],
                extraartists: None,
            })
            .collect(),
        master_id: None,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

/// 2. Folder scan produces correct candidates from a multi-album directory.
#[tokio::test]
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
        .await
        .unwrap();

    let mut candidates = vec![];
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(100), scan_rx.recv()).await {
            Ok(Some(ScanEvent::FolderCandidate { candidate: c, .. })) => {
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
}

/// The watcher reconciles a folder against the candidates it last emitted: a new
/// release folder appears as a candidate, and a deleted one emits
/// `CandidateRemoved`. Driven by `scan_watched_folders` re-triggers (plus the
/// watcher's own debounced FS reconciles), so it doesn't hinge on
/// filesystem-event timing — both paths reconcile to the same on-disk truth, so
/// each step waits for its expected candidate event regardless of how many fire.
#[tokio::test]
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
        .await
        .unwrap();

    let first = scan_batch_until(&mut scan_rx, "the first album's candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album1_key.as_str()))
    })
    .await;
    assert!(
        first.added.contains(&album1_key),
        "initial scan should surface the first album"
    );
    assert!(
        first.removed.is_empty(),
        "the initial scan of a fresh folder removes nothing"
    );

    // A new release folder appears on disk; re-scan surfaces it.
    let album2 = collection.join("Artist - Second Album");
    fs::create_dir_all(&album2).unwrap();
    generate_album_files(&album2, &["01 Track.flac"]);
    let album2_key = album2.to_string_lossy().into_owned();
    f.handle.scan_watched_folders().unwrap();

    let second = scan_batch_until(&mut scan_rx, "the newly-added folder's candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album2_key.as_str()))
    })
    .await;
    assert!(
        second.added.contains(&album2_key),
        "the newly-added folder should surface as a candidate"
    );

    // The first release folder is deleted; re-scan removes its candidate.
    fs::remove_dir_all(&album1).unwrap();
    f.handle.scan_watched_folders().unwrap();

    let third = scan_batch_until(&mut scan_rx, "the deleted folder's candidate removal", |e| {
        matches!(e, ScanEvent::CandidateRemoved { candidate_key } if candidate_key == &album1_key)
    })
    .await;
    assert!(
        third.removed.contains(&album1_key),
        "the deleted folder's candidate should be removed"
    );
}

struct ScanBatch {
    added: Vec<String>,
    removed: Vec<String>,
}

/// Drain scan events until one matching `done` arrives, collecting the candidate
/// paths added and candidate keys removed along the way. Event-driven: returns
/// the instant the awaited event lands, and fails loud if it never does within a
/// bounded deadline. Positive assertions read the returned batch. (The watcher's
/// debounced FS reconcile can land alongside an explicit re-scan; both reconcile
/// to the same on-disk truth, so the batch is stable however many fire.)
async fn scan_batch_until(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    what: &str,
    mut done: impl FnMut(&ScanEvent) -> bool,
) -> ScanBatch {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out after 10s waiting for {what}");
        }
        match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await {
            Ok(Some(event)) => {
                let finished = done(&event);
                match &event {
                    ScanEvent::FolderCandidate { candidate: c, .. } => {
                        added.push(c.path.to_string_lossy().into_owned())
                    }
                    ScanEvent::CandidateRemoved { candidate_key } => {
                        removed.push(candidate_key.clone())
                    }
                    _ => {}
                }
                if finished {
                    return ScanBatch { added, removed };
                }
            }
            Ok(None) => panic!("scan channel closed before {what}"),
            Err(_) => {}
        }
    }
}

/// Wait for a single scan event matching `pred`, failing loud if none arrives
/// within a bounded deadline — the event-driven form of a fixed positive window.
async fn wait_for_scan_event(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    what: &str,
    mut pred: impl FnMut(&ScanEvent) -> bool,
) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out after 10s waiting for {what}");
        }
        match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await {
            Ok(Some(event)) if pred(&event) => return,
            Ok(Some(_)) => {}
            Ok(None) => panic!("scan channel closed before {what}"),
            Err(_) => {}
        }
    }
}

/// Collect every scan event that arrives within a fixed window. For the
/// negative assertions below — that after a handle call NO event of some kind
/// arrives (an unwatched folder surfaces no candidate; a redundant skip
/// re-broadcasts nothing) — where a window is the assertion, not overhead.
async fn drain_scan_events(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<ScanEvent>,
    window: std::time::Duration,
) -> Vec<ScanEvent> {
    let deadline = tokio::time::Instant::now() + window;
    let mut events = Vec::new();
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await {
            Ok(Some(event)) => events.push(event),
            Ok(None) => break,
            Err(_) => {}
        }
    }
    events
}

/// `remove_watched_folder` drops the folder from the persisted list, drops its
/// candidates from the reducer, and broadcasts the shortened list (plus sending
/// the watcher an `Unwatch`). Exercises the handle's remove path and
/// `watched_folders` accessor.
#[tokio::test]
async fn remove_watched_folder_drops_folder_and_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album = collection.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let album_key = album.to_string_lossy().into_owned();
    let collection_key = collection.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection_key.clone())
        .await
        .unwrap();

    let batch = scan_batch_until(&mut scan_rx, "the album candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album_key.as_str()))
    })
    .await;
    assert!(
        batch.added.contains(&album_key),
        "initial scan should surface the album candidate"
    );
    assert_eq!(
        f.handle
            .watched_folders()
            .iter()
            .map(|w| w.path.clone())
            .collect::<Vec<_>>(),
        vec![collection_key.clone()],
    );
    assert!(
        !f.handle
            .get_import_candidates()
            .folder_candidates
            .is_empty(),
        "the reducer holds the scanned candidate"
    );

    f.handle
        .remove_watched_folder(collection_key.clone())
        .await
        .unwrap();

    // The list accessor and the reducer both reflect the removal synchronously.
    assert!(
        f.handle.watched_folders().is_empty(),
        "removed folder is gone from the persisted list"
    );
    assert!(
        f.handle
            .get_import_candidates()
            .folder_candidates
            .is_empty(),
        "the reducer dropped the removed folder's candidates"
    );

    // The shortened (now empty) list is broadcast.
    wait_for_scan_event(
        &mut scan_rx,
        "the shortened folder-list broadcast",
        |event| matches!(event, ScanEvent::WatchedFoldersChanged { folders } if folders.is_empty()),
    )
    .await;

    // The watcher actually stopped: a new release folder appearing under the
    // now-unwatched root produces no scan activity (the reconcile that would
    // surface it never runs).
    let new_album = collection.join("Artist - Second Album");
    fs::create_dir_all(&new_album).unwrap();
    generate_album_files(&new_album, &["01 Track.flac"]);
    let after_unwatch = drain_scan_events(&mut scan_rx, std::time::Duration::from_secs(2)).await;
    assert!(
        !after_unwatch
            .iter()
            .any(|event| matches!(event, ScanEvent::FolderCandidate { candidate: _, .. })),
        "an unwatched folder must not surface new candidates, got {after_unwatch:?}",
    );
}

#[tokio::test]
async fn unavailable_watched_folder_remains_durable_and_reports_scan_failure() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let missing = f.temp_path().join("does-not-exist");
    let missing_key = missing.to_string_lossy().into_owned();

    f.handle
        .add_watched_folder(missing_key.clone())
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let snapshot = f.handle.get_import_candidates();
        if snapshot.folder_scan_statuses.iter().any(|status| {
            status.watched_folder_path == missing_key
                && matches!(
                    status.status,
                    bae_core::import::FolderScanStatus::Failed { .. }
                )
        }) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "unavailable watched root did not report a failed scan"
        );
        tokio::task::yield_now().await;
    }
    assert_eq!(f.handle.watched_folders().len(), 1);
}

/// A refresh is the awaited scan operation. If a watched root disappears, it
/// reports the failure, records the failed status, and preserves the last
/// candidate snapshot rather than turning an unavailable filesystem into
/// removals.
#[tokio::test]
async fn refresh_missing_watched_folder_fails_and_preserves_candidates() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let root = f.temp_path().join("unplugged-drive");
    let album = root.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let root_key = root.to_string_lossy().into_owned();
    let album_key = album.to_string_lossy().into_owned();
    f.handle.add_watched_folder(root_key.clone()).await.unwrap();
    f.handle
        .refresh_watched_folder(root_key.clone())
        .await
        .unwrap();

    fs::remove_dir_all(&root).unwrap();
    let result = f.handle.refresh_watched_folder(root_key.clone()).await;
    assert!(
        result.is_err(),
        "refreshing a missing watched folder must fail"
    );
    let snapshot = f.handle.get_import_candidates();
    assert!(snapshot.folder_candidates.iter().any(|candidate| candidate
        .candidate
        .path
        .to_string_lossy()
        == album_key));
    assert!(snapshot.folder_scan_statuses.iter().any(|status| {
        status.watched_folder_path == root_key
            && matches!(
                status.status,
                bae_core::import::FolderScanStatus::Failed { .. }
            )
    }));
}

/// `set_candidate_skipped` flips the reducer's skip flag and broadcasts
/// `CandidateSkipChanged`; a no-op request (already in the target state) changes
/// nothing and emits nothing.
#[tokio::test]
async fn set_candidate_skipped_flips_flag_and_is_idempotent() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let collection = f.temp_path().join("Collection");
    let album = collection.join("Artist - Album");
    fs::create_dir_all(&album).unwrap();
    generate_album_files(&album, &["01 Track.flac"]);
    let album_key = album.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    let batch = scan_batch_until(&mut scan_rx, "the album candidate", |e| {
        matches!(e, ScanEvent::FolderCandidate { candidate: c, .. } if c.path.to_str() == Some(album_key.as_str()))
    })
    .await;
    assert!(batch.added.contains(&album_key));

    let skipped_of = |f: &ImportFixture| {
        f.handle
            .get_import_candidates()
            .folder_candidates
            .iter()
            .find(|c| c.candidate.path == album)
            .expect("candidate present")
            .skipped
    };
    assert!(!skipped_of(&f), "candidate starts unskipped");

    f.handle
        .set_candidate_skipped(album_key.clone(), true)
        .await
        .unwrap();
    assert!(skipped_of(&f), "skip flag flips in the reducer");
    wait_for_scan_event(
        &mut scan_rx,
        "the CandidateSkipChanged broadcast",
        |event| {
            matches!(
                event,
                ScanEvent::CandidateSkipChanged { candidate_key, skipped }
                    if candidate_key == &album_key && *skipped
            )
        },
    )
    .await;

    // A redundant skip=true request is a no-op: no event, flag unchanged.
    f.handle
        .set_candidate_skipped(album_key.clone(), true)
        .await
        .unwrap();
    assert!(skipped_of(&f));
    let events = drain_scan_events(&mut scan_rx, std::time::Duration::from_millis(300)).await;
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ScanEvent::CandidateSkipChanged { .. })),
        "a redundant skip must not re-broadcast, got {events:?}",
    );

    f.handle
        .set_candidate_skipped(album_key.clone(), false)
        .await
        .unwrap();
    assert!(!skipped_of(&f), "unskip flips the flag back");
}

/// 3. Local folder import: album, release, tracks all in DB, files stay in place.
#[tokio::test]
async fn local_folder_import() {
    support::tracing_init();

    let release = discogs_release("Test Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    // Subscribe to library events BEFORE import
    let mut event_rx = f.library_manager.subscribe_events();

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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.clone(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    // Verify release in DB: local (not remote), with its files registered as
    // coven external refs at their in-place import location.
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert!(!release.remote);
    let files = f.db.get_files_for_release(&release_id).await.unwrap();
    let local_path = f
        .library_manager
        .file_local_path(&files[0].id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(local_path.parent().unwrap(), album_dir);

    // Verify tracks
    let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].title, "Track One");
    assert_eq!(tracks[1].title, "Track Two");
    assert_eq!(tracks[2].title, "Track Three");

    // Verify files
    let files = f.db.get_files_for_release(&release_id).await.unwrap();
    assert_eq!(files.len(), 3);

    // Original files still in place (local)
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

/// 4. Import produces correct audio format records.
#[tokio::test]
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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
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
}

#[tokio::test]
async fn exact_metadata_import_stores_dsd_audio_format() {
    support::tracing_init();

    for (index, fixture_name, import_name) in [
        (1, "placeholder-dsd.dsf", "01 Track.dsf"),
        (2, "placeholder-dsd.dff", "01 Track.dff"),
    ] {
        let release = discogs_release(&format!("DSD Format Album {index}"), &["Track"]);
        let release_id_key = seed_discogs_test_release(release);
        let f = ImportFixture::new().await;

        let album_dir = f.temp_path().join("album");
        fs::create_dir_all(&album_dir).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("test-fixtures")
                .join("audio-format")
                .join(fixture_name),
            album_dir.join(import_name),
        )
        .unwrap();

        let import_id = uuid::Uuid::new_v4().to_string();
        f.handle
            .send_command(ImportCommand {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir,
                scope: bae_core::import::ReleaseFileScope::Recursive,
                selected_cover: None,
                storage_mode: StorageMode::Local,
                pin: false,
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
                },
                user_edit: None,
            })
            .await
            .unwrap();

        let mut progress_rx = f.handle.subscribe_import(import_id);
        let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

        let files = f.db.get_files_for_release(&release_id).await.unwrap();
        assert_eq!(files.len(), 1, "{fixture_name}");
        assert_eq!(
            files[0].content_type,
            bae_core::util::content_type::ContentType::Dsd,
            "{fixture_name}"
        );

        let tracks = f.db.get_tracks_for_release(&release_id).await.unwrap();
        assert_eq!(tracks.len(), 1, "{fixture_name}");
        let format =
            f.db.find_audio_format_by_track_id(&tracks[0].id)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            format.content_type,
            bae_core::util::content_type::ContentType::Dsd,
            "{fixture_name}"
        );
    }
}

/// The loudness pass emits a continuous `fraction` (0 → 1) as it scans, ticking
/// ~0.1s of audio at a time rather than once per track, so the import UI bar
/// advances during a track's measure span, not only at track boundaries.
#[tokio::test]
async fn loudness_pass_emits_within_track_progress() {
    use bae_core::import::ImportEvent;

    support::tracing_init();

    let release = discogs_release("Loudness Album", &["Track One", "Track Two", "Track Three"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    // Subscribe to the full import event stream before the import runs; the
    // 1024-slot broadcast buffer holds every tick until we drain it below.
    let mut event_rx = f.handle.subscribe_events();

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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let _ = support::wait_for_import_complete(&mut progress_rx).await;

    // Drain the buffered events; keep the loudness ticks for our candidate in
    // arrival order.
    let mut ticks: Vec<(u32, u32, f32)> = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        if let ImportEvent::ImportLoudnessProgress {
            candidate_key,
            tracks_done,
            tracks_total,
            fraction,
        } = event
        {
            if candidate_key == "test" {
                ticks.push((tracks_done, tracks_total, fraction));
            }
        }
    }

    // Three tracks measured ~0.1s at a time → many ticks, not one per track, so
    // the bar creeps within each track instead of stepping. `fraction` is the
    // overall scan progress: monotonic, starting at 0 and reaching exactly 1.0 so
    // the bar always completes; total is constant; the last track-count is N/N.
    assert!(
        ticks.len() > 4,
        "within-track measurement emits many ticks, not one per track: {} ticks",
        ticks.len()
    );
    assert!(
        ticks.iter().all(|(_, total, _)| *total == 3),
        "every tick reports total=3: {ticks:?}"
    );
    let fractions: Vec<f32> = ticks.iter().map(|(_, _, f)| *f).collect();
    assert!(
        fractions.windows(2).all(|w| w[1] >= w[0]),
        "fraction is monotonic non-decreasing: {fractions:?}"
    );
    assert_eq!(fractions.first().copied(), Some(0.0), "starts at 0");
    assert_eq!(fractions.last().copied(), Some(1.0), "reaches exactly 1.0");
    assert_eq!(
        ticks.last().map(|(d, t, _)| (*d, *t)),
        Some((3, 3)),
        "final tick labels the last track N/N"
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
/// Samples are full-range i32 PCM; the FLAC encoder writes their high 16 bits.
/// A small deterministic dither keeps the stream from compressing below the
/// import's FLAC truncation check (file must be >= 10% of raw PCM); the dither
/// sits ~60 dB down, moving neither the loudness nor the peak.
fn sine(amplitude: f64, sample_rate: u32, secs: f64, spikes: bool) -> Vec<i32> {
    use std::f64::consts::PI;
    let n = (sample_rate as f64 * secs) as usize;
    let spike_period = (sample_rate as f64 * 0.1) as usize;
    let full_scale = i32::MAX;
    let mut rng: u32 = 0x1234_5678;
    let mut dither = || {
        rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (((rng >> 16) as i32 & 0x3F) - 32) << 16
    };
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let s = if spikes && i % spike_period == 0 {
            full_scale
        } else {
            let t = i as f64 / sample_rate as f64;
            ((2.0 * PI * 1000.0 * t).sin() * amplitude * i32::MAX as f64) as i32 + dither()
        };
        out.push(s);
        out.push(s);
    }
    out
}

/// Write a synthetic 16-bit stereo FLAC of `samples` to `path`.
fn write_flac(path: &Path, samples: &[i32], sample_rate: u32) {
    let bytes = bae_core::audio_codec::encode_i32(
        bae_core::audio_codec::EncodeFormat::Flac {
            bits_per_sample: 16,
        },
        samples,
        sample_rate,
        2,
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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
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
        .library_manager
        .resolve_track_audio(&quiet.id)
        .await
        .unwrap();
    let loud_audio = f
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
            .send_command(ImportCommand {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir,
                scope: bae_core::import::ReleaseFileScope::Recursive,
                selected_cover: None,
                storage_mode: StorageMode::Local,
                pin: false,
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_keys[i].clone(), MetadataSource::Discogs),
                },
                user_edit: None,
            })
            .await
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
}

#[tokio::test]
async fn reimport_cover_download_failure_preserves_prior_release() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("cover-failure-reimport");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01 Track Title.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );

    let (prior_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await
    .expect("initial import succeeds");
    assert_release_has_external_ref(&f, &prior_release_id).await;

    let result = import_folder(
        &f,
        &album_dir,
        Some(CoverSelection::Remote(
            "http://127.0.0.1:9/cover.jpg".to_string(),
            MetadataSource::MusicBrainz,
        )),
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await;

    assert!(result.is_err(), "cover download should fail");
    assert_release_has_external_ref(&f, &prior_release_id).await;
}

#[tokio::test]
async fn reimport_decode_verification_failure_preserves_prior_release() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.set_decode_verification(false);

    let album_dir = f.temp_path().join("decode-failure-reimport");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01 Track Title.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );
    truncate_flac_body(&album_dir.join("01 Track Title.flac"));

    let (prior_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await
    .expect("initial import succeeds while decode verification is disabled");
    assert_release_has_external_ref(&f, &prior_release_id).await;

    f.set_decode_verification(true);
    let result = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await;

    let error = result.expect_err("decode verification should fail");
    assert!(
        error.contains("decode verification failed"),
        "unexpected error: {error}"
    );
    assert_release_has_external_ref(&f, &prior_release_id).await;
}

#[tokio::test]
async fn successful_reimport_replaces_prior_release_once() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    f.connect_cloud().await;

    let album_dir = f.temp_path().join("successful-reimport");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Album Title",
        "Artist Name",
        None,
        &[TaggedTrack {
            filename: "01 Track Title.flac",
            title: "Track Title",
            track_number: 1,
        }],
    );

    let (prior_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Remote,
        IdentityChoice::Unknown,
    )
    .await
    .expect("initial remote import queues upload");
    let upload_count = f
        .library_manager
        .drain_uploads_expecting_work()
        .await
        .unwrap();
    assert_eq!(
        upload_count, 1,
        "initial remote import should upload one file"
    );
    let prior_release =
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .expect("prior release exists after upload");
    assert!(prior_release.remote, "prior release should be remote");
    let content_hash = prior_release.content_hash.clone().unwrap();

    let (replacement_release_id, _) = import_folder(
        &f,
        &album_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await
    .expect("re-import succeeds");

    assert!(
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .is_none(),
        "prior release should be replaced"
    );
    let release_ids =
        f.db.release_ids_for_content_hash(&content_hash)
            .await
            .unwrap();
    assert_eq!(
        release_ids,
        vec![replacement_release_id],
        "one release should carry the re-imported content hash"
    );
    assert_eq!(
        f.db.handle().queued_deletes().await.unwrap().len(),
        1,
        "replacing the prior remote release should queue its cloud blob for deletion"
    );
}

/// The remote-transition rollback: the mirror of the local unit test
/// `failed_import_before_finalize_leaves_only_import_audit_row`, but one stage
/// later. The release is finalized (status Importing), then the cloud
/// transition fails and `run_import` calls `fail_import_and_delete_release`. A
/// Remote import with no sync provider connected fails at exactly that point
/// (`coven_make_remote` returns `SyncNotReady`) — the honest injection for a
/// post-finalize transition failure, since the upload itself is deferred to the
/// drain and never runs synchronously. The rollback must delete the
/// just-finalized release and its album, mark the import Failed with its release
/// link cleared, and leave a pre-existing release untouched.
#[tokio::test]
async fn remote_transition_failure_rolls_back_finalized_release() {
    support::tracing_init();
    // No cloud/sync connected, so the make-Remote transition fails.
    let f = ImportFixture::new().await;

    // A prior local release already in the library; the failed remote import
    // below must not touch it.
    let prior_dir = f.temp_path().join("prior");
    fs::create_dir_all(&prior_dir).unwrap();
    generate_tagged_album_files(
        &prior_dir,
        "Prior Album",
        "Prior Artist",
        None,
        &[TaggedTrack {
            filename: "01 Prior Track.flac",
            title: "Prior Track",
            track_number: 1,
        }],
    );
    let (prior_release_id, _) = import_folder(
        &f,
        &prior_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Unknown,
    )
    .await
    .expect("prior local import succeeds");

    // The remote import: finalize commits the release (status Importing), then
    // coven_make_remote fails because sync was never connected.
    let album_dir = f.temp_path().join("remote");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Remote Album",
        "Remote Artist",
        None,
        &[TaggedTrack {
            filename: "01 Remote Track.flac",
            title: "Remote Track",
            track_number: 1,
        }],
    );

    let import_id = f.ids.new_id();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Remote,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id.clone());
    let error = support::try_wait_for_import_complete(&mut progress_rx)
        .await
        .expect_err("remote transition without a sync provider fails");
    assert!(error.contains("cloud upload"), "unexpected error: {error}");

    // The rollback deleted the finalized remote release, its album, and the
    // artist row that finalize inserted for it; only the prior release, album,
    // and artist remain. The remote import's artist is referenced by nothing
    // else, so leaving it behind would orphan a row on every failed remote
    // import.
    let (release_count, album_count, artist_count): (i64, i64, i64) =
        f.db.handle()
            .sql_read(|conn| {
                Ok::<_, coven::CovenError>((
                    conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0))?,
                    conn.query_row("SELECT COUNT(*) FROM albums", [], |row| row.get(0))?,
                    conn.query_row("SELECT COUNT(*) FROM artists", [], |row| row.get(0))?,
                ))
            })
            .await
            .unwrap();
    assert_eq!(release_count, 1, "only the prior release remains");
    assert_eq!(album_count, 1, "only the prior album remains");
    assert_eq!(
        artist_count, 1,
        "only the prior artist remains; the rolled-back import's artist row is gone",
    );
    assert!(
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .is_some(),
        "the prior release is untouched by the failed remote import",
    );
}

/// The MusicBrainz work MBIDs the rollback test seeds. MusicBrainz mints work
/// MBIDs as name-based (version 3) UUIDs, so that is the shape here.
const MB_WORK_SHARED: &str = "8f0a1c2d-3e4b-3a5c-9d6e-7f8091a2b3c4";
const MB_WORK_EXCLUSIVE: &str = "1b2c3d4e-5f60-3718-8293-a4b5c6d7e8f9";
const MB_WORK_PART: &str = "2c3d4e5f-6071-3829-93a4-b5c6d7e8f901";

/// An MbWork with a composer relation and, optionally, one child part.
fn work_with_composer(id: &str, title: &str, composer_mb: &str, part: Option<MbWork>) -> MbWork {
    let mut relations = vec![MbRelation {
        target_type: Some("artist".to_string()),
        relation_type: Some("composer".to_string()),
        artist: Some(MbArtistRef {
            id: Some(composer_mb.to_string()),
            name: Some(format!("Composer {composer_mb}")),
            sort_name: Some(format!("Composer {composer_mb}")),
        }),
        target_credit: Some(format!("Composer {composer_mb}")),
        ..MbRelation::default()
    }];
    if let Some(part) = part {
        relations.push(MbRelation {
            target_type: Some("work".to_string()),
            relation_type: Some("parts".to_string()),
            direction: Some("forward".to_string()),
            work: Some(part),
            ..MbRelation::default()
        });
    }
    MbWork {
        id: id.to_string(),
        title: title.to_string(),
        disambiguation: None,
        work_type: Some("work".to_string()),
        relations,
    }
}

/// An MbTrack whose recording performs `work`.
fn mb_track_performing(position: i64, title: &str, work: MbWork) -> MbTrack {
    MbTrack {
        position: Some(position),
        number: Some(position.to_string()),
        title: Some(title.to_string()),
        length: None,
        recording: Some(MbRecording {
            id: Some(format!("rec-{position}-{}", work.id)),
            title: Some(title.to_string()),
            artist_credit: vec![],
            relations: vec![MbRelation {
                target_type: Some("work".to_string()),
                relation_type: Some("performance".to_string()),
                work: Some(work),
                ..MbRelation::default()
            }],
        }),
        artist_credit: vec![],
    }
}

/// Seed a MusicBrainz release (no Discogs cross-link) whose recordings perform
/// the given work graph. Returns the MB release id.
fn seed_mb_release_with_works(
    mb_release_id: &str,
    mb_group_id: &str,
    title: &str,
    tracks: Vec<MbTrack>,
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
            tracks,
        }],
        relations: vec![],
    };
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(mb_release_id, (response, None, raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    mb_release_id.to_string()
}

/// Work-graph sibling of `remote_transition_failure_rolls_back_finalized_release`.
/// A classical MusicBrainz import finalizes works, work_parts, work_artists
/// (composers), and track_works; a post-finalize transition failure must roll
/// all of that back too. Otherwise every failed classical remote import leaks
/// orphaned works, and the composer artist rows those surviving work_artists
/// keep alive slip past the artist-rollback guard. A work still performed by a
/// surviving release — and its composer — must be left alone.
#[tokio::test]
async fn remote_transition_failure_rolls_back_finalized_works() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    // A prior LOCAL MusicBrainz import that survives. Its recording performs
    // the shared work, so that work and its composer are referenced by a
    // release the failed remote import below must not touch.
    let prior_mb = seed_mb_release_with_works(
        "mb-rel-prior",
        "mb-group-prior",
        "Prior Symphony",
        vec![mb_track_performing(
            1,
            "Prior Movement",
            work_with_composer(MB_WORK_SHARED, "Shared Work", "composer-shared", None),
        )],
    );
    f.cover_art
        .seed_lookup(Some(&prior_mb), Some("mb-group-prior"), None);
    f.cover_art
        .seed_lookup(Some("mb-rel-remote"), Some("mb-group-remote"), None);
    let prior_dir = f.temp_path().join("prior");
    fs::create_dir_all(&prior_dir).unwrap();
    generate_album_files(&prior_dir, &["01 Prior Movement.flac"]);
    let (prior_release_id, _) = import_folder(
        &f,
        &prior_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Exact {
            release_ref: MetadataRef::new(prior_mb, MetadataSource::MusicBrainz),
        },
    )
    .await
    .expect("prior local MB import succeeds");

    // The failing REMOTE import: one recording performs the shared work (also
    // performed by the prior release), the other an exclusive work that has its
    // own child part and its own composer.
    let remote_mb = seed_mb_release_with_works(
        "mb-rel-remote",
        "mb-group-remote",
        "Remote Symphony",
        vec![
            mb_track_performing(
                1,
                "Remote Movement One",
                work_with_composer(MB_WORK_SHARED, "Shared Work", "composer-shared", None),
            ),
            mb_track_performing(
                2,
                "Remote Movement Two",
                work_with_composer(
                    MB_WORK_EXCLUSIVE,
                    "Exclusive Work",
                    "composer-exclusive",
                    Some(MbWork {
                        id: MB_WORK_PART.to_string(),
                        title: "Exclusive Part".to_string(),
                        disambiguation: None,
                        work_type: Some("part".to_string()),
                        relations: vec![],
                    }),
                ),
            ),
        ],
    );
    let album_dir = f.temp_path().join("remote");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(
        &album_dir,
        &["01 Remote Movement One.flac", "02 Remote Movement Two.flac"],
    );

    let import_id = f.ids.new_id();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Remote,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(remote_mb, MetadataSource::MusicBrainz),
            },
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let error = support::try_wait_for_import_complete(&mut progress_rx)
        .await
        .expect_err("remote transition without a sync provider fails");
    assert!(error.contains("cloud upload"), "unexpected error: {error}");

    let (
        shared_work,
        exclusive_work,
        part_work,
        work_parts_count,
        shared_composer,
        exclusive_composer,
        orphan_work_artists,
    ): (bool, bool, bool, i64, bool, bool, i64) =
        f.db.handle()
            .sql_read(|conn| {
                let exists = |q: &str| -> coven::rusqlite::Result<bool> {
                    Ok(conn.query_row(q, [], |r| r.get::<_, i64>(0))? > 0)
                };
                Ok::<_, coven::CovenError>((
                exists(&format!(
                    "SELECT COUNT(*) FROM works WHERE musicbrainz_work_id = '{MB_WORK_SHARED}'"
                ))?,
                exists(&format!(
                    "SELECT COUNT(*) FROM works WHERE musicbrainz_work_id = '{MB_WORK_EXCLUSIVE}'"
                ))?,
                exists(&format!(
                    "SELECT COUNT(*) FROM works WHERE musicbrainz_work_id = '{MB_WORK_PART}'"
                ))?,
                conn.query_row("SELECT COUNT(*) FROM work_parts", [], |r| r.get(0))?,
                exists(
                    "SELECT COUNT(*) FROM artists WHERE musicbrainz_artist_id = 'composer-shared'",
                )?,
                exists(
                    "SELECT COUNT(*) FROM artists \
                     WHERE musicbrainz_artist_id = 'composer-exclusive'",
                )?,
                conn.query_row(
                    "SELECT COUNT(*) FROM work_artists wa \
                     WHERE wa.work_id NOT IN (SELECT work_id FROM track_works)",
                    [],
                    |r| r.get(0),
                )?,
            ))
            })
            .await
            .unwrap();

    assert!(
        shared_work,
        "the shared work stays; the prior release still performs it",
    );
    assert!(shared_composer, "the shared work's composer stays");
    assert!(
        !exclusive_work,
        "the failed import's exclusive work is deleted",
    );
    assert!(!part_work, "the exclusive work's child part is deleted");
    assert_eq!(
        work_parts_count, 0,
        "no work_parts rows survive the rollback"
    );
    assert!(
        !exclusive_composer,
        "the exclusive composer, referenced by nothing else, is deleted",
    );
    assert_eq!(
        orphan_work_artists, 0,
        "no work_artists point at a work with no surviving track link",
    );

    assert!(
        f.db.find_release_by_id(&prior_release_id)
            .await
            .unwrap()
            .is_some(),
        "the prior release is untouched by the failed remote import",
    );
}

/// A MusicBrainz work MBID is frequently a name-based (version 3) UUID, which
/// coven's synced-row id policy refuses. A `works` row therefore carries a
/// minted id and keeps the MBID in `musicbrainz_work_id`.
const V3_WORK: &str = "e0dc7948-f188-3346-adee-db5cfb6361a9";

/// Two releases performing the same work land one `works` row, keyed on the
/// MBID, with an id the sync layer accepts.
#[tokio::test]
async fn work_mbid_is_stored_beside_a_minted_row_id_and_shared_across_releases() {
    support::tracing_init();
    let f = ImportFixture::new().await;

    let first_mb = seed_mb_release_with_works(
        "mb-rel-first",
        "mb-group-first",
        "First Release",
        vec![mb_track_performing(
            1,
            "First Movement",
            work_with_composer(V3_WORK, "Shared Work", "composer-shared", None),
        )],
    );
    f.cover_art
        .seed_lookup(Some(&first_mb), Some("mb-group-first"), None);
    let first_dir = f.temp_path().join("first");
    fs::create_dir_all(&first_dir).unwrap();
    generate_album_files(&first_dir, &["01 First Movement.flac"]);
    let (first_release_id, _) = import_folder(
        &f,
        &first_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Exact {
            release_ref: MetadataRef::new(first_mb, MetadataSource::MusicBrainz),
        },
    )
    .await
    .expect("first local MB import succeeds");

    let second_mb = seed_mb_release_with_works(
        "mb-rel-second",
        "mb-group-second",
        "Second Release",
        vec![mb_track_performing(
            1,
            "Second Movement",
            work_with_composer(V3_WORK, "Shared Work", "composer-shared", None),
        )],
    );
    f.cover_art
        .seed_lookup(Some(&second_mb), Some("mb-group-second"), None);
    let second_dir = f.temp_path().join("second");
    fs::create_dir_all(&second_dir).unwrap();
    generate_album_files(&second_dir, &["01 Second Movement.flac"]);
    let (second_release_id, _) = import_folder(
        &f,
        &second_dir,
        None,
        StorageMode::Local,
        IdentityChoice::Exact {
            release_ref: MetadataRef::new(second_mb, MetadataSource::MusicBrainz),
        },
    )
    .await
    .expect("second local MB import succeeds");

    let (work_ids, linked_release_ids): (Vec<String>, Vec<String>) =
        f.db.handle()
            .sql_read(move |conn| {
                let work_ids = conn.query(
                    "SELECT id FROM works WHERE musicbrainz_work_id = ?",
                    [V3_WORK],
                    |r| r.get::<_, String>(0),
                )?;
                let linked_release_ids = conn.query(
                    "SELECT DISTINCT t.release_id FROM track_works tw \
                     JOIN tracks t ON t.id = tw.track_id \
                     JOIN works w ON w.id = tw.work_id \
                     WHERE w.musicbrainz_work_id = ? ORDER BY t.release_id",
                    [V3_WORK],
                    |r| r.get::<_, String>(0),
                )?;
                Ok::<_, coven::CovenError>((work_ids, linked_release_ids))
            })
            .await
            .unwrap();

    assert_eq!(
        work_ids.len(),
        1,
        "the work the two releases share is one row",
    );
    assert_ne!(
        work_ids[0], V3_WORK,
        "the row id is minted, not the MusicBrainz MBID",
    );
    let mut expected = vec![first_release_id, second_release_id];
    expected.sort();
    assert_eq!(
        linked_release_ids, expected,
        "both releases' track_works point at the one work row",
    );
}

/// coven verifies a blob's plaintext against its row's content hash on every
/// cloud fetch, so a `covers` row must describe the bytes actually stored — the
/// resized thumbnail the import writes, never the image it was made from. A row
/// hashing anything else makes the cover unreadable on every other device.
async fn assert_cover_row_describes_stored_bytes(f: &ImportFixture, release_id: &str) {
    let cover =
        f.db.find_library_image(release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row");
    let bytes = support::read_cover_image_blob(&f.db, &f.library_manager, release_id)
        .await
        .expect("cover blob readable");
    assert_eq!(
        cover.content_hash.as_str(),
        bae_core::util::fs::hash_bytes(&bytes).as_str(),
        "the covers row's hash must be the hash of the stored blob",
    );
    assert_eq!(
        cover.file_size,
        bytes.len() as i64,
        "the covers row's file_size must be the size of the stored blob",
    );
}

/// 6. Import with local cover art: covers row + the cover blob in coven's local store.
#[tokio::test]
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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: Some(CoverSelection::Local("scans/back.jpg".to_string())),
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
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

    // Cover bytes readable through the image ref path (coven's local store while Local).
    let cover_bytes = support::read_cover_image_blob(&f.db, &f.library_manager, &release_id)
        .await
        .expect("cover blob should be readable");
    assert!(!cover_bytes.is_empty(), "cover bytes should not be empty");

    assert_cover_row_describes_stored_bytes(&f, &release_id).await;
}

/// An oversized folder cover is resized to a ≤600 JPEG thumbnail at import: the
/// stored blob decodes to 600×600 JPEG (from a 1000×1000 PNG source) and the
/// `covers` row records JPEG, not the source PNG.
#[tokio::test]
async fn import_resizes_oversized_cover_to_jpeg_thumbnail() {
    support::tracing_init();

    let release = discogs_release("Oversized Cover Album", &["Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let cover_path = generate_album_with_oversized_cover(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: Some(CoverSelection::Local(cover_path)),
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    // The row records the stored format (JPEG), not the source PNG.
    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover image in DB");
    assert_eq!(
        cover.content_type,
        bae_core::util::content_type::ContentType::Jpeg
    );

    // The stored blob is a ≤600 JPEG downscaled from the 1000×1000 PNG source.
    let cover_bytes = support::read_cover_image_blob(&f.db, &f.library_manager, &release_id)
        .await
        .expect("cover blob should be readable");
    assert_eq!(
        image::guess_format(&cover_bytes).unwrap(),
        image::ImageFormat::Jpeg
    );
    let decoded = image::load_from_memory(&cover_bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (600, 600));

    assert_cover_row_describes_stored_bytes(&f, &release_id).await;
}

/// On a browsable home the readable cloud_path is computed AT IMPORT — for every
/// release file and for the cover — and stored on its row, so coven keys the blob
/// readably when the gate flips. (An opaque home leaves them NULL; covered by the
/// other imports, which assert nothing here.)
#[tokio::test]
async fn import_on_browsable_home_writes_readable_cloud_paths_at_import() {
    support::tracing_init();

    let release = discogs_release("Browsable Album", &["Track"]);
    let release_id_key = seed_discogs_test_release(release);
    let f = ImportFixture::new().await;
    // Make the home browsable BEFORE importing, so finalize computes readable keys.
    f.library_manager
        .set_home_storage(bae_core::config::HomeStorage::Browsable);

    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_with_cover(&album_dir, &["01 Track.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: Some(CoverSelection::Local("scans/back.jpg".to_string())),
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let album_id =
        f.db.find_release_by_id(&release_id)
            .await
            .unwrap()
            .unwrap()
            .album_id;
    let prefix = format!("{album_id}/{release_id}/");

    // Every release file carries a readable cloud_path under the release prefix.
    let files = f.db.get_files_for_release(&release_id).await.unwrap();
    assert!(!files.is_empty(), "the import recorded release files");
    for file in &files {
        // The readable key is `{album}/{release}/{source_folder}/{original_filename}`,
        // and `original_filename` is the file's path within the release folder — so a
        // `scans/back.jpg` image file keys under a `scans/` level, mirroring the
        // folder the release was imported from.
        let cp = file.cloud_path.as_deref().unwrap_or_else(|| {
            panic!(
                "file {} has no cloud_path on a browsable home",
                file.original_filename
            )
        });
        assert!(
            cp.starts_with(&prefix),
            "audio cloud_path {cp} is under the release prefix {prefix}",
        );
        assert!(
            cp.ends_with(&file.original_filename),
            "audio cloud_path {cp} ends with the file's stored path {}",
            file.original_filename,
        );
    }

    // The cover carries its readable cloud_path too. It names the cover's blob, not
    // just the release, so a later cover change writes a new object rather than
    // overwriting this one.
    let cover =
        f.db.find_library_image(&release_id, &LibraryImageType::Cover)
            .await
            .unwrap()
            .expect("cover row present");
    assert_eq!(
        cover.cloud_path.as_deref(),
        Some(format!("{prefix}cover-{}.jpg", cover.blob_id).as_str()),
        "the cover's readable cloud_path is computed at import and names its blob",
    );
}

// ── identity-choice + user-edit at commit ──────────────────────────────────

/// A Discogs release with rich pressing fields, used to assert that Exact
/// preserves them and Approximate clears them.
fn discogs_release_rich(title: &str, master_id: &str, tracks: &[&str]) -> DiscogsRelease {
    DiscogsRelease {
        id: synthetic_release_id(title),
        title: title.to_string(),
        year: Some(1996),
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
        extraartists: Some(vec![]),
        tracklist: tracks
            .iter()
            .enumerate()
            .map(|(i, t)| DiscogsTrack {
                type_: "track".to_string(),
                position: format!("{}", i + 1),
                title: t.to_string(),
                duration: Some("3:00".to_string()),
                artists: vec![],
                extraartists: None,
            })
            .collect(),
        master_id: Some(master_id.to_string()),
    }
}

/// Exact import: identity row carries `source_release_id`; pressing fields
/// (year, format, label, catalog number, country) seed from the picked
/// release.
#[tokio::test]
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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key.clone(), MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
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
    assert_eq!(
        identities[0].source_group_id,
        support::discogs_fixture_id("master-exact")
    );
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some(release_id_key.as_str())
    );
}

/// User-edit overlay applies on top of the Approximate seed: the user
/// can fill country (cleared by Approximate) and the committed value
/// reflects the edit.
#[tokio::test]
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
            file: None,
        }],
    };

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(release_id_key.clone(), MetadataSource::Discogs),
            },
            user_edit: Some(edit),
        })
        .await
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

/// Seed a Discogs release the MB-rooted import path will resolve via MB
/// url-rels. Returns the Discogs release id.
///
/// The document is rendered as the release endpoint's own JSON and read back
/// through the production parser: it is what the import archives, and every
/// later projection replays from the archived bytes rather than from anything
/// handed over beside them. `master_id` is the fixture's own spelling, rendered
/// numerically as the endpoint numbers its ids.
fn seed_discogs_for_xref(release_id: &str, master_id: &str, title: &str) -> String {
    let rendered_master = support::discogs_fixture_id(master_id);
    bae_core::discogs::client::seed_master_cache(
        &rendered_master,
        Some(1996),
        serde_json::json!({ "id": rendered_master, "year": 1996 }).to_string(),
    );
    let raw_json = serde_json::json!({
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "title": title,
        "year": 1996,
        "country": "US",
        "master_id": rendered_master.parse::<u64>().expect("a rendered master id is numeric"),
        "labels": [{ "name": "Label Name", "catno": "CAT-001" }],
        "formats": [{ "name": "CD" }],
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [{
            "position": "1",
            "title": "Track One",
            "duration": "3:00",
            "type_": "track",
            "artists": [],
        }],
    })
    .to_string();
    let parsed = bae_core::discogs::client::parse_discogs_release_json(&raw_json)
        .expect("the rendered Discogs release parses");
    bae_core::discogs::client::seed_release_cache(release_id, (parsed, raw_json));
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
                    id: None,
                    title: Some("Track One".to_string()),
                    artist_credit: vec![],
                    relations: vec![],
                }),
                artist_credit: vec![],
            }],
        }],
        relations: vec![],
    };
    let discogs_url = Some(format!(
        "https://www.discogs.com/release/{}",
        discogs_release_id
    ));
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(mb_release_id, (response, discogs_url, raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    mb_release_id.to_string()
}

/// MB-rooted Exact import with a Discogs cross-link writes two identity
/// rows, both carrying their per-source `source_release_id`.
#[tokio::test]
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
    f.cover_art
        .seed_lookup(Some(&mb_id), Some("xref-mb-group-exact"), None);
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(mb_id.clone(), MetadataSource::MusicBrainz),
            },
            user_edit: None,
        })
        .await
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
    assert_eq!(
        discogs.source_group_id,
        support::discogs_fixture_id("xref-d-master-exact")
    );
    assert_eq!(
        discogs.source_release_id.as_deref(),
        Some(discogs_id.as_str())
    );
}

/// MB-rooted Approximate import with a Discogs cross-link still writes
/// two identity rows, but both have `source_release_id = NULL` — the
/// user's "I don't claim a specific pressing" applies across sources.
#[tokio::test]
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
    f.cover_art
        .seed_lookup(Some(&mb_id), Some("xref-mb-group-approx"), None);
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(mb_id.clone(), MetadataSource::MusicBrainz),
            },
            user_edit: None,
        })
        .await
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
    assert_eq!(
        discogs.source_group_id,
        support::discogs_fixture_id("xref-d-master-approx")
    );
    assert!(
        discogs.source_release_id.is_none(),
        "Discogs row release_id should be NULL for Approximate, got {:?}",
        discogs.source_release_id,
    );
}

/// Discogs-rooted Approximate import. MB has a back-link to this Discogs
/// release; the Discogs mapper emits both rows. Approximate clears
/// `source_release_id` on both identity rows and clears the pressing fields the
/// Discogs seed supplied on the release row, but keeps
/// `metadata_source_release_id` so a later re-projection can replay the seed.
#[tokio::test]
async fn cross_source_discogs_rooted_approximate_nulls_both_release_ids() {
    support::tracing_init();

    let discogs_id = seed_discogs_for_xref("90000003", "xref-drooted-d-master", "Album Title");
    // The Discogs commit path reaches MB via the URL-lookup cache.
    let mb_release_id = "xref-drooted-mb-rel".to_string();
    let mb_group_id = "xref-drooted-mb-group".to_string();
    bae_core::musicbrainz::seed_discogs_url_lookup(&discogs_id, Some(mb_release_id.clone()));
    let mb_response = MbReleaseResponse {
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
    };
    let mb_raw_json = serde_json::to_string(&mb_response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(&mb_release_id, (mb_response, None, mb_raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        &mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Approximate {
                release_ref: MetadataRef::new(discogs_id.clone(), MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _) = support::wait_for_import_complete(&mut progress_rx).await;

    let identities = f.db.get_release_identities(&release_id).await.unwrap();
    assert_eq!(identities.len(), 2, "expected Discogs + MB identity rows");

    let discogs = identities
        .iter()
        .find(|i| i.source == MetadataSource::Discogs)
        .expect("Discogs identity row missing");
    assert_eq!(
        discogs.source_group_id,
        support::discogs_fixture_id("xref-drooted-d-master")
    );
    assert!(discogs.source_release_id.is_none());

    let mb = identities
        .iter()
        .find(|i| i.source == MetadataSource::MusicBrainz)
        .expect("MB identity row missing");
    assert_eq!(mb.source_group_id, mb_group_id);
    assert!(mb.source_release_id.is_none());

    // The release row: Approximate clears the pressing fields the Discogs seed
    // supplied, but keeps metadata_source_release_id so a later re-projection can
    // replay the seed.
    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
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
    assert_eq!(
        release.metadata_source_release_id.as_deref(),
        Some(discogs_id.as_str()),
        "Approximate keeps metadata_source_release_id for re-projection"
    );
}

// ── "Add as Unknown" ────────────────────────────────────────────────────────

/// Unknown commit reads embedded tags, writes zero `release_identities`
/// rows, sets `metadata_source = file_tags`, leaves
/// `metadata_source_release_id = NULL`, and seeds the album / tracks
/// from what's on disk. No external source consulted.
#[tokio::test]
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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
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

#[tokio::test]
async fn unknown_preview_for_cue_matches_unknown_commit_layout() {
    support::tracing_init();

    let f = ImportFixture::new().await;
    let album_dir = f.temp_path().join("cue-album");
    fs::create_dir_all(&album_dir).unwrap();
    copy_cue_flac_fixture(&album_dir);

    let candidate_key = album_dir.to_string_lossy().into_owned();
    f.handle
        .add_watched_folder(candidate_key.clone())
        .await
        .unwrap();
    f.handle
        .refresh_watched_folder(candidate_key.clone())
        .await
        .unwrap();
    let preview = f
        .handle
        .preview_file_tags_for_folder(candidate_key)
        .await
        .unwrap();

    assert_eq!(preview.album_title, "Test Album");
    assert_eq!(preview.album_artist_names, vec!["Test Artist".to_string()]);
    assert_eq!(preview.pressing.year, None);
    assert_eq!(preview.pressing.format.as_deref(), Some("FLAC"));

    let preview_tracks: Vec<(String, Vec<String>)> = preview
        .tracks
        .iter()
        .map(|t| (t.title.clone(), t.artist_names.clone()))
        .collect();
    assert_eq!(
        preview_tracks,
        vec![
            (
                "Track One (Silence)".to_string(),
                vec!["Test Artist".to_string()],
            ),
            (
                "Track Two (White Noise)".to_string(),
                vec!["Test Artist".to_string()],
            ),
            (
                "Track Three (Brown Noise)".to_string(),
                vec!["Test Artist".to_string()],
            ),
        ],
    );

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "cue".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();

    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, album_id) = support::wait_for_import_complete(&mut progress_rx).await;
    let album = f.db.find_album_by_id(&album_id).await.unwrap().unwrap();
    assert_eq!(preview.album_title, album.title);

    let release = f.db.find_release_by_id(&release_id).await.unwrap().unwrap();
    assert_eq!(preview.pressing.year, release.pressing.year);
    assert_eq!(preview.pressing.format, release.pressing.format);

    let album_detail =
        f.db.find_album_detail(&album_id)
            .await
            .unwrap()
            .expect("album detail");
    let committed_album_artist_names: Vec<String> = album_detail
        .artists
        .iter()
        .map(|a| a.name.clone())
        .collect();
    assert_eq!(preview.album_artist_names, committed_album_artist_names);

    let release_detail =
        f.db.find_release_detail(&release_id)
            .await
            .unwrap()
            .expect("release detail");
    let committed_tracks: Vec<(String, Vec<String>)> = release_detail
        .tracks
        .iter()
        .map(|track| {
            (
                track.track.title.clone(),
                track.artists.iter().map(|a| a.name.clone()).collect(),
            )
        })
        .collect();
    assert_eq!(preview_tracks, committed_tracks);
}

/// A tagged rip whose only artwork is embedded in the audio (no folder
/// image, no remote selection) gets that embedded picture as its cover.
#[tokio::test]
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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
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

    // The embedded picture (≤600) keeps its dimensions but the store path
    // re-encodes it to JPEG, so assert on the decoded image, not raw bytes.
    let bytes = support::read_cover_image_blob(&f.db, &f.library_manager, &release_id)
        .await
        .expect("cover blob readable");
    assert_eq!(
        image::guess_format(&bytes).unwrap(),
        image::ImageFormat::Jpeg
    );
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), EMBEDDED_COVER_DIMS);

    assert_cover_row_describes_stored_bytes(&f, &release_id).await;
}

/// A folder image outranks the embedded picture: when both exist, the
/// folder artwork is the cover and the embedded picture is ignored.
#[tokio::test]
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
    fs::write(scans.join("cover.jpg"), embedded_cover_jpeg()).unwrap();

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "identified".to_string(),
            folder: identified_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
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
        .send_command(ImportCommand {
            import_id: import_id2.clone(),
            candidate_key: "unknown".to_string(),
            folder: unknown_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
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
            file: None,
        }],
    };

    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: Some(edit),
        })
        .await
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
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
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

/// Truncate a FLAC's body on disk while keeping its header and tags: a valid
/// STREAMINFO (declaring the full sample count) over a short audio body. Kept
/// size stays above the scanner's gross-size floor (10% of the raw PCM size, ~44
/// KB for the 5s mono fixture) so the file passes scan and reaches the loudness
/// loop, but far too little audio to decode the declared samples -- the
/// decode-verify shortfall signature.
fn truncate_flac_body(path: &Path) {
    let bytes = fs::read(path).expect("read flac to truncate");
    // The 5s/44.1k/mono/16-bit fixture declares 220_500 samples => 441_000 raw
    // bytes; the scan rejects below 44_100. 46_000 clears that while cutting the
    // bulk of the ~68 KB audio body.
    let keep = 46_000usize.min(bytes.len());
    assert!(
        bytes.len() > keep,
        "fixture is smaller than the truncation target ({} bytes)",
        bytes.len(),
    );
    fs::write(path, &bytes[..keep]).expect("write truncated flac");
}

/// Import a one-track album whose FLAC is truncated (valid header, short body),
/// with `verify_decode_on_import` set to `verify`. Returns the import outcome.
async fn import_truncated_album(verify: bool) -> Result<(String, String), String> {
    let temp = TempDir::new().unwrap();
    let db_dir = temp.path().join("db");
    fs::create_dir_all(&db_dir).unwrap();
    let db = Database::new_test(
        db_dir.join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let library_dir = StoreDir::new(db_dir.clone());
    let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
    config_handle
        .update(|c| c.verify_decode_on_import = verify)
        .expect("set verify_decode_on_import");
    let library_manager = LibraryManager::new(
        db.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::CoverArtArchiveClient::hermetic(),
    );
    let handle = ImportService::start(tokio::runtime::Handle::current(), library_manager)
        .await
        .expect("import service starts");

    let album_dir = temp.path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    generate_tagged_album_files(
        &album_dir,
        "Broken Album",
        "Broken Artist",
        None,
        &[TaggedTrack {
            filename: "01.flac",
            title: "Broken Track",
            track_number: 1,
        }],
    );
    truncate_flac_body(&album_dir.join("01.flac"));

    let import_id = uuid::Uuid::new_v4().to_string();
    handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = handle.subscribe_import(import_id);
    let result = support::try_wait_for_import_complete(&mut progress_rx).await;
    // Keep the temp dir (and its files) alive until the import has finished.
    drop(temp);
    result
}

/// `verify_decode_on_import` gates a broken track end to end: a truncated FLAC
/// (valid header, short body) that decodes short imports fine with the flag off,
/// and fails at the decode-verify gate -- before finalize commits anything -- with
/// the flag on. Proves the flag drives the outcome, not the fixture.
#[tokio::test]
async fn verify_decode_on_import_gates_a_broken_track() {
    support::tracing_init();

    // Flag off: the import does not assert on decode integrity, so the broken
    // album still imports.
    let off = import_truncated_album(false).await;
    assert!(
        off.is_ok(),
        "with verify_decode_on_import off, a broken album must still import, got: {off:?}",
    );

    // Flag on (the default): the same album fails at the decode-verify gate.
    let on = import_truncated_album(true).await;
    let err = on.expect_err("with verify_decode_on_import on, a broken album must fail the import");
    assert!(
        err.contains("decode verification failed"),
        "the failure must come from decode-verify, got: {err}",
    );
}

// ── album artists survive the confirmation editor ────────────────────────

/// A MusicBrainz release credited to two artists, one CD track.
fn seed_two_credit_mb_release(mb_release_id: &str, mb_group_id: &str) -> String {
    let credit = |id: &str, name: &str| MbArtistCredit {
        name: name.to_string(),
        artist: Some(MbArtistRef {
            id: Some(id.to_string()),
            name: Some(name.to_string()),
            sort_name: Some(name.to_string()),
        }),
    };
    let response = MbReleaseResponse {
        id: mb_release_id.to_string(),
        title: "Split Album".to_string(),
        date: Some("1999".to_string()),
        country: Some("US".to_string()),
        barcode: None,
        artist_credit: vec![
            credit("mb-artist-a", "Artist A"),
            credit("mb-artist-b", "Artist B"),
        ],
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
                    id: None,
                    title: Some("Track One".to_string()),
                    artist_credit: vec![],
                    relations: vec![],
                }),
                artist_credit: vec![],
            }],
        }],
        relations: vec![],
    };
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(mb_release_id, (response, None, raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    mb_release_id.to_string()
}

/// A release credited to two artists keeps both through the confirmation editor:
/// the primary on `albums.artist_id`, the second as an `album_artists` junction
/// row still carrying its MusicBrainz id.
///
/// The editor seeds from the same projection the commit worker maps, so a user
/// who changes nothing sends back the artist list the mapper produced, and the
/// commit's artist comparison sees no edit. Seeding it from the picker's display
/// shape — which collapses the credits to one name — read as "the user deleted
/// artist B" and destroyed her junction row on every such import.
#[tokio::test]
async fn two_credit_mb_release_keeps_both_album_artists() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_two_credit_mb_release("two-credit-mb-rel", "two-credit-mb-group");
    f.cover_art
        .seed_lookup(Some(&mb_id), Some("two-credit-mb-group"), None);

    // Scan the album in so the prefetch runs against a candidate key the
    // service actually knows — the key is what core reads the identify evidence
    // behind the claim from, so a made-up one would exercise a path no surface
    // takes.
    let collection = f.temp_path().join("two-credit-collection");
    let album_dir = collection.join("two-credit");
    fs::create_dir_all(&album_dir).unwrap();
    generate_album_files(&album_dir, &["01 Track One.flac"]);
    let candidate_key = album_dir.to_string_lossy().into_owned();

    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    wait_for_scan_event(
        &mut scan_rx,
        "the two-credit candidate",
        |event| matches!(event, ScanEvent::FolderCandidate { candidate: c, .. } if c.path == album_dir),
    )
    .await;

    let choice = IdentityChoice::Exact {
        release_ref: MetadataRef::new(mb_id.clone(), MetadataSource::MusicBrainz),
    };

    // The confirmation pane's seed, unedited: what the UI sends back as the
    // command's overlay when the user touches nothing.
    let prefetch = f
        .handle
        .prefetch_release(&candidate_key, &mb_id, MetadataSource::MusicBrainz)
        .await
        .unwrap();
    let user_edit = bae_core::import::shape_user_edit_for_choice(&prefetch.seed, &choice);

    let import_id = f.ids.new_id();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: candidate_key.clone(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: choice,
            user_edit: Some(user_edit),
        })
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (_release_id, album_id) = support::wait_for_import_complete(&mut rx).await;

    let artists = f.db.get_artists_for_album(&album_id).await.unwrap();
    let names: Vec<&str> = artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Artist A", "Artist B"]);
    assert_eq!(
        artists[1].musicbrainz_artist_id.as_deref(),
        Some("mb-artist-b")
    );
}

/// Seed a plain MusicBrainz release of `track_count` tracks on one CD, credited
/// to one artist. The tracklist a folder's audio gets mapped against.
fn seed_mb_release_with_track_count(
    mb_release_id: &str,
    mb_group_id: &str,
    track_count: usize,
) -> String {
    let response = MbReleaseResponse {
        id: mb_release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("2004".to_string()),
        country: Some("GB".to_string()),
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-slots".to_string()),
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
            tracks: (1..=track_count)
                .map(|position| MbTrack {
                    position: Some(position as i64),
                    number: Some(position.to_string()),
                    title: None,
                    length: None,
                    recording: Some(MbRecording {
                        id: Some(format!("rec-slots-{position}")),
                        title: Some(format!("Source Track {position}")),
                        artist_credit: vec![],
                        relations: vec![],
                    }),
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
    };
    let raw_json = serde_json::to_string(&response).expect("the test response serializes");
    bae_core::musicbrainz::seed_release_cache(mb_release_id, (response, None, raw_json));
    bae_core::musicbrainz::seed_release_group_json_cache(
        mb_group_id,
        serde_json::json!({ "id": mb_group_id }).to_string(),
    );
    mb_release_id.to_string()
}

/// A release's tracks in track order, each with the file its samples come from.
async fn committed_track_files(f: &ImportFixture, release_id: &str) -> Vec<(String, String)> {
    let release_id = release_id.to_string();
    f.db.handle()
        .sql_read(move |sql| {
            let rows = sql.query(
                "SELECT t.title, rf.original_filename \
                 FROM tracks t \
                 JOIN audio_formats af ON af.track_id = t.id \
                 JOIN audio_format_segments seg \
                   ON seg.audio_format_id = af.id AND seg.role = 'main' \
                 JOIN release_files rf ON rf.id = seg.file_id \
                 WHERE t.release_id = ?1 \
                 ORDER BY t.side, t.track_number",
                [&release_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?;
            Ok::<_, coven::CovenError>(rows)
        })
        .await
        .expect("read committed track files")
}

/// Scan `album_dir` in and pick `mb_id` for it, returning the candidate key and
/// the mapping the pick produced. The path both desktop surfaces take.
async fn pick_release_for_folder(
    f: &ImportFixture,
    collection: &Path,
    album_dir: &Path,
    mb_id: &str,
) -> (String, bae_core::import::search::ImportReleasePrefetch) {
    let candidate_key = album_dir.to_string_lossy().into_owned();
    let mut scan_rx = f.handle.subscribe_folder_scan_events();
    f.handle
        .add_watched_folder(collection.to_string_lossy().into_owned())
        .await
        .unwrap();
    let expected = album_dir.to_path_buf();
    wait_for_scan_event(
        &mut scan_rx,
        "the slot candidate",
        move |event| matches!(event, ScanEvent::FolderCandidate { candidate: c, .. } if c.path == expected),
    )
    .await;

    let prefetch = f
        .handle
        .prefetch_release(&candidate_key, mb_id, MetadataSource::MusicBrainz)
        .await
        .unwrap();
    (candidate_key, prefetch)
}

/// The edit a surface sends back when the user changes nothing in the mapping
/// table: the album fields from the seed, the tracks from the table's own rows
/// in the order core lays them out, bindings and all.
fn edit_from_mapping(
    prefetch: &bae_core::import::search::ImportReleasePrefetch,
) -> ReleaseUserEdit {
    let mut raw =
        bae_core::import::RawReleaseEdit::from_user_edit(prefetch.seed.clone(), "import-track");
    raw.tracks = bae_core::import::mapping_tracks(&prefetch.mapping);
    raw.shape()
        .expect("the table's rows shape into a savable edit")
}

/// The tracks the mapping commits, and what the source said about each — the
/// two halves the assertions below read off a row.
fn mapping_rows(
    prefetch: &bae_core::import::search::ImportReleasePrefetch,
) -> Vec<(bae_core::import::RawTrackEdit, Option<String>)> {
    prefetch
        .mapping
        .rows
        .iter()
        .flat_map(bae_core::import::MappingRow::units)
        .filter_map(|unit| match &unit.becomes {
            bae_core::import::MappingBecomes::Track {
                track,
                source_position,
                ..
            } => Some((track.clone(), source_position.clone())),
            _ => None,
        })
        .collect()
}

/// Thirteen files against a twelve-track source. The pick produces twelve
/// paired slots and one `FileOnly`, and committing writes thirteen tracks — the
/// thirteenth named after its file. This folder used to fail the commit
/// outright.
#[tokio::test]
async fn thirteen_files_against_a_twelve_track_source_commits_thirteen_tracks() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_mb_release_with_track_count("mb-rel-13v12", "mb-group-13v12", 12);
    f.cover_art
        .seed_lookup(Some(&mb_id), Some("mb-group-13v12"), None);

    let collection = f.temp_path().join("collection-13v12");
    let album_dir = collection.join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let names: Vec<String> = (1..=13).map(|n| format!("{n:02} Track.flac")).collect();
    generate_album_files(
        &album_dir,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let (candidate_key, prefetch) =
        pick_release_for_folder(&f, &collection, &album_dir, &mb_id).await;

    let rows = mapping_rows(&prefetch);
    assert_eq!(rows.len(), 13);
    // Twelve rows the source names, and one the folder offers that it does not.
    assert_eq!(
        rows.iter()
            .filter(|(_, position)| position.is_some())
            .count(),
        12
    );
    assert_eq!(rows[12].1, None);

    let import_id = f.ids.new_id();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key,
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(mb_id, MetadataSource::MusicBrainz),
            },
            user_edit: Some(edit_from_mapping(&prefetch)),
        })
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut rx).await;

    let committed = committed_track_files(&f, &release_id).await;
    assert_eq!(committed.len(), 13);
    assert_eq!(
        committed[0],
        ("Source Track 1".to_string(), names[0].clone())
    );
    assert_eq!(
        committed[11],
        ("Source Track 12".to_string(), names[11].clone())
    );
    // Nobody named the thirteenth slot, so it commits under its file's name.
    assert_eq!(committed[12], ("13 Track".to_string(), names[12].clone()));
}

/// Fourteen source tracks against thirteen files. The pick produces one
/// `TrackOnly` slot; leaving it unanswered commits the thirteen tracks that
/// have audio and nothing else.
#[tokio::test]
async fn a_track_with_no_audio_commits_as_the_user_left_it() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_mb_release_with_track_count("mb-rel-14v13", "mb-group-14v13", 14);
    f.cover_art
        .seed_lookup(Some(&mb_id), Some("mb-group-14v13"), None);

    let collection = f.temp_path().join("collection-14v13");
    let album_dir = collection.join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let names: Vec<String> = (1..=13).map(|n| format!("{n:02} Track.flac")).collect();
    generate_album_files(
        &album_dir,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let (candidate_key, prefetch) =
        pick_release_for_folder(&f, &collection, &album_dir, &mb_id).await;

    let rows = mapping_rows(&prefetch);
    assert_eq!(rows.len(), 14);
    // The fourteenth track the source names has no audio behind it.
    assert_eq!(rows[13].1.as_deref(), Some("14"));
    assert_eq!(rows[13].0.file, None);

    let import_id = f.ids.new_id();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key,
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(mb_id, MetadataSource::MusicBrainz),
            },
            user_edit: Some(edit_from_mapping(&prefetch)),
        })
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut rx).await;

    let committed = committed_track_files(&f, &release_id).await;
    assert_eq!(committed.len(), 13);
    let titles: Vec<&str> = committed.iter().map(|(title, _)| title.as_str()).collect();
    assert!(
        !titles.contains(&"Source Track 14"),
        "the slot nobody gave audio to has nothing to write: {titles:?}",
    );
}

/// A rip whose files are named in the wrong order. The user re-pairs two slots
/// in the mapping, and the commit binds the files they chose — not the ones
/// their positions would have given them.
#[tokio::test]
async fn a_corrected_pairing_survives_the_commit() {
    support::tracing_init();
    let f = ImportFixture::new().await;
    let mb_id = seed_mb_release_with_track_count("mb-rel-repair", "mb-group-repair", 3);
    f.cover_art
        .seed_lookup(Some(&mb_id), Some("mb-group-repair"), None);

    let collection = f.temp_path().join("collection-repair");
    let album_dir = collection.join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let names = ["01 Track.flac", "02 Track.flac", "03 Track.flac"];
    generate_album_files(&album_dir, &names);

    let (candidate_key, prefetch) =
        pick_release_for_folder(&f, &collection, &album_dir, &mb_id).await;

    let mut edit = edit_from_mapping(&prefetch);
    assert_eq!(edit.tracks.len(), 3);
    // Re-pairing moves the bindings, not the tracks: the first two source
    // tracks keep their titles and numbers and swap the audio behind them.
    let first = edit.tracks[0].file.clone();
    edit.tracks[0].file = edit.tracks[1].file.clone();
    edit.tracks[1].file = first;

    let import_id = f.ids.new_id();
    f.handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key,
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(mb_id, MetadataSource::MusicBrainz),
            },
            user_edit: Some(edit),
        })
        .await
        .unwrap();
    let mut rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut rx).await;

    assert_eq!(
        committed_track_files(&f, &release_id).await,
        vec![
            ("Source Track 1".to_string(), "02 Track.flac".to_string()),
            ("Source Track 2".to_string(), "01 Track.flac".to_string()),
            ("Source Track 3".to_string(), "03 Track.flac".to_string()),
        ],
    );
}
