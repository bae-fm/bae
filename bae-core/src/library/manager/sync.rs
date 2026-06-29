//! Cloud sync surface for [`LibraryManager`]: provider connection, the upload
//! pipeline, and pause state.

use super::resolve::*;
use super::*;

impl LibraryManager {
    /// Whether a cloud provider is connected. Reads config, not manager presence:
    /// the connected provider lives in config and is known synchronously from the
    /// first read, whereas the `SyncManager` (and its cloud client) is built lazily
    /// once connected and still absent on mobile when the first listing query runs.
    pub fn is_sync_configured(&self) -> bool {
        self.config_handle.config().cloud_home.provider.is_some()
    }

    pub fn has_cloud_home(&self) -> bool {
        self.sync_connected()
    }

    /// The coven `BlobRef` for a remote release file's audio blob — its identity
    /// in coven's cache (and the cloud on a miss). `cloud_path` is the row's value
    /// RELATIVE to the `release_files` namespace coven prepends. A release file is
    /// a coven **user-provided** blob (the user's own imported file): Local = the
    /// file at the user's path (an external ref coven holds), Remote = uploaded and
    /// `CacheLazy` (fetched into the cache on first read). coven resolves which by
    /// where the bytes are — the same `BlobRef` addresses every locality.
    pub(crate) fn release_file_blob_ref(file: &DbFile) -> coven::BlobRef {
        coven::BlobRef {
            namespace: crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
            id: file.id.clone(),
            scope: coven::BlobScope::Master,
            cloud_path: file.cloud_path.clone(),
            provenance: coven::Provenance::UserProvided,
            fill: coven::CacheFill::CacheLazy,
        }
    }

