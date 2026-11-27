//! What the upload queue shows of itself: whether the person paused
//! uploads, and whether files move only on unmetered networks.

use super::*;
use bae_store::statement_row;
use bae_store::upload_flags::{UnmeteredOnlyParams, UploadsPausedParams};

impl LibraryViews {
    /// Whether the person paused uploads, live, whether or not anything is
    /// queued.
    pub fn subscribe_uploads_paused(&self) -> coven::LiveQuery<bool, bae_store::DbError> {
        self.store.subscribe(|sql| Ok(statement_row(&sql, &UploadsPausedParams {})?.paused))
    }

    /// Whether files move only on unmetered networks, live.
    pub fn subscribe_unmetered_only(&self) -> coven::LiveQuery<bool, bae_store::DbError> {
        self.store.subscribe(|sql| Ok(statement_row(&sql, &UnmeteredOnlyParams {})?.unmetered_only))
    }
}
