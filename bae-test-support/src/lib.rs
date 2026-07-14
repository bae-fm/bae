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

/// Pre-populate the Discogs release cache, master cache (if the
/// release carries a `master_id`), and the MB URL-lookup cache for a
/// synthetic test release. The worker's `prepare_release` →
/// `client.get_release` chain hits the cache; the cross-reference call
/// resolves to "no MB link" without touching the network.
pub fn seed_discogs_test_release(release: bae_core::discogs::DiscogsRelease) -> String {
    let id = release.id.clone();
    if let Some(ref master_id) = release.master_id {
        bae_core::discogs::client::seed_master_cache(master_id, release.year, "{}".to_string());
    }
    bae_core::discogs::client::seed_release_cache(&id, (release, "{}".to_string()));
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
/// Start an [`ImportService`] wired to a hermetic cover-art client — the shared
/// setup every import/playback test binary needs, so none of them keep a private
/// copy that can drift from the others.
///
/// [`ImportService`]: bae_core::import::ImportService
pub fn start_test_import(
    runtime_handle: tokio::runtime::Handle,
    library_manager: bae_core::library::LibraryManager,
) -> bae_core::import::ImportServiceHandle {
    bae_core::import::ImportService::start(
        runtime_handle.clone(),
        library_manager,
        bae_core::import::cover_art::CoverArtArchiveClient::hermetic(),
    )
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

pub async fn read_cover_image_blob(
    db: &bae_core::db::Database,
    mgr: &bae_core::library::LibraryManager,
    release_id: &str,
) -> Option<Vec<u8>> {
    let version = db.cover_version(release_id).await.unwrap()?;
    mgr.read_image_blob(&bae_core::album_detail::ImageRef {
        id: release_id.to_string(),
        version,
        image_type: bae_core::db::LibraryImageType::Cover,
    })
    .await
    .unwrap()
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
    // Unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
    let library_id = format!("test-{}", uuid::Uuid::new_v4());
    let mut config = bae_core::config::Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    // Seed both stores the way production's `set_discogs_key` does: the keyring
    // holds the token and the config records the validation. `discogs_client`
    // gates on both, so seeding only the keyring leaves it unusable.
    config.discogs = Some(bae_core::config::DiscogsValidation::Valid);
    let key_service = bae_core::keys::StoreKeys::new(library_id);
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
        library_dir,
        "Test Library".to_string(),
    );
    config.save_to_config_yaml().expect("save config");
    config.save_active_library().expect("save active library");

    let db_path = config.store_dir.db_path();
    let database = runtime
        .block_on(bae_core::db::Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        ))
        .expect("create database");

    // coven's StoreKeys reads the keyring; seed the encryption key there so the
    // sync codepaths these tests exercise find it (instead of the OS keyring).
    // Namespace it under this library's unique id so the shared process-global
    // mock store (see `install_test_keyring`) can't collide across tests.
    bae_core::config::install_test_keyring();
    let enc_key_hex = hex::encode([42u8; 32]);
    let key_service = bae_core::keys::StoreKeys::new(library_id);
    key_service
        .set_encryption_key(&enc_key_hex)
        .expect("seed encryption key into test keyring");
    let config_handle = std::sync::Arc::new(bae_core::config::ConfigHandle::new(config));
    let lm = bae_core::library::LibraryManager::new(
        database,
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        runtime.handle().clone(),
    );

    (lm, tmp)
}