    /// The one coven data handle. The playback reader clones it to stream blob
    /// ranges; callers route every blob read/write and the sync lifecycle through
    /// it rather than reaching into coven's internals.
    pub(crate) fn handle(&self) -> &CovenHandle {
        &self.handle
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn database_for_test(&self) -> Database {
        self.database.clone()
    }

    /// Configure coven's per-namespace cache budgets for this device: the bulk for
    /// `release_files` (audio), a small reserved slice each for `covers` and
    /// `artist_images`, so each namespace evicts against its own budget and audio
    /// pressure never wipes the cover cache. Device-local; set once at startup.
    pub(crate) async fn configure_cache_budgets(&self) -> Result<(), LibraryError> {
        self.handle
            .set_cache_budget(
                crate::sync::RELEASE_FILES_NAMESPACE,
                crate::sync::RELEASE_FILES_CACHE_BUDGET,
            )
            .await?;
        self.handle
            .set_cache_budget(
                crate::sync::COVERS_NAMESPACE,
                crate::sync::COVERS_CACHE_BUDGET,
            )
            .await?;
        self.handle
            .set_cache_budget(
                crate::sync::ARTIST_IMAGES_NAMESPACE,
                crate::sync::ARTIST_IMAGES_CACHE_BUDGET,
            )
            .await?;
        Ok(())
    }

    /// Store a bae-produced host-provided image and its row in one coven batch.
    pub async fn store_library_image_blob(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), LibraryError> {
        self.database.write_library_image_blob(image, bytes).await?;
        Ok(())
    }

    /// Read a release file's whole plaintext through coven's locality-aware read:
    /// served from the user's file (Local user-provided), coven's local store
    /// (Local host-provided), `storage/pinned`/`storage/cache` on a Remote hit, or
    /// fetched from the cloud (into `cache/`) on a Remote miss. For the
    /// non-streaming readers (export, gallery images); playback streams ranges via
    /// `open_blob_stream` instead. A vanished/changed external file maps to a
    /// storage error so the caller surfaces a "files missing / moved" state.
    pub(crate) async fn read_release_blob(&self, file: &DbFile) -> Result<Vec<u8>, LibraryError> {
        let blob = Self::release_file_blob_ref(file);
        self.handle
            .read_blob(&blob)
            .await
            .map_err(|e| LibraryError::Storage(format!("read of {}: {e}", file.id)))
    }

    /// The coven `BlobRef` for a host-provided library image (a cover or an artist
    /// image) — its identity in coven's local store while Local and its cache while
    /// Remote. `namespace` is `covers` or `artist_images`; `id` is the release id
    /// (a cover) or artist id (an artist image). A host-provided `CacheEager` blob:
    /// the bytes are produced by bae and kept by coven, fetched into the cache on
    /// pull so a grid renders from local bytes. `cloud_path` is the row's readable
    /// path on a browsable home (`None` on an opaque one).
    pub(crate) fn image_blob_ref(
        namespace: &str,
        id: &str,
        cloud_path: Option<String>,
    ) -> coven::BlobRef {
        coven::BlobRef {
            namespace: namespace.to_string(),
            id: id.to_string(),
            scope: coven::BlobScope::Master,
            cloud_path,
            provenance: coven::Provenance::HostProvided,
            fill: coven::CacheFill::CacheEager,
        }
    }

    /// The cover [`ImageRef`] for one release — its image id paired with the
    /// `covers` row's `_updated_at` — or `None` when the release has no cover row.
    pub(super) async fn cover_ref(
        &self,
        release_id: &str,
    ) -> Result<Option<ImageRef>, LibraryError> {
        cover_ref_for(&self.database, release_id).await
    }

    /// The cover [`ImageRef`] for each of `release_ids` that has a `covers` row,
    /// in one query. The batch source for the list/grid resolvers, which build a
    /// `Fn(&str) -> Option<ImageRef>` over the returned map.
    pub(super) async fn cover_refs(
        &self,
        release_ids: &[String],
    ) -> Result<HashMap<String, ImageRef>, LibraryError> {
        Ok(self
            .database
            .cover_versions(release_ids)
            .await?
            .into_iter()
            .map(|(id, version)| (id.clone(), ImageRef { id, version }))
            .collect())
    }

    /// Read a host-provided library image's whole bytes through coven's
    /// locality-aware read: coven's local store while Local, the pinned/evictable
    /// cache or the cloud while Remote. `id` is a release id (a cover) or an artist
    /// id (an artist image); the `covers` row is probed first (the common grid
    /// case), then `artist_images`. `None` when no such image row exists (no cover
    /// produced); a read error surfaces rather than being masked.
    pub async fn read_image_blob(&self, id: &str) -> Result<Option<Vec<u8>>, LibraryError> {
        for (namespace, image_type) in [
            (crate::sync::COVERS_NAMESPACE, LibraryImageType::Cover),
            (
                crate::sync::ARTIST_IMAGES_NAMESPACE,
                LibraryImageType::Artist,
            ),
        ] {
            let Some(row) = self.database.find_library_image(id, &image_type).await? else {
                continue;
            };
            let blob = Self::image_blob_ref(namespace, id, row.cloud_path.clone());
            let bytes = self
                .handle
                .read_blob(&blob)
                .await
                .map_err(|e| LibraryError::Storage(format!("read image {id}: {e}")))?;
            return Ok(Some(bytes));
        }
        Ok(None)
    }

    /// Whether coven holds this release pinned on this device — true iff its
    /// representative blob (any one of the release's files; pin/unpin act on all a
    /// release's blobs together) is kept in coven's `storage/pinned/`. `None` (a
    /// release with no files) reads as not pinned. Pinned-ness is coven cache
    /// state, never a bae column — answered through the handle, not by stat-ing
    /// coven's cache layout.
    pub(crate) async fn release_pinned(
        &self,
        any_file_id: Option<&str>,
    ) -> Result<bool, LibraryError> {
        match any_file_id {
            Some(file_id) => release_file_pinned(&self.handle, file_id).await,
            None => Ok(false),
        }
    }

    /// Pin a remote release's blobs for offline: coven fetches every blob into
    /// `storage/pinned/` (from the evictable cache if already there, else the
    /// cloud). Idempotent. Pinned-ness is coven cache state — there is no bae flag.
    /// The low-level cache op behind the "Pin" transition.
    pub(crate) async fn pin_release_blobs(&self, release_id: &str) -> Result<(), LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let blobs: Vec<_> = files.iter().map(Self::release_file_blob_ref).collect();
        self.handle
            .pin(&blobs)
            .await
            .map_err(|e| LibraryError::Storage(format!("pin release {release_id}: {e}")))
    }

