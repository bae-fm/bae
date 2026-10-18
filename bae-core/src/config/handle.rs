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

    pub async fn update_preferences(
        self: &std::sync::Arc<Self>,
        edit: impl FnOnce(&mut Preferences) + Send + 'static,
    ) -> Result<(), ConfigError> {
        let handle = self.clone();
        match tokio::task::spawn_blocking(move || handle.update_preferences_now(edit)).await {
            Ok(result) => result,
            Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
            Err(error) => Err(ConfigError::Config(format!(
                "preference write cancelled: {error}"
            ))),
        }
    }

    pub(crate) fn update_preferences_now(
        &self,
        edit: impl FnOnce(&mut Preferences),
    ) -> Result<(), ConfigError> {
        let _writing = self.writing.lock().expect("config writer mutex poisoned");
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
