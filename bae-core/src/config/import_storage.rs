//! Where an import puts a release: `preferences.yaml`'s `import_storage`
//! mapping.

use serde::{Deserialize, Serialize};

/// The import storage choice a person last made in an import pane. Device-local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportStoragePreferences {
    /// Whether imports go to the cloud home, when the library has one.
    pub cloud: bool,
    /// Whether a release that goes to the cloud stays downloaded here.
    pub pinned: bool,
}

impl Default for ImportStoragePreferences {
    fn default() -> Self {
        Self {
            cloud: true,
            pinned: true,
        }
    }
}

impl super::Config {
    /// Where an import goes: to the cloud, pinned as chosen, when the choice is
    /// the cloud and the library has a cloud home; locally otherwise.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn import_destination(&self) -> crate::import::ImportDestination {
        let storage = self.prefs.import_storage;
        if storage.cloud && self.cloud_home.provider.is_some() {
            crate::import::ImportDestination::Remote {
                pin: storage.pinned,
            }
        } else {
            crate::import::ImportDestination::Local
        }
    }
}
