//! Where each watched folder's scan stands, as its own live query.
//!
//! The found count moves with every folder a scan walks, so this read is kept
//! to the roots and a count of their current generation's folders — nothing a
//! scan's write wakes here costs a read of the queue.

use super::*;
use crate::folder_scan_status::FolderScanProgress;
use crate::watched_folder::WatchedFolder;
use crate::{FolderScanStatus, WatchedFolderScanStatus};

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

fn load_folder_scan_progress_on(
    sql: &SqlReadContext<'_>,
) -> Result<Vec<WatchedFolderScanStatus>, DbError> {
    let mut statuses = Vec::new();
    for (watched_folder_path, status, error, volume, found_count) in
        sql.query(FOLDER_SCAN_PROGRESS, [], |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
    {
        let volume = super::columns::volume_of(&volume)?;
        let status = match (status.as_str(), error) {
            ("scanning", None) => FolderScanStatus::Scanning {
                found_count: to_u64(found_count, "current folder-scan candidate count")?,
            },
            ("complete", None) => FolderScanStatus::Complete,
            ("failed", Some(error)) => FolderScanStatus::Failed { error },
            (status, error) => {
                return Err(DbError::Message(format!(
                    "folder scan root {} has invalid status {status:?} and error {error:?}",
                    coven::path_text(&watched_folder_path)
                )))
            }
        };
        statuses.push(WatchedFolderScanStatus {
            on_network_volume: volume == crate::VolumeKind::Network,
            watched_folder_name: WatchedFolder::from_path(watched_folder_path.clone()).name,
            watched_folder_path,
            status,
        });
    }
    Ok(statuses)
}

impl ImportTables {
    /// Where each watched folder's scan stands now.
    pub async fn load_folder_scan_progress(&self) -> Result<FolderScanProgress, DbError> {
        self.database
            .read(move |sql| load_folder_scan_progress_on(&sql))
            .process(|statuses| Ok(FolderScanProgress::of(statuses)))
            .await
    }

    /// Where each watched folder's scan stands, live.
    pub(crate) fn subscribe_folder_scan_progress(&self) -> coven::LiveQuery<FolderScanProgress> {
        self.database
            .subscribe(move |sql| load_folder_scan_progress_on(&sql).map_err(CovenError::from))
            .process(|statuses| Ok(FolderScanProgress::of(statuses)))
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
