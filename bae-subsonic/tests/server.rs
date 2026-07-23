//! Integration tests driving the real Subsonic router against a seeded bae
//! library. Each test states the spec behavior it checks; the library is built
//! through the ordinary import path (a per-track release and a single-file CUE
//! release), so the assertions run against real rows, audio, and cover blobs.

use std::path::Path;

use axum::body::{to_bytes, Body};
use axum::http::header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use axum::http::{Request, StatusCode};
use axum::Router;
use bae_core::config::SubsonicCredential;
use bae_core::db::{Database, DbAlbum, DbArtist, DbRelease, DbTrack};
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{IdentityChoice, ImportCommand, MetadataRef, MetadataSource, StorageMode};
use bae_core::library::LibraryManager;
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
    );
    (manager, temp)
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
    manager: LibraryManager,
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

    let import = support::start_test_import(tokio::runtime::Handle::current(), manager.clone());
    let import_id = "pt".to_string();
    import
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "pt".to_string(),
            folder: pt_dir,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
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
    let cue_import = support::start_test_import(tokio::runtime::Handle::current(), manager.clone());
    let cue_id = "cue".to_string();
    cue_import
        .send_command(ImportCommand {
            import_id: cue_id.clone(),
            candidate_key: "cue".to_string(),
            folder: cue_dir,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(discogs_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();
    let mut cue_rx = cue_import.subscribe_import(cue_id);
    let (cue_release, _cue_album) = support::wait_for_import_complete(&mut cue_rx).await;

    Library {
        manager,
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
    let (manager, _t) = new_manager().await;
    let router = bae_subsonic::router(manager, credential());

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
    let (manager, _t) = new_manager().await;
    let router = bae_subsonic::router(manager, credential());

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
    let (manager, _t) = new_manager().await;
    let router = bae_subsonic::router(manager, credential());

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
    let (manager, _t) = new_manager().await;
    let router = bae_subsonic::router(manager, credential());

    let malformed = call(&router, "getAlbum", &authed("id=not-namespaced")).await;
    assert!(malformed.text().contains(r#"code="70""#));

    let wrong_kind = call(&router, "getAlbum", &authed("id=ar-someartist")).await;
    assert!(wrong_kind.text().contains(r#"code="70""#));
}

// ───────────────────────────────── browse ───────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn get_artists_indexes_and_counts() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.manager.clone(), credential());

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
    let router = bae_subsonic::router(lib.manager.clone(), credential());

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
    let router = bae_subsonic::router(lib.manager.clone(), credential());

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
    let router = bae_subsonic::router(lib.manager.clone(), credential());

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
    let router = bae_subsonic::router(lib.manager.clone(), credential());

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
    let router = bae_subsonic::router(lib.manager.clone(), credential());

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
    let router = bae_subsonic::router(lib.manager.clone(), credential());

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

// ──────────────────────────────────── media ─────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn stream_raw_serves_original_bytes_and_ranges() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.manager.clone(), credential());
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let full = call(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=raw")),
    )
    .await;
    assert_eq!(full.status, StatusCode::OK);
    assert_eq!(full.content_type, "audio/flac");
    let total: u64 = full.content_length.as_ref().unwrap().parse().unwrap();
    assert_eq!(
        full.body.len() as u64,
        total,
        "full body matches Content-Length"
    );
    assert_eq!(&full.body[0..4], b"fLaC", "raw FLAC bytes");

    // A range request returns 206 with the right slice.
    let ranged = call_with_range(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=raw")),
        Some("bytes=0-9"),
    )
    .await;
    assert_eq!(ranged.status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(ranged.body.len(), 10);
    assert_eq!(ranged.content_length.as_deref(), Some("10"));
    assert!(ranged.content_range.unwrap().starts_with("bytes 0-9/"));
    assert_eq!(&ranged.body, &full.body[0..10], "range slice matches");
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_transcode_mp3_is_chunked_and_decodes() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.manager.clone(), credential());
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let resp = call(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=mp3&maxBitRate=128")),
    )
    .await;
    assert_eq!(resp.status, StatusCode::OK);
    assert_eq!(resp.content_type, "audio/mpeg");
    // Chunked: no (bogus) Content-Length on a streaming transcode.
    assert!(
        resp.content_length.is_none(),
        "transcode must not send Content-Length"
    );
    assert!(
        resp.body.len() > 1000,
        "mp3 body present: {} bytes",
        resp.body.len()
    );

    // The bytes decode back with the source's channel count and duration.
    let song = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    let expected_channels = song.sub()["song"]["channelCount"].as_i64().unwrap() as u32;
    let source_secs = song.sub()["song"]["duration"].as_i64().unwrap();
    let decoded = decode_bytes(&resp.body);
    assert_eq!(
        decoded.channels, expected_channels,
        "transcode preserves channels"
    );
    assert!(!decoded.samples.is_empty());
    let decoded_secs =
        (decoded.samples.len() as f64 / decoded.channels as f64) / decoded.sample_rate as f64;
    assert!(
        (decoded_secs - source_secs as f64).abs() <= 1.0,
        "decoded duration {decoded_secs:.2}s must match source {source_secs}s (±1s for codec delay)"
    );

    // maxBitRate has a bitrate-dependent effect: a CBR MP3 of fixed duration is
    // larger at a higher bitrate.
    let low = call(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=mp3&maxBitRate=64")),
    )
    .await;
    let high = call(
        &router,
        "stream",
        &authed(&format!("id={song_id}&format=mp3&maxBitRate=320")),
    )
    .await;
    assert!(
        high.body.len() > low.body.len(),
        "320kbps ({} bytes) must exceed 64kbps ({} bytes)",
        high.body.len(),
        low.body.len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cover_art_resolves_every_id_kind() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.manager.clone(), credential());

    // Album id serves the release cover.
    let by_album = call(
        &router,
        "getCoverArt",
        &authed(&format!("id=al-{}", lib.per_track_release)),
    )
    .await;
    assert_eq!(by_album.status, StatusCode::OK);
    assert!(by_album.content_type.starts_with("image/"));
    assert!(!by_album.body.is_empty());

    // Track id resolves to its release's cover.
    let song_id = first_song_id(&router, &lib.per_track_release).await;
    let by_track = call(&router, "getCoverArt", &authed(&format!("id={song_id}"))).await;
    assert_eq!(by_track.status, StatusCode::OK);
    assert!(by_track.content_type.starts_with("image/"));

    // Artist id resolves to one of the artist's release covers.
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
    let by_artist = call(&router, "getCoverArt", &authed(&format!("id={artist_id}"))).await;
    assert_eq!(
        by_artist.status,
        StatusCode::OK,
        "artist id resolves to a release cover"
    );
    assert!(by_artist.content_type.starts_with("image/"));

    // The CUE release has no cover art → error 70.
    let missing = call(
        &router,
        "getCoverArt",
        &authed(&format!("id=al-{}", lib.cue_release)),
    )
    .await;
    assert!(
        missing.text().contains(r#"code="70""#),
        "missing art: {}",
        missing.text()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scrobble_is_an_accepted_no_op() {
    let lib = seed_library().await;
    let router = bae_subsonic::router(lib.manager.clone(), credential());
    let song_id = first_song_id(&router, &lib.per_track_release).await;

    let resp = call(
        &router,
        "scrobble",
        &authed(&format!("f=json&id={song_id}")),
    )
    .await;
    assert_eq!(resp.sub()["status"], "ok");
}

// ─────────────────────────── musicBrainzId + lossy ─────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn musicbrainz_id_surfaces_when_present() {
    // Spec: ArtistID3 carries musicBrainzId when the artist has one. Seed an
    // artist with a MusicBrainz id and a minimal album/release, then assert both
    // getArtists and getArtist emit it.
    let (manager, _t) = new_manager().await;
    let artist = DbArtist {
        id: "mb-artist-1".to_string(),
        name: "MB Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mbid-abc-123".to_string()),
        created_at: chrono::Utc::now(),
    };
    manager.insert_artist(&artist).await.unwrap();
    let now = chrono::Utc::now();
    let album = DbAlbum {
        id: "mb-album-1".to_string(),
        title: "MB Album".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2010),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: "mb-release-1".to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: bae_core::db::Pressing::blank(),
        disc_id: None,
        metadata_source: bae_core::db::ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        remote: false,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = DbTrack {
        id: "mb-track-1".to_string(),
        release_id: release.id.clone(),
        title: "MB Track".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(1000),
        discogs_position: None,
        created_at: now,
    };
    manager
        .insert_album_with_release_and_tracks(&album, &release, &[track], &[], &[])
        .await
        .unwrap();

    let router = bae_subsonic::router(manager, credential());

    let artists = call(&router, "getArtists", &authed("f=json")).await;
    let sub = artists.sub();
    let indexes = sub["artists"]["index"].as_array().unwrap();
    let mb = find_artist(indexes, "MB Artist");
    assert_eq!(mb["musicBrainzId"], "mbid-abc-123", "getArtists emits mbid");

    let one = call(&router, "getArtist", &authed("f=json&id=ar-mb-artist-1")).await;
    assert_eq!(
        one.sub()["artist"]["musicBrainzId"],
        "mbid-abc-123",
        "getArtist emits mbid"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn lossy_track_reports_bit_depth_zero() {
    // Spec/OpenSubsonic: bitDepth is required on every Child; a lossy codec has
    // no fixed sample depth, so it reports 0. Import a lossy Opus track and assert
    // its Child bitDepth is 0, in contrast to the lossless bitDepth=16 case.
    // (AAC-in-MP4 is unsuitable: the MP4 sound sample entry always declares 16,
    // so FFmpeg reports a bit depth for it; Opus carries none.)
    let (manager, release_id, _temps) = seed_lossy_release().await;
    let router = bae_subsonic::router(manager, credential());

    let album = call(
        &router,
        "getAlbum",
        &authed(&format!("f=json&id=al-{release_id}")),
    )
    .await;
    let song = &album.sub()["album"]["song"][0];
    assert_eq!(song["bitDepth"], 0, "a lossy track reports bitDepth 0");
    assert!(song["samplingRate"].as_i64().unwrap() > 0);
    assert!(song["channelCount"].as_i64().unwrap() > 0);

    let song_id = song["id"].as_str().unwrap().to_string();
    let one = call(&router, "getSong", &authed(&format!("f=json&id={song_id}"))).await;
    assert_eq!(
        one.sub()["song"]["bitDepth"],
        0,
        "getSong reports bitDepth 0 for lossy"
    );
}

/// Import the single-file lossy Opus fixture through the normal import path.
async fn seed_lossy_release() -> (LibraryManager, String, Vec<TempDir>) {
    let (manager, db_temp) = new_manager().await;
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("lossy album");
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("bae-core/test-fixtures/audio-format/placeholder-opus.opus");
    std::fs::copy(&fixture, dir.join("track.opus")).unwrap();

    let discogs_key = support::seed_discogs_test_release(DiscogsRelease {
        id: "test-lossy".to_string(),
        title: "Lossy Album".to_string(),
        year: Some(2024),
        format: vec![],
        country: None,
        label: vec![],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            id: "discogs-lossy-artist".to_string(),
            name: "Lossy Artist".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: vec![cue_track("1", "Only Track")],
        master_id: None,
    });
    let import = support::start_test_import(tokio::runtime::Handle::current(), manager.clone());
    let import_id = "lossy".to_string();
    import
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "lossy".to_string(),
            folder: dir,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(discogs_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .unwrap();
    let mut rx = import.subscribe_import(import_id);
    let (release_id, _album) = support::wait_for_import_complete(&mut rx).await;
    (manager, release_id, vec![db_temp, temp])
}

async fn first_song_id(router: &Router, release_id: &str) -> String {
    let album = call(
        router,
        "getAlbum",
        &authed(&format!("f=json&id=al-{release_id}")),
    )
    .await;
    album.sub()["album"]["song"][0]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Decode encoded audio bytes by writing them to a temp file, filling a sparse
/// buffer through the local reader, and running the real decoder.
fn decode_bytes(bytes: &[u8]) -> bae_core::audio_codec::DecodedAudio {
    use bae_core::playback::data_source::{AudioDataReader, LocalReader};
    use bae_core::playback::sparse_buffer::create_sparse_buffer;
    use std::io::Write;

    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
    let buffer = create_sparse_buffer(bytes.len() as u64);
    let reader = Box::new(LocalReader::new(file.path().to_str().unwrap()));
    reader.start_reading(buffer.clone(), Box::new(|_| {}));
    bae_core::audio_codec::decode_audio(buffer, None, None).expect("decode transcoded bytes")
}
