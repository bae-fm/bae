use super::*;
use crate::types::*;

#[uniffi::export(callback_interface)]
pub trait BootstrapCallback: Send + Sync {
    fn on_status(&self, status: BridgeBootstrapStatus);
}

#[derive(uniffi::Object)]
pub struct BootstrapCancellation {
    cancel: tokio::sync::watch::Sender<bool>,
}
#[uniffi::export]
impl BootstrapCancellation {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        let (cancel, _) = tokio::sync::watch::channel(false);
        Arc::new(Self { cancel })
    }
    pub fn cancel(&self) {
        self.cancel.send_replace(true);
    }
}
struct CancelOnDrop(Arc<BootstrapCancellation>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(uniffi::Object)]
pub struct BridgeSavedRestoreCode {
    saved: coven::SavedRestoreCode,
}
#[derive(Clone, uniffi::Record)]
pub struct BridgeSavedRestoreCodeInfo {
    pub library_id: String,
    pub location: BridgeStorageLocation,
    pub has_s3_credentials: bool,
}
#[uniffi::export]
impl BridgeSavedRestoreCode {
    pub fn info(&self) -> BridgeSavedRestoreCodeInfo {
        BridgeSavedRestoreCodeInfo {
            library_id: self.saved.store_id.to_string(),
            location: BridgeStorageLocation::from_core(self.saved.location.clone()),
            has_s3_credentials: self.saved.has_s3_credentials,
        }
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeHost {
    pub async fn saved_restore_codes(
        self: Arc<Self>,
    ) -> Result<Vec<Arc<BridgeSavedRestoreCode>>, BridgeError> {
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move {
                self.keychain
                    .saved_restore_codes(&self.app_dir)
                    .await
                    .map(|codes| {
                        codes
                            .into_iter()
                            .map(|saved| Arc::new(BridgeSavedRestoreCode { saved }))
                            .collect()
                    })
                    .map_err(|error| {
                        BridgeError::diagnostic(
                            BridgeErrorCategory::from_core(error.category()),
                            error,
                        )
                    })
            },
        )
        .await
    }
    pub async fn restore_from_code(
        self: Arc<Self>,
        code: String,
        credentials: Option<BridgeS3Credentials>,
        device_name: String,
        callback: Box<dyn BootstrapCallback>,
        cancellation: Arc<BootstrapCancellation>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelOnDrop(cancellation);
        crate::operation_runtime::run_to_completion(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            self.clone(),
            "restore library",
            move || async move {
                match bae_core::library::restore_from_code(
                    self.builder(),
                    &code,
                    credentials.map(BridgeS3Credentials::into_core),
                    &device_name,
                    |status| callback.on_status(BridgeBootstrapStatus::from_core(status)),
                    &cancel,
                )
                .await
                {
                    Ok(handle) => Ok(BridgeBootstrapResult::opened(handle, self)),
                    Err(error) => BridgeBootstrapResult::failure(error, self),
                }
            },
        )
        .await
    }
    /// Restore a library this device holds with the person's current restore
    /// code, after another of their devices replaced their keys. The caller
    /// closed the library and showed the restore's warning before calling. A
    /// failed restore leaves the library on this device as it was, to open
    /// again or to retry. Called again, it finishes a restore that stopped
    /// partway.
    pub async fn restore_replacing_device(
        self: Arc<Self>,
        library_id: String,
        code: String,
        credentials: Option<BridgeS3Credentials>,
        device_name: String,
        callback: Box<dyn BootstrapCallback>,
        cancellation: Arc<BootstrapCancellation>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let store = coven::StoreId(library_id.parse().map_err(BridgeError::config)?);
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelOnDrop(cancellation);
        crate::operation_runtime::run_to_completion(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            self.clone(),
            "restore library over this device",
            move || async move {
                match bae_core::library::restore_replacing_device(
                    self.builder(),
                    store,
                    &code,
                    credentials.map(BridgeS3Credentials::into_core),
                    &device_name,
                    |status| callback.on_status(BridgeBootstrapStatus::from_core(status)),
                    &cancel,
                )
                .await
                {
                    Ok(handle) => Ok(BridgeBootstrapResult::opened(handle, self)),
                    Err(error) => BridgeBootstrapResult::failure(error, self),
                }
            },
        )
        .await
    }
    pub async fn restore_from_keychain(
        self: Arc<Self>,
        saved: Arc<BridgeSavedRestoreCode>,
        credentials: Option<BridgeS3Credentials>,
        device_name: String,
        callback: Box<dyn BootstrapCallback>,
        cancellation: Arc<BootstrapCancellation>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelOnDrop(cancellation);
        crate::operation_runtime::run_to_completion(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            self.clone(),
            "restore saved library",
            move || async move {
                match bae_core::library::restore_from_keychain(
                    self.builder(),
                    &saved.saved,
                    credentials.map(BridgeS3Credentials::into_core),
                    &device_name,
                    |status| callback.on_status(BridgeBootstrapStatus::from_core(status)),
                    &cancel,
                )
                .await
                {
                    Ok(Some(handle)) => Ok(BridgeBootstrapResult::opened(handle, self)),
                    Ok(None) => Ok(BridgeBootstrapResult::SavedCodeRemoved),
                    Err(error) => BridgeBootstrapResult::failure(error, self),
                }
            },
        )
        .await
    }
    pub async fn join_with_invite(
        self: Arc<Self>,
        code: String,
        device_name: String,
        person_name: String,
        callback: Box<dyn BootstrapCallback>,
        cancellation: Arc<BootstrapCancellation>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelOnDrop(cancellation);
        crate::operation_runtime::run_to_completion(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            self.clone(),
            "join library",
            move || async move {
                match bae_core::library::join_with_invite(
                    self.builder(),
                    &code,
                    &device_name,
                    &person_name,
                    Arc::new(coven::SystemClock),
                    |status| callback.on_status(BridgeBootstrapStatus::from_core(status)),
                    &cancel,
                )
                .await
                {
                    Ok(Some((handle, Ok(_)))) => Ok(BridgeBootstrapResult::opened(handle, self)),
                    Ok(Some((handle, Err(error)))) => Ok(BridgeBootstrapResult::ProfileRequired {
                        library: Arc::new(crate::OpenedLibrary::new(handle, self)),
                        error: error.into(),
                    }),
                    Ok(None) => Ok(BridgeBootstrapResult::Declined),
                    Err(error) => BridgeBootstrapResult::failure(error, self),
                }
            },
        )
        .await
    }

    /// The caller closes the active library and shows the reset warning before
    /// confirming. Confirming again finishes a reset that stopped partway,
    /// whose warning the person already saw.
    pub fn prepare_device_reset(
        self: Arc<Self>,
        library_id: String,
        device_name: String,
    ) -> Result<Arc<BridgeDeviceReset>, BridgeError> {
        Ok(Arc::new(BridgeDeviceReset {
            host: self,
            store: coven::StoreId(library_id.parse().map_err(BridgeError::config)?),
            device_name,
        }))
    }
}
#[derive(uniffi::Object)]
pub struct BridgeDeviceReset {
    host: Arc<BridgeHost>,
    store: coven::StoreId,
    device_name: String,
}
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeDeviceReset {
    pub fn warning_key(&self) -> String {
        "core.sync.reset_device.warning".into()
    }
    pub async fn confirm(self: Arc<Self>) -> Result<BridgeBootstrapResult, BridgeError> {
        crate::operation_runtime::run_to_completion(
            self.host
                .runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            self.clone(),
            "reset device",
            move || async move {
                match bae_core::library::reset_device(
                    self.host.builder(),
                    self.store,
                    &self.device_name,
                )
                .await
                {
                    Ok(handle) => Ok(BridgeBootstrapResult::opened(handle, self.host.clone())),
                    Err(error) => BridgeBootstrapResult::failure(error, self.host.clone()),
                }
            },
        )
        .await
    }
}
