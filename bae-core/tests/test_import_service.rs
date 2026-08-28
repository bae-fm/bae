#![cfg(feature = "test-utils")]
//! Integration tests for the import service.
//!
//! Tests the real ImportService through its public handle API:
//! scan → import. All through real infrastructure.

use bae_test_support as support;

use bae_core::db::{Database, LibraryImageType};
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{
    ArtistAssignment, CoverSelection, ImportCommand, MetadataProvenance, MetadataSource,
    PressingEdit, ReleaseUserEdit, ScanEvent, StorageMode, TrackArtistAssignments, TrackUserEdit,
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
    _temp: TempDir,
}

impl ImportFixture {
    async fn new() -> Self {
        // Every MusicBrainz release group offers a derived cover address. Route
        // unseeded addresses to the local archive's 404 response rather than
        // depending on another test to start the archive first.
        support::cover_art_archive();

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
        let config_handle = support::test_config(&library_dir);
        let ids: Arc<dyn coven::IdProvider> = Arc::new(coven::UuidProvider);
        let library_manager = LibraryManager::new(
            db.clone(),
            config_handle.clone(),
            std::sync::Arc::new(coven::SystemClock),
            ids.clone(),
            bae_core::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );
        support::configure_test_discogs(&library_manager);

        let handle = library_manager
            .start_import_service(tokio::runtime::Handle::current())
            .await
            .expect("import service starts");

        Self {
            db,
            handle,
            library_manager,
            config_handle,
            ids,
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
    metadata_provenance: MetadataProvenance,
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
            metadata_provenance: Some(metadata_provenance),
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

include!("test_import_service/scan_and_watch.rs");
include!("test_import_service/audio_and_loudness.rs");
include!("test_import_service/reimports.rs");
include!("test_import_service/works_and_covers.rs");
include!("test_import_service/identity.rs");
include!("test_import_service/validation_and_mapping.rs");
