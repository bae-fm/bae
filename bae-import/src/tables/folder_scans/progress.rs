//! What each watched folder's scan stored, as its own live query: every
//! watched folder, with its scan root once a scan has opened one.
//!
//! The found count moves with every folder a scan walks, so it is not read
//! here: it has a read of its own, [`ImportTables::subscribe_scan_found_counts`],
//! and nothing a scan's write wakes here costs a read of the queue or of the
//! candidates. Whether a root is being read is the folder scan coordinator's
//! to say, not the table's ([`crate::folder_scan_status`]).

use super::*;
use crate::folder_scan_status::{ScanFoundCounts, StoredRootScan, StoredScan, StoredScanOutcome};

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

    /// What each watched folder's scan has stored, live.
    pub fn subscribe_stored_folder_scans(
        &self,
    ) -> coven::LiveQuery<Vec<StoredRootScan>, bae_store::DbError> {
        self.store.subscribe(move |sql| load_stored_folder_scans_on(&sql))
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
