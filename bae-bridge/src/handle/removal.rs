use super::*;

#[derive(Clone, Copy, uniffi::Enum)]
pub enum BridgeIfUnsent {
    Refuse,
    Discard,
}
#[derive(Clone, uniffi::Enum)]
pub enum BridgeLibraryRemoval {
    Removed,
    /// This device holds changes it hasn't sent, so nothing was removed:
    /// `writes` edits and `files` attached files waiting to upload. Removing
    /// with `BridgeIfUnsent::Discard` loses them.
    Unsent {
        writes: u64,
        files: u64,
    },
    /// The library left its store, but this device's files for it remain.
    Unfinished {
        removal: BridgeUnfinishedRemoval,
    },
}

/// A recorded removal that never finished, with why. When `files_remain`,
/// the store is gone and `BridgeHost::finish_removals` deletes this device's
/// files for it; until then `BridgeHost::unfinished_removal` reports it,
/// across restarts. Otherwise the removal stopped before the store was
/// deleted: the library is still here and can be removed again.
#[derive(Clone, uniffi::Record)]
pub struct BridgeUnfinishedRemoval {
    pub error: BridgeError,
    pub files_remain: bool,
}

impl BridgeUnfinishedRemoval {
    pub(crate) fn new(
        category: bae_core::ui::UiErrorCategory,
        why: &dyn std::fmt::Display,
        files_remain: bool,
    ) -> Self {
        Self {
            error: BridgeError::diagnostic(crate::BridgeErrorCategory::from_core(category), why),
            files_remain,
        }
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    /// Stop services and leave the store before deleting its local files.
    pub async fn remove_library(
        self: std::sync::Arc<Self>,
        unsent: BridgeIfUnsent,
    ) -> Result<BridgeLibraryRemoval, BridgeError> {
        crate::operation_runtime::run_to_completion(
            self.runtime.clone(),
            self.clone(),
            "remove library",
            move || async move {
                #[cfg(feature = "desktop")]
                self.desktop.shutdown().await;
                let unsent = match unsent {
                    BridgeIfUnsent::Refuse => coven::IfUnsent::Refuse,
                    BridgeIfUnsent::Discard => coven::IfUnsent::Discard,
                };
                use bae_core::library::LibraryRemovalError as E;
                match self.services.remove_library(unsent).await {
                    Ok(()) => Ok(BridgeLibraryRemoval::Removed),
                    Err(E::Unsent(work)) => Ok(BridgeLibraryRemoval::Unsent {
                        writes: work.writes,
                        files: work.files,
                    }),
                    Err(error @ E::Unfinished(_)) => Ok(BridgeLibraryRemoval::Unfinished {
                        removal: BridgeUnfinishedRemoval::new(error.category(), &error, true),
                    }),
                    Err(error @ (E::Record(_) | E::Store(_))) => Err(BridgeError::diagnostic(
                        crate::BridgeErrorCategory::from_core(error.category()),
                        &error,
                    )),
                }
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A removal that stops after leaving the store is reported, read back by
    /// the host after the library is gone, and finished there.
    #[cfg(unix)]
    #[test]
    fn an_unfinished_removal_is_reported_and_finished_by_the_host() {
        use std::os::unix::fs::PermissionsExt;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .unwrap();
        let (manager, root) = bae_test_support::setup_fresh_library(&runtime);
        manager.get_config().save_preferences().unwrap();
        let services = runtime.block_on(AppServices::for_test(manager)).unwrap();
        let app = Arc::new(AppHandle::start(services, runtime).unwrap());
        let host = crate::BridgeHost::new(
            crate::BridgeDiagnostics::noop(),
            crate::BridgeAppDir::new(root.path().to_str().unwrap().into()),
        )
        .unwrap();
        let libraries = bae_core::config::AppDir::under_home(root.path()).libraries();
        std::fs::set_permissions(&libraries, std::fs::Permissions::from_mode(0o555)).unwrap();
        let caller = tokio::runtime::Runtime::new().unwrap();
        let result = caller
            .block_on(app.clone().remove_library(BridgeIfUnsent::Discard))
            .unwrap();
        let BridgeLibraryRemoval::Unfinished { removal } = result else {
            panic!("the directory left behind must be reported");
        };
        assert!(
            removal.error.to_string().contains("store was removed"),
            "{}",
            removal.error
        );
        drop(app);

        caller.block_on(async {
            assert!(host.clone().unfinished_removal().await.unwrap().is_some());
            assert!(host.clone().finish_removals().await.is_err());
            std::fs::set_permissions(&libraries, std::fs::Permissions::from_mode(0o755)).unwrap();
            host.clone().finish_removals().await.unwrap();
            assert!(host.clone().unfinished_removal().await.unwrap().is_none());
            host.clone().finish_removals().await.unwrap();
        });
    }
}
