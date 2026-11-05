//! The steps of closing the open library, or removing it, which the app
//! shows while it waits instead of a bare spinner.

/// A step closing or removing the open library runs, in the order they run.
/// Each starts once the one before it has ended. A close stops sync and
/// closes the store; a removal leaves the library and deletes its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosingStep {
    /// Stopping playback, saving where it was.
    StoppingPlayback,
    /// Stopping identification and import, waiting for the file each is on.
    #[cfg(desktop)]
    StoppingImports,
    /// Stopping sync, once the pass that runs has ended.
    StoppingSync,
    /// Ending the library's background work.
    FinishingWork,
    /// Closing the library's database.
    ClosingStore,
    /// Leaving the library: this device tells the library's other devices it
    /// is gone, discarding what it hasn't sent, and its copy of the store here
    /// is deleted.
    LeavingLibrary,
    /// Deleting this device's preferences and caches for the library.
    DeletingFiles,
}
