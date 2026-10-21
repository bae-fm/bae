//! The dev reset's remote step, on the host runtime. Only debug macOS builds
//! carry it.
use super::*;
use crate::types::*;
use crate::BridgeEmptiedS3Prefix;
use bae_core::dev_reset::EmptyStoragePrefixError;

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeHost {
    /// Delete every object and abort every unfinished upload under the dev
    /// profile's storage prefix, whatever wrote them, so the library the reset
    /// creates there starts on an empty remote. Safe to retry after any
    /// failure: what went before it stays gone.
    pub async fn empty_dev_reset_prefix(
        self: Arc<Self>,
        sync_config: BridgeSaveSyncConfig,
    ) -> Result<BridgeEmptiedS3Prefix, BridgeError> {
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move {
                let emptied = bae_core::dev_reset::empty_storage_prefix(sync_config.into_core())
                    .await
                    .map_err(empty_error)?;
                Ok(BridgeEmptiedS3Prefix {
                    objects: emptied.objects as u64,
                    uploads: emptied.uploads as u64,
                })
            },
        )
        .await
    }
}

/// A storage failure keeps coven's provider reason; a profile coven refused
/// before any request is a configuration error.
fn empty_error(error: EmptyStoragePrefixError) -> BridgeError {
    match &error {
        EmptyStoragePrefixError::Coven(coven::EmptyS3PrefixError::Storage(storage)) => {
            BridgeError::diagnostic(
                BridgeErrorCategory::Storage {
                    failure: BridgeStorageFailure::from_core(storage.failure()),
                },
                &error,
            )
        }
        EmptyStoragePrefixError::Endpoint(_)
        | EmptyStoragePrefixError::Coven(coven::EmptyS3PrefixError::WholeBucket) => {
            BridgeError::config(&error)
        }
    }
}
