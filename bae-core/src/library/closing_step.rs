//! The steps of closing the open library, which the app shows while it
//! waits instead of a bare spinner.

/// A step closing the open library runs, in the order they run. Each starts
/// once the one before it has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosingStep {
    /// Stopping playback, saving where it was.
    StoppingPlayback,
    /// Stopping identification and import, waiting for the file each is on.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    StoppingImports,
    /// Stopping sync, once the pass that runs has ended.
    StoppingSync,
    /// Ending the library's background work.
    FinishingWork,
    /// Closing the library's database.
    ClosingStore,
}
