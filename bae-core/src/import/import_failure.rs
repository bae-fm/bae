//! What became of an import that failed.
//!
//! Written the moment the import fails and read back by the per-candidate
//! query, so the pane holds no copy of its own.

use chrono::{DateTime, Utc};

/// The last import of this candidate that failed, as the pane still shows it
/// after a relaunch. An artist identity conflict carries the two library rows
/// the pane can offer to consolidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFailure {
    pub error: String,
    pub failed_at: DateTime<Utc>,
    pub artist_identity_conflict: Option<crate::import::ArtistIdentityConflict>,
}

#[cfg(test)]
impl ImportFailure {
    pub(crate) fn error_only(error: impl Into<String>, failed_at: DateTime<Utc>) -> Self {
        Self {
            error: error.into(),
            failed_at,
            artist_identity_conflict: None,
        }
    }
}
