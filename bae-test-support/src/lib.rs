//! Helpers shared by bae-core's integration tests.
//!
//! Every `bae-core/tests/*.rs` binary is its own crate and reaches these through
//! `use bae_test_support as support;`. Keeping them in a library — rather than a
//! `mod support;` file textually recompiled into each binary — is what lets
//! `dead_code` and `unreachable_pub` stay armed here: the helpers are this
//! crate's public API, so neither lint has to be silenced for the binaries that
//! happen not to call a given one.

/// The FLAC fixture tree lives in bae-core, so it is reached relative to the
/// workspace root — `CARGO_MANIFEST_DIR` here is this crate, not bae-core.
fn bae_core_fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("bae-test-support sits one level under the workspace root")
        .join("bae-core/tests/fixtures")
}

/// Convert a decode's full-range i32 PCM samples to the f32 shape emitted by
/// streaming playback, for comparing a ground-truth decode against captured
/// playback output.
pub fn samples_as_f32(decoded: &bae_core::audio_codec::DecodedAudio) -> Vec<f32> {
    let scale = i32::MAX as f32;
    decoded.samples.iter().map(|&s| s as f32 / scale).collect()
}

/// Align captured playback samples against a decoded reference and assert they
/// match. The streaming decoder can begin a captured region slightly before the
/// reference (a seek lands on a codec frame boundary), so this searches for the
/// whole-sample offset in `[0, max_alignment]` that minimizes the per-sample max
/// difference against the reference's first 500 frames, asserts that best offset
/// is within `tolerance`, then asserts up to `compare_samples` samples from it
/// match within `tolerance` (capped so neither buffer is over-read). `label`
/// names the subject in panic messages. Returns the aligned offset in samples.
pub fn assert_captured_matches_reference(
    captured: &[f32],
    reference: &[f32],
    channels: usize,
    sample_rate: u32,
    max_alignment: usize,
    compare_samples: usize,
    tolerance: f32,
    label: &str,
) -> usize {
    let sample_ms = |sample: usize| sample as f64 / channels as f64 / sample_rate as f64 * 1000.0;
    let snippet_len = 500 * channels;
    let limit = max_alignment.min(captured.len().saturating_sub(snippet_len));

    let mut best_max_diff = f32::MAX;
    let mut best_offset = 0usize;
    for offset in 0..=limit {
        let mut max_diff = 0.0f32;
        for i in 0..snippet_len {
            let diff = (captured[offset + i] - reference[i]).abs();
            max_diff = max_diff.max(diff);
            if max_diff > best_max_diff {
                break;
            }
        }
        if max_diff < best_max_diff {
            best_max_diff = max_diff;
            best_offset = offset;
        }
    }

    assert!(
        best_max_diff < tolerance,
        "could not align {label} with the reference within tolerance {tolerance:.6}: \
         best offset {:.1}ms, max sample diff {best_max_diff:.6}",
        sample_ms(best_offset),
    );

    let compare_count = compare_samples
        .min(captured.len() - best_offset)
        .min(reference.len());
    for i in 0..compare_count {
        let diff = (captured[best_offset + i] - reference[i]).abs();
        assert!(
            diff < tolerance,
            "{label} audio mismatch at index {i} ({:.1}ms): captured={:.6}, reference={:.6}",
            sample_ms(i),
            captured[best_offset + i],
            reference[i],
        );
    }
    best_offset
}

/// Write one tagged FLAC into `dir` (copied from the test fixture) with the
/// given `title`, so an Unknown-identity import can map it from file tags.
/// Returns the on-disk bytes after tagging.
pub fn write_tagged_flac(dir: &std::path::Path, filename: &str, title: &str) -> Vec<u8> {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    let fixture = bae_core_fixtures().join("flac/01 Test Track 1.flac");
    let flac = std::fs::read(&fixture).expect("FLAC fixture missing");

    let dest = dir.join(filename);
    std::fs::write(&dest, &flac).unwrap();
    let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.to_string());
    tag.set_artist("Artist Name".to_string());
    tag.set_album("Album Title".to_string());
    tag.insert_text(ItemKey::AlbumArtist, "Artist Name".to_string());
    tag.set_track(1);
    tagged.insert_tag(tag);
    tagged
        .save_to_path(&dest, WriteOptions::default())
        .expect("write tags");

    std::fs::read(&dest).unwrap()
}

