//! The transfer queues' tables: `download_queue` and `output_queue`, each
//! entry a release in the order queued with its phase, and whether each queue
//! is paused (`release_queues`), as functions over coven's SQL contexts. The
//! queue owners ([`crate::QueuedReleases`]) are the only writers; the panes,
//! the detail views and the sync activity read the same tables through live
//! queries, whose statements are in `rows` and
//! [`download_list`](super::download_list).
//!
//! Its tables are `tables.sql`, and the statements that write them are in
//! `writes`.

use super::rows;
use super::writes::{self, QueueWrites};
use crate::download_entry::DownloadKind;
use crate::download_snapshot::DownloadRequest;
use crate::output_snapshot::{OutputKind, OutputRequest};
#[cfg(any(desktop, test, feature = "test-utils"))]
use crate::release_queue::ReleaseQueueRow;
#[cfg(any(desktop, test, feature = "test-utils"))]
use crate::release_queue::ReleaseQueueSnapshot;
use crate::release_queue::{ReleaseQueueCounts, ReleaseQueueEntry};
use bae_store::{DbError, Store, WriteStatements};
use coven::{CovenWrite, SqlReadContext};
use rusqlite::OptionalExtension;

/// What a queue's entries carry beyond their release, and how the queue reads
/// and writes its table.
pub trait QueuedPayload: QueueWrites + Clone + PartialEq + Send + Sync + 'static {
    /// The queue's row in `release_queues`.
    const QUEUE: &'static str;
    /// What a look at the queue sees of the network: coven's word on it for
    /// a queue whose entries need it, nothing for the others.
    type Network: Send + 'static;
    /// The entry the worker takes next, unless the queue is paused or, for a
    /// queue that needs the network, coven says the device has none: the
    /// active entry if there is one, else the first queued. An active entry is
    /// one the worker took and has not ended, so after a restart it is taken
    /// first. Beside it, what the look saw of the network.
    #[doc(hidden)]
    fn next_up_on(sql: &SqlReadContext<'_>) -> Result<rows::NextUp<Self>, DbError>;
    /// Resolves once the network may let an entry go that it held back from
    /// the look that saw `seen`; never for a queue that needs no network.
    #[doc(hidden)]
    fn network_changed_from(
        store: &Store,
        seen: Self::Network,
    ) -> impl std::future::Future<Output = ()> + Send;
    /// How many entries the queue holds in each phase, and what holds it
    /// back.
    fn counts_on(sql: &SqlReadContext<'_>) -> Result<ReleaseQueueCounts, DbError>;
    /// Every entry of the queue, in the order queued, each beside its
    /// release.
    #[cfg(any(desktop, test, feature = "test-utils"))]
    fn rows_on(sql: &SqlReadContext<'_>) -> Result<Vec<ReleaseQueueRow<Self, ()>>, DbError>;
    /// What cancelling `release_id`'s entry does in the write that takes it
    /// out of the queue, before it goes.
    fn cancelling_in(_sql: &CovenWrite<'_>, _release_id: &str) -> Result<(), DbError> {
        Ok(())
    }
}

/// A download carries what it does: a pin nothing more, a move-to-this-device its
/// folder.
impl QueuedPayload for DownloadRequest {
    const QUEUE: &'static str = "download";
    type Network = Option<bool>;

    fn next_up_on(sql: &SqlReadContext<'_>) -> Result<rows::NextUp<Self>, DbError> {
        rows::next_download_on(sql)
    }

    fn network_changed_from(
        store: &Store,
        seen: Option<bool>,
    ) -> impl std::future::Future<Output = ()> + Send {
        super::network_changed_from(store, seen)
    }

    fn counts_on(sql: &SqlReadContext<'_>) -> Result<ReleaseQueueCounts, DbError> {
        rows::download_counts_on(sql)
    }

    #[cfg(any(desktop, test, feature = "test-utils"))]
    fn rows_on(sql: &SqlReadContext<'_>) -> Result<Vec<ReleaseQueueRow<Self, ()>>, DbError> {
        let every = bae_store::ListWindow::top(u64::MAX);
        let read = super::download_list::download_window_on(sql, &every)?;
        Ok(read.rows.into_iter().map(|(row, _key)| row).collect())
    }

    /// Cancelling a pin removes what it downloaded: the release's files
    /// leave this device in the write that takes the entry out, the partial
    /// download a stopped pin keeps among them. A move-to-this-device's cancel leaves
    /// the files as they are.
    fn cancelling_in(sql: &CovenWrite<'_>, release_id: &str) -> Result<(), DbError> {
        let kind: Option<String> = sql
            .query_row(
                "SELECT kind FROM download_queue WHERE release_id = ?",
                [release_id],
                |row| row.get(0),
            )
            .optional()?;
        if kind.as_deref() == Some("pin") {
            crate::storage_moves::unpin_files_on(sql, release_id)?;
        }
        Ok(())
    }
}

/// An output carries its target folder and what it writes there, a save its
/// preset and filename and folder name patterns whole.
impl QueuedPayload for OutputRequest {
    const QUEUE: &'static str = "output";
    type Network = ();

    fn next_up_on(sql: &SqlReadContext<'_>) -> Result<rows::NextUp<Self>, DbError> {
        rows::next_output_on(sql)
    }

    fn network_changed_from(
        _store: &Store,
        _seen: (),
    ) -> impl std::future::Future<Output = ()> + Send {
        std::future::pending()
    }

    fn counts_on(sql: &SqlReadContext<'_>) -> Result<ReleaseQueueCounts, DbError> {
        rows::output_counts_on(sql)
    }

    #[cfg(any(desktop, test, feature = "test-utils"))]
    fn rows_on(sql: &SqlReadContext<'_>) -> Result<Vec<ReleaseQueueRow<Self, ()>>, DbError> {
        rows::output_rows_on(sql)
    }
}

/// What a download does, from its stored `kind` and folder, or what of them
/// no writer here stores together.
pub(super) fn download_request(
    kind: DownloadKind,
    target_dir: Option<std::path::PathBuf>,
) -> Result<DownloadRequest, String> {
    match (kind, target_dir) {
        (DownloadKind::Pin, None) => Ok(DownloadRequest::Pin),
        #[cfg(desktop)]
        (DownloadKind::MoveToThisDevice, Some(target_dir)) => {
            Ok(DownloadRequest::MoveToThisDevice { target_dir })
        }
        (kind, target_dir) => Err(format!("download kind {kind:?} with folder {target_dir:?}")),
    }
}

/// What an output writes and where, from its stored folder, `kind`, and the
/// preset, filename pattern, folder pattern and extras a save stores, in that
/// order.
pub(super) fn output_request(
    target_dir: std::path::PathBuf,
    kind: &str,
    save: [Option<String>; 4],
) -> Result<OutputRequest, String> {
    let refused =
        |what: &str, json: &str, error: serde_json::Error| format!("save {what} {json}: {error}");
    let kind = match (kind, save) {
        ("export", [None, None, None, None]) => OutputKind::Export,
        ("save", [Some(preset), Some(filename_pattern), Some(folder_pattern), Some(extras)]) => {
            OutputKind::Save {
                preset: serde_json::from_str(&preset)
                    .map_err(|error| refused("preset", &preset, error))?,
                filename_pattern: serde_json::from_str(&filename_pattern)
                    .map_err(|error| refused("filename pattern", &filename_pattern, error))?,
                folder_pattern: serde_json::from_str(&folder_pattern)
                    .map_err(|error| refused("folder pattern", &folder_pattern, error))?,
                extras: serde_json::from_str(&extras)
                    .map_err(|error| refused("extras", &extras, error))?,
            }
        }
        (kind, [preset, filename_pattern, folder_pattern, extras]) => {
            return Err(format!(
                "output kind {kind:?} with preset {preset:?}, filename pattern \
                 {filename_pattern:?}, folder pattern {folder_pattern:?} and extras {extras:?}"
            ))
        }
    };
    Ok(OutputRequest { target_dir, kind })
}

#[cfg(any(desktop, test, feature = "test-utils"))]
pub(crate) fn release_queue_on<P: QueuedPayload>(
    sql: &SqlReadContext<'_>,
) -> Result<ReleaseQueueSnapshot<P, ()>, DbError> {
    let rows = P::rows_on(sql)?;
    let counts = P::counts_on(sql)?;
    let paused = counts.paused();
    Ok(ReleaseQueueSnapshot { rows, total: counts.total, paused })
}

/// `release_id`'s entries in the download and export queues.
#[cfg(desktop)]
pub fn release_queue_entries_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<crate::release_queue::ReleaseQueueEntries, DbError> {
    Ok(crate::release_queue::ReleaseQueueEntries {
        download: rows::download_of_release_on(sql, release_id)?,
        output: rows::output_of_release_on(sql, release_id)?,
    })
}

/// Queue `entries` in order, passing over a release already in the queue or
/// one the library doesn't hold; `true` when any went in.
#[cfg(desktop)]
pub(crate) fn enqueue_releases_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    entries: &[ReleaseQueueEntry<P>],
) -> Result<bool, DbError> {
    let mut added = false;
    for entry in entries {
        added |= P::insert_on(sql, entry)? > 0;
    }
    Ok(added)
}

/// Queue each of `entries` whose release the library holds, in order, in one
/// write that decides which: it passes over a release already in the queue,
/// and names each one the library no longer holds.
#[cfg(desktop)]
pub(crate) fn enqueue_held_releases_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    entries: Vec<ReleaseQueueEntry<P>>,
) -> Result<ReleasesQueued, DbError> {
    let release_ids =
        serde_json::Value::from_iter(entries.iter().map(|entry| entry.release_id.as_str()))
            .to_string();
    let missing = missing_releases_on(sql, &release_ids)?;
    let held: Vec<_> =
        entries.into_iter().filter(|entry| !missing.contains(&entry.release_id)).collect();
    Ok(ReleasesQueued { added: enqueue_releases_on(sql, &held)?, missing })
}

