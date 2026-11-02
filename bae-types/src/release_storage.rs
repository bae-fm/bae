//! The storage transitions a person can ask of a release.

/// A storage transition the user can trigger from the release "Storage…" sheet.
/// `Upload` / `DetachAudio` move between the two storage states; `Pin` / `Unpin`
/// toggle the orthogonal cache pin on a remote release. The core computes which are
/// available; the UI renders them and never re-derives availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageAction {
    /// Local -> Remote (upload to the cloud home).
    Upload,
    /// Keep a remote release offline: fetch its files into coven's pinned cache.
    Pin,
    /// Stop keeping a remote release offline: drop its files from the pinned
    /// cache (still in the cloud).
    Unpin,
    /// Remote -> Local (move the files back out to a user folder).
    DetachAudio,
}
