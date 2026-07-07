/// Write one tagged FLAC into `dir` (copied from the test fixture) with the
/// given `title`, so an Unknown-identity import can map it from file tags.
/// Returns the on-disk bytes after tagging.
#[allow(dead_code)]
pub fn write_tagged_flac(dir: &std::path::Path, filename: &str, title: &str) -> Vec<u8> {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/flac/01 Test Track 1.flac");
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

/// Pre-populate the Discogs release cache, master cache (if the
/// release carries a `master_id`), and the MB URL-lookup cache for a
/// synthetic test release. The worker's `prepare_release` →
/// `client.get_release` chain hits the cache; the cross-reference call
/// resolves to "no MB link" without touching the network.
#[allow(dead_code)]
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
/// confirmation.
/// Panics on failure.
#[allow(dead_code)]
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
#[allow(dead_code)]
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

#[allow(dead_code)]
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

/// Initialize tracing for tests with proper test output handling
#[allow(dead_code)]
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
/// HTTP work. (coven's KeyService reads the keyring, not env vars.)
#[allow(dead_code)]
pub fn test_config_and_keys(
    library_dir: &coven::LibraryDir,
) -> (
    std::sync::Arc<bae_core::config::ConfigHandle>,
    bae_core::keys::KeyService,
) {
    use bae_core::keys::BaeKeyServiceExt;
    bae_core::config::install_test_keyring();
    // Unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
    let library_id = format!("test-{}", uuid::Uuid::new_v4());
    let config = bae_core::config::Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    let key_service = bae_core::keys::KeyService::new(library_id);
    key_service
        .set_discogs_key("test-discogs-token")
        .expect("seed discogs key into test keyring");
    (
        std::sync::Arc::new(bae_core::config::ConfigHandle::new(config)),
        key_service,
    )
}

/// Set up a fresh library + LibraryManager using the real codepath
/// (LibraryDir::create + active-library pointer + saved config.yaml). No sync
/// manager — tests configure sync themselves via connect_*/save_s3_config.
#[allow(dead_code)]
pub fn setup_fresh_library(
    runtime: &tokio::runtime::Runtime,
) -> (bae_core::library::LibraryManager, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let library_id = uuid::Uuid::new_v4().to_string();
    let device_id = uuid::Uuid::new_v4().to_string();
    let library_dir = coven::LibraryDir::new(tmp.path().join("libraries").join(&library_id));
    std::fs::create_dir_all(&*library_dir).expect("create library dir");
    let config = bae_core::config::Config::with_defaults(
        library_id.clone(),
        device_id,
        library_dir,
        "Test Library".to_string(),
    );
    config.save_to_config_yaml().expect("save config");
    config.save_active_library().expect("save active library");

    let db_path = config.library_dir.db_path();
    let database = runtime
        .block_on(bae_core::db::Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(coven::SystemClock),
        ))
        .expect("create database");

    // coven's KeyService reads the keyring; seed the encryption key there so the
    // sync codepaths these tests exercise find it (instead of the OS keyring).
    // Namespace it under this library's unique id so the shared process-global
    // mock store (see `install_test_keyring`) can't collide across tests.
    bae_core::config::install_test_keyring();
    let enc_key_hex = hex::encode([42u8; 32]);
    let key_service = bae_core::keys::KeyService::new(library_id);
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
        runtime.handle().clone(),
    );

    (lm, tmp)
}

/// Read a test env var with a default fallback. `NotPresent` silently uses
/// the default (the intended path); `NotUnicode` panics so a misconfigured
/// env var fails loudly instead of silently substituting bytes-as-default.
#[allow(dead_code)]
pub fn test_env(name: &str, default: &str) -> String {
    match std::env::var(name) {
        Ok(s) => s,
        Err(std::env::VarError::NotPresent) => default.to_string(),
        Err(std::env::VarError::NotUnicode(raw)) => {
            panic!("test env var {name} is non-utf8: {raw:?}");
        }
    }
}