/// What one write queueing releases did.
#[derive(Debug, PartialEq, Eq)]
pub struct ReleasesQueued {
    /// Whether the write added any release or queued one again.
    pub added: bool,
    /// Each release asked for that the library no longer holds, in the order
    /// asked. Queueing it changed nothing.
    pub missing: Vec<String>,
}

/// The releases of `release_ids` (a JSON array) the library no longer holds,
/// in the selection's order.
fn missing_releases_on(sql: &CovenWrite<'_>, release_ids: &str) -> Result<Vec<String>, DbError> {
    Ok(sql.query(
        "SELECT requested.value \
         FROM json_each(?1) AS requested \
         LEFT JOIN releases ON releases.id = requested.value \
         WHERE releases.id IS NULL \
         ORDER BY requested.key",
        [release_ids],
        |row| row.get(0),
    )?)
}

/// Queue to pin, in the order given, each of `release_ids` not wholly pinned
/// on this device, as its storage row says, in one write that decides which.
/// The write skips one waiting or downloading, or already pinned, and queues
/// one whose download failed again in its place, as a retry does. The write
/// names each one the library no longer holds. Whether coven pins a release's
/// files is coven's to say when the pin runs: it refuses a file not in the
/// cloud, and the entry fails with its reason.
pub(crate) fn enqueue_pins_on(
    sql: &CovenWrite<'_>,
    release_ids: &[String],
    created_at: i64,
) -> Result<ReleasesQueued, DbError> {
    let release_ids = serde_json::Value::from(release_ids.to_vec()).to_string();
    let added = sql.run(&writes::QueuePinsParams { created_at, releases: &release_ids })? > 0;
    Ok(ReleasesQueued { added, missing: missing_releases_on(sql, &release_ids)? })
}

