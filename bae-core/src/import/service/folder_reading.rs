//! Changing how one folder reads.
//!
//! "Keep as separate releases" and "combine as one release" trade the folder's
//! candidates for the ones the other reading gives. The decision and those
//! candidates are one fact, so they are stored in one write: the new reading
//! is taken first, off the commit lock, and stored together with the decision
//! — the old candidates leave in the same transaction the new ones arrive in.
//!
//! Only the folder directly under the root that holds the changed folder is
//! read again; see
//! [`crate::import::folder_scanner::scan_top_level_folder_with_reader`] for why
//! that folder and nothing less.

use super::*;
use crate::import::folder_scanner::{
    FolderReleaseDecision, FolderReleaseDecisionAuthor, FolderReleaseDecisionKey,
};

impl ImportService {
    /// Store `target` together with the candidates its folder reads as under
    /// it.
    ///
    /// The root is this pass's alone while it runs — the coordinator starts
    /// nothing else over it — so what can move under the reading is the store
    /// written from elsewhere: a file decision on one of the folder's
    /// candidates, or anything that advanced the root's generation. Either
    /// fails the commit rather than storing a reading of a folder that is no
    /// longer the one read.
    pub(super) async fn change_folder_reading(
        root: &Path,
        target: &(FolderReleaseDecisionKey, FolderReleaseDecision),
        scan: &ScanServices,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<(), crate::import::ImportError> {
        let started = std::time::Instant::now();
        let services = &scan.services;
        let library_manager = &services.library_manager;
        let root_key = root.to_string_lossy().into_owned();
        let (key, decision) = target;
        if key.watched_folder_path != root_key {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "folder decision for {} was sent to {root_key}",
                    key.watched_folder_path
                ),
            });
        }
        let Some(folder) = key
            .relative_folder_path
            .split('/')
            .next()
            .filter(|folder| !folder.is_empty())
            .map(str::to_string)
        else {
            return Err(crate::import::ImportError::Watch {
                detail: format!("{root_key} is never a release, so it has no reading to change"),
            });
        };

        let stored_items = library_manager.load_folder_scan_items(&root_key).await?;
        if !crate::import::candidates::names_a_current_folder_reading(&stored_items, key) {
            return Err(crate::import::ImportError::Watch {
                detail: format!(
                    "{} is not a current release boundary",
                    key.relative_folder_path
                ),
            });
        }
        let stamp = library_manager.begin_folder_reading(&root_key).await?;
        let stored_edits = library_manager.load_stored_candidate_edits().await?;
        let mut decisions = library_manager
            .load_folder_release_decisions(&root_key)
            .await?;
        decisions.insert(
            key.relative_folder_path.clone(),
            *decision,
            FolderReleaseDecisionAuthor::User,
        );
        let skipped = library_manager
            .load_skipped_import_candidates(&root_key)
            .await?;

        let directories = services.directories.clone();
        let walk_root = root.to_path_buf();
        let walk_folder = PathBuf::from(&folder);
        let walk_cancellation = cancellation.clone();
        let walked = tokio::task::spawn_blocking(move || {
            let mut items = Vec::new();
            crate::import::folder_scanner::scan_top_level_folder_with_reader(
                directories.as_ref(),
                &walk_root,
                &walk_folder,
                &stored_edits,
                &decisions,
                &walk_cancellation,
                |item| items.push(item),
            )
            .map(|()| items)
        })
        .await
        .map_err(|error| crate::import::ImportError::Internal {
            detail: format!("folder reading task failed: {error}"),
        })??;
        let read_at = started.elapsed();

        let mut scanned_decisions = Vec::new();
        let mut items = Vec::with_capacity(walked.len());
        for item in walked {
            let path = match &item {
                ScanItem::Decided { key, decision } => {
                    scanned_decisions.push((key.clone(), *decision));
                    continue;
                }
                ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                    candidate.path.clone()
                }
                ScanItem::Invalid(candidate) => candidate.path.clone(),
            };
            let folder_date = tokio::task::spawn_blocking(move || {
                crate::import::folder_scanner::FolderDate::read(&path)
            })
            .await
            .map_err(|error| crate::import::ImportError::Internal {
                detail: format!("folder date task failed: {error}"),
            })??;
            let file_metadata = library_manager
                .scan_item_seed(&item, stamp.generation, services.file_tags.as_ref())
                .await?;
            items.push(crate::db::ScanItemToWrite {
                item,
                file_metadata,
                folder_date,
            });
        }
        let prepared_at = started.elapsed();

        let _commit = services.folder_state_commit.lock().await;
        let locked_at = started.elapsed();
        if cancellation.is_cancelled() {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "the decision for {} was cancelled before it was stored",
                    key.relative_folder_path
                ),
            });
        }
        // The walk read each candidate with the file decisions stored then. One
        // stored since describes files this reading did not settle.
        for to_write in &items {
            let (ScanItem::Discovered(candidate) | ScanItem::Valid(candidate)) = &to_write.item
            else {
                continue;
            };
            let edits = library_manager
                .load_candidate_file_edits(&candidate.files.content_hash())
                .await?;
            if edits.revision != candidate.file_edit_revision {
                return Err(crate::import::ImportError::Internal {
                    detail: format!(
                        "{} changed while its folder was being read again: its file decisions \
                         were edited",
                        candidate.display_path
                    ),
                });
            }
        }
        let crate::db::FolderReadingWrite { writes, pruned } = library_manager
            .commit_folder_reading(crate::db::FolderReadingCommit {
                watched_folder_path: root_key,
                folder: folder.clone(),
                stamp,
                decision: target.clone(),
                scanned_decisions,
                items,
            })
            .await?;
        let committed_at = started.elapsed();
        let changed = writes.iter().filter(|(_, write)| write.changed()).count();
        let unchanged = writes.len() - changed;
        for (item, write) in writes {
            Self::announce_scan_write(item, &write, &skipped, services).await?;
        }
        for candidate_key in &pruned {
            services
                .event_tx
                .send(crate::import::handle::ImportEvent::Scan(
                    ScanEvent::CandidateRemoved {
                        candidate_key: candidate_key.clone(),
                    },
                ));
        }
        debug!(
            "folder decision for {} under {} stored in {:?}: read {folder} in {read_at:?}, \
             dates and tags by {prepared_at:?}, commit lock by {locked_at:?}, stored by \
             {committed_at:?}; {changed} entries written, {unchanged} unchanged, pruned \
             {pruned:?}",
            key.relative_folder_path,
            root.display(),
            started.elapsed(),
        );
        Ok(())
    }
}
