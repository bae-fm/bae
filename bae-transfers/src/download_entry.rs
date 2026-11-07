//! A release's entry in this device's download queue, as every reader of it
//! shows it: where it stands, how many entries are ahead of it, and what holds
//! the queue back.

use rusqlite::Row;

/// What a release's entry in the download queue does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadKind {
    /// Keeps the release's files whole on this device.
    Pin,
    /// Downloads the release's files into the person's folder and makes the
    /// release local.
    MakeLocal,
}

/// A release's entry in the download queue: what it does, and where it
/// stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadEntry {
    pub kind: DownloadKind,
    pub step: DownloadStep,
}

impl DownloadEntry {
    /// Whether the release's storage actions wait on this entry: it is
    /// downloading, or it is a make-local waiting, which would be undone by
    /// another action. A pin waiting, or an entry that failed, leaves the
    /// release to its actions.
    pub fn holds_the_release(&self) -> bool {
        match (self.kind, &self.step) {
            (_, DownloadStep::Running { .. }) => true,
            (DownloadKind::MakeLocal, DownloadStep::Queued { .. }) => true,
            (_, DownloadStep::Queued { .. } | DownloadStep::Failed { .. }) => false,
        }
    }
}

/// Where a release's entry in the download queue stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadStep {
    /// Waiting, with `ahead` entries before it that have not failed.
    Queued { ahead: u32, wait: Option<QueueWait> },
    /// Downloading.
    Running { wait: Option<QueueWait> },
    /// Stopped by its error, until retried.
    Failed { error: String },
}

/// What keeps the download queue from taking its next entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueWait {
    /// The device has no usable network, as coven says.
    Offline,
    /// The person paused the queue.
    Paused,
}

/// The columns [`download_entry_column`] reads, for the release `release` (an
/// SQL expression naming its id): the entry's kind, state and error, the number of
/// entries ahead of it that have not failed, whether the download queue is
/// paused, and whether coven says the network is usable (NULL when it cannot
/// tell).
pub fn download_entry_sql(release: &str) -> String {
    format!(
        "(SELECT q.kind FROM download_queue q WHERE q.release_id = {release}) AS download_kind, \
         (SELECT q.state FROM download_queue q WHERE q.release_id = {release}) AS download_state, \
         (SELECT q.error FROM download_queue q WHERE q.release_id = {release}) AS download_error, \
         (SELECT COUNT(*) FROM download_queue a \
          JOIN download_queue q ON q.release_id = {release} \
          WHERE a.position < q.position AND a.state <> 'failed') AS download_ahead, \
         (SELECT paused FROM release_queues WHERE queue = 'download') AS downloads_paused, \
         (SELECT usable FROM coven_network) AS network_usable"
    )
}

/// The entry [`download_entry_sql`] read from `row`; `None` when the release
/// has none.
pub fn download_entry_column(row: &Row<'_>) -> rusqlite::Result<Option<DownloadEntry>> {
    let state_column = row.as_ref().column_index("download_state")?;
    let Some(state) = row.get::<_, Option<String>>(state_column)? else {
        return Ok(None);
    };
    let wait = || -> rusqlite::Result<Option<QueueWait>> {
        let usable: Option<bool> = row.get("network_usable")?;
        let paused: bool = row.get("downloads_paused")?;
        Ok(if usable == Some(false) {
            Some(QueueWait::Offline)
        } else if paused {
            Some(QueueWait::Paused)
        } else {
            None
        })
    };
    let kind_column = row.as_ref().column_index("download_kind")?;
    let kind = match row.get::<_, String>(kind_column)?.as_str() {
        "pin" => DownloadKind::Pin,
        "make_local" => DownloadKind::MakeLocal,
        other => {
            return Err(bae_store::read::column_conversion_error(
                row,
                kind_column,
                format!("download kind {other} names no kind"),
            ))
        }
    };
    let step = match state.as_str() {
        "queued" => DownloadStep::Queued { ahead: row.get("download_ahead")?, wait: wait()? },
        "active" => DownloadStep::Running { wait: wait()? },
        "failed" => DownloadStep::Failed { error: row.get("download_error")? },
        other => {
            return Err(bae_store::read::column_conversion_error(
                row,
                state_column,
                format!("download state {other} names no phase"),
            ))
        }
    };
    Ok(Some(DownloadEntry { kind, step }))
}