/// Start `release_id`'s entry, or start it again when the process that
/// started it ended; `false` when the queue does not hold it, having been
/// cancelled or never queued, or when the queue is paused, a pause having
/// landed after the worker picked the entry.
pub(crate) fn activate_queued_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<bool, DbError> {
    Ok(P::activate_on(sql, release_id)? > 0)
}

/// The active entry for `release_id` finished: it leaves the queue.
/// `false` when it had already left, cancelled while it ran.
pub(crate) fn finish_queued_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<bool, DbError> {
    Ok(P::finish_on(sql, release_id)? > 0)
}

/// `release_id`'s entry failed, starting or running: it stays listed
/// with `error` until retried. `false` when it had already left.
pub(crate) fn fail_queued_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
    error: &str,
) -> Result<bool, DbError> {
    Ok(P::fail_on(sql, release_id, error)? > 0)
}

/// The running work of `release_id`'s entry failed: it stays listed with
/// `error` until retried. `false` when the entry is no longer active, having
/// left the queue or gone back to queued when a pause stopped its work.
pub(crate) fn fail_active_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
    error: &str,
) -> Result<bool, DbError> {
    Ok(P::fail_active_on(sql, release_id, error)? > 0)
}

/// Queue every failed entry again, in its place; `true` when any had
/// failed.
pub(crate) fn retry_failed_releases_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
) -> Result<bool, DbError> {
    Ok(P::retry_all_on(sql)? > 0)
}

/// Queue `release_id`'s failed entry again, in its place; `true` when it
/// had failed.
pub(crate) fn retry_failed_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<bool, DbError> {
    Ok(P::retry_on(sql, release_id)? > 0)
}

/// Pause or resume the queue, saying whether it was paused. Pausing puts the
/// active entry back to queued, in its place, since its work is stopped with
/// the pause: resuming starts it again.
pub(crate) fn set_release_queue_paused_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    paused: bool,
) -> Result<bool, DbError> {
    let was: bool =
        sql.query_row("SELECT paused FROM release_queues WHERE queue = ?", [P::QUEUE], |row| {
            row.get(0)
        })?;
    sql.run(&writes::SetQueuePausedParams { queue: P::QUEUE, paused })?;
    if paused {
        P::requeue_active_on(sql)?;
    }
    Ok(was)
}

