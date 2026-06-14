//! Construction of bae's sync manager + the `LibraryManager`↔bridge sync DTOs.
//!
//! The sync manager itself is coven's — bae uses it directly (re-exported here so
//! `crate::sync::sync_manager::SyncManager` resolves). `build_sync_manager` wires
//! it up with bae's pieces: a config provider that reads bae's live `ConfigHandle`
//! (so connect/disconnect are picked up without rebuilding), bae's blob plan, and
//! the upload observer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::config::ConfigHandle;
use crate::db::Database;
use crate::encryption::EncryptionService;
use crate::keys::KeyService;
use crate::library::{LibraryEvent, UploadThroughput};
use crate::sync::blob_plan::{BaeBlobPlan, ReleaseUploadObserver};

// coven owns the sync manager; bae uses it directly.
pub use coven::sync::sync_manager::SyncManager;

/// S3 configuration data for save_s3_config.
pub struct S3ConfigData {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub key_prefix: Option<String>,
    pub access_key: String,
    pub secret_key: String,
}

/// Build coven's `SyncManager` wired with bae's config provider, blob plan, and
/// upload observer. The provider reads bae's live config whenever coven needs it,
/// so connecting/disconnecting a provider is reflected without rebuilding.
///
/// The manager takes the same `coven::Database` the host opened; it reads the
/// synced-table set and the shared register clock from it, so the sync loop's
/// advance-on-pull and envelope stamps order against the clock the host stamps
/// rows from. Construction is synchronous and infallible: seeding happened in
/// [`coven::Database::open`] at startup.
#[allow(clippy::too_many_arguments)]
pub fn build_sync_manager(
    config_handle: Arc<ConfigHandle>,
    key_service: KeyService,
    encryption_service: EncryptionService,
    database: Database,
    outbox_in_flight: Arc<Mutex<HashMap<String, u64>>>,
    upload_throughput: Arc<UploadThroughput>,
    sync_paused: Arc<std::sync::atomic::AtomicBool>,
    events: broadcast::Sender<LibraryEvent>,
) -> SyncManager {
    let clock = database.clock().clone();
    let coven_db = database.coven_db().clone();
    let library_dir = config_handle.config().library_dir.clone();
    let blob_plan: Arc<dyn coven::blob::BlobPlan> = Arc::new(BaeBlobPlan::new(library_dir.clone()));
    let observer: Arc<dyn coven::blob::BlobUploadObserver> = Arc::new(ReleaseUploadObserver::new(
        Arc::new(database),
        library_dir,
        outbox_in_flight,
        upload_throughput,
        sync_paused,
        events,
    ));

    let ch = config_handle;
    let config_provider: coven::sync::sync_manager::ConfigProvider =
        Arc::new(move || ch.config().to_coven());

    SyncManager::new(
        config_provider,
        key_service,
        encryption_service,
        coven_db,
        clock,
        blob_plan,
        Some(observer),
    )
}
