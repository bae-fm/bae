//! Integration tests for the import service.
//!
//! Tests the real ImportService through its public handle API:
//! scan → import. All through real infrastructure.

use bae_catalogs::catalog::LookupCatalog;

use bae_catalogs::catalog::Catalog;
use bae_catalogs::discogs::models::DiscogsRelease;
use bae_catalogs::musicbrainz::{
    MbArtistCredit, MbArtistRef, MbRecording, MbRelation, MbReleaseResponse, MbTrack,
    MbUrlResource, MbWork,
};
use bae_import::{
    ArtistAssignment, CoverSelection, ImportCommand, ImportDestination, ReleaseUserEdit, ScanEvent,
    TrackArtistAssignments, TrackUserEdit,
};
use bae_records::LibraryImageType;
use bae_types::pressing::ReleaseLabel;
use bae_util::test_channels::HANG_GUARD;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

// ── Helpers ─────────────────────────────────────────────────────────────────

struct ImportFixture {
    library: bae_library::test_libraries::TestLibrary,
    storage: Arc<coven::MemoryStorage>,
    handle: bae_import::ImportServiceHandle,
    /// The catalog fakes the library asks, which a test seeds.
    fakes: bae_catalogs::fake_catalogs::FakeCatalogs,
    ids: Arc<dyn coven::IdSource>,
    /// The Cover Art Archive and image host this library's requests reach.
    /// Every MusicBrainz release group offers a derived cover address, and an
    /// address no test registered answers 404.
    images: bae_catalogs::test_image_host::RemoteImageHost,
    _temp: TempDir,
}

impl ImportFixture {
    async fn new() -> Self {
        let images = bae_catalogs::test_image_host::RemoteImageHost::start().await;
        let temp = TempDir::new().unwrap();
        let db_dir = temp.path().join("db");
        fs::create_dir_all(&db_dir).unwrap();

        let http = images.route(bae_util::http::Http::for_test());
        let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::over(http.clone());
        let (library, storage) =
            bae_library::test_libraries::storage_library(&db_dir, fakes.providers(), http).await;
        let ids: Arc<dyn coven::IdSource> = library.ids().clone();
        let handle = library.start_discogs_import().await;

        Self { library, storage, handle, fakes, ids, images, _temp: temp }
    }

    fn temp_path(&self) -> &Path {
        self._temp.path()
    }

    /// No sync loop behind the connection: the remote-import tests drain the
    /// upload queue themselves and assert what that pass moved.
    async fn connect_cloud(&self) {
        bae_library::test_libraries::connect_storage(
            self.library.manager(),
            self.library.store(),
            &self.storage,
        )
        .await;
    }
}

async fn import_folder(
    f: &ImportFixture,
    album_dir: &Path,
    selected_cover: Option<CoverSelection>,
    destination: ImportDestination,
    draft_source: bae_import::test_imports::DraftSource,
) -> Result<(String, String), String> {
    let import_id = f.ids.new_id().to_string();
    f.handle
        .send_command(ImportCommand {
            selected_cover,
            destination,
            ..bae_import::test_imports::folder_import(
                &import_id,
                album_dir.to_path_buf(),
                draft_source,
            )
        })
        .await
        .map_err(|error| error.to_string())?;
    let mut progress_rx = f.handle.subscribe_import(import_id);
    bae_import::test_imports::try_wait_for_import_complete(&mut progress_rx).await
}

async fn assert_release_has_external_ref(f: &ImportFixture, release_id: &str) {
    assert!(
        f.library.records().find_release_by_id(release_id).await.unwrap().is_some(),
        "prior release row should remain"
    );
    let files = f.library.records().get_files_for_release(release_id).await.unwrap();
    assert!(!files.is_empty(), "prior release files should remain");
    assert!(
        f.library.records().external_file(&files[0].id).await.unwrap().is_some(),
        "prior local file reference should remain"
    );
}

fn generate_album_files(dir: &Path, filenames: &[&str]) {
    let fixture = bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac");
    let flac = fs::read(&fixture).expect("FLAC fixture missing");
    for name in filenames {
        fs::write(dir.join(name), &flac).unwrap();
    }
}

