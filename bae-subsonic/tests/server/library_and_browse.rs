use std::path::Path;

use axum::body::{to_bytes, Body};
use axum::http::header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use axum::http::{Request, StatusCode};
use axum::Router;
use bae_core::config::SubsonicCredential;
use bae_core::db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack};
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{
    IdentityChoice, ImportCommand, MetadataRef, MetadataSource, ReleaseFileScope, StorageMode,
};
use bae_core::library::{AppServices, LibraryManager};
use bae_test_support as support;
use coven::StoreDir;
use md5::{Digest, Md5};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

const USER: &str = "listener";
const PASS: &str = "s3cret-pass";
const SALT: &str = "abcdef";

fn credential() -> SubsonicCredential {
    SubsonicCredential {
        username: USER.to_string(),
        password: PASS.to_string(),
    }
}

fn token(password: &str, salt: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(password.as_bytes());
    hasher.update(salt.as_bytes());
    hex::encode(hasher.finalize())
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
        response
            .headers()
            .get(name)
            .map(|v| v.to_str().unwrap().to_string())
    };
    let content_type = header(&CONTENT_TYPE).unwrap_or_default();
    let content_length = header(&CONTENT_LENGTH);
    let content_range = header(&CONTENT_RANGE);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    Resp {
        status,
        content_type,
        content_length,
        content_range,
        body,
    }
}

async fn new_manager() -> (LibraryManager, TempDir) {
    support::tracing_init();
    let temp = TempDir::new().unwrap();
    let db_dir = temp.path().join("db");
    std::fs::create_dir_all(&db_dir).unwrap();
    let database = Database::new_test(
        db_dir.join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .expect("database");
    let library_dir = StoreDir::new(db_dir);
    let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
    let manager = LibraryManager::new(
        database,
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    (manager, temp)
}

async fn new_services() -> (AppServices, TempDir) {
    let (manager, temp) = new_manager().await;
    let services = AppServices::for_test(manager).await.expect("app services");
    (services, temp)
}

fn flac_fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("bae-core/tests/fixtures/flac")
        .join(name)
}

fn cue_fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("bae-core/tests/fixtures/cue_flac")
        .join(name)
}

/// A seeded library plus the ids a test addresses it by.
struct Library {
    services: AppServices,
    per_track_release: String,
    cue_release: String,
    _temps: Vec<TempDir>,
}

