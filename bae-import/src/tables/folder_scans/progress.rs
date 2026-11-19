//! Where each watched folder's scan stands, as one live query
//! (`progress.sql`): each folder's status from its stored scan and what the
//! folder scan coordinator holds of it, whether any folder has a problem,
//! whether bae is scanning any folder, and the problems not among the alerts
//! the app says it raised. Also the coordinator's writes of what it holds.
//!
//! The found count moves with every folder a scan walks, so it is not read
//! here: the scanning folders have a read of their own,
//! [`ImportTables::subscribe_scanning_folders`], and when a scan stores a
//! folder, coven runs only that read again, never a read of the queue.

use super::*;
use crate::folder_scan_status::{
    FolderScanAlert, FolderScanProblem, FolderScanProgress, FolderScanStatus, HeldRootScan,
    HeldRootScans, RaisedScanAlerts, ScanningFolder, StoredRootScan, StoredScan, StoredScanOutcome,
    WatchedFolderScanStatus,
};

/// Each watched folder's status, in the order the folders are listed (their
/// `position`, by its index).
const FOLDER_SCAN_STATUSES: &str = "SELECT watched_folder_path, status, detail, \
            unwatched_directory, unwatched_cause, volume \
     FROM folder_scan_status ORDER BY position";

/// Whether any folder's scan failed or left it not watched, and whether any
/// folder is being scanned.
const FOLDER_SCAN_FLAGS: &str = "SELECT \
     EXISTS (SELECT 1 FROM folder_scan_status WHERE status IN ('failed', 'unwatched')), \
     EXISTS (SELECT 1 FROM folder_scan_status WHERE status = 'scanning')";

/// Each folder's problem that is not among the raised alerts `?1` names, as a
/// JSON array of each one's folder path in hex, kind and detail, in the order
/// the folders are listed.
const OWED_FOLDER_SCAN_ALERTS: &str = "SELECT folder.watched_folder_path, folder.status, \
            folder.detail, folder.unwatched_directory, folder.unwatched_cause, folder.volume \
     FROM folder_scan_status AS folder \
     WHERE folder.status IN ('failed', 'unwatched') \
       AND (folder.watched_folder_path, folder.status, folder.detail) NOT IN \
           (SELECT unhex(value ->> 0), value ->> 1, value ->> 2 FROM json_each(?1)) \
     ORDER BY folder.position";

/// `raised` as the JSON array [`OWED_FOLDER_SCAN_ALERTS`] binds.
fn raised_alerts_json(raised: &RaisedScanAlerts) -> String {
    serde_json::to_string(
        &raised
            .iter()
            .map(|alert| {
                let (kind, detail) = match &alert.problem {
                    FolderScanProblem::Failed { error } => ("failed", error.as_str()),
                    FolderScanProblem::NotWatched { reason } => {
                        ("unwatched", reason.message.as_str())
                    }
                };
                (hex::encode(coven::path_bytes(&alert.folder.path)), kind, detail)
            })
            .collect::<Vec<_>>(),
    )
    .expect("raised alerts serialize as JSON")
}

/// A folder's status from the view's `status`, `detail` and `volume`.
fn folder_scan_status(
    watched_folder_path: &std::path::Path,
    status: String,
    detail: Option<String>,
    unwatched: Option<(std::path::PathBuf, String)>,
    volume: Option<String>,
) -> Result<FolderScanStatus, DbError> {
    let on_network_volume = volume
        .as_deref()
        .map(|volume| {
            Ok::<_, DbError>(super::columns::volume_of(volume)? == crate::VolumeKind::Network)
        })
        .transpose()?;
    Ok(match (status.as_str(), detail, unwatched, on_network_volume) {
        ("not_scanned", None, None, None) => FolderScanStatus::NotScanned,
        ("scanning", None, None, Some(on_network_volume)) => {
            FolderScanStatus::Scanning { on_network_volume }
        }
        ("complete", None, None, Some(on_network_volume)) => {
            FolderScanStatus::Complete { on_network_volume }
        }
        ("failed", Some(error), None, Some(on_network_volume)) => {
            FolderScanStatus::Failed { error, on_network_volume }
        }
        ("unwatched", Some(message), Some((directory, cause)), Some(on_network_volume)) => {
            FolderScanStatus::NotWatched {
                reason: super::columns::unwatched_reason_of(message, directory, &cause)?,
                on_network_volume,
            }
        }
        stored => {
            return Err(DbError::unreadable(
                format!("folder_scan_status of {}", coven::path_text(watched_folder_path)),
                stored,
            ))
        }
    })
}

