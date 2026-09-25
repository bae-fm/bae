use super::*;
use coven::StoreDir;
use tokio::sync::watch;

/// Reactive config state for the running app — the single source of truth.
///
/// Holds the live `Config` in a `watch` channel: readers borrow the current
/// value via `config()`, and subscribers receive the whole latest `Config` on
/// every change via `subscribe()`. Every mutation goes through `update`, which
/// edits the value, persists it to disk, and publishes it — so the UI reacts
/// without polling, re-reading, or a restart.
pub struct ConfigHandle {
    state: watch::Sender<Config>,
    store_dir: StoreDir,
}

impl ConfigHandle {
    pub fn new(config: Config) -> Self {
        let store_dir = StoreDir::new(config.library_path.clone());
        let (state, _) = watch::channel(config);
        Self { state, store_dir }
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
    pub fn update_store(&self, edit: impl FnOnce(&mut coven::Config)) -> Result<(), ConfigError> {
        self.update_with(|config| edit(&mut config.inner), Config::write_store_config)
    }

    /// [`Self::update_store`] from async code: the write, a durable replace
    /// of `config.yaml`, runs on a blocking thread.
    pub(crate) async fn update_store_off_runtime(
        self: &std::sync::Arc<Self>,
        edit: impl FnOnce(&mut coven::Config) + Send + 'static,
    ) -> Result<(), ConfigError> {
        let handle = std::sync::Arc::clone(self);
        match tokio::task::spawn_blocking(move || handle.update_store(edit)).await {
            Ok(result) => result,
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        }
    }

    /// Edit bae's preferences, persist them to `preferences.yaml`, and publish
    /// the new state to subscribers. The write path for every bae setting.
    pub fn update_preferences(
        &self,
        edit: impl FnOnce(&mut Preferences),
    ) -> Result<(), ConfigError> {
        self.update_with(|config| edit(&mut config.prefs), Config::write_preferences)
    }

    /// Apply `edit` to a copy, write the file it touched, and publish the copy
    /// once that file holds it — including when only the durability step after
    /// the install failed, since readers of the file already see the new value.
    fn update_with(
        &self,
        edit: impl FnOnce(&mut Config),
        write: impl FnOnce(&Config) -> Result<(), WriteError<ConfigError>>,
    ) -> Result<(), ConfigError> {
        let mut save_err = None;
        self.state.send_if_modified(|config| {
            let mut edited = config.clone();
            edit(&mut edited);
            match write(&edited) {
                Ok(()) => {
                    *config = edited;
                    true
                }
                Err(e) => {
                    let committed = e.committed();
                    save_err = Some(e.into_inner());
                    if committed {
                        *config = edited;
                    }
                    committed
                }
            }
        });
        match save_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Rename the library. The name is already validated non-blank by its type.
    pub fn rename_library(
        &self,
        name: &crate::library_name::LibraryName,
    ) -> Result<(), ConfigError> {
        self.update_store(|c| c.store_name = name.as_str().to_string())
    }
}
