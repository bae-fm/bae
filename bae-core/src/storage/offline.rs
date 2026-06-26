//! A `SyncStorage` that has no cloud home.
//!
//! coven's locality-aware read ([`coven::blob::cache::read_blob`] /
//! [`coven::blob::cache::open_blob_stream`]) takes a `&dyn SyncStorage`, but only
//! reaches it on the final cloud-miss branch — a Local blob is served from its
//! external ref (the user's file) or the local store first, never touching
//! storage. A library with no cloud provider configured has only Local releases
//! (a release cannot be Remote without a home), so the cloud branch is never
//! reached; this stub stands in so the read path is uniform whether or not a home
//! is configured. Every method errors: reaching one means a Remote blob was read
//! with no home, which is a real fault, surfaced rather than masked.

use async_trait::async_trait;
use coven::sync::storage::{DeviceHead, MinSchemaVersion, StorageError, SyncStorage};

/// A `SyncStorage` with no backing cloud home: every operation errors. Used as
/// the storage argument to coven's locality-aware read for a library with no
/// cloud configured, where only the never-reached cloud branch would call it.
pub struct OfflineSyncStorage;

impl OfflineSyncStorage {
    fn err() -> StorageError {
        StorageError::S3("no cloud home configured".to_string())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl SyncStorage for OfflineSyncStorage {
    async fn list_heads(&self) -> Result<Vec<DeviceHead>, StorageError> {
        Err(Self::err())
    }
    async fn get_changeset(&self, _device_id: &str, _seq: u64) -> Result<Vec<u8>, StorageError> {
        Err(Self::err())
    }
    async fn put_changeset(
        &self,
        _device_id: &str,
        _seq: u64,
        _data: Vec<u8>,
    ) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn put_head(
        &self,
        _device_id: &str,
        _seq: u64,
        _snapshot_seq: Option<u64>,
        _timestamp: &str,
    ) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn put_blob(
        &self,
        _namespace: &str,
        _id: &str,
        _scope: coven::blob::ResolvedScope,
        _cloud_path: Option<&str>,
        _data: Vec<u8>,
    ) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn get_blob(
        &self,
        _namespace: &str,
        _id: &str,
        _scope: coven::blob::ResolvedScope,
        _cloud_path: Option<&str>,
    ) -> Result<Vec<u8>, StorageError> {
        Err(Self::err())
    }
    async fn read_blob_range(
        &self,
        _namespace: &str,
        _id: &str,
        _scope: coven::blob::ResolvedScope,
        _cloud_path: Option<&str>,
        _source_size: u64,
        _offset: u64,
        _len: u64,
    ) -> Result<Vec<u8>, StorageError> {
        Err(Self::err())
    }
    async fn put_snapshot(
        &self,
        _author: &str,
        _seq: u64,
        _data: Vec<u8>,
    ) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn get_snapshot(&self, _author: &str, _seq: u64) -> Result<Vec<u8>, StorageError> {
        Err(Self::err())
    }
    async fn delete_changeset(&self, _device_id: &str, _seq: u64) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn list_changesets(&self, _device_id: &str) -> Result<Vec<u64>, StorageError> {
        Err(Self::err())
    }
    async fn get_min_schema_version(&self) -> Result<Option<MinSchemaVersion>, StorageError> {
        Err(Self::err())
    }
    async fn set_min_schema_version(&self, _version: u32) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn put_membership_entry(
        &self,
        _author_pubkey: &str,
        _seq: u64,
        _data: Vec<u8>,
    ) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn get_membership_entry(
        &self,
        _author_pubkey: &str,
        _seq: u64,
    ) -> Result<Vec<u8>, StorageError> {
        Err(Self::err())
    }
    async fn list_membership_entries(&self) -> Result<Vec<(String, u64)>, StorageError> {
        Err(Self::err())
    }
    async fn put_wrapped_key(
        &self,
        _user_pubkey: &str,
        _data: Vec<u8>,
    ) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn get_wrapped_key(&self, _user_pubkey: &str) -> Result<Vec<u8>, StorageError> {
        Err(Self::err())
    }
    async fn delete_wrapped_key(&self, _user_pubkey: &str) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn put_snapshot_meta(
        &self,
        _author: &str,
        _seq: u64,
        _data: Vec<u8>,
    ) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn get_snapshot_meta(&self, _author: &str, _seq: u64) -> Result<Vec<u8>, StorageError> {
        Err(Self::err())
    }
    async fn put_snapshot_pointer(&self, _data: Vec<u8>) -> Result<(), StorageError> {
        Err(Self::err())
    }
    async fn get_snapshot_pointer(&self) -> Result<Vec<u8>, StorageError> {
        Err(Self::err())
    }
    async fn list_own_snapshot_generations(&self, _author: &str) -> Result<Vec<u64>, StorageError> {
        Err(Self::err())
    }
    async fn delete_snapshot_generation(
        &self,
        _author: &str,
        _seq: u64,
    ) -> Result<(), StorageError> {
        Err(Self::err())
    }
}