/// Copy `source` into `dest_dir/name`, stamp Vorbis-comment tags on it (title,
/// artist, album, album artist, year, track), and return the destination path.
/// For tests that need a tagged audio file derived from a specific fixture.
pub fn copy_and_tag(
    source: &std::path::Path,
    dest_dir: &std::path::Path,
    name: &str,
    title: &str,
    artist: &str,
    album_title: &str,
    album_artist: &str,
    year: u16,
    track: u32,
) -> std::path::PathBuf {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::items::Timestamp;
    use lofty::tag::{Tag, TagType};

    let dest = dest_dir.join(name);
    std::fs::copy(source, &dest).expect("copy fixture");

    let mut tagged = lofty::read_from_path(&dest).expect("read for tagging");
    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title(title.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album_title.to_string());
    tag.insert_text(ItemKey::AlbumArtist, album_artist.to_string());
    tag.set_date(Timestamp {
        year,
        month: None,
        day: None,
        hour: None,
        minute: None,
        second: None,
    });
    tag.set_track(track);
    tagged.insert_tag(tag);
    tagged
        .save_to_path(&dest, WriteOptions::default())
        .expect("save tags");
    dest
}

/// Drain library events from `rx`, returning them in arrival order once no new
/// event arrives within `timeout` (the quiet-window settle). Positive
/// assertions check the expected events are present; negative ones check the
/// returned set is empty.
pub async fn collect_library_events(
    rx: &mut tokio::sync::broadcast::Receiver<bae_core::library::LibraryEvent>,
    timeout: std::time::Duration,
) -> Vec<bae_core::library::LibraryEvent> {
    let mut events = Vec::new();
    while let Ok(Ok(event)) = tokio::time::timeout(timeout, rx.recv()).await {
        events.push(event);
    }
    events
}

/// The id [`seed_discogs_test_release`] renders for a fixture's own spelling of
/// a release or master id.
///
/// Discogs' release endpoint numbers its ids, so a fixture that writes
/// `"master-exact"` is archived — and read back — under a number. A test that
/// asserts on the id it seeded asks for it here rather than hard-coding the
/// rendering.
pub fn discogs_fixture_id(fixture_id: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fixture_id.hash(&mut hasher);
    // Kept inside 2^53 so the id survives any JSON reader that stores numbers
    // as doubles.
    (hasher.finish() % (1 << 53)).to_string()
}

