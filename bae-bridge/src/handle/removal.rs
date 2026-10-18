use super::*;
use std::sync::Arc;

#[derive(Clone, Copy, uniffi::Enum)]
pub enum BridgeIfUnsent {
    Refuse,
    Discard,
}
#[derive(Clone, uniffi::Enum)]
pub enum BridgeLibraryRemoval {
    Removed,
    PreferencesRemain {
        retry: Arc<BridgeRemovalRetry>,
        error: BridgeError,
    },
}
#[derive(uniffi::Object)]
pub struct BridgeRemovalRetry {
    library: tokio::sync::Mutex<Option<bae_core::library::RemovedLibrary>>,
    app: Arc<AppHandle>,
}
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeRemovalRetry {
    pub async fn retry(self: Arc<Self>) -> Result<(), BridgeError> {
        crate::operation_runtime::run_to_completion(
            self.app.runtime.clone(),
            self.clone(),
            "finish library removal",
            move || async move {
                let mut pending = self.library.lock().await;
                let Some(library) = pending.take() else {
                    return Ok(());
                };
                match self.app.services.finish_removed_library(library).await {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        let diagnostic = BridgeError::diagnostic(
                            crate::BridgeErrorCategory::from_core(error.category()),
                            &error,
                        );
                        if let bae_core::library::LibraryRemovalError::Preferences {
                            library, ..
                        } = error
                        {
                            *pending = Some(library);
                        }
                        Err(diagnostic)
                    }
                }
            },
        )
        .await
    }
}
#[uniffi::export(async_runtime = "tokio", cancellable)]
impl AppHandle {
    /// Stop services and leave the store before deleting its local files.
    pub async fn remove_library(
        self: Arc<Self>,
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
                match self.services.remove_library(unsent).await {
                    Ok(()) => Ok(BridgeLibraryRemoval::Removed),
                    Err(error) => {
                        let diagnostic = BridgeError::diagnostic(
                            crate::BridgeErrorCategory::from_core(error.category()),
                            &error,
                        );
                        match error {
                            bae_core::library::LibraryRemovalError::Store(_) => Err(diagnostic),
                            bae_core::library::LibraryRemovalError::Preferences {
                                library, ..
                            } => Ok(BridgeLibraryRemoval::PreferencesRemain {
                                retry: Arc::new(BridgeRemovalRetry {
                                    library: tokio::sync::Mutex::new(Some(library)),
                                    app: self.clone(),
                                }),
                                error: diagnostic,
                            }),
                        }
                    }
                }
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_retry_keeps_its_runtime_after_the_ui_releases_the_library() {
        let root = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_stack_size(16 * 1024 * 1024)
            .enable_all()
            .build()
            .unwrap();
        let (manager, _, storage) = runtime.block_on(bae_test_support::storage_library(
            root.path(),
            bae_core::providers::Providers::offline(),
        ));
        runtime.block_on(bae_test_support::connect_storage(&manager, &storage));
        let services = runtime.block_on(AppServices::for_test(manager)).unwrap();
        let app = Arc::new(AppHandle::start(services, runtime).unwrap());
        let pointer = bae_core::config::AppDir::under_home(root.path()).active_library_pointer();
        std::fs::create_dir_all(&pointer).unwrap();
        let caller = tokio::runtime::Runtime::new().unwrap();
        let result = caller
            .block_on(app.clone().remove_library(BridgeIfUnsent::Discard))
            .unwrap();
        let BridgeLibraryRemoval::PreferencesRemain { retry, .. } = result else {
            panic!("the invalid preference pointer must be reported");
        };
        drop(app);
        std::fs::remove_dir(&pointer).unwrap();
        caller.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), retry.clone().retry())
                .await
                .unwrap()
                .unwrap();
            retry.retry().await.unwrap();
        });
    }
}
