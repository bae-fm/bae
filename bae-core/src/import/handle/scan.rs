use super::*;

impl ImportServiceHandle {
    /// Mark the candidate at `path` skipped or unskipped, persisting the change
    /// and broadcasting it so the import view re-tabs the row (New ↔ Skipped).
    /// A no-op request (already in the requested state) persists nothing and
    /// emits no event.
    pub fn set_candidate_skipped(&self, path: String, skipped: bool) -> Result<(), String> {
        self.watched_folders.set_candidate_skipped(path, skipped)
    }

    /// Subscribe to scan events, filtered from the unified event channel.
    /// Returns an mpsc receiver that yields only ScanEvent variants.
    pub fn subscribe_folder_scan_events(&self) -> mpsc::UnboundedReceiver<ScanEvent> {
        self.watched_folders.subscribe_folder_scan_events()
    }
}
