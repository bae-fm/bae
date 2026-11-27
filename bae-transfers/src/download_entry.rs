//! A release's entry in this device's download queue, as every reader of it
//! shows it: what it does and where it stands.

use bae_types::release_storage::ReleaseStorageAction;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};

/// What a release's entry in the download queue does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadKind {
    /// Keeps the release's files whole on this device.
    Pin,
    /// Downloads the release's files into the person's folder and makes the
    /// release local.
    MoveToThisDevice,
}

impl DownloadKind {
    /// The kind's name in the download queue's and its batch's `kind` columns:
    /// its release storage action's.
    pub fn key(self) -> &'static str {
        match self {
            Self::Pin => ReleaseStorageAction::Pin,
            Self::MoveToThisDevice => ReleaseStorageAction::MoveToThisDevice,
        }
        .key()
    }
}

impl FromSql for DownloadKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let name = value.as_str()?;
        [Self::Pin, Self::MoveToThisDevice].into_iter().find(|kind| kind.key() == name).ok_or_else(
            || FromSqlError::Other(format!("download kind {name} names no kind").into()),
        )
    }
}

/// A release's entry in the download queue: what it does, and where it
/// stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadEntry {
    pub kind: DownloadKind,
    pub step: DownloadStep,
}

/// Where a release's entry in the download queue stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadStep {
    /// Waiting its turn.
    Queued,
    /// Downloading.
    Running,
    /// Stopped by its error, until retried.
    Failed { error: String },
}

// Each release's entry in the download queue, by the release.
sqlive_compose::fragment!(
    QueuedDownloadEntry,
    r#"
    SELECT queued.release_id, queued.kind, queued.state, queued.error
    FROM download_queue AS queued
"#
);

impl QueuedDownloadEntry {
    /// The entry; an error for a state no entry has.
    pub fn entry(&self) -> Result<DownloadEntry, bae_store::DbError> {
        let kind = [DownloadKind::Pin, DownloadKind::MoveToThisDevice]
            .into_iter()
            .find(|kind| kind.key() == self.kind)
            .ok_or_else(|| bae_store::DbError::unreadable("a download's kind", &self.kind))?;
        let step = match self.state.as_str() {
            "queued" => DownloadStep::Queued,
            "active" => DownloadStep::Running,
            "failed" => DownloadStep::Failed {
                error: self.error.clone().ok_or_else(|| {
                    bae_store::DbError::unreadable("a failed download's error", None::<String>)
                })?,
            },
            _ => return Err(bae_store::DbError::unreadable("a download's state", &self.state)),
        };
        Ok(DownloadEntry { kind, step })
    }
}
