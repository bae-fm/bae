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

// Each watched folder's status, with the `position` the folders are listed in.
coven::fragment!(
    WatchedScanStatus {
        watched_folder_path: Vec<u8>,
        status: String,
        detail: Option<String>,
        unwatched_directory: Option<Vec<u8>>,
        unwatched_cause: Option<String>,
        volume: Option<String>
    },
    r#"
    SELECT folder.position, folder.watched_folder_path, folder.status, folder.detail,
           folder.unwatched_directory, folder.unwatched_cause, folder.volume
    FROM folder_scan_status AS folder
"#
);

// Each watched folder's status, in the order the folders are listed (their
// `position`, by its index).
coven::read!(
    writer FolderScanStatuses,
    r#"
    SELECT shown.*
    FROM @crate::tables::folder_scans::progress::WatchedScanStatus AS shown
    ORDER BY shown.position
"#
);

// Whether any folder's scan failed or left it not watched, and whether any
// folder is being scanned, beside each problem owed an alert, in the order the
// folders are listed: a failed or not-watched folder that is not among the
// raised alerts `raised` names, a JSON array of each one's folder path in hex,
// kind and detail. One row with no alert when none is owed.
coven::read!(
    writer FolderScanAlerts(raised: String) { has_problem: bool, scanning: bool },
    r#"
    SELECT EXISTS (SELECT 1 FROM folder_scan_status AS folder
                   WHERE folder.status IN ('failed', 'unwatched')) AS has_problem,
           EXISTS (SELECT 1 FROM folder_scan_status AS folder
                   WHERE folder.status = 'scanning') AS scanning,
           owed.*
    FROM (SELECT 1) AS flags
    LEFT JOIN @crate::tables::folder_scans::progress::WatchedScanStatus AS owed
           ON owed.status IN ('failed', 'unwatched')
          AND NOT EXISTS (SELECT 1 FROM json_each(:raised) AS alert
                          WHERE unhex(alert.value ->> 0) = owed.watched_folder_path
                            AND alert.value ->> 1 = owed.status
                            AND alert.value ->> 2 = owed.detail)
    ORDER BY owed.position
"#
);

/// `raised` as the JSON array [`FolderScanAlerts`] binds.
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
    bae_store::statement_rows(sql, &FolderScanStatusesParams {})?
        .into_iter()
        .map(|row| watched_scan_status_of(row.shown))
        .collect()
}

/// A watched folder's status from its [`WatchedScanStatus`] row.
fn watched_scan_status_of(row: WatchedScanStatus) -> Result<WatchedFolderScanStatus, DbError> {
    let watched_folder_path = path_column::decode(row.watched_folder_path)?;
    let directory = row.unwatched_directory.map(path_column::decode).transpose()?;
    let status = folder_scan_status(
        &watched_folder_path,
        row.status,
        row.detail,
        directory.zip(row.unwatched_cause),
        row.volume,
    )?;
    let folder = crate::WatchedFolder::from_path(watched_folder_path);
    Ok(WatchedFolderScanStatus { status, folder })
}

/// The reads of where each watched folder's scan stands, with an alert for
/// each problem not among `raised`.
fn folder_scan_progress_read(
    raised: &RaisedScanAlerts,
) -> coven::Both<FolderScanStatusesParams, FolderScanAlertsParams> {
    coven::Both(
        FolderScanStatusesParams {},
        FolderScanAlertsParams { raised: raised_alerts_json(raised) },
    )
}

