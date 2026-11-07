//! Where an import puts a release: the `preferences` row's
//! `import_to_cloud` and `import_pinned`.

use serde::{Deserialize, Serialize};

/// The import storage choice a person last made in an import pane. Device-local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportStoragePreferences {
    /// Whether imports go to storage, when the library syncs.
    pub cloud: bool,
    /// Whether a release that goes to the cloud stays downloaded here.
    pub pinned: bool,
}

impl Default for ImportStoragePreferences {
    fn default() -> Self {
        Self { cloud: true, pinned: true }
    }
}
