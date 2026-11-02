//! The sync activity indicator's read: everything sync is doing, in one go.

use super::*;

impl LibraryViews {
    /// Follow everything sync is doing, each value read in one go. Bytes
    /// moving change none of what it reads, so they send nothing.
    pub(crate) fn subscribe_sync_activity(&self) -> coven::LiveQuery<crate::library::SyncActivity> {
        self.database.subscribe(crate::library::sync_activity::sync_activity_on)
    }
}
