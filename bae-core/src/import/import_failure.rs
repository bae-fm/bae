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
    pub reason: ImportFailureReason,
    pub failed_at: DateTime<Utc>,
    pub artist_identity_conflict: Option<crate::import::ArtistIdentityConflict>,
}

/// Why an import failed, kept as what it is rather than as rendered text, so
/// every surface words it in the person's language when it shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportFailureReason {
    /// An exact identity of the release is already in the library, as this
    /// album. The failure is stored by the album's id; the title is the album's
    /// title as of the read, so a rename shows.
    AlreadyInLibrary { album_id: String, album_title: String },
    /// Any other failure, as its diagnostic text: untranslated detail beside a
    /// surface's generic line, never a sentence of its own.
    Error { detail: String },
}

impl ImportFailureReason {
    /// This failure as a surface states it: its class, which the surface words
    /// in the person's language, and any diagnostic detail.
    pub fn ui_error(&self) -> crate::ui::UiError {
        match self {
            Self::AlreadyInLibrary { .. } => crate::ui::UiError::diagnostic(
                crate::ui::UiErrorCategory::AlreadyInLibrary,
                "",
            ),
            Self::Error { detail } => crate::ui::UiError::import(detail),
        }
    }
}

#[cfg(test)]
impl ImportFailureReason {
    pub(crate) fn error(detail: impl Into<String>) -> Self {
        Self::Error {
            detail: detail.into(),
        }
    }
}

#[cfg(test)]
impl ImportFailure {
    pub(crate) fn error_only(error: impl Into<String>, failed_at: DateTime<Utc>) -> Self {
        Self {
            reason: ImportFailureReason::Error {
                detail: error.into(),
            },
            failed_at,
            artist_identity_conflict: None,
        }
    }
}