/// Pre-populate the Discogs release cache, master cache (if the release carries
/// a `master_id`), and the MB URL-lookup cache for a synthetic test release, and
/// return the release id the import should pick. The worker's
/// `prepare_release` → `client.get_release` chain hits the cache; the
/// cross-reference call resolves to "no MB link" without touching the network.
///
/// `release` describes what the test wants; this renders it as the release
/// endpoint's own JSON and reads it back through the production parser, so the
/// two halves of the cache entry cannot disagree. That matters because the raw
/// JSON is what the import archives, and every later projection of the release
/// — a reset, a re-open — replays from the archived bytes rather than from
/// whatever a test handed over alongside them.
///
/// The rendered ids are numeric, as the endpoint's are, so the returned id is
/// not the (arbitrary) one the caller wrote on the fixture.
pub fn seed_discogs_test_release(release: bae_core::discogs::DiscogsRelease) -> String {
    let numeric = |value: &str| -> u64 {
        discogs_fixture_id(value)
            .parse()
            .expect("a rendered Discogs id is numeric")
    };
    let credit = |artist: &bae_core::discogs::DiscogsArtist| serde_json::json!({ "id": numeric(&artist.id), "name": artist.name });
    let role_credit = |artist: &bae_core::discogs::DiscogsRoleArtist| {
        serde_json::json!({
            "id": artist.id.as_deref().map(numeric),
            "name": artist.name,
            "role": artist.role,
            "anv": artist.credited_name,
        })
    };
    let master_id = release.master_id.as_deref().map(numeric);

    if let Some(master_id) = master_id {
        // Keyed by the rendered id, which is the one the parsed release names
        // and therefore the one the master fetch asks for.
        let master_json = serde_json::json!({ "id": master_id, "year": release.year });
        bae_core::discogs::client::seed_master_cache(
            &master_id.to_string(),
            release.year,
            master_json.to_string(),
        );
    }

    let raw_json = serde_json::json!({
        "id": numeric(&release.id),
        "title": release.title,
        "year": release.year,
        "country": release.country,
        "master_id": master_id,
        "formats": release.format.iter().map(|name| serde_json::json!({ "name": name })).collect::<Vec<_>>(),
        "labels": release.label.iter().enumerate().map(|(index, name)| serde_json::json!({
            "name": name,
            "catno": if index == 0 { release.catno.clone() } else { None },
        })).collect::<Vec<_>>(),
        "images": release.cover_image.iter().map(|uri| serde_json::json!({
            "type": "primary",
            "uri": uri,
            "uri150": release.thumb,
        })).collect::<Vec<_>>(),
        "artists": release.artists.iter().map(credit).collect::<Vec<_>>(),
        "extraartists": release.extraartists.as_ref().map(|artists| {
            artists.iter().map(role_credit).collect::<Vec<_>>()
        }),
        "tracklist": release.tracklist.iter().map(|track| serde_json::json!({
            "position": track.position,
            "title": track.title,
            "duration": track.duration,
            "type_": track.type_,
            "artists": track.artists.iter().map(credit).collect::<Vec<_>>(),
            "extraartists": track.extraartists.as_ref().map(|artists| {
                artists.iter().map(role_credit).collect::<Vec<_>>()
            }),
        })).collect::<Vec<_>>(),
    })
    .to_string();

    let parsed = bae_core::discogs::client::parse_discogs_release_json(&raw_json)
        .expect("the rendered test release parses as the endpoint's own JSON");
    let id = parsed.id.clone();
    bae_core::discogs::client::seed_release_cache(&id, (parsed, raw_json));
    bae_core::musicbrainz::seed_discogs_url_lookup(&id, None);
    id
}

fn import_terminal_ids(progress: &bae_core::import::ImportProgress) -> Option<(String, String)> {
    match progress {
        bae_core::import::ImportProgress::Complete { id, album_id, .. }
        | bae_core::import::ImportProgress::RemoteUploadQueued { id, album_id, .. } => {
            Some((id.clone(), album_id.clone()))
        }
        _ => None,
    }
}

/// Wait for the import worker to finish, returning (release_id, album_id).
///
/// Local imports emit `Complete`. Remote imports emit `RemoteUploadQueued`: the
/// import worker is finished, while remote completion waits for coven upload
/// Start an [`ImportService`] over a test library manager — the shared setup
/// every import/playback test binary needs, so none of them keep a private copy
/// that can drift from the others. The cover-art client comes off the manager,
/// which builds a hermetic one.
///
/// [`ImportService`]: bae_core::import::ImportService
pub async fn start_test_import(
    runtime_handle: tokio::runtime::Handle,
    library_manager: bae_core::library::LibraryManager,
) -> bae_core::import::ImportServiceHandle {
    library_manager
        .start_import_service(runtime_handle.clone())
        .await
        .expect("test import service starts")
}