/// Where each watched folder's scan stands, as its reads return it.
fn folder_scan_progress_of(
    (statuses, alerts): (Vec<FolderScanStatusesRow>, Vec<FolderScanAlertsRow>),
) -> Result<FolderScanProgress, DbError> {
    let statuses = statuses
        .into_iter()
        .map(|row| watched_scan_status_of(row.shown))
        .collect::<Result<_, _>>()?;
    // Every row holds the same flags.
    let (has_problem, scanning) = alerts
        .first()
        .map(|row| (row.has_problem, row.scanning))
        .ok_or_else(|| DbError::unreadable("the folder scans' flags", None::<String>))?;
    let alerts = alerts
        .into_iter()
        .filter_map(|row| row.owed)
        .map(|owed| {
            let status = watched_scan_status_of(owed)?;
            let problem = match status.status {
                FolderScanStatus::Failed { error, .. } => FolderScanProblem::Failed { error },
                FolderScanStatus::NotWatched { reason, .. } => {
                    FolderScanProblem::NotWatched { reason }
                }
                other => {
                    return Err(DbError::unreadable(
                        "folder_scan_status of an owed alert",
                        format!("{other:?}"),
                    ))
                }
            };
            Ok(FolderScanAlert { folder: status.folder, problem })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(FolderScanProgress { statuses, has_problem, scanning, alerts })
}

coven::write!(
    HoldRootScan(
        watched_folder_path: Vec<u8>,
        state: &'static str,
        error: Option<&'p str>,
        volume: Option<&'static str>,
    ),
    r#"
    INSERT INTO folder_scan_held (watched_folder_path, state, error, volume)
    VALUES (:watched_folder_path, :state, :error, :volume)
    ON CONFLICT (watched_folder_path) DO UPDATE SET
        state = excluded.state, error = excluded.error, volume = excluded.volume
    WHERE state IS NOT excluded.state OR error IS NOT excluded.error
        OR volume IS NOT excluded.volume
"#
);

// Every root's held state but those of the roots `kept` names, a JSON array
// of their paths' bytes in hex.
coven::write!(
    DropUnheldRootScans(kept: String),
    r#"
    DELETE FROM folder_scan_held
    WHERE watched_folder_path NOT IN (
        SELECT unhex(wanted.value) FROM json_each(:kept) AS wanted)
"#
);

coven::write!(ClearHeldRootScans, r#"DELETE FROM folder_scan_held"#);

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
    sql.run(&HoldRootScanParams {
        watched_folder_path: coven::path_bytes(watched_folder_path),
        state,
        error,
        volume,
    })?;
    Ok(())
}

// What the folder scan coordinator holds of each root's scan.
#[cfg(test)]
coven::read!(
    writer HeldRootScanRows {
        watched_folder_path: Vec<u8>,
        state: String,
        error: Option<String>,
        volume: Option<String>
    },
    r#"
    SELECT held.watched_folder_path, held.state, held.error, held.volume
    FROM folder_scan_held AS held
"#
);

/// What the folder scan coordinator holds of each root's scan, as its rows
/// hold it.
#[cfg(test)]
fn held_root_scans_of(rows: Vec<HeldRootScanRowsRow>) -> Result<HeldRootScans, DbError> {
    let rows = rows
        .into_iter()
        .map(|row| {
            Ok((path_column::decode(row.watched_folder_path)?, row.state, row.error, row.volume))
        })
        .collect::<Result<Vec<_>, DbError>>()?;
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

// Every watched folder, in the order the folders are listed (their
// `position`, by its index), with its scan root when a scan has opened one
// A scan root is always a watched folder: deleting the folder deletes its
// root. `volume` is never NULL on a root, so a NULL one is a folder with
// none.
coven::read!(
    writer StoredFolderScans,
    r#"
    SELECT watched.*, roots.status, roots.detail, roots.unwatched_directory,
        roots.unwatched_cause, roots.volume
    FROM @crate::tables::watched_folders::fragments::WatchedFolderRows AS watched
    LEFT JOIN folder_scan_roots AS roots ON roots.watched_folder_path = watched.watched_path
    ORDER BY watched.position
"#
);

fn load_stored_folder_scans_on(sql: &SqlReadContext<'_>) -> Result<Vec<StoredRootScan>, DbError> {
    let mut scans = Vec::new();
    for row in bae_store::statement_rows(sql, &StoredFolderScansParams {})? {
        let watched_folder_path = path_column::decode(row.watched.watched_path)?;
        let last_scan = match row.volume {
            None => None,
            Some(volume) => Some(StoredScan {
                outcome: stored_outcome(
                    &watched_folder_path,
                    row.status,
                    row.detail,
                    row.unwatched_directory
                        .map(path_column::decode)
                        .transpose()?
                        .zip(row.unwatched_cause),
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

// The folders bae is scanning, in the order the folders are listed, each
// with how many folders its scan's current generation has stored, which the
// query counts through the index on `scan_candidate`.
coven::read!(
    writer ScanningFolders { watched_folder_path: Vec<u8>, found: i64 },
    r#"
    SELECT folder.watched_folder_path,
           (SELECT COUNT(*) FROM scan_candidate AS candidate
            WHERE candidate.watched_folder_path = roots.watched_folder_path
              AND candidate.generation = roots.generation
              AND candidate.source_kind = 'folder') AS found
    FROM folder_scan_status AS folder
    JOIN folder_scan_roots AS roots ON roots.watched_folder_path = folder.watched_folder_path
    WHERE folder.status = 'scanning'
    ORDER BY folder.position
"#
);

/// The folders bae is scanning, as their rows hold them.
fn scanning_folders_of(rows: Vec<ScanningFoldersRow>) -> Result<Vec<ScanningFolder>, DbError> {
    rows.into_iter()
        .map(|row| {
            Ok(ScanningFolder {
                folder: crate::WatchedFolder::from_path(path_column::decode(
                    row.watched_folder_path,
                )?),
                found: to_u64(row.found, "a folder scan's found count")?,
            })
        })
        .collect()
}

impl ImportTables {
    /// The folders bae is scanning now, each with how many folders its scan
    /// has stored.
    #[cfg(test)]
    pub(crate) async fn load_scanning_folders(&self) -> Result<Vec<ScanningFolder>, DbError> {
        scanning_folders_of(self.store.read_rows(ScanningFoldersParams {}).await?)
    }

    /// The folders bae is scanning, each with how many folders its scan has
    /// stored, live: a value for every write that changes one.
    pub fn subscribe_scanning_folders(
        &self,
    ) -> coven::LiveQuery<Vec<ScanningFolder>, bae_store::DbError> {
        self.store.subscribe(ScanningFoldersParams {}).process(scanning_folders_of)
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
        let reads = folder_scan_progress_read(&raised);
        self.store.read(move |sql| folder_scan_progress_of(sql.reads(&reads)?)).await
    }

    /// Where each watched folder's scan stands, live, with an alert for each
    /// problem not among the raised alerts the request names, which start as
    /// none. A new request changes them on the same query.
    pub fn subscribe_folder_scan_progress(
        &self,
    ) -> coven::ReconfigurableLiveQuery<RaisedScanAlerts, FolderScanProgress, DbError> {
        self.store
            .subscribe_reconfigurable(RaisedScanAlerts::new(), |raised| {
                Ok(folder_scan_progress_read(raised))
            })
            .process(|_, read| folder_scan_progress_of(read))
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
        held_root_scans_of(self.store.read_rows(HeldRootScanRowsParams {}).await?)
    }

    /// What the folder scan coordinator holds of each root's scan, live.
    #[cfg(test)]
    pub(crate) fn subscribe_held_root_scans(
        &self,
    ) -> coven::LiveQuery<HeldRootScans, bae_store::DbError> {
        self.store.subscribe(HeldRootScanRowsParams {}).process(held_root_scans_of)
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
                let kept = path_column::hex_list(held.keys().map(|path| path.as_path()));
                sql.run(&DropUnheldRootScansParams { kept })?;
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
                sql.run(&ClearHeldRootScansParams {})?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
