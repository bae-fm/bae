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

/// One scanned root as stored, before its volume is looked at.
struct StoredScanStatus {
    watched_folder_path: String,
    watched_folder_name: String,
    status: FolderScanStatus,
}

fn load_folder_scan_progress_on(
    sql: &SqlReadContext<'_>,
) -> Result<impl FnOnce() -> Result<FolderScanProgress, DbError> + Send + 'static, DbError> {
    let watched_folders: Vec<WatchedFolder> = sql
        .query(
            "SELECT path FROM watched_import_folders ORDER BY position",
            [],
            |row| row.get::<_, String>(0),
        )?
        .into_iter()
        .map(WatchedFolder::from_path)
        .collect();
    let mut statuses = Vec::new();
    for (watched_folder_path, status, error, found_count) in sql.query(
        "SELECT roots.watched_folder_path, roots.status, roots.error, COUNT(candidate.path) \
         FROM folder_scan_roots AS roots \
         LEFT JOIN scan_candidate AS candidate \
           ON candidate.watched_folder_path = roots.watched_folder_path \
          AND candidate.generation = roots.generation AND candidate.source_kind = 'folder' \
         GROUP BY roots.watched_folder_path, roots.status, roots.error",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )? {
        let position = watched_folders
            .iter()
            .position(|folder| folder.path == watched_folder_path)
            .ok_or_else(|| {
                DbError::Message(format!(
                    "folder scan root {watched_folder_path} is not a watched folder"
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
                    "folder scan root {watched_folder_path} has invalid status {status:?} \
                     and error {error:?}"
                )))
            }
        };
        statuses.push((
            position,
            StoredScanStatus {
                watched_folder_name: watched_folders[position].name.clone(),
                watched_folder_path,
                status,
            },
        ));
    }
    statuses.sort_by_key(|(position, _)| *position);
    Ok(move || {
        Ok(FolderScanProgress::of(
            statuses
                .into_iter()
                .map(|(_, stored)| WatchedFolderScanStatus {
                    on_network_volume: crate::import::volume::volume_kind_blocking(Path::new(
                        &stored.watched_folder_path,
                    )) == crate::import::volume::VolumeKind::Network,
                    watched_folder_path: stored.watched_folder_path,
                    watched_folder_name: stored.watched_folder_name,
                    status: stored.status,
                })
                .collect(),
        ))
    })
}

impl Database {
    /// Where each watched folder's scan stands, live.
    pub(crate) fn subscribe_folder_scan_progress(&self) -> coven::LiveQuery<FolderScanProgress> {
        self.inner
            .handle
            .subscribe(move |sql| load_folder_scan_progress_on(&sql).map_err(CovenError::from))
            .process(|process| process().map_err(CovenError::from))
    }
}
