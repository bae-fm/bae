//! What each watched folder's scan stored, as its own live query.
//!
//! The found count moves with every folder a scan walks, so this read is kept
//! to the roots and a count of their current generation's folders — nothing a
//! scan's write wakes here costs a read of the queue. Whether a root is being
//! read is the folder scan coordinator's to say, not the table's
//! ([`crate::folder_scan_status`]).

use super::*;
use crate::folder_scan_status::{StoredRootScan, StoredScanOutcome};

/// Each watched folder's scan, in the order the folders are listed (their
/// `position`, by its index), with the count of its current generation's
/// folders. A scan root is always a watched folder: deleting the folder
/// deletes its root.
const FOLDER_SCAN_PROGRESS: &str = "SELECT roots.watched_folder_path, roots.status, \
            roots.error, roots.volume, \
            (SELECT COUNT(*) FROM scan_candidate AS candidate \
             WHERE candidate.watched_folder_path = roots.watched_folder_path \
               AND candidate.generation = roots.generation \
               AND candidate.source_kind = 'folder') \
     FROM watched_import_folders AS watched \
     JOIN folder_scan_roots AS roots ON roots.watched_folder_path = watched.path \
     ORDER BY watched.position";

fn load_stored_folder_scans_on(sql: &SqlReadContext<'_>) -> Result<Vec<StoredRootScan>, DbError> {
    let mut scans = Vec::new();
    for (watched_folder_path, status, error, volume, found_count) in
        sql.query(FOLDER_SCAN_PROGRESS, [], |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
    {
        let volume = super::columns::volume_of(&volume)?;
        let outcome = match (status.as_deref(), error) {
            (None, None) => StoredScanOutcome::Unfinished {
                found_count: to_u64(found_count, "current folder-scan candidate count")?,
            },
            (Some("complete"), None) => StoredScanOutcome::Complete,
            (Some("failed"), Some(error)) => StoredScanOutcome::Failed { error },
            (status, error) => {
                return Err(DbError::Message(format!(
                    "folder scan root {} has invalid status {status:?} and error {error:?}",
                    coven::path_text(&watched_folder_path)
                )))
            }
        };
        scans.push(StoredRootScan {
            watched_folder_path,
            outcome,
            on_network_volume: volume == crate::VolumeKind::Network,
        });
    }
    Ok(scans)
}

impl ImportTables {
    /// What each watched folder's scan has stored now.
    pub async fn load_stored_folder_scans(&self) -> Result<Vec<StoredRootScan>, DbError> {
        self.store.read(move |sql| load_stored_folder_scans_on(&sql)).await
    }

    /// What each watched folder's scan has stored, live.
    pub fn subscribe_stored_folder_scans(&self) -> coven::LiveQuery<Vec<StoredRootScan>> {
        self.store.subscribe(move |sql| load_stored_folder_scans_on(&sql).map_err(CovenError::from))
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