/// Test S3 endpoint + credentials. Defaults target a local minio at
/// `localhost:19000` with creds `baetest`/`baetestpass`.
#[allow(dead_code)]
pub struct TestS3Endpoint {
    pub url: String,
    pub access_key: String,
    pub secret_key: String,
}

#[allow(dead_code)]
impl TestS3Endpoint {
    pub fn from_env() -> Self {
        Self {
            url: test_env("BAE_TEST_S3_URL", "http://localhost:19000"),
            access_key: test_env("BAE_TEST_S3_KEY", "baetest"),
            secret_key: test_env("BAE_TEST_S3_SECRET", "baetestpass"),
        }
    }

    /// Build an `S3ConfigData` against this endpoint for the given bucket,
    /// optionally overriding the secret key (for negative tests).
    pub fn config(&self, bucket: &str, secret_override: Option<&str>) -> S3ConfigData {
        S3ConfigData {
            bucket: bucket.to_string(),
            region: "us-east-1".to_string(),
            endpoint: Some(self.url.clone()),
            key_prefix: None,
            access_key: self.access_key.clone(),
            secret_key: secret_override
                .map(str::to_string)
                .unwrap_or_else(|| self.secret_key.clone()),
            storage: bae_core::config::HomeStorage::Opaque,
        }
    }

    /// Provision a uniquely-named bucket against this endpoint via a direct
    /// AWS SDK call. `S3CloudHome` itself has no CreateBucket affordance — by
    /// design, since production sync setup never creates buckets — so the
    /// test harness goes through the raw SDK here.
    pub async fn provision_bucket(&self, name: &str) {
        use aws_config::{BehaviorVersion, Region};
        use aws_credential_types::Credentials;
        let credentials = Credentials::new(
            self.access_key.clone(),
            self.secret_key.clone(),
            None,
            None,
            "bae-test",
        );
        let http_client = aws_smithy_http_client::Builder::new()
            .tls_provider(aws_smithy_http_client::tls::Provider::Rustls(
                aws_smithy_http_client::tls::rustls_provider::CryptoMode::Ring,
            ))
            .build_https();
        let cfg = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .credentials_provider(credentials)
            .http_client(http_client)
            .endpoint_url(&self.url)
            .load()
            .await;
        let s3 = aws_sdk_s3::config::Builder::from(&cfg)
            .force_path_style(true)
            .build();
        let client = aws_sdk_s3::Client::from_conf(s3);
        client
            .create_bucket()
            .bucket(name)
            .send()
            .await
            .expect("provision bucket");
    }
}

#[allow(unused_imports)]
use bae_core::sync::S3ConfigData;

// ---------------------------------------------------------------------------
// MockCloudHome
// ---------------------------------------------------------------------------

/// In-memory `CloudHome` for transfer/storage tests. Stores uploaded bytes
/// keyed by cloud key; `read` returns them. `fail_writes` makes uploads error
/// (to drive "upload fails" paths). Methods the tests don't exercise panic
/// loudly so a wrong call site is obvious.
#[allow(dead_code)]
pub struct MockCloudHome {
    blobs: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
    fail_writes: std::sync::atomic::AtomicBool,
    /// How many of the next `read_range` calls error before serving real bytes.
    /// Drives the pin-retry test: a transient stall must not fail the pin.
    fail_next_range_reads: std::sync::atomic::AtomicUsize,
    /// Count of `read` (full-object) calls — a chunked pin must issue none.
    full_reads: std::sync::atomic::AtomicUsize,
}