fn copy_cue_flac_fixture(dir: &Path) {
    let fixture = bae_audio::test_fixtures::cue_flac_dir();
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

    let fixture = bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac");
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
        tagged.save_to_path(&dest, WriteOptions::default()).expect("save tags");
    }
}

/// Embedded cover width/height for the fixture below.
const EMBEDDED_COVER_DIMS: (u32, u32) = (64, 48);

/// A real, decodable JPEG cover (solid color, [`EMBEDDED_COVER_DIMS`]) embedded
/// in test audio. The store path decodes every cover and re-encodes it to a
/// ≤600 JPEG, so fixtures must be genuine images, not hand-written JPEG headers.
fn embedded_cover_jpeg() -> Vec<u8> {
    solid_jpeg(EMBEDDED_COVER_DIMS)
}

/// A real, decodable solid-color JPEG of `(width, height)` pixels.
fn solid_jpeg((w, h): (u32, u32)) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 60, 40]));
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img).write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
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

    let fixture = bae_import::test_fixtures::flac_dir().join("01 Test Track 1.flac");
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
        tagged.save_to_path(&dest, WriteOptions::default()).expect("save tags");
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
    image::DynamicImage::ImageRgb8(img).write_to(&mut buf, image::ImageFormat::Png).unwrap();
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
        coven::IdSource::new_id(&coven::UuidIds)
    )
}

fn discogs_release(title: &str, tracks: &[&str]) -> DiscogsRelease {
    let tracks: Vec<(&str, &str)> = tracks.iter().map(|title| (*title, "3:00")).collect();
    DiscogsRelease {
        country: None,
        labels: vec![],
        ..bae_catalogs::test_releases::discogs_test_release(
            &synthetic_release_id(title),
            title,
            &tracks,
        )
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

mod audio_and_loudness;
mod cloud_moves;
mod identity;
// Files whose names aren't UTF-8. Only Linux takes such a name: macOS and
// Windows store names as Unicode.
#[cfg(target_os = "linux")]
mod non_utf8_names;
mod reimports;
mod scan_and_watch;
mod storage_choice;
mod validation_and_mapping;
mod works_and_covers;

/// coven verifies a file's plaintext against its record's content hash on
/// every cloud fetch, so the coven file a `covers` row references must hold
/// the bytes actually stored — the resized thumbnail the import writes, never
/// the image it was made from — and their size. The record carries the
/// file's hash once it is stored.
async fn assert_cover_row_describes_stored_bytes(f: &ImportFixture, release_id: &str) {
    f.library
        .records()
        .find_library_image(release_id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("cover row");
    let bytes =
        bae_library::test_libraries::read_cover_image_bytes(f.library.manager(), release_id)
            .await
            .expect("cover file readable");
    let file = f
        .library
        .manager()
        .library_image_file_for_test(release_id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("the cover's file");
    if let coven::FilePlace::Cloud { content_hash, .. } = file.place() {
        let stored_hash: String =
            content_hash.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(stored_hash, bae_util::fs::hash_bytes(&bytes));
    }
    assert_eq!(
        file.size(),
        bytes.len() as u64,
        "the cover's coven file must be the size of the stored bytes",
    );
}

/// The first read of one tab that `accept` admits; panics at the hang guard.
async fn wait_for_tab(
    f: &ImportFixture,
    what: &str,
    tab: bae_import::TriageTab,
    accept: impl FnMut(&bae_import::ImportListSnapshot) -> bool,
) -> bae_import::ImportListSnapshot {
    let view = bae_import::ImportListView { tab, ..bae_import::ImportListView::default() };
    tokio::time::timeout(HANG_GUARD, f.handle.wait_for_list(view, accept))
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
}

/// Wait for a scan event matching `pred`; panics at the hang guard.
async fn wait_for_scan_event(
    rx: &mut bae_import::ScanEvents,
    what: &str,
    mut pred: impl FnMut(&ScanEvent) -> bool,
) {
    bae_util::test_channels::next_matching(rx, HANG_GUARD, |event| pred(&event).then_some(()))
        .await
        .unwrap_or_else(|| panic!("timed out waiting for {what}"));
}
