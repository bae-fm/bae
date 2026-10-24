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
/// The app's hold on a running join: cancel it before its request is sent,
/// or check for approval now.
#[derive(uniffi::Object)]
pub struct BridgeJoinControl {
    control: bae_core::library::JoinControl,
}
#[uniffi::export]
impl BridgeJoinControl {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            control: bae_core::library::JoinControl::new(),
        })
    }
    pub fn cancel(&self) {
        self.control.cancel();
    }
    /// End the join's wait so it checks for approval now; a provider's
    /// cooldown still holds.
    pub fn check_now(&self) {
        self.control.check_now();
    }
}
struct CancelOnDrop(Arc<BootstrapCancellation>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeHost {
    pub async fn saved_restore_codes(
        self: Arc<Self>,
    ) -> Result<Vec<BridgeSavedRestoreEntry>, BridgeError> {
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
                            .map(BridgeSavedRestoreEntry::from_core)
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
    /// Delete the library's saved restore code, readable or not, and the S3
    /// key saved beside it, from iCloud Keychain on all the person's Apple
    /// devices. The library, its storage and every device's copy stay.
    pub async fn forget_saved_restore_code(
        self: Arc<Self>,
        library_id: String,
    ) -> Result<(), BridgeError> {
        let store = coven::StoreId(library_id.parse().map_err(BridgeError::config)?);
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move {
                self.keychain
                    .forget_saved_restore_code(store)
                    .await
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
    /// Join a library from an invite. The profile takes the invite's name.
    /// Dropping the call, as leaving the screen does, keeps the join for
    /// `resume_join`; only `control.cancel()` cancels, and only before the
    /// request may have been sent.
    pub async fn join_with_invite(
        self: Arc<Self>,
        code: String,
        device_name: String,
        callback: Box<dyn BootstrapCallback>,
        control: Arc<BridgeJoinControl>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move {
                let joined = bae_core::library::join_with_invite(
                    self.builder(),
                    &code,
                    &device_name,
                    Arc::new(coven::SystemClock),
                    |status| callback.on_status(BridgeBootstrapStatus::from_core(status)),
                    &control.control,
                )
                .await;
                BridgeBootstrapResult::joined(joined, self)
            },
        )
        .await
    }

    /// The joins this device has under way, to resume after bae restarts.
    pub async fn unfinished_joins(
        self: Arc<Self>,
    ) -> Result<Vec<Arc<BridgeUnfinishedJoin>>, BridgeError> {
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move {
                bae_core::library::unfinished_joins(&self.builder())
                    .await
                    .map(|joins| {
                        joins
                            .into_iter()
                            .map(|join| Arc::new(BridgeUnfinishedJoin { join }))
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

    /// Continue a join from `unfinished_joins` with the provider sign-in coven
    /// kept for it, signing in again only when coven asks for one. Dropping
    /// the call keeps the join, as for `join_with_invite`.
    pub async fn resume_join(
        self: Arc<Self>,
        join: Arc<BridgeUnfinishedJoin>,
        callback: Box<dyn BootstrapCallback>,
        control: Arc<BridgeJoinControl>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        crate::operation_runtime::run(
            self.runtime
                .as_ref()
                .expect("host runtime exists until drop")
                .handle()
                .clone(),
            move || async move {
                let joined = bae_core::library::resume_join(
                    || self.builder(),
                    &join.join,
                    Arc::new(coven::SystemClock),
                    |status| callback.on_status(BridgeBootstrapStatus::from_core(status)),
                    &control.control,
                )
                .await;
                BridgeBootstrapResult::joined(joined, self)
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
