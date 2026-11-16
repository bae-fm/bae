//! Where each watched folder's scan stands, as one live query
//! (`progress.sql`): each folder's status from its stored scan and what the
//! folder scan coordinator holds of it, whether any folder has a problem,
//! the folders being scanned, and the problems the app has raised no alert
//! for. Also the coordinator's writes of what it holds, and the app's record
//! of the alerts it raised.
//!
//! The found count moves with every folder a scan walks, so it is not read
//! here: it has a read of its own, [`ImportTables::subscribe_scan_found_counts`],
//! and nothing a scan's write wakes here costs a read of the queue or of the
//! candidates.

use super::*;
use crate::folder_scan_status::{
    FolderScanActivity, FolderScanAlert, FolderScanAlertKind, FolderScanProgress, FolderScanStatus,
    HeldRootScan, HeldRootScans, ScanFoundCounts, StoredRootScan, StoredScan, StoredScanOutcome,
    WatchedFolderScanStatus,
};

/// Each watched folder's status, in the order the folders are listed (their
/// `position`, by its index).
const FOLDER_SCAN_STATUSES: &str = "SELECT watched_folder_path, status, detail, on_network_volume \
     FROM folder_scan_status ORDER BY position";

/// Whether any folder's scan failed or left it not watched, and whether any
/// folder is being scanned.
const FOLDER_SCAN_FLAGS: &str = "SELECT \
     EXISTS (SELECT 1 FROM folder_scan_status WHERE status IN ('failed', 'unwatched')), \
     EXISTS (SELECT 1 FROM folder_scan_status WHERE status = 'scanning')";

/// The folders being scanned, in the order the folders are listed.
const SCANNING_FOLDERS: &str = "SELECT watched_folder_path FROM folder_scan_status \
     WHERE status = 'scanning' ORDER BY position";

/// Each folder's problem with no alert shown for it, in the order the folders
/// are listed.
const OWED_FOLDER_SCAN_ALERTS: &str = "SELECT folder.watched_folder_path, folder.status, \
            folder.detail \
     FROM folder_scan_status AS folder \
     WHERE folder.status IN ('failed', 'unwatched') \
       AND NOT EXISTS (SELECT 1 FROM folder_scan_alert_shown AS shown \
           WHERE shown.watched_folder_path = folder.watched_folder_path \
             AND shown.kind = folder.status AND shown.detail = folder.detail) \
     ORDER BY folder.position";

/// The name a watched folder is shown by.
fn folder_name(watched_folder_path: &std::path::Path) -> String {
    crate::WatchedFolder::from_path(watched_folder_path.to_path_buf()).name
}

/// A folder's status from the view's `status` and `detail`.
fn folder_scan_status(
    watched_folder_path: &std::path::Path,
    status: String,
    detail: Option<String>,
) -> Result<FolderScanStatus, DbError> {
    Ok(match (status.as_str(), detail) {
        ("not_scanned", None) => FolderScanStatus::NotScanned,
        ("scanning", None) => FolderScanStatus::Scanning,
        ("complete", None) => FolderScanStatus::Complete,
        ("failed", Some(error)) => FolderScanStatus::Failed { error },
        ("unwatched", Some(reason)) => FolderScanStatus::NotWatched { reason },
        stored => {
            return Err(DbError::unreadable(
                format!("folder_scan_status of {}", coven::path_text(watched_folder_path)),
                stored,
            ))
        }
    })
}

fn alert_kind(kind: &str) -> Result<FolderScanAlertKind, DbError> {
    match kind {
        "failed" => Ok(FolderScanAlertKind::Failed),
        "unwatched" => Ok(FolderScanAlertKind::NotWatched),
        other => Err(DbError::unreadable("folder_scan_status.status of an alert", other)),
    }
}

fn alert_kind_text(kind: FolderScanAlertKind) -> &'static str {
    match kind {
        FolderScanAlertKind::Failed => "failed",
        FolderScanAlertKind::NotWatched => "unwatched",
    }
}

