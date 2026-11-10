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
    /// The restore codes saved for this person, on the caller's task.
    pub(super) async fn saved_restore_codes_now(
        &self,
    ) -> Result<Vec<coven::SavedRestoreEntry>, BridgeError> {
        bae_library::saved_restore_codes(&self.coven, &self.diagnostics)
            .await
            .map_err(|error| BridgeError::categorized(error.category(), error))
    }

    /// Delete `store`'s saved restore code, on the caller's task.
    pub(super) async fn forget_saved_restore_code_now(
        &self,
        store: coven::StoreId,
    ) -> Result<(), BridgeError> {
        self.coven.forget_saved_restore_code(store).await.map_err(|error| {
            let error = coven::task::reported(&self.diagnostics, error);
            BridgeError::categorized(bae_diagnostics::bootstrap_category(&error), error)
        })
    }
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
                Ok(self
                    .saved_restore_codes_now()
                    .await?
                    .into_iter()
                    .map(BridgeSavedRestoreEntry::from_core)
                    .collect())
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
        let store = crate::from_app::store(&library_id)?;
        self.operations
            .clone()
            .run("BridgeHost.forget_saved_restore_code", move || async move {
                self.forget_saved_restore_code_now(store).await
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
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        self.operations
            .clone()
            .run_to_completion(self.clone(), "restore library", move || async move {
                match bae_library::restore_from_code(
                    self.library_builder(),
                    &code,
                    credentials.map(BridgeS3Credentials::into_core),
                    &device_name,
                    |status| steps.bootstrap(status, true),
                    &cancel,
                    &self.diagnostics,
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
        let store = crate::from_app::store(&library_id)?;
        let report = opening.report();
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelBootstrapOnDrop(cancellation);
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        self.operations
            .clone()
            .run_to_completion(
                self.clone(),
                "restore library over this device",
                move || async move {
                    self.closed(store).await?;
                    match bae_library::restore_replacing_device(
                        || self.library_builder(),
                        store,
                        &code,
                        credentials.map(BridgeS3Credentials::into_core),
                        &device_name,
                        |status| steps.bootstrap(status, true),
                        report,
                        &cancel,
                        self.clock.clone(),
                        &self.diagnostics,
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
    /// Finish this device's reset or restore of a library that stopped
    /// partway, from what core kept on the device: no restore code, and no
    /// sign-in. A Dropbox or OneDrive sign-in the provider refuses fails as a
    /// credentials error, and the call that began it, with its sign-in,
    /// finishes it then. `by` is the call the library's listing names, which
    /// says whether `cancellation` stops it: a restore's finishing stops, a
    /// reset's goes on. A restore cancelled once this device registered
    /// fails as `RestoreRegistered`, still unfinished.
    pub async fn resume_replacement(
        self: Arc<Self>,
        library_id: String,
        by: BridgeReplacedBy,
        setup: Arc<crate::BridgeLibrarySetup>,
        cancellation: Arc<BootstrapCancellation>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let store = crate::from_app::store(&library_id)?;
        let cancellable = bae_library::replacement_cancellable(by.into_core());
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelBootstrapOnDrop(cancellation);
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        self.operations
            .clone()
            .run_to_completion(
                self.clone(),
                "finish replacing this device's copy",
                move || async move {
                    self.closed(store).await?;
                    match bae_library::resume_replacement(
                        self.library_builder(),
                        store,
                        |status| steps.bootstrap(status, cancellable),
                        &cancel,
                        &self.diagnostics,
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
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        self.operations
            .clone()
            .run_to_completion(self.clone(), "restore saved library", move || async move {
                match bae_library::restore_from_saved_code(
                    self.library_builder(),
                    &saved.saved,
                    credentials.map(BridgeS3Credentials::into_core),
                    &device_name,
                    |status| steps.bootstrap(status, true),
                    &cancel,
                    &self.diagnostics,
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
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        self.operations
            .clone()
            .run("BridgeHost.join_with_invite", move || async move {
                let joined = bae_library::join_with_invite(
                    self.library_builder(),
                    &code,
                    &device_name,
                    self.clock.clone(),
                    |status| {
                        let cancellable = bae_library::join_cancellable(&status);
                        steps.bootstrap(status, cancellable)
                    },
                    &control.control,
                    &self.diagnostics,
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
                bae_library::unfinished_joins(&self.library_builder(), &self.diagnostics)
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
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        self.operations
            .clone()
            .run("BridgeHost.resume_join", move || async move {
                let joined = bae_library::resume_join(
                    || self.library_builder(),
                    &join.join,
                    self.clock.clone(),
                    |status| {
                        let cancellable = bae_library::join_cancellable(&status);
                        steps.bootstrap(status, cancellable)
                    },
                    &control.control,
                    &self.diagnostics,
                )
                .await;
                BridgeBootstrapResult::joined(joined, self)
            })
            .await
    }

    /// The restores of a library onto this device that stopped after they
    /// recorded themselves, to finish after bae restarts.
    pub async fn unfinished_restores(
        self: Arc<Self>,
    ) -> Result<Vec<Arc<BridgeUnfinishedRestore>>, BridgeError> {
        self.operations
            .clone()
            .run("BridgeHost.unfinished_restores", move || async move {
                bae_library::unfinished_restores(&self.library_builder(), &self.diagnostics)
                    .await
                    .map(|restores| {
                        restores
                            .into_iter()
                            .map(|restore| Arc::new(BridgeUnfinishedRestore { restore }))
                            .collect()
                    })
                    .map_err(|error| BridgeError::categorized(error.category(), error))
            })
            .await
    }

    /// Finish a restore from `unfinished_restores` with what core kept: no
    /// restore code, and a provider sign-in only when the kept one is gone or
    /// refused. `cancellation` ends it as it ends `restore_from_code`; once
    /// this device registered, the call fails as `RestoreRegistered` and the
    /// restore stays unfinished.
    pub async fn resume_restore(
        self: Arc<Self>,
        restore: Arc<BridgeUnfinishedRestore>,
        setup: Arc<crate::BridgeLibrarySetup>,
        cancellation: Arc<BootstrapCancellation>,
    ) -> Result<BridgeBootstrapResult, BridgeError> {
        let cancel = cancellation.cancel.subscribe();
        let _cancel_on_drop = CancelBootstrapOnDrop(cancellation);
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        self.operations
            .clone()
            .run_to_completion(self.clone(), "finish restoring library", move || async move {
                match bae_library::resume_restore(
                    || self.library_builder(),
                    &restore.restore,
                    |status| steps.bootstrap(status, true),
                    &cancel,
                    &self.diagnostics,
                )
                .await
                {
                    Ok(handle) => Ok(BridgeBootstrapResult::opened(handle, self)),
                    Err(error) => BridgeBootstrapResult::failure(error, self),
                }
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
            store: crate::from_app::store(&library_id)?,
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
        let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
        self.host
            .operations
            .clone()
            .run_to_completion(self.clone(), "reset device", move || async move {
                self.host.closed(self.store).await?;
                match bae_library::reset_device(
                    self.host.library_builder(),
                    self.store,
                    &self.device_name,
                    |status| steps.bootstrap(status, false),
                    &self.host.diagnostics,
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
