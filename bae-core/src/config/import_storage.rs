//! Where an import puts a release: `preferences.yaml`'s `import_storage`
//! mapping.

use serde::{Deserialize, Serialize};

/// The storage an import starts from: the last choice a person made in an
/// import pane, kept in core so every surface that imports, or moves a release
/// to the cloud, starts from the same answer. Device-local, like the rest of
/// the preferences: it names this device's link and disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportStoragePreferences {
    /// Whether an import goes to the cloud home. Read only when the library
    /// has one: without a home every import stays local, and the choice waits
    /// here for a home to exist. Defaults to `true`.
    pub cloud: bool,
    /// Whether a release that goes to the cloud also stays downloaded on this
    /// device. Moving a library release to the cloud offers the same choice
    /// and reads it from here. Defaults to `true`.
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
    /// Whether an import goes to the cloud by this library's stored choice:
    /// only when the choice is the cloud and the library has a cloud home.
    pub fn imports_to_cloud(&self) -> bool {
        self.prefs.import_storage.cloud && self.cloud_home.provider.is_some()
    }
}