fn load_folder_scan_progress_on(sql: &SqlReadContext<'_>) -> Result<FolderScanProgress, DbError> {
    let mut statuses = Vec::new();
    for (watched_folder_path, status, detail, on_network_volume) in
        sql.query(FOLDER_SCAN_STATUSES, [], |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?
    {
        statuses.push(WatchedFolderScanStatus {
            status: folder_scan_status(&watched_folder_path, status, detail)?,
            folder: crate::WatchedFolder::from_path(watched_folder_path),
            on_network_volume,
        });
    }
    let (has_problem, scanning) =
        sql.query_row(FOLDER_SCAN_FLAGS, [], |row| Ok((row.get(0)?, row.get::<_, bool>(1)?)))?;
    let activity = if scanning {
        let folders = sql
            .query(SCANNING_FOLDERS, [], |row| path_column::get(row, 0))?
            .into_iter()
            .map(crate::WatchedFolder::from_path)
            .collect();
        Some(FolderScanActivity { folders })
    } else {
        None
    };
    let mut alerts = Vec::new();
    for (watched_folder_path, kind, detail) in sql.query(OWED_FOLDER_SCAN_ALERTS, [], |row| {
        Ok((path_column::get(row, 0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
    })? {
        alerts.push(FolderScanAlert {
            watched_folder_name: folder_name(&watched_folder_path),
            watched_folder_path,
            kind: alert_kind(&kind)?,
            detail,
        });
    }
    Ok(FolderScanProgress { statuses, has_problem, activity, alerts })
}

/// Store `held` as the one row of `watched_folder_path`, leaving a row that
/// already says it unwritten.
fn hold_root_scan_on(
    sql: &CovenWrite<'_>,
    watched_folder_path: &std::path::Path,
    held: &HeldRootScan,
) -> Result<(), DbError> {
    let (state, error, volume) = match held {
        HeldRootScan::Running => ("running", None, None),
        HeldRootScan::UnstoredFailure(failure) => (
            "failed",
            Some(failure.error.as_str()),
            Some(super::columns::volume_text(failure.volume)),
        ),
    };
    sql.execute(
        "INSERT INTO folder_scan_held (watched_folder_path, state, error, volume) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (watched_folder_path) DO UPDATE SET \
             state = excluded.state, error = excluded.error, volume = excluded.volume \
         WHERE state IS NOT excluded.state OR error IS NOT excluded.error \
             OR volume IS NOT excluded.volume",
        params![coven::path_bytes(watched_folder_path), state, error, volume],
    )?;
    Ok(())
}

/// What the folder scan coordinator holds of each root's scan, as its rows
/// store it.
#[cfg(test)]
fn load_held_root_scans_on(sql: &SqlReadContext<'_>) -> Result<HeldRootScans, DbError> {
    let rows = sql.query(
        "SELECT watched_folder_path, state, error, volume FROM folder_scan_held",
        [],
        |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    let mut held = HeldRootScans::new();
    for (path, state, error, volume) in rows {
        let root = match (state.as_str(), error, volume) {
            ("running", None, None) => HeldRootScan::Running,
            ("failed", Some(error), Some(volume)) => {
                HeldRootScan::UnstoredFailure(crate::folder_scan_status::UnstoredScanFailure {
                    error,
                    volume: super::columns::volume_of(&volume)?,
                })
            }
            stored => return Err(DbError::unreadable("folder_scan_held", stored)),
        };
        held.insert(path, root);
    }
    Ok(held)
}

/// Every watched folder, in the order the folders are listed (their
/// `position`, by its index), with its scan root when a scan has opened one
/// A scan root is always a watched folder: deleting the folder deletes its
/// root. `volume` is never NULL on a root, so a NULL one is a folder with
/// none.
const FOLDER_SCAN_PROGRESS: &str = "SELECT watched.path, roots.status, \
            roots.detail, roots.volume \
     FROM watched_import_folders AS watched \
     LEFT JOIN folder_scan_roots AS roots ON roots.watched_folder_path = watched.path \
     ORDER BY watched.position";

fn load_stored_folder_scans_on(sql: &SqlReadContext<'_>) -> Result<Vec<StoredRootScan>, DbError> {
    let mut scans = Vec::new();
    for (watched_folder_path, status, detail, volume) in
        sql.query(FOLDER_SCAN_PROGRESS, [], |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
    {
        let last_scan = match volume {
            None => None,
            Some(volume) => Some(StoredScan {
                outcome: stored_outcome(&watched_folder_path, status, detail)?,
                on_network_volume: super::columns::volume_of(&volume)?
                    == crate::VolumeKind::Network,
            }),
        };
        scans.push(StoredRootScan { watched_folder_path, last_scan });
    }
    Ok(scans)
}

/// How a root's current generation ended, from its row: `status` and
/// `detail`.
fn stored_outcome(
    watched_folder_path: &std::path::Path,
    status: Option<String>,
    detail: Option<String>,
) -> Result<StoredScanOutcome, DbError> {
    Ok(match (status.as_deref(), detail) {
        (None, None) => StoredScanOutcome::Unfinished,
        (Some("complete"), None) => StoredScanOutcome::Complete,
        (Some("failed"), Some(error)) => StoredScanOutcome::Failed { error },
        (Some("unwatched"), Some(reason)) => StoredScanOutcome::NotWatched { reason },
        stored => {
            return Err(DbError::unreadable(
                format!(
                    "folder_scan_roots' status and detail of {}",
                    coven::path_text(watched_folder_path)
                ),
                stored,
            ))
        }
    })
}

/// How many folders each scan still under way has stored, by its root: the
/// roots whose current generation has stored no outcome.
const SCAN_FOUND_COUNTS: &str = "SELECT roots.watched_folder_path, \
            (SELECT COUNT(*) FROM scan_candidate AS candidate \
             WHERE candidate.watched_folder_path = roots.watched_folder_path \
               AND candidate.generation = roots.generation \
               AND candidate.source_kind = 'folder') \
     FROM folder_scan_roots AS roots WHERE roots.status IS NULL";

fn load_scan_found_counts_on(sql: &SqlReadContext<'_>) -> Result<ScanFoundCounts, DbError> {
    sql.query(SCAN_FOUND_COUNTS, [], |row| Ok((path_column::get(row, 0)?, row.get::<_, i64>(1)?)))?
        .into_iter()
        .map(|(path, found)| Ok((path, to_u64(found, "current folder-scan candidate count")?)))
        .collect()
}

impl ImportTables {
    /// How many folders each scan under way has stored now.
    pub async fn load_scan_found_counts(&self) -> Result<ScanFoundCounts, DbError> {
        self.store.read(move |sql| load_scan_found_counts_on(&sql)).await
    }

    /// How many folders each scan under way has stored, live: a value for
    /// every write that moves one.
    pub fn subscribe_scan_found_counts(
        &self,
    ) -> coven::LiveQuery<ScanFoundCounts, bae_store::DbError> {
        self.store.subscribe(move |sql| load_scan_found_counts_on(&sql))
    }

    /// What each watched folder's scan has stored now.
    pub async fn load_stored_folder_scans(&self) -> Result<Vec<StoredRootScan>, DbError> {
        self.store.read(move |sql| load_stored_folder_scans_on(&sql)).await
    }

    /// Where each watched folder's scan stands now.
    pub async fn load_folder_scan_progress(&self) -> Result<FolderScanProgress, DbError> {
        self.store.read(move |sql| load_folder_scan_progress_on(&sql)).await
    }

    /// Where each watched folder's scan stands, live.
    pub fn subscribe_folder_scan_progress(
        &self,
    ) -> coven::LiveQuery<FolderScanProgress, bae_store::DbError> {
        self.store.subscribe(move |sql| load_folder_scan_progress_on(&sql))
    }

    /// What the folder scan coordinator holds of each root's scan now.
    #[cfg(test)]
    pub(crate) async fn load_held_root_scans(&self) -> Result<HeldRootScans, DbError> {
        self.store.read(move |sql| load_held_root_scans_on(&sql)).await
    }

    /// What the folder scan coordinator holds of each root's scan, live.
    #[cfg(test)]
    pub(crate) fn subscribe_held_root_scans(
        &self,
    ) -> coven::LiveQuery<HeldRootScans, bae_store::DbError> {
        self.store.subscribe(move |sql| load_held_root_scans_on(&sql))
    }

    /// Plant damage for a test: every write to what the folder scan
    /// coordinator holds, in the library under `directory`, fails, as a full
    /// disk fails it.
    #[cfg(test)]
    pub(crate) async fn fail_held_root_scan_writes_for_test(directory: &std::path::Path) {
        let directory = directory.to_path_buf();
        bae_util::test_blocking::on_blocking_pool(move || {
            for (name, event) in [
                ("fail_held_inserts", "INSERT"),
                ("fail_held_updates", "UPDATE"),
                ("fail_held_deletes", "DELETE"),
            ] {
                bae_store::test_fixtures::planted_damage::plant_in_library_file(
                    &directory,
                    "folder_scan_held",
                    &format!(
                        "CREATE TRIGGER {name} BEFORE {event} ON folder_scan_held \
                         BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;"
                    ),
                );
            }
        })
        .await;
    }

    /// Store `held` as the whole state the folder scan coordinator keeps for
    /// the roots' scans, in one write: a root `held` does not name holds
    /// nothing, and the write leaves a row that already says what `held`
    /// does unwritten.
    pub(crate) async fn hold_root_scans(&self, held: HeldRootScans) -> Result<(), DbError> {
        self.store
            .call(move |sql| {
                let kept: Vec<Vec<u8>> = held.keys().map(|path| coven::path_bytes(path)).collect();
                let places = vec!["?"; kept.len()].join(", ");
                sql.execute(
                    &format!(
                        "DELETE FROM folder_scan_held WHERE watched_folder_path NOT IN ({places})"
                    ),
                    rusqlite::params_from_iter(&kept),
                )?;
                for (path, root) in &held {
                    hold_root_scan_on(sql, path, root)?;
                }
                Ok(())
            })
            .await
    }

    /// Record that the app raised the alert for `watched_folder_path`'s
    /// problem of `kind` with `detail`, so the folder's progress lists no
    /// alert for it. A folder no longer watched records nothing.
    pub async fn mark_folder_scan_alert_shown(
        &self,
        watched_folder_path: &std::path::Path,
        kind: FolderScanAlertKind,
        detail: String,
    ) -> Result<(), DbError> {
        let watched_folder_path = coven::path_bytes(watched_folder_path);
        let kind = alert_kind_text(kind);
        self.store
            .call(move |sql| {
                sql.execute(
                    "INSERT INTO folder_scan_alert_shown (watched_folder_path, kind, detail) \
                     SELECT path, ?, ? FROM watched_import_folders WHERE path = ? \
                     ON CONFLICT (watched_folder_path) DO UPDATE SET \
                         kind = excluded.kind, detail = excluded.detail",
                    params![kind, detail, watched_folder_path],
                )?;
                Ok(())
            })
            .await
    }

    /// Remove the state the folder scan coordinator kept for every root's
    /// scan and every alert the app raised. The import service calls this
    /// as the library opens, because those rows describe only the process
    /// that wrote them.
    pub(crate) async fn clear_held_folder_scans(&self) -> Result<(), DbError> {
        self.store
            .call(|sql| {
                sql.execute("DELETE FROM folder_scan_alert_shown", [])?;
                sql.execute("DELETE FROM folder_scan_held", [])?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
