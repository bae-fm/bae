//! What the upload queue shows of itself: whether the person paused
//! uploads, and whether files move only on unmetered networks.

use super::*;

impl LibraryViews {
    /// Whether the person paused uploads, live, whether or not anything is
    /// queued.
    pub fn subscribe_uploads_paused(&self) -> coven::LiveQuery<bool, bae_store::DbError> {
        self.store
            .subscribe(|sql| Ok(sql.query_row(bae_store::UPLOADS_PAUSED, [], |row| row.get(0))?))
    }

    /// Whether files move only on unmetered networks, live.
    pub fn subscribe_unmetered_only(&self) -> coven::LiveQuery<bool, bae_store::DbError> {
        self.store
            .subscribe(|sql| Ok(sql.query_row(bae_store::UNMETERED_ONLY, [], |row| row.get(0))?))
    }
}
