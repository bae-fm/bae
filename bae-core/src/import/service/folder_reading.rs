//! Reading one folder under a watched root again.
//!
//! Two things change what one folder yields without touching anything beside
//! it: the person decides how it reads ("keep as separate releases", "combine
//! as one release"), or its contents change on disk. Either way only the folder
//! directly under the root that holds the change is read again — see
//! [`crate::import::folder_scanner::scan_top_level_folder_with_reader`] for why
//! that folder and nothing less — and what it now yields is stored in one
//! write: the new candidates arrive in the transaction the old ones leave in,
//! together with the decision when there is one.
//!
//! The reading is taken off the commit lock and stored under it. The root is
//! the pass's alone while it runs — the coordinator starts nothing else over it
//! — so what can move under the reading is the store written from elsewhere: a
//! file decision on one of the folder's candidates, or anything that advanced
//! the root's generation. Either fails the write rather than storing a reading
//! of a folder that is no longer the one read.

use super::*;
use crate::import::folder_scanner::{
    FolderReleaseDecision, FolderReleaseDecisionAuthor, FolderReleaseDecisionKey,
};
use crate::import::CandidateKey;
use std::ffi::{OsStr, OsString};

impl ImportService {
    /// Store `target` together with the candidates its folder reads as under
    /// it, and answer with the keys of the releases the decided folder now
    /// reads as, in path order.
    pub(super) async fn change_folder_reading(
        root: &Path,
        target: &(FolderReleaseDecisionKey, FolderReleaseDecision),
        scan: &ScanServices,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<Vec<CandidateKey>, crate::import::ImportError> {
        let (key, _) = target;
        if key.watched_folder_path.as_os_str() != root.as_os_str() {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "folder decision for {} was sent to {}",
                    coven::path_text(&key.watched_folder_path),
                    coven::path_text(root)
                ),
            });
        }
        let decided = crate::import::relative_folder::folder_below(root, &key.relative_folder);
        let Some(std::path::Component::Normal(folder)) = decided
            .strip_prefix(root)
            .ok()
            .and_then(|below| below.components().next())
        else {
            return Err(crate::import::ImportError::Watch {
                detail: format!(
                    "{} is never a release, so it has no reading to change",
                    coven::path_text(root)
                ),
            });
        };
        let folder = folder.to_os_string();
        if !scan
            .services
            .library_manager
            .folder_reading_offered(key, target.1)
            .await?
        {
            return Err(crate::import::ImportError::Watch {
                detail: format!(
                    "{} is not a current release boundary",
                    key.relative_folder.text()
                ),
            });
        }
        let stored =
            Self::read_folder_again(root, &folder, Some(target), scan, cancellation).await?;
        Ok(crate::import::list::release_keys_in_path_order(
            stored.iter().filter(|item| match item {
                ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                    candidate.path.starts_with(&decided)
                }
                ScanItem::Invalid(candidate) => candidate.path.starts_with(&decided),
                ScanItem::Decided { .. } | ScanItem::Sidecar(_) => false,
            }),
        ))
    }

    /// Read again each folder directly under `root` whose contents changed
    /// on disk, storing each in a write of its own.
    ///
    /// A folder that cannot be read or stored is said so — as the root's
    /// status, and on the event stream — and the others are read regardless:
    /// each one's write stands on its own.
    pub(super) async fn read_changed_folders(
        root: &Path,
        folders: &std::collections::BTreeSet<OsString>,
        scan: &ScanServices,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) {
        for folder in folders {
            if cancellation.is_cancelled() {
                return;
            }
            let Err(error) = Self::read_folder_again(root, folder, None, scan, cancellation).await
            else {
                continue;
            };
            if cancellation.is_cancelled() {
                return;
            }
            let message = format!(
                "{} could not be read again: {error}",
                coven::path_text(Path::new(folder))
            );
            warn!("{}: {message}", coven::path_text(root));
            match scan
                .services
                .library_manager
                .current_folder_scan_generation(root)
                .await
            {
                Ok(Some(generation)) => {
                    if let Err(status_error) =
                        Self::record_scan_failure(root, generation, message.clone(), &scan.services)
                            .await
                    {
                        error!(
                            "{}'s failed reading could not be stored: {status_error}",
                            root.display()
                        );
                        Self::announce_scan_failure(root, message, &scan.services.event_tx).await;
                    }
                }
                Ok(None) => {
                    Self::announce_scan_failure(root, message, &scan.services.event_tx).await
                }
                Err(status_error) => {
                    error!(
                        "{}'s generation could not be read to store a failed reading: \
                         {status_error}",
                        root.display()
                    );
                    Self::announce_scan_failure(root, message, &scan.services.event_tx).await;
                }
            }
        }
    }

    /// Read `folder` — directly under `root` — again as `decision`, when there
    /// is one, reads it, store what it yields in one write, and answer with
    /// every release it stored, as stored, in the order it first stored them.
    ///
    /// A folder that is no longer there yields nothing, which removes every
    /// entry that was under it. The root itself must be there: a root that
    /// cannot be read is not a root whose folders all went away.
    async fn read_folder_again(
        root: &Path,
        folder: &OsStr,
        decision: Option<&(FolderReleaseDecisionKey, FolderReleaseDecision)>,
        scan: &ScanServices,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<Vec<ScanItem>, crate::import::ImportError> {
        let started = std::time::Instant::now();
        let services = &scan.services;
        let library_manager = &services.library_manager;
        let stamp = library_manager.begin_folder_reading(root).await?;
        let stored_edits = library_manager.load_stored_candidate_edits().await?;
        let mut decisions = library_manager.load_folder_release_decisions(root).await?;
        // The person's answer reads the folder under the grouping it already
        // has, when it has one: the release it reads as keeps its key.
        let decision = decision.map(|(key, decision)| {
            let grouping = decisions
                .get(&key.relative_folder)
                .map(|stored| stored.grouping.clone())
                .unwrap_or_else(|| new_grouping_key(services.ids.as_ref()));
            let reading = crate::import::folder_scanner::FolderReading {
                decision: *decision,
                author: FolderReleaseDecisionAuthor::User,
                grouping,
            };
            decisions.insert(key.relative_folder.clone(), reading.clone());
            (key.clone(), reading)
        });
        let reads = Arc::new(
            library_manager
                .load_file_reads(root, services.file_tags.clone())
                .await?,
        );
        let walk_reads = reads.clone();

        let directories = services.directories.clone();
        let watcher = scan.folder_watcher.clone();
        let walk_root = root.to_path_buf();
        let walk_folder = PathBuf::from(folder);
        let walk_cancellation = cancellation.clone();
        let wants_folder = decision.is_some();
        let ids = services.ids.clone();
        let walked = tokio::task::spawn_blocking(move || {
            let new_key = || new_grouping_key(ids.as_ref());
            read_top_level_folder(
                directories.as_ref(),
                &watcher,
                &walk_root,
                &walk_folder,
                &crate::import::folder_scanner::ScanReadings {
                    stored: &stored_edits,
                    decisions: &decisions,
                    new_grouping_key: &new_key,
                },
                &walk_reads,
                &walk_cancellation,
                wants_folder,
            )
        })
        .await
        .map_err(|error| crate::import::ImportError::Internal {
            detail: format!("folder reading task failed: {error}"),
        })??;
        let read_at = started.elapsed();

        let mut scanned_decisions = Vec::new();
        let mut items = Vec::with_capacity(walked.items.len());
        for item in walked.items {
            let path = match &item {
                ScanItem::Decided {
                    key,
                    decision,
                    grouping,
                } => {
                    scanned_decisions.push((
                        key.clone(),
                        crate::import::folder_scanner::FolderReading {
                            decision: *decision,
                            author: FolderReleaseDecisionAuthor::Heuristic,
                            grouping: grouping.clone(),
                        },
                    ));
                    continue;
                }
                ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                    candidate.path.clone()
                }
                ScanItem::Invalid(candidate) => candidate.path.clone(),
                // A folder's sidecar files are no release: no date, no tags.
                ScanItem::Sidecar(_) => {
                    items.push(crate::db::ScanItemToWrite {
                        item,
                        seed: crate::db::ScanItemSeed::Tags(None),
                        folder_date: None,
                    });
                    continue;
                }
            };
            let folder_date = tokio::task::spawn_blocking(move || {
                crate::import::folder_scanner::FolderDate::read(&path)
            })
            .await
            .map_err(|error| crate::import::ImportError::Internal {
                detail: format!("folder date task failed: {error}"),
            })??;
            let seed = library_manager
                .scan_item_seed(&item, stamp.generation, reads.clone())
                .await?;
            items.push(crate::db::ScanItemToWrite {
                item,
                seed,
                folder_date,
            });
        }
        let prepared_at = started.elapsed();

        let _commit = services
            .folder_state_commit
            .lock("store a folder reading")
            .await;
        let locked_at = started.elapsed();
        if cancellation.is_cancelled() {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "reading {} again was cancelled before it was stored",
                    coven::path_text(Path::new(folder))
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
                        candidate.relative_folder.text()
                    ),
                });
            }
        }
        let crate::db::FolderReadingWrite {
            writes,
            pruned,
            regrouped,
        } = library_manager
            .commit_folder_reading(crate::db::FolderReadingCommit {
                watched_folder_path: root.to_path_buf(),
                folder: folder.to_os_string(),
                stamp,
                decision: decision.clone(),
                scanned_decisions,
                items,
                directories: walked.directory_mtimes,
            })
            .await?;
        let committed_at = started.elapsed();
        // The walk can write one release twice — found before its folder's
        // reading was known, then settled — so each key is stored as its last
        // write left it.
        let mut stored: Vec<ScanItem> = Vec::new();
        let mut stored_at: HashMap<CandidateKey, usize> = HashMap::new();
        for (item, _) in &writes {
            let Some(key) = item.persisted_key() else {
                continue;
            };
            match stored_at.entry(key) {
                std::collections::hash_map::Entry::Occupied(at) => {
                    stored[*at.get()] = item.clone();
                }
                std::collections::hash_map::Entry::Vacant(at) => {
                    at.insert(stored.len());
                    stored.push(item.clone());
                }
            }
        }
        let changed = writes.iter().filter(|(_, write)| write.changed()).count();
        let unchanged = writes.len() - changed;
        for (item, write) in writes {
            Self::announce_scan_write(item, &write, services).await?;
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
        Self::announce_regrouped(&regrouped, services).await?;
        debug!(
            "{} under {} read again{} in {:?}: read in {read_at:?}, dates and tags by \
             {prepared_at:?}, commit lock by {locked_at:?}, stored by {committed_at:?}; \
             {changed} entries written, {unchanged} unchanged, pruned {pruned:?}",
            coven::path_text(Path::new(folder)),
            coven::path_text(root),
            decision
                .as_ref()
                .map(|(key, reading)| format!(
                    " as {:?} for {}",
                    reading.decision,
                    key.relative_folder.text()
                ))
                .unwrap_or_default(),
            started.elapsed(),
        );
        Ok(stored)
    }
}