/// Take `release_id`'s entry out, in any phase, doing what cancelling
/// that kind of entry does in the same write; `false` when the queue does not
/// hold it.
pub(crate) fn cancel_queued_release_on<P: QueuedPayload>(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<bool, DbError> {
    P::cancelling_in(sql, release_id)?;
    Ok(P::delete_on(sql, release_id)? > 0)
}

/// Queue to move to this device into `target_dir`, in the order given, each of
/// `release_ids`, in one write that decides which. The write skips one waiting
/// or downloading and turns one whose download failed into this
/// move-to-this-device in its place. The write names each one the library no
/// longer holds. Whether coven moves a release's files is coven's to say when
/// the move runs: it refuses a file not in the cloud, and the entry fails
/// with its reason.
#[cfg(desktop)]
pub(crate) fn enqueue_moves_to_this_device_on(
    sql: &CovenWrite<'_>,
    release_ids: &[String],
    target_dir: &std::path::Path,
    created_at: i64,
) -> Result<ReleasesQueued, DbError> {
    let release_ids = serde_json::Value::from(release_ids.to_vec()).to_string();
    let added = sql.run(&writes::QueueMovesToThisDeviceParams {
        created_at,
        releases: &release_ids,
        target_dir: coven::path_bytes(target_dir),
    })? > 0;
    Ok(ReleasesQueued { added, missing: missing_releases_on(sql, &release_ids)? })
}

/// Empty the export and save queue and let it run: what the library opens
/// with, since it does not outlive the process that ran it. The download
/// queue and its pause are kept, as coven's upload pause is: the entry that
/// was downloading runs again.
pub fn clear_transfers_on(sql: &CovenWrite<'_>) -> Result<(), DbError> {
    sql.run(&writes::ClearOutputQueueParams {})?;
    sql.run(&writes::SetQueuePausedParams { queue: OutputRequest::QUEUE, paused: false })?;
    Ok(())
}

/// The worker's moves of a queue's entries, for a test that moves entries in
/// the table without a worker running them.
#[cfg(any(test, feature = "test-utils"))]
pub mod worker_moves {
    use super::*;

    /// Start `release_id`'s entry, as the worker does once its task runs. An
    /// entry starts only on a running queue, so the queue is resumed in the
    /// same write, and no worker is woken: one parked on the paused queue
    /// stays parked, leaving the entry to the test.
    pub fn activate_on<P: QueuedPayload>(
        sql: &CovenWrite<'_>,
        release_id: &str,
    ) -> Result<bool, DbError> {
        set_release_queue_paused_on::<P>(sql, false)?;
        activate_queued_release_on::<P>(sql, release_id)
    }

    /// The handle a worker gives the operation running `release_id`'s entry,
    /// for a test that runs the operation without a worker.
    pub fn active_entry<P: QueuedPayload>(release_id: &str) -> crate::ActiveEntry<P> {
        crate::queued_releases::ActiveEntry::new(release_id)
    }

    /// Finish `release_id`'s active entry, as the worker does on success.
    pub fn finish_on<P: QueuedPayload>(
        sql: &CovenWrite<'_>,
        release_id: &str,
    ) -> Result<bool, DbError> {
        finish_queued_release_on::<P>(sql, release_id)
    }

    /// Fail `release_id`'s entry with `error`, as the worker does on failure.
    pub fn fail_on<P: QueuedPayload>(
        sql: &CovenWrite<'_>,
        release_id: &str,
        error: &str,
    ) -> Result<bool, DbError> {
        fail_queued_release_on::<P>(sql, release_id, error)
    }
}

/// The download queue at the sizes the lists' window costs are measured at.
#[cfg(any(test, feature = "test-utils"))]
pub mod bulk {
    use super::*;

    /// Queue `release_ids` to pin, the first at place `start` and each next
    /// one a place later. No one queues a hundred thousand releases within a
    /// run, so they go in by one set-based statement.
    pub fn queue_downloads_on(
        sql: &CovenWrite<'_>,
        start: u64,
        release_ids: &[String],
    ) -> Result<(), DbError> {
        let releases = serde_json::Value::from(release_ids.to_vec()).to_string();
        sql.run(&writes::QueueDownloadsAtPlacesParams {
            start: bae_store::query::to_i64(start, "download_queue.position")?,
            releases: &releases,
        })?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "tables_tests.rs"]
mod tests;