/// confirmation.
/// Panics on failure.
pub async fn wait_for_import_complete(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> (String, String) {
    while let Some(progress) = progress_rx.recv().await {
        if let Some(ids) = import_terminal_ids(&progress) {
            return ids;
        }
        if let bae_core::import::ImportProgress::Failed { error, .. } = &progress {
            panic!("Import failed: {}", error);
        }
    }
    panic!("Progress channel closed without completion");
}

/// Like `wait_for_import_complete` but returns Result instead of panicking.
///
/// Used by test fixtures that catch setup errors gracefully (e.g., returning
/// early from a test when a fixture file fails validation).
pub async fn try_wait_for_import_complete(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> Result<(String, String), String> {
    while let Some(progress) = progress_rx.recv().await {
        if let Some(ids) = import_terminal_ids(&progress) {
            return Ok(ids);
        }
        if let bae_core::import::ImportProgress::Failed { error, .. } = &progress {
            return Err(error.clone());
        }
    }
    Err("Progress channel closed without completion".to_string())
}

/// Write a small solid-color PNG to `path` — a folder cover an import picks up,
/// so a test release actually has art (for exercising cover-embedding paths).
pub fn write_cover_png(path: &std::path::Path) {
    std::fs::write(path, cover_png()).expect("write cover png");
}

/// A small solid-color PNG, big enough to clear the download path's 100-byte
/// floor — the bytes a stand-in image host serves.
pub fn cover_png() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(16, 16, image::Rgb([90, 30, 160]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("encode cover png");
    bytes.into_inner()
}

/// A local stand-in for the Cover Art Archive, shared by every test in a
/// binary.
///
/// Cover addresses are derived from a release's MusicBrainz ids rather than
/// looked up, so any fixture whose release document says the archive holds a
/// front image sends the commit to one of those addresses. This answers them on
/// localhost. An address no test registered answers 404 — the archive holding
/// nothing there, which is both what an unregistered release means and what
/// keeps the rest of the suite from reaching the real service.
pub struct CoverArtArchive {
    routes: std::sync::Mutex<std::collections::HashMap<String, (u16, Vec<u8>)>>,
}

impl CoverArtArchive {
    /// Serve `bytes` as a MusicBrainz release's front image, at both the full
    /// and the thumbnail address.
    pub fn serve_front(&self, release_id: &str, bytes: Vec<u8>) {
        self.answer_front(release_id, 200, bytes);
    }

    /// Answer `status` for a release's front image, for a test driving the
    /// download's failure path.
    pub fn fail_front(&self, release_id: &str, status: u16) {
        self.answer_front(release_id, status, Vec::new());
    }

    fn answer_front(&self, release_id: &str, status: u16, bytes: Vec<u8>) {
        let mut routes = self.routes.lock().expect("archive routes mutex poisoned");
        for suffix in ["front", "front-250"] {
            routes.insert(
                format!("/release/{release_id}/{suffix}"),
                (status, bytes.clone()),
            );
        }
    }
}

/// The binary's stand-in archive, started and pointed at on first use.
pub fn cover_art_archive() -> &'static CoverArtArchive {
    static ARCHIVE: std::sync::OnceLock<&'static CoverArtArchive> = std::sync::OnceLock::new();
    ARCHIVE.get_or_init(start_cover_art_archive)
}

fn start_cover_art_archive() -> &'static CoverArtArchive {
    use axum::extract::{Request, State};
    use axum::http::StatusCode;

    let archive: &'static CoverArtArchive = Box::leak(Box::new(CoverArtArchive {
        routes: std::sync::Mutex::new(std::collections::HashMap::new()),
    }));

    async fn handler(
        State(archive): State<&'static CoverArtArchive>,
        request: Request,
    ) -> (
        StatusCode,
        [(axum::http::HeaderName, &'static str); 1],
        Vec<u8>,
    ) {
        let answer = archive
            .routes
            .lock()
            .expect("archive routes mutex poisoned")
            .get(request.uri().path())
            .cloned();
        let (status, bytes) = answer.unwrap_or((404, Vec::new()));
        (
            StatusCode::from_u16(status).expect("a valid stub status"),
            [(axum::http::header::CONTENT_TYPE, "image/png")],
            bytes,
        )
    }

    // Its own runtime on its own thread: the archive outlives each `#[tokio::test]`
    // that reaches it, so it cannot live on any one test's runtime.
    let (address_tx, address_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the stub archive's runtime builds");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("the stub archive binds");
            address_tx
                .send(
                    listener
                        .local_addr()
                        .expect("the stub archive has an address"),
                )
                .expect("the starting thread is waiting for the address");
            let app = axum::Router::new().fallback(handler).with_state(archive);
            let _ = axum::serve(listener, app).await;
        });
    });

    let address = address_rx.recv().expect("the stub archive starts");
    bae_core::import::cover_art::set_base_url_for_test(Some(format!("http://{address}")));
    archive
}

pub async fn read_cover_image_blob(
    mgr: &bae_core::library::LibraryManager,
    release_id: &str,
) -> Option<Vec<u8>> {
    mgr.read_cover_image_blob(release_id).await.unwrap()
}

pub fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

