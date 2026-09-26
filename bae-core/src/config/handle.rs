use super::*;
use coven::StoreDir;
use tokio::sync::watch;

/// Reactive config state for the running app — the single source of truth.
///
/// Holds the live `Config` in a `watch` channel: readers borrow the current
/// value via `config()`, and subscribers receive the whole latest `Config` on
/// every change via `subscribe()`. Every mutation goes through
/// [`Self::update_preferences`] or [`Self::update_store`], which edit the
/// value, persist it to disk, and publish it — so the UI reacts without
/// polling, re-reading, or a restart.
///
/// Persisting is a durable file replace, so it runs on a blocking thread, and
/// readers are never held while it does: the edit is made to a copy under a
/// writers-only lock and published once the file holds it.
pub struct ConfigHandle {
    state: watch::Sender<Config>,
    store_dir: StoreDir,
    /// Held across one edit's read, write and publish, so two edits never
    /// both start from the same value and one lose the other's change.
    writing: std::sync::Mutex<()>,
}

impl ConfigHandle {
    pub fn new(config: Config) -> Self {
        let store_dir = StoreDir::new(config.library_path.clone());
        let (state, _) = watch::channel(config);
        Self {
            state,
            store_dir,
            writing: std::sync::Mutex::new(()),
        }
    }

    /// Borrow the current config.
    pub fn config(&self) -> watch::Ref<'_, Config> {
        self.state.borrow()
    }

    /// Subscribe to the config-state stream. Each change yields the whole latest
    /// `Config`; the channel coalesces to the most recent value.
    pub fn subscribe(&self) -> watch::Receiver<Config> {
        self.state.subscribe()
    }

    /// Begin constructing Coven over this config's retained library directory
    /// and live config stream. The directory itself never leaves this owner.
    pub(crate) fn coven_builder(self: &std::sync::Arc<Self>) -> coven::CovenBuilder {
        let config_handle = std::sync::Arc::clone(self);
        coven::Coven::builder(self.store_dir.clone(), move || {
            config_handle.config().to_coven()
        })
        .coven_migration_policy(coven::CovenMigrationPolicy::ApplyPending)
    }

    #[cfg(test)]
    pub(crate) fn local_blob_exists_for_test(
        &self,
        namespace: &str,
        blob_id: &str,
    ) -> Result<bool, String> {
        self.store_dir
            .local_blob_path(namespace, blob_id)
            .map(|path| path.exists())
            .map_err(|error| error.to_string())
    }

    /// Edit coven's part of the config, persist it to `config.yaml` through
    /// coven, and publish the new state to subscribers. The write path for the
    /// store name and the cloud home.
    pub async fn update_store(
        self: &std::sync::Arc<Self>,
        edit: impl FnOnce(&mut coven::Config) + Send + 'static,
    ) -> Result<(), ConfigError> {
        self.off_runtime(move |handle| handle.update_store_now(edit))
            .await
    }

    /// Edit bae's preferences, persist them to `preferences.yaml`, and publish
    /// the new state to subscribers. The write path for every bae setting.
    pub async fn update_preferences(
        self: &std::sync::Arc<Self>,
        edit: impl FnOnce(&mut Preferences) + Send + 'static,
    ) -> Result<(), ConfigError> {
        self.off_runtime(move |handle| handle.update_preferences_now(edit))
            .await
    }

    /// Rename the library. The name is already validated non-blank by its type.
    pub async fn rename_library(
        self: &std::sync::Arc<Self>,
        name: &crate::library_name::LibraryName,
    ) -> Result<(), ConfigError> {
        let name = name.as_str().to_string();
        self.update_store(move |c| c.store_name = name).await
    }

    /// [`Self::update_store`] for a caller already on a thread that may block.
    pub(crate) fn update_store_now(
        &self,
        edit: impl FnOnce(&mut coven::Config),
    ) -> Result<(), ConfigError> {
        self.update_with(|config| edit(&mut config.inner), Config::write_store_config)
    }

    /// [`Self::update_preferences`] for a caller already on a thread that may
    /// block.
    pub(crate) fn update_preferences_now(
        &self,
        edit: impl FnOnce(&mut Preferences),
    ) -> Result<(), ConfigError> {
        self.update_with(|config| edit(&mut config.prefs), Config::write_preferences)
    }

    /// Run a write on a blocking thread.
    async fn off_runtime(
        self: &std::sync::Arc<Self>,
        write: impl FnOnce(&Self) -> Result<(), ConfigError> + Send + 'static,
    ) -> Result<(), ConfigError> {
        let handle = std::sync::Arc::clone(self);
        match tokio::task::spawn_blocking(move || write(&handle)).await {
            Ok(result) => result,
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        }
    }

    /// Apply `edit` to a copy, write the file it touched, and publish the copy
    /// once that file holds it — including when only the durability step after
    /// the install failed, since readers of the file already see the new value.
    fn update_with(
        &self,
        edit: impl FnOnce(&mut Config),
        write: impl FnOnce(&Config) -> Result<(), WriteError<ConfigError>>,
    ) -> Result<(), ConfigError> {
        let _writing = self.writing.lock().expect("config writer mutex poisoned");
        let mut edited = self.state.borrow().clone();
        edit(&mut edited);
        match write(&edited) {
            Ok(()) => {
                self.state.send_replace(edited);
                Ok(())
            }
            Err(error) => {
                if error.committed() {
                    self.state.send_replace(edited);
                }
                Err(error.into_inner())
            }
        }
    }
}