/// What reading one folder directly under a root yielded: its entries in the
/// order the walk yielded them, and every directory in it with the mtime it
/// had — `None` when one could not be read, which leaves the next cheap check
/// nothing to conclude from.
struct TopLevelReading {
    items: Vec<ScanItem>,
    directory_mtimes: Option<Vec<(PathBuf, i64)>>,
}

/// Read `folder` under `root` on the calling thread, keeping the watch on
/// every directory in it and on nothing it no longer holds.
///
/// A folder that is not there yields nothing, unless the reading was asked
/// for by name — a decision about a folder that went away is refused, not
/// stored as an empty reading.
fn read_top_level_folder(
    reader: &dyn crate::import::folder_scanner::DirectoryReader,
    watcher: &FolderWatcher,
    root: &Path,
    folder: &Path,
    readings: &crate::import::folder_scanner::ScanReadings<'_>,
    reads: &crate::import::folder_scanner::FileReads,
    cancellation: &crate::import::folder_scanner::ScanCancellation,
    wants_folder: bool,
) -> Result<TopLevelReading, crate::import::ImportError> {
    if !std::fs::metadata(root)
        .map_err(|source| crate::import::folder_scanner::FolderScanError::io(root, source))?
        .is_dir()
    {
        return Err(
            crate::import::folder_scanner::FolderScanError::NotADirectory {
                path: root.to_path_buf(),
            }
            .into(),
        );
    }
    let absolute = root.join(folder);
    let present = match std::fs::metadata(&absolute) {
        Ok(metadata) => metadata.is_dir(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(source) => {
            return Err(
                crate::import::folder_scanner::FolderScanError::io(&absolute, source).into(),
            )
        }
    };
    let mut seen = HashSet::new();
    let mut directory_mtimes = Some(Vec::new());
    let mut items = Vec::new();
    if present {
        let mut watch_failures = Vec::new();
        crate::import::folder_scanner::scan_top_level_folder_with_reader(
            reader,
            root,
            folder,
            readings,
            reads,
            cancellation,
            |directory| {
                match (directory_mtimes.as_mut(), directory_modified_at(&directory)) {
                    (Some(recorded), Some(modified_at)) => {
                        recorded.push((directory.clone(), modified_at))
                    }
                    (Some(_), None) => directory_mtimes = None,
                    (None, _) => {}
                }
                if let Err(error) = watcher.install_directory(root, &directory) {
                    watch_failures.push(format!("{}: {error}", directory.display()));
                }
                seen.insert(directory);
            },
            |item| items.push(item),
        )?;
        if !watch_failures.is_empty() {
            warn!(
                "folder watch unavailable under {} ({}); a refresh reads it again",
                absolute.display(),
                watch_failures.join(", ")
            );
        }
    } else if wants_folder {
        return Err(crate::import::ImportError::Watch {
            detail: format!("{} is no longer there", absolute.display()),
        });
    }
    if let Err(error) = watcher.retain_directories_under(root, &absolute, &seen) {
        warn!(
            "could not reconcile folder watches under {}: {error}",
            absolute.display()
        );
    }
    Ok(TopLevelReading {
        items,
        directory_mtimes,
    })
}

/// A key for a grouping the store has not seen: the key the release it reads
/// as is addressed by from here on.
pub(super) fn new_grouping_key(ids: &dyn coven::IdSource) -> CandidateKey {
    CandidateKey::grouping(&ids.new_id().to_string())
}