/// Create test config_handle + key_service for integration tests.
/// Seeds a dummy Discogs key into the in-memory test keyring so the worker can
/// build a `DiscogsClient` and consult the seeded LRU caches without doing real
/// HTTP work. (coven's StoreKeys reads the keyring, not env vars.)
pub fn test_config_and_keys(
    library_dir: &coven::StoreDir,
) -> (
    std::sync::Arc<bae_core::config::ConfigHandle>,
    bae_core::keys::StoreKeys,
) {
    use bae_core::keys::BaeStoreKeysExt;
    bae_core::config::install_test_keyring();
    // No test has any business reaching api.discogs.com. Point every client
    // built from here at a port nothing listens on: the seeded session caches
    // answer what the tests actually assert on, and anything else fails fast
    // and locally instead of spending a fixture's fake key on a real auth
    // check — which comes back 401 and marks the stored key rejected for every
    // later call in the process.
    bae_core::discogs::client::set_base_url_for_test(Some("http://127.0.0.1:9".to_string()));
    // Unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
    let library_id = format!("test-{}", uuid::Uuid::new_v4());
    let mut config = bae_core::config::Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir,
        "Test Library".to_string(),
    );
    // Seed both stores the way production's `set_discogs_key` does: the keyring
    // holds the token and the config records the validation. `discogs_client`
    // gates on both, so seeding only the keyring leaves it unusable.
    config.discogs = Some(bae_core::config::DiscogsValidation::Valid);
    let key_service = bae_core::keys::StoreKeys::bind(library_id);
    key_service
        .set_discogs_key("test-discogs-token")
        .expect("seed discogs key into test keyring");
    (
        std::sync::Arc::new(bae_core::config::ConfigHandle::new(config)),
        key_service,
    )
}

/// Set up a fresh library + LibraryManager using the real codepath
/// (StoreDir::create + active-library pointer + saved config.yaml). No sync
/// manager — tests configure sync themselves via connect_*/save_s3_config.
pub fn setup_fresh_library(
    runtime: &tokio::runtime::Runtime,
) -> (bae_core::library::LibraryManager, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let library_id = uuid::Uuid::new_v4().to_string();
    let device_id = uuid::Uuid::new_v4().to_string();
    let library_dir = coven::StoreDir::new(tmp.path().join("libraries").join(&library_id));
    std::fs::create_dir_all(&*library_dir).expect("create library dir");
    let config = bae_core::config::Config::with_defaults(
        library_id.clone(),
        device_id,
        &library_dir,
        "Test Library".to_string(),
    );
    config.save_to_config_yaml().expect("save config");
    config.save_active_library().expect("save active library");

    // coven's StoreKeys reads the keyring; seed the encryption key there so the
    // sync codepaths these tests exercise find it (instead of the OS keyring).
    // Namespace it under this library's unique id so the shared process-global
    // mock store (see `install_test_keyring`) can't collide across tests.
    bae_core::config::install_test_keyring();
    let enc_key_hex = hex::encode([42u8; 32]);
    let key_service = bae_core::keys::StoreKeys::bind(library_id);
    key_service
        .set_encryption_key(&enc_key_hex)
        .expect("seed encryption key into test keyring");
    let config_handle = std::sync::Arc::new(bae_core::config::ConfigHandle::new(config));
    let lm = bae_core::library::LibraryManager::open(
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        runtime.handle().clone(),
        None,
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    )
    .expect("open library manager");

    (lm, tmp)
}

/// A stable v4 UUID for a fixture moniker.
///
/// coven validates every synced row's primary key as a canonical RFC 4122 v4
/// UUID (`RowIdentity::IndependentUuid`) — which is what bae's real ids are, so
/// fixtures must carry UUIDs too. Tests that name a row by a readable moniker
/// (`"test-artist-id"`) or mint one per index (`format!("track-{i}")`) get their
/// id through this: the moniker stays visible at the call site, and the same
/// moniker always maps to the same id within and across runs.
pub fn test_uuid(moniker: &str) -> String {
    // FNV-1a over the moniker, run under four seeds for the 128 bits a UUID
    // needs. Any stable spread works — the value only has to be a well-formed
    // v4 UUID and collision-free across a test's handful of monikers.
    let word = |seed: u64| -> u64 {
        let mut hash = seed;
        for byte in moniker.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    };
    let (high, low) = (word(0xcbf2_9ce4_8422_2325), word(0x9e37_79b9_7f4a_7c15));
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        (high >> 32) as u32,
        (high >> 16) as u16,
        high & 0x0fff,
        (low >> 48) & 0x0fff,
        low & 0xffff_ffff_ffff_u64,
    )
}
