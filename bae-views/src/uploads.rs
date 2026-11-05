//! What the upload queue shows of itself: whether the person paused
//! uploads.

use super::*;

impl LibraryViews {
    /// Whether the person paused uploads, live, whether or not anything is
    /// queued.
    pub fn subscribe_uploads_paused(&self) -> coven::LiveQuery<bool> {
        self.store
            .subscribe(|sql| Ok(sql.query_row(bae_store::UPLOADS_PAUSED, [], |row| row.get(0))?))
    }
}
