use super::*;
use crate::types::*;

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
    control: bae_library::JoinControl,
}
#[uniffi::export]
impl BridgeJoinControl {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self { control: bae_library::JoinControl::new() })
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
impl BridgeHost {
    /// The libraries whose restore codes are saved for this person, by
    /// store, on the caller's task.
    #[cfg(all(feature = "dev-reset", feature = "desktop", unix))]
    pub(super) async fn saved_restore_codes_now(&self) -> Result<Vec<coven::StoreId>, BridgeError> {
        Ok(bae_library::saved_restore_codes(&self.coven)
            .await
            .map_err(|error| BridgeError::categorized(error.category(), error))?
            .iter()
            .map(coven::SavedRestoreEntry::store_id)
            .collect())
    }

    /// Delete `store`'s saved restore code, on the caller's task.
    #[cfg(all(feature = "dev-reset", feature = "desktop", unix))]
    pub(super) async fn forget_saved_restore_code_now(
        &self,
        store: coven::StoreId,
    ) -> Result<(), BridgeError> {
        self.coven.forget_saved_restore_code(store).await.map_err(forget_failed)
    }
}

/// Why deleting a saved restore code failed, as the app shows it.
fn forget_failed(error: coven::BootstrapError) -> BridgeError {
    BridgeError::diagnostic(
        BridgeErrorCategory::from_core(bae_diagnostics::bootstrap_category(&error)),
        error,
    )
}

