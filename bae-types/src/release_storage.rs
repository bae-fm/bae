//! The storage transitions a person can ask of a release.

/// A storage transition the user can trigger from the release "Storage…" sheet.
/// `Upload` / `MoveToThisDevice` move between the two storage states; `Pin` / `Unpin`
/// toggle the orthogonal cache pin on a remote release. The core computes which are
/// available; the UI renders them and never re-derives availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageAction {
    /// Local -> Remote (upload to storage).
    Upload,
    /// Keep a remote release offline: fetch its files into coven's pinned cache.
    Pin,
    /// Stop keeping a remote release offline: drop its files from the pinned
    /// cache (still in the cloud).
    Unpin,
    /// Remote -> Local (move the files back out to a user folder).
    MoveToThisDevice,
}

impl ReleaseStorageAction {
    /// The name the transition goes by wherever it is stored or reported: the
    /// `release_storage_action` column, telemetry, automation requests.
    pub fn key(self) -> &'static str {
        match self {
            Self::Upload => "upload",
            Self::Pin => "pin",
            Self::Unpin => "unpin",
            Self::MoveToThisDevice => "move_to_this_device",
        }
    }

    /// The transition named `key`, or `None` when no transition goes by it.
    pub fn from_key(key: &str) -> Option<Self> {
        [Self::Upload, Self::Pin, Self::Unpin, Self::MoveToThisDevice]
            .into_iter()
            .find(|action| action.key() == key)
    }
}

#[cfg(test)]
#[path = "release_storage_tests.rs"]
mod tests;
