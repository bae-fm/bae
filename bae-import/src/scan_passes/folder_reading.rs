//! Reading one folder under a watched root again.
//!
//! Two things change what one folder yields without touching anything beside
//! it: the person decides how it reads ("keep as separate releases", "combine
//! as one release"), or its contents change on disk. Either way only the folder
//! directly under the root that holds the change is read again — see
//! [`crate::folder_scanner::scan_top_level_folder_with_reader`] for why
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
use crate::folder_scanner::{
    FolderReleaseDecision, FolderReleaseDecisionAuthor, FolderReleaseDecisionKey,
};
use crate::CandidateKey;
use bae_util::task_failure::TaskFailed;
use std::ffi::{OsStr, OsString};

/// Store `target` together with the candidates its folder reads as under
/// it, and answer with the keys of the releases the decided folder now
/// reads as, in path order.
pub(super) async fn change_folder_reading(
    root: &Path,
    target: &(FolderReleaseDecisionKey, FolderReleaseDecision),
    scan: &ScanServices,
    cancellation: &crate::folder_scanner::ScanCancellation,
) -> Result<Vec<CandidateKey>, crate::ImportError> {
    let (key, _) = target;
    if key.watched_folder_path.as_os_str() != root.as_os_str() {
        return Err(crate::ImportError::Internal {
            detail: format!(
                "folder decision for {} was sent to {}",
                coven::path_text(&key.watched_folder_path),
                coven::path_text(root)
            ),
        });
    }
    let decided = crate::tables::watched_path::folder_below(root, &key.relative_folder);
    let Some(std::path::Component::Normal(folder)) =
        decided.strip_prefix(root).ok().and_then(|below| below.components().next())
    else {
        return Err(crate::ImportError::Watch {
            detail: format!(
                "{} is never a release, so it has no reading to change",
                coven::path_text(root)
            ),
        });
    };
    let folder = folder.to_os_string();
    if !scan.services.tables.folder_reading_offered(key, target.1).await? {
        return Err(crate::ImportError::Watch {
            detail: format!("{} is not a current release boundary", key.relative_folder.text()),
        });
    }
    let stored = read_folder_again(root, &folder, Some(target), scan, cancellation).await?;
    Ok(crate::list::release_keys_in_path_order(stored.iter().filter(|item| match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            candidate.path.starts_with(&decided)
        }
        ScanItem::Invalid(candidate) => candidate.path.starts_with(&decided),
    })))
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
    cancellation: &crate::folder_scanner::ScanCancellation,
) {
    for folder in folders {
        if cancellation.is_cancelled() {
            return;
        }
        // A reading that panics fails that folder as an error does.
        let read = bae_util::task_failure::caught(
            "read a changed folder again",
            read_folder_again(root, folder, None, scan, cancellation),
        )
        .await
        .unwrap_or_else(|bug| {
            Err(TaskFailed::Panicked(scan.services.diagnostics.panicked(bug)).into())
        });
        let Err(error) = read else {
            continue;
        };
        if cancellation.is_cancelled() {
            return;
        }
        let message =
            format!("{} could not be read again: {error}", coven::path_text(Path::new(folder)));
        warn!("{}: {message}", coven::path_text(root));
        match scan.services.tables.current_folder_scan_generation(root).await {
            Ok(Some(generation)) => {
                if let Err(status_error) =
                    record_scan_failure(root, generation, message.clone(), &scan.services).await
                {
                    error!(
                        "{}'s failed reading could not be stored: {status_error}",
                        root.display()
                    );
                    announce_scan_failure(root, message, &scan.services).await;
                }
            }
            Ok(None) => announce_scan_failure(root, message, &scan.services).await,
            Err(status_error) => {
                error!(
                    "{}'s generation could not be read to store a failed reading: \
                     {status_error}",
                    root.display()
                );
                announce_scan_failure(root, message, &scan.services).await;
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
    cancellation: &crate::folder_scanner::ScanCancellation,
) -> Result<Vec<ScanItem>, crate::ImportError> {
    let started = std::time::Instant::now();
    let services = &scan.services;
    let tables = &services.tables;
    let stamp = tables.begin_folder_reading(root).await?;
    let stored_edits = tables.load_stored_candidate_edits().await?;
    let mut decisions = tables.load_folder_release_decisions(root).await?;
    // The person's answer reads the folder under the grouping it already
    // has, when it has one: the release it reads as keeps its key.
    let decision = decision.map(|(key, decision)| {
        let grouping = decisions
            .get(&key.relative_folder)
            .map(|stored| stored.grouping.clone())
            .unwrap_or_else(|| new_grouping_key(services.ids.as_ref()));
        let reading = crate::folder_scanner::FolderReading {
            decision: *decision,
            author: FolderReleaseDecisionAuthor::User,
            grouping,
        };
        decisions.insert(key.relative_folder.clone(), reading.clone());
        (key.clone(), reading)
    });
    let reads = Arc::new(tables.load_file_reads(root, services.file_tags.clone()).await?);
    let walk_reads = reads.clone();

    let directories = services.directories.clone();
    let watcher = scan.folder_watcher.clone();
    let walk_root = root.to_path_buf();
    let walk_folder = PathBuf::from(folder);
    let walk_cancellation = cancellation.clone();
    let wants_folder = decision.is_some();
    let ids = services.ids.clone();
    let walked = bae_util::task_failure::blocking("read a folder again", move || {
        let new_key = || new_grouping_key(ids.as_ref());
        read_top_level_folder(
            directories.as_ref(),
            &watcher,
            &walk_root,
            &walk_folder,
            &crate::folder_scanner::ScanReadings {
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
    .map_err(|failed| services.diagnostics.bug(failed))??;
    let read_at = started.elapsed();

    let mut scanned_decisions = Vec::new();
    let mut items = Vec::with_capacity(walked.items.len());
    for item in walked.items {
        let item = match item {
            crate::folder_scanner::WalkItem::Entry(FolderEntry::Release(item)) => item,
            // A folder's sidecar files are no release: no date, no tags.
            crate::folder_scanner::WalkItem::Entry(FolderEntry::Sidecar(sidecar)) => {
                items.push(crate::tables::FolderEntryToWrite::Sidecar(sidecar));
                continue;
            }
            crate::folder_scanner::WalkItem::Reading { key, decision, grouping } => {
                scanned_decisions.push((
                    key,
                    crate::folder_scanner::FolderReading {
                        decision,
                        author: FolderReleaseDecisionAuthor::Heuristic,
                        grouping,
                    },
                ));
                continue;
            }
        };
        let folder_date = super::scan_items::folder_date(&item, &services.diagnostics).await?;
        let seed = super::scan_item_seed(
            tables,
            &services.diagnostics,
            &item,
            stamp.generation,
            reads.clone(),
        )
        .await?;
        items.push(crate::tables::FolderEntryToWrite::Release(crate::tables::ScanItemToWrite {
            item,
            seed,
            folder_date,
        }));
    }
    let prepared_at = started.elapsed();

    let _commit = services.folder_state_commit.lock("store a folder reading").await;
    let locked_at = started.elapsed();
    if cancellation.is_cancelled() {
        return Err(crate::ImportError::Internal {
            detail: format!(
                "reading {} again was cancelled before it was stored",
                coven::path_text(Path::new(folder))
            ),
        });
    }
    // The walk read each candidate with the file decisions stored then. One
    // stored since describes files this reading did not settle.
    for to_write in &items {
        let crate::tables::FolderEntryToWrite::Release(crate::tables::ScanItemToWrite {
            item: ScanItem::Discovered(candidate) | ScanItem::Valid(candidate),
            ..
        }) = to_write
        else {
            continue;
        };
        let edits = tables.load_candidate_file_edits(&candidate.files.content_hash()).await?;
        if edits.revision != candidate.file_edit_revision {
            return Err(crate::ImportError::Internal {
                detail: format!(
                    "{} changed while its folder was being read again: its file decisions \
                     were edited",
                    candidate.relative_folder.text()
                ),
            });
        }
    }
    let crate::tables::FolderReadingWrite { writes, pruned, regrouped } = tables
        .commit_folder_reading(crate::tables::FolderReadingCommit {
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
    for (entry, _) in &writes {
        let FolderEntry::Release(item) = entry else {
            continue;
        };
        match stored_at.entry(item.key()) {
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
    for (entry, write) in writes {
        announce_scan_write(entry, &write, services).await?;
    }
    for candidate_key in &pruned {
        services.event_tx.send(crate::ImportEvent::Scan(ScanEvent::CandidateRemoved {
            candidate_key: candidate_key.clone(),
        }));
    }
    announce_regrouped(&regrouped, services).await?;
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

/// What reading one folder directly under a root yielded: its entries in the
/// order the walk yielded them, and every directory in it with the mtime it
/// had — `None` when one could not be read, which leaves the next cheap check
/// nothing to conclude from.
struct TopLevelReading {
    items: Vec<crate::folder_scanner::WalkItem>,
    directory_mtimes: Option<Vec<(PathBuf, i64)>>,
}

/// Read `folder` under `root` on the calling thread, keeping the watch on
/// every directory in it and on nothing it no longer holds.
///
/// A folder that is not there yields nothing, unless the reading was asked
/// for by name — a decision about a folder that went away is refused, not
/// stored as an empty reading.
fn read_top_level_folder(
    reader: &dyn crate::folder_scanner::DirectoryReader,
    watcher: &FolderWatcher,
    root: &Path,
    folder: &Path,
    readings: &crate::folder_scanner::ScanReadings<'_>,
    reads: &crate::folder_scanner::FileReads,
    cancellation: &crate::folder_scanner::ScanCancellation,
    wants_folder: bool,
) -> Result<TopLevelReading, crate::ImportError> {
    if !std::fs::metadata(root)
        .map_err(|source| crate::folder_scanner::FolderScanError::io(root, source))?
        .is_dir()
    {
        return Err(crate::folder_scanner::FolderScanError::NotADirectory {
            path: root.to_path_buf(),
        }
        .into());
    }
    let absolute = root.join(folder);
    let present = match std::fs::metadata(&absolute) {
        Ok(metadata) => metadata.is_dir(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(source) => {
            return Err(crate::folder_scanner::FolderScanError::io(&absolute, source).into())
        }
    };
    let mut seen = HashSet::new();
    let mut directory_mtimes = Some(Vec::new());
    let mut items = Vec::new();
    if present {
        let mut watch_failures = Vec::new();
        crate::folder_scanner::scan_top_level_folder_with_reader(
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
        return Err(crate::ImportError::Watch {
            detail: format!("{} is no longer there", absolute.display()),
        });
    }
    if let Err(error) = watcher.retain_directories_under(root, &absolute, &seen) {
        warn!("could not reconcile folder watches under {}: {error}", absolute.display());
    }
    Ok(TopLevelReading { items, directory_mtimes })
}

/// A key for a grouping the store has not seen: the key the release it reads
/// as is addressed by from here on.
pub(super) fn new_grouping_key(ids: &dyn coven::IdSource) -> CandidateKey {
    CandidateKey::grouping(&ids.new_id().to_string())
}

#[cfg(test)]
#[path = "folder_reading_tests.rs"]
mod tests;