/// Each watched folder's status, in the order the folders are listed.
fn load_folder_scan_statuses_on(
    sql: &SqlReadContext<'_>,
) -> Result<Vec<WatchedFolderScanStatus>, DbError> {
    sql.query(FOLDER_SCAN_STATUSES, [], read_folder_scan_status)?.into_iter().collect()
}

/// A watched folder's status from a row of `folder_scan_status`:
/// `watched_folder_path`, `status`, `detail`, `unwatched_directory`,
/// `unwatched_cause` and `volume`.
fn read_folder_scan_status(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<WatchedFolderScanStatus, DbError>> {
    let watched_folder_path = path_column::get(row, 0)?;
    let status: String = row.get(1)?;
    let detail: Option<String> = row.get(2)?;
    let directory = path_column::get_optional(row, 3)?;
    let cause: Option<String> = row.get(4)?;
    let volume: Option<String> = row.get(5)?;
    Ok(folder_scan_status(&watched_folder_path, status, detail, directory.zip(cause), volume).map(
        |status| WatchedFolderScanStatus {
            status,
            folder: crate::WatchedFolder::from_path(watched_folder_path),
        },
    ))
}

fn load_folder_scan_progress_on(
    sql: &SqlReadContext<'_>,
    raised: &RaisedScanAlerts,
) -> Result<FolderScanProgress, DbError> {
    let statuses = load_folder_scan_statuses_on(sql)?;
    let (has_problem, scanning) =
        sql.query_row(FOLDER_SCAN_FLAGS, [], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut alerts = Vec::new();
    for owed in sql.query(OWED_FOLDER_SCAN_ALERTS, [raised_alerts_json(raised)], |row| {
        read_folder_scan_status(row)
    })? {
        let WatchedFolderScanStatus { folder, status } = owed?;
        let problem = match status {
            FolderScanStatus::Failed { error, .. } => FolderScanProblem::Failed { error },
            FolderScanStatus::NotWatched { reason, .. } => FolderScanProblem::NotWatched { reason },
            other => {
                return Err(DbError::unreadable(
                    "folder_scan_status of an owed alert",
                    format!("{other:?}"),
                ))
            }
        };
        alerts.push(FolderScanAlert { folder, problem });
    }
    Ok(FolderScanProgress { statuses, has_problem, scanning, alerts })
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
            roots.detail, roots.unwatched_directory, roots.unwatched_cause, roots.volume \
     FROM watched_import_folders AS watched \
     LEFT JOIN folder_scan_roots AS roots ON roots.watched_folder_path = watched.path \
     ORDER BY watched.position";

fn load_stored_folder_scans_on(sql: &SqlReadContext<'_>) -> Result<Vec<StoredRootScan>, DbError> {
    let mut scans = Vec::new();
    for (watched_folder_path, status, detail, directory, cause, volume) in
        sql.query(FOLDER_SCAN_PROGRESS, [], |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                path_column::get_optional(row, 3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
    {
        let last_scan = match volume {
            None => None,
            Some(volume) => Some(StoredScan {
                outcome: stored_outcome(
                    &watched_folder_path,
                    status,
                    detail,
                    directory.zip(cause),
                )?,
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
pub(super) fn stored_outcome(
    watched_folder_path: &std::path::Path,
    status: Option<String>,
    detail: Option<String>,
    unwatched: Option<(std::path::PathBuf, String)>,
) -> Result<StoredScanOutcome, DbError> {
    Ok(match (status.as_deref(), detail, unwatched) {
        (None, None, None) => StoredScanOutcome::Unfinished,
        (Some("complete"), None, None) => StoredScanOutcome::Complete,
        (Some("failed"), Some(error), None) => StoredScanOutcome::Failed { error },
        (Some("unwatched"), Some(message), Some((directory, cause))) => {
            StoredScanOutcome::NotWatched {
                reason: super::columns::unwatched_reason_of(message, directory, &cause)?,
            }
        }
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

/// The folders bae is scanning, in the order the folders are listed, each
/// with how many folders its scan's current generation has stored, which
/// the query counts through the index on `scan_candidate`.
const SCANNING_FOLDERS: &str = "SELECT folder.watched_folder_path, \
            (SELECT COUNT(*) FROM scan_candidate AS candidate \
             WHERE candidate.watched_folder_path = roots.watched_folder_path \
               AND candidate.generation = roots.generation \
               AND candidate.source_kind = 'folder') \
     FROM folder_scan_status AS folder \
     JOIN folder_scan_roots AS roots ON roots.watched_folder_path = folder.watched_folder_path \
     WHERE folder.status = 'scanning' ORDER BY folder.position";

fn load_scanning_folders_on(sql: &SqlReadContext<'_>) -> Result<Vec<ScanningFolder>, DbError> {
    let mut folders = Vec::new();
    for (path, found) in sql
        .query(SCANNING_FOLDERS, [], |row| Ok((path_column::get(row, 0)?, row.get::<_, i64>(1)?)))?
    {
        folders.push(ScanningFolder {
            folder: crate::WatchedFolder::from_path(path),
            found: to_u64(found, "a folder scan's found count")?,
        });
    }
    Ok(folders)
}

impl ImportTables {
    /// The folders bae is scanning now, each with how many folders its scan
    /// has stored.
    #[cfg(test)]
    pub(crate) async fn load_scanning_folders(&self) -> Result<Vec<ScanningFolder>, DbError> {
        self.store.read(move |sql| load_scanning_folders_on(&sql)).await
    }

    /// The folders bae is scanning, each with how many folders its scan has
    /// stored, live: a value for every write that changes one.
    pub fn subscribe_scanning_folders(
        &self,
    ) -> coven::LiveQuery<Vec<ScanningFolder>, bae_store::DbError> {
        self.store.subscribe(move |sql| load_scanning_folders_on(&sql))
    }

    /// What each watched folder's scan has stored now.
    pub async fn load_stored_folder_scans(&self) -> Result<Vec<StoredRootScan>, DbError> {
        self.store.read(move |sql| load_stored_folder_scans_on(&sql)).await
    }

    /// Each watched folder's scan status now.
    pub async fn load_folder_scan_statuses(&self) -> Result<Vec<WatchedFolderScanStatus>, DbError> {
        self.store.read(move |sql| load_folder_scan_statuses_on(&sql)).await
    }

    /// Where each watched folder's scan stands now, with an alert for each
    /// problem not among `raised`.
    #[cfg(test)]
    pub(crate) async fn load_folder_scan_progress(
        &self,
        raised: RaisedScanAlerts,
    ) -> Result<FolderScanProgress, DbError> {
        self.store.read(move |sql| load_folder_scan_progress_on(&sql, &raised)).await
    }

    /// Where each watched folder's scan stands, live, with an alert for each
    /// problem not among the raised alerts the request names, which start as
    /// none. A new request changes them on the same query.
    pub fn subscribe_folder_scan_progress(
        &self,
    ) -> coven::ReconfigurableLiveQuery<RaisedScanAlerts, FolderScanProgress, DbError> {
        self.store.subscribe_reconfigurable(RaisedScanAlerts::new(), |raised, sql| {
            load_folder_scan_progress_on(&sql, raised)
        })
    }

    /// Hold `root`'s scan as running, as the folder scan coordinator holds a
    /// root that a pass reads, for a test that writes a scan straight to the
    /// tables with no coordinator.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn hold_root_running_for_test(&self, root: &std::path::Path) -> Result<(), DbError> {
        self.hold_root_scans(HeldRootScans::from([(root.to_path_buf(), HeldRootScan::Running)]))
            .await
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
        bae_util::test_spawn::on_blocking_pool(move || {
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

    /// Remove the state the folder scan coordinator kept for every root's
    /// scan. The import service calls this as the library opens, because
    /// those rows describe only the process that wrote them.
    pub(crate) async fn clear_held_folder_scans(&self) -> Result<(), DbError> {
        self.store
            .call(|sql| {
                sql.execute("DELETE FROM folder_scan_held", [])?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
