//! Where an import puts a release: the `preferences` row's
//! `import_destination`.

use serde::{Deserialize, Serialize};

/// Where an import's files go: the one choice a person last made in an import
/// pane or in settings. Device-local.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportDestination {
    /// The files stay in place on this device and are never uploaded.
    ThisDevice,
    /// The files are uploaded, so every device can play them. This device
    /// keeps them only until they are stored.
    Cloud,
    /// The files are uploaded, and this device keeps them whole once they are
    /// stored.
    #[default]
    CloudPinned,
}

impl ImportDestination {
    /// Where an import actually goes: the stored choice when the library has
    /// storage to upload to, this device otherwise.
    pub fn with_storage(self, storage_configured: bool) -> Self {
        if storage_configured {
            self
        } else {
            Self::ThisDevice
        }
    }

    /// Whether the files are uploaded.
    pub fn goes_to_cloud(self) -> bool {
        match self {
            Self::ThisDevice => false,
            Self::Cloud | Self::CloudPinned => true,
        }
    }

    /// Whether this device keeps the files whole once they are stored,
    /// pinned rather than evictable.
    pub fn keeps_files_here(self) -> bool {
        matches!(self, Self::CloudPinned)
    }
}
