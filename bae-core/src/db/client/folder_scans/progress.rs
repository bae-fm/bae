//! Where each watched folder's scan stands, as its own live query.
//!
//! The found count moves with every folder a scan walks, so this read is kept
//! to the roots and a count of their current generation's folders — nothing a
//! scan's write wakes here costs a read of the queue.

use super::columns::to_u64;
use super::*;
use crate::import::list::FolderScanProgress;
use crate::import::watched_folder::WatchedFolder;
use crate::import::{FolderScanStatus, WatchedFolderScanStatus};

fn load_folder_scan_progress_on(
    sql: &SqlReadContext<'_>,
) -> Result<Vec<WatchedFolderScanStatus>, DbError> {
    let mut statuses = Vec::new();
    for (watched_folder_path, status, error, volume, found_count) in
        sql.query(super::FOLDER_SCAN_PROGRESS, [], |row| {
            Ok((
                path_column::get(row, 0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
    {
        let volume = crate::import::VolumeKind::from_column(&volume).ok_or_else(|| {
            DbError::Message(format!(
                "folder scan root {} has invalid volume {volume:?}",
                coven::path_text(&watched_folder_path)
            ))
        })?;
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
            on_network_volume: volume == crate::import::VolumeKind::Network,
            watched_folder_name: WatchedFolder::from_path(watched_folder_path.clone()).name,
            watched_folder_path,
            status,
        });
    }
    Ok(statuses)
}

impl Database {
    /// Where each watched folder's scan stands now.
    pub(crate) async fn load_folder_scan_progress(&self) -> Result<FolderScanProgress, DbError> {
        self.read(move |sql| load_folder_scan_progress_on(&sql))
            .process(|statuses| Ok(FolderScanProgress::of(statuses)))
            .await
    }

    /// Where each watched folder's scan stands, live.
    pub(crate) fn subscribe_folder_scan_progress(&self) -> coven::LiveQuery<FolderScanProgress> {
        self.inner
            .handle
            .subscribe(move |sql| load_folder_scan_progress_on(&sql).map_err(CovenError::from))
            .process(|statuses| Ok(FolderScanProgress::of(statuses)))
    }
}
