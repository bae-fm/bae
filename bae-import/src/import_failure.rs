//! What became of an import that failed.
//!
//! Written the moment the import fails and read back by the per-candidate
//! query, so the pane holds no copy of its own.

use chrono::{DateTime, Utc};

/// The last import of this candidate that failed, as the pane still shows it
/// after a relaunch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFailure {
    pub reason: ImportFailureReason,
    pub failed_at: DateTime<Utc>,
}

/// The diagnostic detail of an import failure. Surfaces pair it with their
/// localized import-failure message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFailureReason {
    pub detail: String,
}

impl ImportFailureReason {
    pub fn ui_error(&self) -> bae_diagnostics::UiError {
        bae_diagnostics::UiError::import(&self.detail)
    }
}

impl ImportFailure {
    /// The failure an import's error is recorded as.
    pub(crate) fn of(error: &crate::ImportError, failed_at: DateTime<Utc>) -> Self {
        Self { reason: ImportFailureReason { detail: error.to_string() }, failed_at }
    }
}

#[cfg(test)]
impl ImportFailureReason {
    pub(crate) fn error(detail: impl Into<String>) -> Self {
        Self { detail: detail.into() }
    }
}

#[cfg(test)]
impl ImportFailure {
    pub(crate) fn error_only(error: impl Into<String>, failed_at: DateTime<Utc>) -> Self {
        Self { reason: ImportFailureReason { detail: error.into() }, failed_at }
    }
}
