//! What each watched folder's scan stored, as its own live query: every
//! watched folder, with its scan root once a scan has opened one.
//!
//! The found count moves with every folder a scan walks, so this read is kept
//! to the roots and a count of their current generation's folders — nothing a
//! scan's write wakes here costs a read of the queue. Whether a root is being
//! read is the folder scan coordinator's to say, not the table's
//! ([`crate::folder_scan_status`]).

use super::*;
use crate::folder_scan_status::{StoredRootScan, StoredScan, StoredScanOutcome};

/// Every watched folder, in the order the folders are listed (their
/// `position`, by its index), with its scan root when a scan has opened one
/// and the count of that generation's folders. A scan root is always a
/// watched folder: deleting the folder deletes its root. `volume` is never
/// NULL on a root, so a NULL one is a folder with none.
const FOLDER_SCAN_PROGRESS: &str = "SELECT watched.path, roots.status, \
            roots.error, roots.volume, \
            (SELECT COUNT(*) FROM scan_candidate AS candidate \
             WHERE candidate.watched_folder_path = roots.watched_folder_path \
               AND candidate.generation = roots.generation \
               AND candidate.source_kind = 'folder') \
     FROM watched_import_folders AS watched \
     LEFT JOIN folder_scan_roots AS roots ON roots.watched_folder_path = watched.path \
     ORDER BY watched.position";

fn load_stored_folder_scans_on(sql: &SqlReadContext<'_>) -> Result<Vec<StoredRootScan>, DbError> {
    let mut scans = Vec::new();
    for (watched_folder_path, status, error, volume, found_count) in
        sql.query(FOLDER_SCAN_PROGRESS, [], |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
    {
        let last_scan = match volume {
            None => None,
            Some(volume) => Some(StoredScan {
                outcome: stored_outcome(&watched_folder_path, status, error, found_count)?,
                on_network_volume: super::columns::volume_of(&volume)?
                    == crate::VolumeKind::Network,
            }),
        };
        scans.push(StoredRootScan { watched_folder_path, last_scan });
    }
    Ok(scans)
}

/// How a root's current generation ended, from its row: `status` and
/// `error`, and the count of folders it has stored.
fn stored_outcome(
    watched_folder_path: &std::path::Path,
    status: Option<String>,
    error: Option<String>,
    found_count: i64,
) -> Result<StoredScanOutcome, DbError> {
    Ok(match (status.as_deref(), error) {
        (None, None) => StoredScanOutcome::Unfinished {
            found_count: to_u64(found_count, "current folder-scan candidate count")?,
        },
        (Some("complete"), None) => StoredScanOutcome::Complete,
        (Some("failed"), Some(error)) => StoredScanOutcome::Failed { error },
        (status, error) => {
            return Err(DbError::Message(format!(
                "folder scan root {} has invalid status {status:?} and error {error:?}",
                coven::path_text(watched_folder_path)
            )))
        }
    })
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