struct CancelBootstrapOnDrop(Arc<BootstrapCancellation>);
impl Drop for CancelBootstrapOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeHost {
    pub async fn saved_restore_codes(
        self: Arc<Self>,
    ) -> Result<Vec<BridgeSavedRestoreEntry>, BridgeError> {
        self.operations
            .clone()
            .run("BridgeHost.saved_restore_codes", move || async move {
                bae_library::saved_restore_codes(&self.coven)
                    .await
                    .map(|codes| {
                        codes.into_iter().map(BridgeSavedRestoreEntry::from_core).collect()
                    })
                    .map_err(|error| BridgeError::categorized(error.category(), error))
            })
            .await
    }
    /// Delete the library's saved restore code, readable or not, and the S3
    /// key saved beside it: from iCloud Keychain on all the person's Apple
    /// devices, or from this phone's Block Store on Android. The library, its
    /// storage and every device's copy stay.
    pub async fn forget_saved_restore_code(
        self: Arc<Self>,
        library_id: String,
    ) -> Result<(), BridgeError> {
        let store = coven::StoreId(library_id.parse().map_err(BridgeError::config)?);
        self.operations
            .clone()
            .run("BridgeHost.forget_saved_restore_code", move || async move {
                self.coven.forget_saved_restore_code(store).await.map_err(forget_failed)
            })
            .await
    }
    pub async fn restore_from_code(
        self: Arc<Self>,
        code: String,
        credentials: Option<BridgeS3Credentials>,
        device_name: String,
        setup: Arc<crate::BridgeLibrarySetup>,
        cancellation: Arc<BootstrapCancellation>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelBootstrapOnDrop(cancellation);
        let steps = setup.writer();
        self.operations
            .clone()
            .run_to_completion(self.clone(), "restore library", move || async move {
                match bae_library::restore_from_code(
                    self.builder(),
                    &code,
                    credentials.map(BridgeS3Credentials::into_core),
                    &device_name,
                    |status| steps.bootstrap(status),
                    &cancel,
                )
                .await
                {
                    Ok(handle) => Ok(BridgeBootstrapResult::opened(handle, self)),
                    Err(error) => BridgeBootstrapResult::failure(error, self),
                }
            })
            .await
    }
    /// Restore a library this device holds with the person's current restore
    /// code, after another of their devices replaced their keys. The caller
    /// closed the library and showed the restore's warning before calling. A
    /// failed restore leaves the library on this device as it was, to open
    /// again or to retry. Called again, it finishes a restore that stopped
    /// partway. When this device already holds the code's keys, the call
    /// just opens the library, and that open's steps are written into
    /// `opening`'s stream.
    pub async fn restore_replacing_device(
        self: Arc<Self>,
        library_id: String,
        code: String,
        credentials: Option<BridgeS3Credentials>,
        device_name: String,
        setup: Arc<crate::BridgeLibrarySetup>,
        opening: Arc<crate::BridgeLibraryOpening>,
        cancellation: Arc<BootstrapCancellation>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let store = coven::StoreId(library_id.parse().map_err(BridgeError::config)?);
        let report = opening.report();
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelBootstrapOnDrop(cancellation);
        let steps = setup.writer();
        self.operations
            .clone()
            .run_to_completion(
                self.clone(),
                "restore library over this device",
                move || async move {
                    self.closed(store).await?;
                    match bae_library::restore_replacing_device(
                        || self.builder(),
                        store,
                        &code,
                        credentials.map(BridgeS3Credentials::into_core),
                        &device_name,
                        |status| steps.bootstrap(status),
                        report,
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
    pub async fn restore_from_saved_code(
        self: Arc<Self>,
        saved: Arc<BridgeSavedRestoreCode>,
        credentials: Option<BridgeS3Credentials>,
        device_name: String,
        setup: Arc<crate::BridgeLibrarySetup>,
        cancellation: Arc<BootstrapCancellation>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelBootstrapOnDrop(cancellation);
        let steps = setup.writer();
        self.operations
            .clone()
            .run_to_completion(self.clone(), "restore saved library", move || async move {
                match bae_library::restore_from_saved_code(
                    self.builder(),
                    &saved.saved,
                    credentials.map(BridgeS3Credentials::into_core),
                    &device_name,
                    |status| steps.bootstrap(status),
                    &cancel,
                )
                .await
                {
                    Ok(Some(handle)) => Ok(BridgeBootstrapResult::opened(handle, self)),
                    Ok(None) => Ok(BridgeBootstrapResult::SavedCodeRemoved),
                    Err(error) => BridgeBootstrapResult::failure(error, self),
                }
            })
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
        setup: Arc<crate::BridgeLibrarySetup>,
        control: Arc<BridgeJoinControl>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let steps = setup.writer();
        self.operations
            .clone()
            .run("BridgeHost.join_with_invite", move || async move {
                let joined = bae_library::join_with_invite(
                    self.builder(),
                    &code,
                    &device_name,
                    Arc::new(coven::SystemClock),
                    |status| steps.bootstrap(status),
                    &control.control,
                )
                .await;
                BridgeBootstrapResult::joined(joined, self)
            })
            .await
    }

    /// The joins this device has under way, to resume after bae restarts.
    pub async fn unfinished_joins(
        self: Arc<Self>,
    ) -> Result<Vec<Arc<BridgeUnfinishedJoin>>, BridgeError> {
        self.operations
            .clone()
            .run("BridgeHost.unfinished_joins", move || async move {
                bae_library::unfinished_joins(&self.builder())
                    .await
                    .map(|joins| {
                        joins
                            .into_iter()
                            .map(|join| Arc::new(BridgeUnfinishedJoin { join }))
                            .collect()
                    })
                    .map_err(|error| BridgeError::categorized(error.category(), error))
            })
            .await
    }

    /// Continue a join from `unfinished_joins` with the provider sign-in coven
    /// kept for it, signing in again only when coven asks for one. Dropping
    /// the call keeps the join, as for `join_with_invite`.
    pub async fn resume_join(
        self: Arc<Self>,
        join: Arc<BridgeUnfinishedJoin>,
        setup: Arc<crate::BridgeLibrarySetup>,
        control: Arc<BridgeJoinControl>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let steps = setup.writer();
        self.operations
            .clone()
            .run("BridgeHost.resume_join", move || async move {
                let joined = bae_library::resume_join(
                    || self.builder(),
                    &join.join,
                    Arc::new(coven::SystemClock),
                    |status| steps.bootstrap(status),
                    &control.control,
                )
                .await;
                BridgeBootstrapResult::joined(joined, self)
            })
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
    /// Reset the device after the person accepted the warning: coven loads
    /// the library from storage again, telling `callback` how far it has
    /// come.
    /// Reset, writing where the reset stands into `setup`'s stream: the
    /// new installation's open, then each step of loading the library.
    pub async fn confirm(
        self: Arc<Self>,
        setup: Arc<crate::BridgeLibrarySetup>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let steps = setup.writer();
        self.host
            .operations
            .clone()
            .run_to_completion(self.clone(), "reset device", move || async move {
                self.host.closed(self.store).await?;
                match bae_library::reset_device(
                    self.host.builder(),
                    self.store,
                    &self.device_name,
                    |status| steps.bootstrap(status),
                )
                .await
                {
                    Ok(handle) => Ok(BridgeBootstrapResult::opened(handle, self.host.clone())),
                    Err(error) => BridgeBootstrapResult::failure(error, self.host.clone()),
                }
            })
            .await
    }
}