    /// Unpin a remote release's blobs: coven moves every blob from
    /// `storage/pinned/` to the evictable `storage/cache/` (still readable, now
    /// droppable). No cloud read, no bae flag. The low-level cache op behind the
    /// "Unpin" transition.
    pub(crate) async fn unpin_release_blobs(&self, release_id: &str) -> Result<(), LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let blobs: Vec<_> = files.iter().map(Self::release_file_blob_ref).collect();
        self.handle
            .unpin(&blobs)
            .await
            .map_err(|e| LibraryError::Storage(format!("unpin release {release_id}: {e}")))
    }

    /// Pause or resume the cloud-upload pipeline. Paused means new enqueues
    /// still land in the outbox but the sync cycle won't drain them; in-flight
    /// uploads finish (coven's `drain_uploads` checks the flag between
    /// entries, not mid-write). Re-emits the outbox snapshot so the UI's
    /// paused indicator and the bottom-panel summary update.
    pub async fn set_sync_paused(&self, paused: bool) {
        self.sync_paused
            .store(paused, std::sync::atomic::Ordering::SeqCst);
        if !paused {
            // Kick the loop so the queue starts draining immediately on resume
            // rather than waiting for the next idle tick.
            self.trigger_sync();
        }
        self.emit_outbox_changed().await;
    }

    /// Current paused state of the upload pipeline. The snapshot builder
    /// reads this so the UI can render its paused indicator.
    pub fn is_sync_paused(&self) -> bool {
        self.sync_paused.load(std::sync::atomic::Ordering::SeqCst)
    }

    // Sync provider configuration

    /// Whether the background sync loop is running and draining uploads. The
    /// manage gate requires this: managing has no inline remote flip — the
    /// release only becomes remote once the upload observer (which fires from
    /// inside the running loop) confirms the last upload landed.
    pub fn is_sync_ready(&self) -> bool {
        self.sync_connected() && self.handle.is_syncing()
    }

    pub fn trigger_sync(&self) {
        self.handle.sync_now();
    }

    pub async fn save_s3_config(&self, data: S3ConfigData) -> Result<(), String> {
        use crate::keys::CloudHomeCredentials;
        use coven::CloudHome;
        use coven::S3CloudHome;

        // Probe the bucket with the proposed credentials *before* persisting
        // anything. A typo or a missing bucket would otherwise leave the UI
        // showing "Connected" and the user discovering broken sync only via
        // the reconnect banner after the first failed cycle.
        let probe_home = S3CloudHome::new(
            data.bucket.clone(),
            data.region.clone(),
            data.endpoint.clone(),
            data.access_key.clone(),
            data.secret_key.clone(),
            data.key_prefix.clone(),
        )
        .await
        .map_err(|e| format!("Failed to build S3 client: {e}"))?;
        probe_home.probe().await.map_err(|e| format!("{e}"))?;

        let creds = CloudHomeCredentials::S3 {
            access_key: data.access_key,
            secret_key: data.secret_key,
        };
        self.key_service
            .set_cloud_home_credentials(&creds)
            .map_err(|e| format!("Failed to save credentials: {e}"))?;

        self.config_handle
            .update(move |c| {
                c.cloud_home.provider = Some(CloudProvider::S3);
                c.cloud_home.s3_bucket = Some(data.bucket);
                c.cloud_home.s3_region = Some(data.region);
                c.cloud_home.s3_endpoint = data.endpoint.filter(|s| !s.is_empty());
                c.cloud_home.s3_key_prefix = data.key_prefix.filter(|s| !s.is_empty());
                c.cloud_home.storage = data.storage;
            })
            .map_err(|e| format!("Failed to save config: {e}"))?;

        self.ensure_sync_manager_and_start().await?;
        // A cloud home now exists, so every release gains its storage actions.
        self.emit_all_albums_updated().await;

        info!("Saved S3 sync configuration");
        Ok(())
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in_cloud_provider(
        &self,
        provider: CloudProvider,
        storage: crate::config::HomeStorage,
    ) -> Result<(), String> {
        use coven::{sign_in_dropbox, sign_in_google_drive, sign_in_onedrive};

        // Hold the sender alive across the await so cancel.wait_for inside
        // oauth::authorize never fires (this fn doesn't surface a cancel
        // signal). When this future is dropped, oauth::authorize's own
        // AbortOnDrop guard tears the listener task down.
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let library_name = self.config_handle.config().library_name.clone();
        let clock = self.clock.as_ref();
        // coven's sign-ins authorize, resolve the cloud folder, and save tokens to
        // the keyring, returning the folder identifiers; bae persists them here.
        match provider {
            CloudProvider::GoogleDrive => {
                let folder_id =
                    sign_in_google_drive(&self.key_service, &library_name, cancel_rx, clock)
                        .await
                        .map_err(|e| e.to_string())?;
                self.config_handle
                    .update(move |c| {
                        c.cloud_home.provider = Some(CloudProvider::GoogleDrive);
                        c.cloud_home.google_drive_folder_id = Some(folder_id);
                        c.cloud_home.storage = storage;
                    })
                    .map_err(|e| e.to_string())?;
            }
            CloudProvider::Dropbox => {
                let folder_path =
                    sign_in_dropbox(&self.key_service, &library_name, cancel_rx, clock)
                        .await
                        .map_err(|e| e.to_string())?;
                self.config_handle
                    .update(move |c| {
                        c.cloud_home.provider = Some(CloudProvider::Dropbox);
                        c.cloud_home.dropbox_folder_path = Some(folder_path);
                        c.cloud_home.storage = storage;
                    })
                    .map_err(|e| e.to_string())?;
            }
            CloudProvider::OneDrive => {
                let (drive_id, folder_id) = sign_in_onedrive(&self.key_service, cancel_rx, clock)
                    .await
                    .map_err(|e| e.to_string())?;
                self.config_handle
                    .update(move |c| {
                        c.cloud_home.provider = Some(CloudProvider::OneDrive);
                        c.cloud_home.onedrive_drive_id = Some(drive_id);
                        c.cloud_home.onedrive_folder_id = Some(folder_id);
                        c.cloud_home.storage = storage;
                    })
                    .map_err(|e| e.to_string())?;
            }
            _ => return Err("This provider does not use OAuth sign-in".to_string()),
        }
        self.ensure_sync_manager_and_start().await?;
        // A cloud home now exists, so every release gains its storage actions.
        self.emit_all_albums_updated().await;
        Ok(())
    }

    pub async fn use_cloudkit(&self, storage: crate::config::HomeStorage) -> Result<(), String> {
        self.config_handle
            .update(move |c| {
                c.cloud_home.provider = Some(CloudProvider::CloudKit);
                c.cloud_home.storage = storage;
            })
            .map_err(|e| format!("Failed to save CloudKit config: {e}"))?;
        self.ensure_sync_manager_and_start().await?;
        // A cloud home now exists, so every release gains its storage actions.
        self.emit_all_albums_updated().await;

        info!("Configured CloudKit cloud provider");
        Ok(())
    }

    pub fn disconnect_cloud_provider(&self) -> Result<(), String> {
        // Stop the sync loop and drop the installed manager; the library becomes
        // home-less until the next connect.
        self.handle.disconnect_sync();
        self.sync_connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
        *self.encryption_service.write().unwrap() = None;

        // Connecting fills the whole cloud home; disconnecting clears it as a unit.
        self.config_handle
            .update(|c| c.cloud_home = Default::default())
            .map_err(|e| e.to_string())?;

        // Clearing the cloud-home credentials from the keyring is coven's concern.
        if let Err(e) = self.key_service.delete_cloud_home_credentials() {
            tracing::warn!("Failed to delete cloud home credentials: {e}");
        }

        // The cloud home is gone, so releases lose their storage actions; re-emit
        // every album so cached UI details drop the now-invalid actions. Spawned
        // because this fn is sync and the re-emit re-resolves each album (async).
        let manager = self.clone();
        self.runtime_handle.spawn(async move {
            manager.emit_all_albums_updated().await;
        });
        Ok(())
    }

    /// Warning text to append to the disconnect-sync confirmation when the
    /// library has releases reachable only through cloud sync — remote and not
    /// pinned in coven's cache. Returns `None` when no releases are at risk, so the
    /// dialog just shows its base message. Asks coven's cache per remote release
    /// (a representative blob in `storage/pinned/`); pinned-ness is coven cache
    /// state, never a bae column.
    pub async fn disconnect_warning_message(&self) -> Result<Option<String>, String> {
        let remote_file_ids = self
            .database
            .get_remote_release_file_ids()
            .await
            .map_err(|e| format!("list remote releases: {e}"))?;
        let mut count: u64 = 0;
        for any_file_id in &remote_file_ids {
            if !self
                .release_pinned(any_file_id.as_deref())
                .await
                .map_err(|e| format!("pin-state check: {e}"))?
            {
                count += 1;
            }
        }
        Ok(match count {
            0 => None,
            1 => Some(
                "1 release is only stored in the cloud — it will become unplayable \
                 until you reconnect."
                    .to_string(),
            ),
            n => Some(format!(
                "{n} releases are only stored in the cloud — they will become \
                 unplayable until you reconnect."
            )),
        })
    }

    /// Build, start, and attach a sync manager. Used once at startup for a
    /// returning user with a configured cloud home: an unlocked key for an opaque
    /// home (`Some`), or no key for a browsable home (`None`). Shares this manager's
    /// outbox in-flight set and event channel with the sync loop's upload
    /// observer. Call before [`Self::start`].
    pub async fn attach_and_start_sync(
        &self,
        encryption_service: Option<EncryptionService>,
    ) -> Result<(), String> {
        self.handle.connect_sync(encryption_service.clone()).await?;
        *self.encryption_service.write().unwrap() = encryption_service;
        self.sync_connected
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Ensure a SyncManager exists (creating encryption key if needed) and start sync.
    async fn ensure_sync_manager_and_start(&self) -> Result<(), String> {
        // If we already have a sync manager, just (re)start its loop.
        if self.sync_connected() {
            self.handle.start_sync().await?;
            return Ok(());
        }

        // An opaque home mints (or reuses) the library key and seals every object
        // under it; a browsable home stores in the clear and has no key at all.
        // Build the encryption service only for an opaque home, so a browsable
        // home never mints a key it would never use. `get_or_create_encryption_key`
        // is idempotent, so a retry after a failed sync init reuses the key.
        let storage = self.config_handle.config().cloud_home.storage;
        let (enc_service, fingerprint) = if storage.is_opaque() {
            let enc_key_hex = self
                .key_service
                .get_or_create_encryption_key()
                .map_err(|e| format!("Failed to create encryption key: {e}"))?;
            let enc = EncryptionService::new(&enc_key_hex)
                .map_err(|e| format!("Failed to create encryption service: {e}"))?;
            let fingerprint = enc.fingerprint();
            (Some(enc), Some(fingerprint))
        } else {
            (None, None)
        };

        // Connect the provider: build the cloud home, start the loop, and install
        // the manager. A cloud-home build or loop-start failure returns `Err` with
        // nothing installed, so it surfaces here rather than leaving a dead manager
        // — and the encryption-key fingerprint below is reached only on success, so
        // a failed setup stays a clean retry (no fingerprint telling the next
        // launch's unlock flow "encryption is set up" while sync is still broken).
        self.handle.connect_sync(enc_service.clone()).await?;
        *self.encryption_service.write().unwrap() = enc_service;
        self.sync_connected
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // Sync started. For an opaque home, persist the encryption-key hint flag so
        // the next launch's unlock flow knows this library has encryption set up. A
        // browsable home records nothing — it has no key, so `encryption_key_stored`
        // stays false and the next launch builds it keyless.
        if let Some(fingerprint) = fingerprint {
            if let Err(e) = self
                .config_handle
                .record_encryption_key_fingerprint(fingerprint)
            {
                self.handle.disconnect_sync();
                *self.encryption_service.write().unwrap() = None;
                self.sync_connected
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                return Err(format!("Failed to save config: {e}"));
            }
        }

        self.trigger_sync();

        Ok(())
    }
}
