use super::*;
use tokio::sync::watch;

/// Publishes this device's preferences after their atomic file replacement.
pub struct ConfigHandle {
    state: watch::Sender<Config>,
    writing: std::sync::Mutex<()>,
}

impl ConfigHandle {
    pub fn new(config: Config) -> Self {
        let (state, _) = watch::channel(config);
        Self {
            state,
            writing: std::sync::Mutex::new(()),
        }
    }

    pub fn config(&self) -> watch::Ref<'_, Config> {
        self.state.borrow()
    }

    pub fn subscribe(&self) -> watch::Receiver<Config> {
        self.state.subscribe()
    }

    /// Edit the preferences, replace their file, then publish them. Blocks on
    /// the file write; [`crate::library::LibraryManager`] runs it off the
    /// async workers.
    pub(crate) fn update_preferences_now(
        &self,
        edit: impl FnOnce(&mut Preferences),
    ) -> Result<(), ConfigError> {
        // The lock orders writers and guards no data of its own. A write that
        // panicked changed at most the file, which is replaced whole or not at
        // all, and the next write replaces it again from the published value.
        let _writing = self
            .writing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut edited = self.state.borrow().clone();
        edit(&mut edited.prefs);
        match edited.save_preferences() {
            Ok(()) => {
                self.state.send_replace(edited);
                Ok(())
            }
            Err(error) => {
                if matches!(&error, ConfigError::File(file) if file.installed_new_bytes()) {
                    self.state.send_replace(edited);
                }
                Err(error)
            }
        }
    }
}
