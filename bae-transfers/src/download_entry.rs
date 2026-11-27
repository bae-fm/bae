//! A release's entry in this device's download queue, as every reader of it
//! shows it: what it does and where it stands.

use bae_types::release_storage::ReleaseStorageAction;
use rusqlite::{
    types::{FromSql, FromSqlError, FromSqlResult, ValueRef},
    Row,
};

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

// Every entry of the download queue, whose SQL a statement built as text
// holds.
sqlive_compose::read!(
    QueuedDownloadEntries,
    r#"
    SELECT entry.*
    FROM @crate::download_entry::QueuedDownloadEntry AS entry
"#
);

/// The columns [`download_entry_column`] reads, for the release `release` (an
/// SQL expression naming its id): the entry's kind, state and error.
pub fn download_entry_sql(release: &str) -> String {
    let column = |name: &str| {
        format!(
            "(SELECT entry.{name} FROM ({entries}) AS entry \
              WHERE entry.release_id = {release}) AS download_{name}",
            entries = QueuedDownloadEntries::SQL,
        )
    };
    [column("kind"), column("state"), column("error")].join(", ")
}

impl QueuedDownloadEntry {
    /// The entry; an error for a state no entry has.
    pub fn entry(&self) -> Result<DownloadEntry, bae_store::DbError> {
        let kind = [DownloadKind::Pin, DownloadKind::MoveToThisDevice]
            .into_iter()
            .find(|kind| kind.key() == self.kind)
            .ok_or_else(|| bae_store::DbError::unreadable("a download's kind", &self.kind))?;
        let step = step_of(&self.state, || {
            self.error.clone().ok_or_else(|| {
                bae_store::DbError::unreadable("a failed download's error", None::<String>)
            })
        })?
        .ok_or_else(|| bae_store::DbError::unreadable("a download's state", &self.state))?;
        Ok(DownloadEntry { kind, step })
    }
}

/// Where an entry in the state `state` stands, reading its error through
/// `error` for a failed one; `None` for a state no entry has.
fn step_of<E>(
    state: &str,
    error: impl FnOnce() -> Result<String, E>,
) -> Result<Option<DownloadStep>, E> {
    Ok(match state {
        "queued" => Some(DownloadStep::Queued),
        "active" => Some(DownloadStep::Running),
        "failed" => Some(DownloadStep::Failed { error: error()? }),
        _ => None,
    })
}

/// The entry [`download_entry_sql`] read from `row`; `None` when the release
/// has none.
pub fn download_entry_column(row: &Row<'_>) -> rusqlite::Result<Option<DownloadEntry>> {
    let state_column = row.as_ref().column_index("download_state")?;
    let Some(state) = row.get::<_, Option<String>>(state_column)? else {
        return Ok(None);
    };
    let kind = row.get("download_kind")?;
    let step = step_of(&state, || row.get("download_error"))?.ok_or_else(|| {
        bae_store::read::column_conversion_error(
            row,
            state_column,
            format!("download state {state} names no phase"),
        )
    })?;
    Ok(Some(DownloadEntry { kind, step }))
}