/// Import a two-track file-per-track release (with a folder cover) and a
/// three-track single-file CUE release into one manager.
async fn seed_library() -> Library {
    let (manager, db_temp) = new_manager().await;

    // Per-track release, imported from file tags (no network).
    let pt_temp = TempDir::new().unwrap();
    let pt_dir = pt_temp.path().join("solo album");
    std::fs::create_dir_all(&pt_dir).unwrap();
    support::copy_and_tag(
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
    support::copy_and_tag(
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
    support::write_cover_png(&pt_dir.join("cover.png"));

    let import =
        support::start_test_import(tokio::runtime::Handle::current(), manager.clone()).await;
    let import_id = "pt".to_string();
    import
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "pt".to_string(),
            folder: pt_dir,
            scope: ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .await
        .unwrap();
    let mut rx = import.subscribe_import(import_id);
    let (per_track_release, _album) = support::wait_for_import_complete(&mut rx).await;

    // CUE-image release, imported against a seeded Discogs release (cached, no
    // network) so it exercises the single-file-with-CUE track windows.
    let cue_temp = TempDir::new().unwrap();
    let cue_dir = cue_temp.path().join("cue album");
    std::fs::create_dir_all(&cue_dir).unwrap();
    std::fs::copy(
        cue_fixture("Test Album.flac"),
        cue_dir.join("Test Album.flac"),
    )
    .unwrap();
    std::fs::copy(
        cue_fixture("Test Album.cue"),
        cue_dir.join("Test Album.cue"),
    )
    .unwrap();
    let discogs_key = support::seed_discogs_test_release(cue_discogs_release());
    let cue_import =
        support::start_test_import(tokio::runtime::Handle::current(), manager.clone()).await;
    let cue_id = "cue".to_string();
    cue_import
        .send_command(ImportCommand {
            import_id: cue_id.clone(),
            candidate_key: "cue".to_string(),
            folder: cue_dir,
            scope: ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(discogs_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();
    let mut cue_rx = cue_import.subscribe_import(cue_id);
    let (cue_release, _cue_album) = support::wait_for_import_complete(&mut cue_rx).await;

    let services = AppServices::for_test(manager).await.expect("app services");
    Library {
        services,
        per_track_release,
        cue_release,
        _temps: vec![db_temp, pt_temp, cue_temp],
    }
}

fn cue_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-cue-flac".to_string(),
        title: "Test Album".to_string(),
        year: Some(2024),
        format: vec![],
        country: Some("US".to_string()),
        label: vec!["Test Label".to_string()],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            id: "discogs-artist-1".to_string(),
            name: "Artist Name".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: vec![
            cue_track("1", "Track One (Silence)"),
            cue_track("2", "Track Two (White Noise)"),
            cue_track("3", "Track Three (Brown Noise)"),
        ],
        master_id: Some("test-master".to_string()),
    }
}

fn cue_track(position: &str, title: &str) -> DiscogsTrack {
    DiscogsTrack {
        type_: "track".to_string(),
        position: position.to_string(),
        title: title.to_string(),
        duration: Some("0:10".to_string()),
        artists: vec![],
        extraartists: None,
    }
}

// ─────────────────────────── auth + envelope + system ───────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn auth_gate() {
    // Spec: token auth. valid → ok; wrong token → 40; missing s/t → 10; short
    // salt → 40; token hex is case-insensitive.
    let (services, _t) = new_services().await;
    let router = bae_subsonic::router(services, credential());

    let ok = call(&router, "ping", &authed("")).await;
    assert!(ok.text().contains(r#"status="ok""#), "valid: {}", ok.text());

    let bad = call(
        &router,
        "ping",
        &format!("u={USER}&s={SALT}&t=deadbeef&v=1&c=t"),
    )
    .await;
    assert!(
        bad.text().contains(r#"code="40""#),
        "bad token: {}",
        bad.text()
    );

    let missing_t = call(&router, "ping", &format!("u={USER}&s={SALT}&v=1&c=t")).await;
    assert!(missing_t.text().contains(r#"code="10""#), "missing t");
    let missing_s = call(&router, "ping", &format!("u={USER}&t=x&v=1&c=t")).await;
    assert!(missing_s.text().contains(r#"code="10""#), "missing s");

    let short = call(
        &router,
        "ping",
        &format!("u={USER}&s=abc&t={}&v=1&c=t", token(PASS, "abc")),
    )
    .await;
    assert!(short.text().contains(r#"code="40""#), "short salt");

    let upper = call(
        &router,
        "ping",
        &format!(
            "u={USER}&s={SALT}&t={}&v=1&c=t",
            token(PASS, SALT).to_uppercase()
        ),
    )
    .await;
    assert!(
        upper.text().contains(r#"status="ok""#),
        "uppercase hex token"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn envelope_formats() {
    // Spec: xml is the default; f=json yields JSON; f=jsonp wraps in the
    // callback; every envelope carries openSubsonic, version, and type.
    let (services, _t) = new_services().await;
    let router = bae_subsonic::router(services, credential());

    let xml = call(&router, "ping", &authed("")).await;
    assert!(xml.content_type.contains("xml"));
    assert!(xml.text().contains(r#"openSubsonic="true""#));
    assert!(xml.text().contains(r#"version="1.16.1""#));
    assert!(xml.text().contains(r#"type="bae""#));

    let json = call(&router, "ping", &authed("f=json")).await;
    assert!(json.content_type.contains("json"));
    let sub = json.sub();
    assert_eq!(sub["status"], "ok");
    assert_eq!(sub["openSubsonic"], true);
    assert_eq!(sub["version"], "1.16.1");
    assert_eq!(sub["type"], "bae");

    let jsonp = call(&router, "ping", &authed("f=jsonp&callback=cb")).await;
    assert!(jsonp.content_type.contains("javascript"));
    let text = jsonp.text();
    assert!(text.starts_with("cb("), "jsonp wrap: {text}");
    assert!(text.trim_end().ends_with(");"), "jsonp wrap: {text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn system_endpoints() {
    let (services, _t) = new_services().await;
    let router = bae_subsonic::router(services, credential());

    let license = call(&router, "getLicense", &authed("f=json")).await;
    assert_eq!(license.sub()["license"]["valid"], true);

    let folders = call(&router, "getMusicFolders", &authed("f=json")).await;
    let list = &folders.sub()["musicFolders"]["musicFolder"];
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], 0);
    assert_eq!(list[0]["name"], "bae");
}

#[tokio::test(flavor = "multi_thread")]
async fn malformed_and_wrong_kind_ids_are_not_found() {
    // Spec: a malformed id, or a wrong-kind id to getAlbum, is error 70.
    let (services, _t) = new_services().await;
    let router = bae_subsonic::router(services, credential());

    let malformed = call(&router, "getAlbum", &authed("id=not-namespaced")).await;
    assert!(malformed.text().contains(r#"code="70""#));

    let wrong_kind = call(&router, "getAlbum", &authed("id=ar-someartist")).await;
    assert!(wrong_kind.text().contains(r#"code="70""#));
}

// ───────────────────────────────── browse ───────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn get_artists_indexes_and_counts() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());

    let resp = call(&router, "getArtists", &authed("f=json")).await;
    let artists = &resp.sub()["artists"];
    let indexes = artists["index"].as_array().unwrap();
    // Two artists in distinct buckets: "Artist Name" (A) and "Solo Artist" (S).
    let letters: Vec<&str> = indexes
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert!(letters.contains(&"A"), "buckets: {letters:?}");
    assert!(letters.contains(&"S"), "buckets: {letters:?}");

    // Each artist's albumCount is its release count (one release each here), and
    // a file-tags artist carries no musicBrainzId.
    let solo = find_artist(indexes, "Solo Artist");
    assert_eq!(solo["albumCount"], 1);
    assert!(
        solo.get("musicBrainzId").is_none(),
        "file-tags artist has no mbid"
    );

    // getIndexes shares the body under the legacy payload name.
    let legacy = call(&router, "getIndexes", &authed("f=json")).await;
    assert!(legacy.sub()["indexes"]["index"].is_array());
}

fn find_artist<'a>(indexes: &'a [Value], name: &str) -> &'a Value {
    indexes
        .iter()
        .flat_map(|index| index["artist"].as_array().unwrap())
        .find(|a| a["name"] == name)
        .unwrap_or_else(|| panic!("artist {name} not found"))
}

#[tokio::test(flavor = "multi_thread")]
async fn get_artist_lists_releases_as_albums() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());

    // Resolve the per-track release's artist id, then browse the artist.
    let album = call(
        &router,
        "getAlbum",
        &authed(&format!("f=json&id=al-{}", lib.per_track_release)),
    )
    .await;
    let artist_id = album.sub()["album"]["artistId"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = call(
        &router,
        "getArtist",
        &authed(&format!("f=json&id={artist_id}")),
    )
    .await;
    let artist = &resp.sub()["artist"];
    let albums = artist["album"].as_array().unwrap();
    assert_eq!(albums.len(), 1, "one release for this artist");
    assert!(albums[0]["id"].as_str().unwrap().starts_with("al-"));

    let unknown = call(&router, "getArtist", &authed("f=json&id=ar-nope")).await;
    assert!(unknown.text().contains(r#""code":70"#));
}

// ──────────────────────────────── album + song ──────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn get_album_lists_songs_with_required_child_fields() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());

    let resp = call(
        &router,
        "getAlbum",
        &authed(&format!("f=json&id=al-{}", lib.per_track_release)),
    )
    .await;
    let album = &resp.sub()["album"];
    assert_eq!(album["songCount"], 2);
    let songs = album["song"].as_array().unwrap();
    assert_eq!(songs.len(), 2);
    for song in songs {
        // OpenSubsonic requires these on every Child.
        assert!(song["bitDepth"].is_number(), "bitDepth required");
        assert!(song["samplingRate"].as_i64().unwrap() > 0);
        assert!(song["channelCount"].as_i64().unwrap() > 0);
        // A lossless FLAC track reports its real depth, not 0.
        assert_eq!(song["bitDepth"], 16);
        assert!(song["id"].as_str().unwrap().starts_with("tr-"));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn get_album_cue_lists_its_windowed_tracks() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());

    let resp = call(
        &router,
        "getAlbum",
        &authed(&format!("f=json&id=al-{}", lib.cue_release)),
    )
    .await;
    let album = &resp.sub()["album"];
    assert_eq!(album["songCount"], 3, "CUE image has three tracks");
    let songs = album["song"].as_array().unwrap();
    for song in songs {
        assert_eq!(song["bitDepth"], 16);
        assert!(song["samplingRate"].as_i64().unwrap() > 0);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn get_song_and_unknown() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());

    let album = call(
        &router,
        "getAlbum",
        &authed(&format!("f=json&id=al-{}", lib.per_track_release)),
    )
    .await;
    let song_id = album.sub()["album"]["song"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    let song = &resp.sub()["song"];
    assert_eq!(song["id"], song_id);
    assert!(song["bitDepth"].is_number());
    assert!(song["samplingRate"].as_i64().unwrap() > 0);
    assert!(song["channelCount"].as_i64().unwrap() > 0);

    let unknown = call(&router, "getSong", &authed("f=json&id=tr-missing")).await;
    assert!(unknown.text().contains(r#""code":70"#));
}

// ──────────────────────────────── lists + search ────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn album_list2_orderings_and_paging() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());

    let names = |resp: &Resp| -> Vec<String> {
        resp.sub()["albumList2"]["album"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["name"].as_str().unwrap().to_string())
            .collect()
    };

    let alpha = call(
        &router,
        "getAlbumList2",
        &authed("f=json&type=alphabeticalByName"),
    )
    .await;
    let alpha_names = names(&alpha);
    assert_eq!(alpha_names.len(), 2);
    let mut sorted = alpha_names.clone();
    sorted.sort_by_key(|s| s.to_lowercase());
    assert_eq!(alpha_names, sorted, "alphabetical order");

    // newest orders by created_at, most-recent first. The CUE release imports
    // after the per-track one, so "Test Album" leads "Solo Album".
    let newest = call(&router, "getAlbumList2", &authed("f=json&type=newest")).await;
    assert_eq!(
        names(&newest),
        vec!["Test Album", "Solo Album"],
        "newest by created_at"
    );

    // Paging: size=1 returns one, offset=1 returns the next.
    let first = call(
        &router,
        "getAlbumList2",
        &authed("f=json&type=alphabeticalByName&size=1"),
    )
    .await;
    assert_eq!(names(&first).len(), 1);
    let second = call(
        &router,
        "getAlbumList2",
        &authed("f=json&type=alphabeticalByName&size=1&offset=1"),
    )
    .await;
    assert_eq!(names(&second).len(), 1);
    assert_ne!(names(&first)[0], names(&second)[0]);

    // byYear range filters (2001 for the per-track release; the CUE release is 2024).
    let by_year = call(
        &router,
        "getAlbumList2",
        &authed("f=json&type=byYear&fromYear=2000&toYear=2010"),
    )
    .await;
    assert_eq!(
        names(&by_year).len(),
        1,
        "only the 2001 release is in range"
    );

    // byYear with toYear < fromYear reverses the order (descending by year):
    // both releases are in range, newest year first.
    let descending = call(
        &router,
        "getAlbumList2",
        &authed("f=json&type=byYear&fromYear=2024&toYear=2000"),
    )
    .await;
    assert_eq!(
        names(&descending),
        vec!["Test Album", "Solo Album"],
        "byYear reversed is descending by year (2024 then 2001)"
    );

    // byGenre has no genre store → empty, not an error.
    let by_genre = call(
        &router,
        "getAlbumList2",
        &authed("f=json&type=byGenre&genre=Rock"),
    )
    .await;
    let genre_empty = by_genre.sub()["albumList2"]
        .get("album")
        .and_then(|v| v.as_array())
        .map_or(0, |a| a.len());
    assert_eq!(genre_empty, 0, "byGenre is empty");
    assert!(
        !by_genre.text().contains("\"error\""),
        "byGenre is not an error"
    );

    // random returns `size` items.
    let random = call(
        &router,
        "getAlbumList2",
        &authed("f=json&type=random&size=1"),
    )
    .await;
    assert_eq!(names(&random).len(), 1);

    // Unsupported types return an empty list, not an error. An empty list omits
    // the `album` array key entirely (Subsonic JSON convention).
    let frequent = call(&router, "getAlbumList2", &authed("f=json&type=frequent")).await;
    let empty = frequent.sub()["albumList2"]
        .get("album")
        .and_then(|v| v.as_array())
        .map_or(0, |a| a.len());
    assert_eq!(empty, 0);
    assert!(
        !frequent.text().contains("\"error\""),
        "frequent is not an error"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn search3_matches_and_caps() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.services.clone(), credential());

    // A query matching the per-track album/artist.
    let hit = call(&router, "search3", &authed("f=json&query=Solo")).await;
    let result = &hit.sub()["searchResult3"];
    assert!(
        result["artist"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "Solo Artist"),
        "artist hit"
    );
    assert!(
        result["album"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == "Solo Album"),
        "album hit"
    );

    // A query matching song titles returns song hits (the per-track titles are
    // "First Track"/"Second Track"; the CUE titles are "Track One/Two/Three").
    let song_hit = call(&router, "search3", &authed("f=json&query=Track")).await;
    let hit_songs = song_hit.sub()["searchResult3"]["song"]
        .as_array()
        .unwrap()
        .len();
    assert!(hit_songs > 0, "song query returns song hits");

    // Per-kind caps: an empty query returns the whole library, capped by each
    // count. artistCount/albumCount cap their kinds independently.
    let capped = call(
        &router,
        "search3",
        &authed("f=json&query=&artistCount=1&albumCount=1&songCount=1"),
    )
    .await;
    let capped = &capped.sub()["searchResult3"];
    assert_eq!(
        capped["artist"].as_array().unwrap().len(),
        1,
        "artistCount caps artists"
    );
    assert_eq!(
        capped["album"].as_array().unwrap().len(),
        1,
        "albumCount caps albums"
    );
    assert_eq!(
        capped["song"].as_array().unwrap().len(),
        1,
        "songCount caps songs"
    );

    // songOffset skips into the song list, so offset 1 differs from offset 0.
    let song_ids = |resp: &Resp| -> String {
        resp.sub()["searchResult3"]["song"][0]["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let first = call(
        &router,
        "search3",
        &authed("f=json&query=&songCount=1&songOffset=0"),
    )
    .await;
    let second = call(
        &router,
        "search3",
        &authed("f=json&query=&songCount=1&songOffset=1"),
    )
    .await;
    assert_ne!(
        song_ids(&first),
        song_ids(&second),
        "songOffset pages the song list"
    );
}
