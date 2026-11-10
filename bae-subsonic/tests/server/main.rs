//! Integration tests driving the real Subsonic router against a seeded bae
//! library. Each test states the spec behavior it checks; the library is built
//! through the ordinary import path (a per-track release and a single-file CUE
//! release), so the assertions run against real rows, audio, and cover files.

mod library_and_browse;
mod streaming;

use axum::body::{to_bytes, Body};
use axum::http::header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use axum::http::{Request, StatusCode};
use axum::Router;
use bae_app::AppServices;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::discogs::models::DiscogsRelease;
use bae_import::release_candidate::CandidateSource;
use bae_import::{ImportCommand, ImportDestination, ReleaseFileScope};
use bae_preferences::SubsonicCredential;
use bae_records::{DbAlbum, DbArtist, DbRelease, DbTrack};
use bae_subsonic::token;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

const USER: &str = "listener";
const PASS: &str = "s3cret-pass";
const SALT: &str = "abcdef";

fn credential() -> SubsonicCredential {
    SubsonicCredential { username: USER.to_string(), password: PASS.to_string() }
}

/// A valid auth query string, plus any extra parameters appended.
fn authed(extra: &str) -> String {
    let base = format!("u={USER}&s={SALT}&t={}&v=1.16.1&c=test", token(PASS, SALT));
    if extra.is_empty() {
        base
    } else {
        format!("{base}&{extra}")
    }
}

struct Resp {
    status: StatusCode,
    content_type: String,
    content_length: Option<String>,
    content_range: Option<String>,
    body: Vec<u8>,
}

impl Resp {
    fn text(&self) -> String {
        String::from_utf8(self.body.clone()).expect("utf-8 body")
    }
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).expect("json body")
    }
    /// The `subsonic-response` object from a JSON body.
    fn sub(&self) -> Value {
        self.json()["subsonic-response"].clone()
    }
}

async fn call(router: &Router, method: &str, qs: &str) -> Resp {
    call_with_range(router, method, qs, None).await
}

async fn call_with_range(router: &Router, method: &str, qs: &str, range: Option<&str>) -> Resp {
    let uri = format!("/rest/{method}?{qs}");
    let mut builder = Request::builder().uri(uri);
    if let Some(range) = range {
        builder = builder.header(RANGE, range);
    }
    let request = builder.body(Body::empty()).unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let header = |name: &axum::http::HeaderName| {
        response.headers().get(name).map(|v| v.to_str().unwrap().to_string())
    };
    let content_type = header(&CONTENT_TYPE).unwrap_or_default();
    let content_length = header(&CONTENT_LENGTH);
    let content_range = header(&CONTENT_RANGE);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec();
    Resp { status, content_type, content_length, content_range, body }
}

async fn new_library() -> (bae_library::test_libraries::TestLibrary, TempDir) {
    bae_library::test_libraries::setup_test_library().await
}

async fn new_services() -> (AppServices, TempDir) {
    let (library, temp) = new_library().await;
    let services = bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start");
    (services, temp)
}

fn flac_fixture(name: &str) -> std::path::PathBuf {
    bae_import::test_fixtures::flac_dir().join(name)
}

fn cue_fixture(name: &str) -> std::path::PathBuf {
    bae_audio::test_fixtures::cue_flac_dir().join(name)
}

/// A seeded library plus the ids a test addresses it by.
struct Library {
    services: AppServices,
    library: bae_library::test_libraries::TestLibrary,
    per_track_release: String,
    /// The folder the per-track release was imported from, which holds its
    /// originals on this device.
    per_track_source: std::path::PathBuf,
    cue_release: String,
    _temps: Vec<TempDir>,
}

/// Import a two-track file-per-track release (with a folder cover) and a
/// three-track single-file CUE release into one library.
async fn seed_library() -> Library {
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let (library, db_temp) = bae_library::test_libraries::setup_test_library_with(
        fakes.providers(),
        bae_util::http::Http::for_test(),
    )
    .await;

    // Per-track release, imported from file metadata (no network).
    let pt_temp = TempDir::new().unwrap();
    let pt_dir = pt_temp.path().join("solo album");
    std::fs::create_dir_all(&pt_dir).unwrap();
    bae_import::test_fixtures::copy_and_tag(
        &flac_fixture("01 Test Track 1.flac"),
        &pt_dir,
        "01.flac",
        "First Track",
        "Solo Artist",
        "Solo Album",
        "Solo Artist",
        2001,
        1,
    );
    bae_import::test_fixtures::copy_and_tag(
        &flac_fixture("02 Test Track 2.flac"),
        &pt_dir,
        "02.flac",
        "Second Track",
        "Solo Artist",
        "Solo Album",
        "Solo Artist",
        2001,
        2,
    );
    bae_import::test_fixtures::write_cover_png(&pt_dir.join("cover.png"));

    let import = library.start_discogs_import().await;
    let import_id = "pt".to_string();
    import
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: bae_import::CandidateKey::from(b"pt".to_vec()),
            source: CandidateSource {
                path: pt_dir.clone(),
                scope: ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            selected_cover: None,
            destination: ImportDestination::ThisDevice,
            pick: None,
            read_file_tags: true,
            user_edit: None,
        })
        .await
        .unwrap();
    let mut rx = import.subscribe_import(import_id);
    let (per_track_release, _album) =
        bae_import::test_imports::wait_for_import_complete(&mut rx).await;

    // CUE-image release, imported against a seeded Discogs release (cached, no
    // network) so it exercises the single-file-with-CUE track windows.
    let cue_temp = TempDir::new().unwrap();
    let cue_dir = cue_temp.path().join("cue album");
    std::fs::create_dir_all(&cue_dir).unwrap();
    std::fs::copy(cue_fixture("Test Album.flac"), cue_dir.join("Test Album.flac")).unwrap();
    std::fs::copy(cue_fixture("Test Album.cue"), cue_dir.join("Test Album.cue")).unwrap();
    let discogs_key = fakes.seed_discogs_test_release(cue_discogs_release());
    let cue_import = library.start_discogs_import().await;
    let cue_id = "cue".to_string();
    cue_import
        .send_command(ImportCommand {
            import_id: cue_id.clone(),
            candidate_key: bae_import::CandidateKey::from(b"cue".to_vec()),
            source: CandidateSource {
                path: cue_dir,
                scope: ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            selected_cover: None,
            destination: ImportDestination::ThisDevice,
            read_file_tags: false,
            pick: Some(bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    discogs_key,
                ),
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .unwrap();
    let mut cue_rx = cue_import.subscribe_import(cue_id);
    let (cue_release, _cue_album) =
        bae_import::test_imports::wait_for_import_complete(&mut cue_rx).await;

    let services = bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start");
    Library {
        services,
        library,
        per_track_release,
        per_track_source: pt_dir,
        cue_release,
        _temps: vec![db_temp, pt_temp, cue_temp],
    }
}

fn cue_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        master_id: Some("test-master".to_string()),
        ..bae_catalogs::test_releases::discogs_test_release(
            "test-cue-flac",
            "Test Album",
            &[
                ("Track One (Silence)", "0:10"),
                ("Track Two (White Noise)", "0:10"),
                ("Track Three (Brown Noise)", "0:10"),
            ],
        )
    }
}

fn find_artist<'a>(indexes: &'a [Value], name: &str) -> &'a Value {
    indexes
        .iter()
        .flat_map(|index| index["artist"].as_array().unwrap())
        .find(|a| a["name"] == name)
        .unwrap_or_else(|| panic!("artist {name} not found"))
}