#[allow(dead_code)]
impl MockCloudHome {
    pub fn new() -> Self {
        Self {
            blobs: std::sync::Mutex::new(std::collections::HashMap::new()),
            fail_writes: std::sync::atomic::AtomicBool::new(false),
            fail_next_range_reads: std::sync::atomic::AtomicUsize::new(0),
            full_reads: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn failing() -> Self {
        let m = Self::new();
        m.fail_writes
            .store(true, std::sync::atomic::Ordering::SeqCst);
        m
    }

    /// Arm `write` to fail, so a test can connect over a working home (its sync
    /// bootstrap writes succeed) and then make uploads fail before driving the
    /// drain.
    pub fn arm_write_failures(&self) {
        self.fail_writes
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Make the next `n` `read_range` calls fail before any serve real bytes.
    pub fn fail_next_range_reads(&self, n: usize) {
        self.fail_next_range_reads
            .store(n, std::sync::atomic::Ordering::SeqCst);
    }

    /// How many full-object `read` calls have been issued.
    pub fn full_read_count(&self) -> usize {
        self.full_reads.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Seed a blob directly (e.g. an encrypted file for a CloudOnly read).
    pub fn put(&self, key: &str, data: Vec<u8>) {
        self.blobs.lock().unwrap().insert(key.to_string(), data);
    }

    pub fn contains(&self, key: &str) -> bool {
        self.blobs.lock().unwrap().contains_key(key)
    }

    /// Drop a blob (e.g. to drive a missing-blob read failure).
    pub fn remove(&self, key: &str) {
        self.blobs.lock().unwrap().remove(key);
    }

    pub fn key_count(&self) -> usize {
        self.blobs.lock().unwrap().len()
    }
}

#[async_trait::async_trait]
impl coven::CloudHome for MockCloudHome {
    async fn put_object(&self, key: &str, data: Vec<u8>) -> Result<(), coven::CloudHomeError> {
        if self.fail_writes.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(coven::CloudHomeError::Storage(
                "mock write failure".to_string(),
            ));
        }
        self.blobs.lock().unwrap().insert(key.to_string(), data);
        Ok(())
    }

    async fn open_multipart<'a>(
        &'a self,
        _key: &str,
        _total_len: u64,
    ) -> Result<coven::BoxPartSink<'a>, coven::CloudHomeError> {
        unimplemented!("multipart uploads not used by storage transition tests")
    }

    fn multipart_threshold(&self) -> u64 {
        u64::MAX
    }

    async fn read(&self, key: &str) -> Result<Vec<u8>, coven::CloudHomeError> {
        self.full_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.blobs
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| coven::CloudHomeError::Storage(format!("missing key {key}")))
    }

    /// Serve `start..end` (inclusive..exclusive) of the stored blob. The first
    /// `fail_next_range_reads` calls error to exercise the retry path.
    async fn read_range(
        &self,
        key: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<u8>, coven::CloudHomeError> {
        if self
            .fail_next_range_reads
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |n| n.checked_sub(1),
            )
            .is_ok()
        {
            return Err(coven::CloudHomeError::Storage(
                "mock range read failure".to_string(),
            ));
        }

        let blobs = self.blobs.lock().unwrap();
        let blob = blobs
            .get(key)
            .ok_or_else(|| coven::CloudHomeError::Storage(format!("missing key {key}")))?;
        let start = usize::try_from(start).unwrap();
        let end = usize::try_from(end).unwrap();
        if start > end || end > blob.len() {
            return Err(coven::CloudHomeError::Storage(format!(
                "range {start}..{end} outside blob length {}",
                blob.len()
            )));
        }
        Ok(blob[start..end].to_vec())
    }

    async fn list(&self, _prefix: &str) -> Result<Vec<String>, coven::CloudHomeError> {
        Ok(self.blobs.lock().unwrap().keys().cloned().collect())
    }

    async fn delete(&self, key: &str) -> Result<(), coven::CloudHomeError> {
        self.blobs.lock().unwrap().remove(key);
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, coven::CloudHomeError> {
        Ok(self.blobs.lock().unwrap().contains_key(key))
    }

    async fn grant_access(
        &self,
        _grant: coven::CloudAccessGrant,
    ) -> Result<coven::CloudHomeJoinInfo, coven::CloudHomeError> {
        unimplemented!("grant_access not used by storage transition tests")
    }

    async fn revoke_access(
        &self,
        _revoke: coven::CloudAccessRevoke,
    ) -> Result<(), coven::CloudHomeError> {
        unimplemented!("revoke_access not used by storage transition tests")
    }
}
