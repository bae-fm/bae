//! The event readers the handle gives out: the feed the identification queue
//! takes, and the test readers of every event, of scan events and of one
//! import's progress.

use super::*;

impl ImportServiceHandle {
    /// Every `ImportProgress` of `import_id` from its start, none dropped.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_import(&self, import_id: String) -> ImportProgressFeed {
        self.event_tx.import_progress_feed(import_id, &self.runtime_handle)
    }

    /// Every import event since the service started, for the one
    /// identification queue that takes it (see
    /// [`crate::ImportEventBus::take_feed`]).
    pub(crate) fn take_event_feed(&self) -> Option<ImportEvents> {
        self.event_tx.take_feed()
    }

    /// Every import event from now on, none dropped, for a test to wait on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_event(&self) -> ImportEvents {
        self.event_tx.every_event()
    }

    /// Every scan event from now on, none dropped, for a test to wait on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_scan_event_for_test(&self) -> ScanEvents {
        self.event_tx.scan_events(&self.runtime_handle)
    }

    /// Stop the first thread that reports a measured percent of `phase` of
    /// an import before the report is recorded or delivered, until
    /// [`Self::release_progress`], for a test that acts while an import is
    /// partway through.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn hold_progress_at(&self, phase: crate::ImportPhase) {
        self.event_tx.hold_progress_at(phase);
    }

    /// Wait until a thread is stopped where [`Self::hold_progress_at`] said.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn progress_held(&self) {
        self.event_tx.send_held().await;
    }

    /// Let the stopped thread go on.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn release_progress(&self) {
        self.event_tx.release_send();
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn emit_event_for_test(&self, event: ImportEvent) {
        self.event_tx.send(event);
    }
}
