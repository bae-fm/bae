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

    /// Edit the config, persist it to disk, and publish the new state to
    /// subscribers. The single write path for every config change.
    pub fn update(&self, edit: impl FnOnce(&mut Config)) -> Result<(), ConfigError> {
        let mut save_err = None;
        self.state.send_if_modified(|config| {
            let mut edited = config.clone();
            edit(&mut edited);
            match edited.write_config_yaml() {
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

    pub fn has_discogs_key(&self) -> bool {
        self.config().discogs.is_some()
    }

    /// Rename the library. The name is already validated non-blank by its type.
    pub fn rename_library(
        &self,
        name: &crate::library_name::LibraryName,
    ) -> Result<(), ConfigError> {
        self.update(|c| c.store_name = name.as_str().to_string())
    }
}
