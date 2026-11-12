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
    new_grouping_key, FolderReleaseDecision, FolderReleaseDecisionAuthor, FolderReleaseDecisionKey,
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
    let FolderReadAgain { stored, watch_failures } =
        read_folder_again(root, &folder, Some(target), scan, cancellation).await?;
    // The decision is stored and its caller hears so; that the folder is not
    // watched is the root's status, as a pass that read it would leave it.
    // A refusal of that write is reported by the write.
    store_watch_failure(root, &watch_failures, &scan.services).await;
    Ok(crate::list::release_keys_in_path_order(stored.iter().filter(|item| match item {
        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
            candidate.path.starts_with(&decided)
        }
        ScanItem::Invalid(candidate) => candidate.path.starts_with(&decided),
    })))
}

/// Read again each folder directly under `root` whose contents changed
/// on disk, storing each in a write of its own, and answer with the last
/// failure the store would not take, on the volume the root is on, for the
/// coordinator to hold.
///
/// A folder that cannot be read or stored is stored as the root's status,
/// and the others are read regardless: each one's write stands on its own.
pub(super) async fn read_changed_folders(
    root: &Path,
    folders: &std::collections::BTreeSet<OsString>,
    scan: &ScanServices,
    cancellation: &crate::folder_scanner::ScanCancellation,
) -> Option<crate::folder_scan_status::UnstoredScanFailure> {
    let mut unstored: Option<String> = None;
    for folder in folders {
        if cancellation.is_cancelled() {
            break;
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
        let error = match read {
            Ok(FolderReadAgain { watch_failures, .. }) => {
                if let Some(message) =
                    store_watch_failure(root, &watch_failures, &scan.services).await
                {
                    unstored = Some(message);
                }
                continue;
            }
            Err(error) => error,
        };
        if cancellation.is_cancelled() {
            break;
        }
        let message =
            format!("{} could not be read again: {error}", coven::path_text(Path::new(folder)));
        warn!("{}: {message}", coven::path_text(root));
        if let Some(message) =
            store_reading_status(root, RootMark::Failed(message), &scan.services).await
        {
            unstored = Some(message);
        }
    }
    let error = unstored?;
    let volume = volume_kind(root, &scan.services.diagnostics).await;
    Some(crate::folder_scan_status::UnstoredScanFailure { error, volume })
}

/// What a pass over a changed folder stores as the root's status.
enum RootMark {
    /// The folder could not be read again, for this reason.
    Failed(String),
    /// The folder was read again, and a directory in it could not be watched,
    /// for this reason.
    NotWatched(String),
}

/// Store `mark` as the status of `root`'s current generation, answering
/// with it when the store would not take it. A refusal is reported as
/// [`report_refused_scan_write`] says.
async fn store_reading_status(
    root: &Path,
    mark: RootMark,
    services: &crate::ImportServices,
) -> Option<String> {
    let message = match &mark {
        RootMark::Failed(message) | RootMark::NotWatched(message) => message.clone(),
    };
    let diagnostics = &services.diagnostics;
    let generation = match services.tables.current_folder_scan_generation(root).await {
        Ok(Some(generation)) => generation,
        // A folder is read again only under a watch a whole-root scan
        // installed, after that scan opened its generation.
        Ok(None) => {
            diagnostics.report_bug(
                "store a failed reading of a watched folder",
                &format_args!("{} has no scan generation", coven::path_text(root)),
            );
            return Some(message);
        }
        Err(error) => {
            report_refused_scan_write(
                root,
                "read a watched folder's scan generation",
                &error,
                diagnostics,
            );
            return Some(message);
        }
    };
    let recorded = match &mark {
        RootMark::Failed(message) => record_scan_failure(root, generation, message, services).await,
        RootMark::NotWatched(reason) => {
            record_scan_unwatched(root, generation, reason, services).await
        }
    };
    match recorded {
        Ok(true) => None,
        Ok(false) => {
            debug!(
                "{}'s failed reading was not stored: a newer scan owns the root",
                root.display()
            );
            None
        }
        Err(error) => {
            report_refused_scan_write(
                root,
                "store a watched folder's failed reading",
                &error,
                diagnostics,
            );
            Some(message)
        }
    }
}

/// What reading a folder again stored.
struct FolderReadAgain {
    /// Every release it stored, as stored, in the order it first stored them.
    stored: Vec<ScanItem>,
    /// Each directory in the folder that could not be watched, with the
    /// reason.
    watch_failures: Vec<String>,
}

/// Store as the status of `root`'s current generation that the directories
/// under it that could not be watched are not. The reading stands: this only
/// says the folder is not watched. A write the store would not take comes
/// back as a failure, for the coordinator to hold.
async fn store_watch_failure(
    root: &Path,
    watch_failures: &[String],
    services: &crate::ImportServices,
) -> Option<String> {
    let reason = super::unwatched_reason(watch_failures)?;
    warn!("{} is not watched for changes: {reason}", coven::path_text(root));
    store_reading_status(root, RootMark::NotWatched(reason), services).await
}

/// Read `folder` — directly under `root` — again as `decision`, when there
/// is one, reads it, store what it yields in one write, and answer with
/// every release it stored (see [`FolderReadAgain`]).
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
) -> Result<FolderReadAgain, crate::ImportError> {
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
    let walked = services
        .diagnostics
        .blocking("read a folder again", move || {
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
        .await??;
    let read_at = started.elapsed();

    let watch_failures = walked.watch_failures;
    let mut scanned_decisions = Vec::new();
    // Each entry the folder yields, a release with its folder's date.
    let mut dated = Vec::with_capacity(walked.items.len());
    for item in walked.items {
        let item = match item {
            crate::folder_scanner::WalkItem::Entry(FolderEntry::Release(item)) => item,
            // A folder's sidecar files are no release: no date, no tags.
            crate::folder_scanner::WalkItem::Entry(FolderEntry::Sidecar(sidecar)) => {
                dated.push((FolderEntry::Sidecar(sidecar), None));
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
        dated.push((FolderEntry::Release(item), folder_date));
    }
    // The entries that changed after a read found them to be the items
    // written over them, which every later read seeds.
    let mut moved = HashSet::new();
    let (_commit, crate::tables::FolderReadingWrite { writes, pruned, regrouped }, timings) = loop {
        let mut items = Vec::with_capacity(dated.len());
        for (entry, folder_date) in &dated {
            let item = match entry {
                FolderEntry::Release(item) => item,
                FolderEntry::Sidecar(sidecar) => {
                    items.push(crate::tables::FolderEntryToWrite::Sidecar(sidecar.clone()));
                    continue;
                }
            };
            let seed = super::scan_item_seed(
                tables,
                &services.diagnostics,
                item,
                stamp.generation,
                reads.clone(),
                &moved,
            )
            .await?;
            items.push(crate::tables::FolderEntryToWrite::Release(
                crate::tables::ScanItemToWrite {
                    item: item.clone(),
                    seed,
                    folder_date: *folder_date,
                },
            ));
        }
        let prepared_at = started.elapsed();

        let commit = services.folder_state_commit.lock("store a folder reading").await;
        let locked_at = started.elapsed();
        if cancellation.is_cancelled() {
            return Err(crate::ImportError::Internal {
                detail: format!(
                    "reading {} again was cancelled before it was stored",
                    coven::path_text(Path::new(folder))
                ),
            });
        }
        // The walk read each candidate with the file decisions stored then.
        // One stored since describes files this reading did not settle.
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
        // An entry that changed since it was read as unchanged was not
        // seeded: seed it as it now stands.
        match tables
            .commit_folder_reading(crate::tables::FolderReadingCommit {
                watched_folder_path: root.to_path_buf(),
                folder: folder.to_os_string(),
                stamp,
                decision: decision.clone(),
                scanned_decisions: scanned_decisions.clone(),
                items,
                directories: walked.directory_mtimes.clone(),
            })
            .await
        {
            Ok(write) => break (commit, write, (prepared_at, locked_at)),
            Err(crate::ImportError::ScanEntryMoved(crate::tables::ScanEntryMoved(key))) => {
                debug!("{key} changed after it was read as unchanged; seeding it again");
                drop(commit);
                moved.insert(key);
            }
            Err(error) => return Err(error),
        }
    };
    let (prepared_at, locked_at) = timings;
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
    let automatic = identifies_automatically(services).await?;
    for (entry, write) in writes {
        announce_scan_write(entry, &write, automatic, services).await?;
    }
    for candidate_key in &pruned {
        services.event_tx.send(crate::ImportEvent::Scan(ScanEvent::CandidateRemoved {
            candidate_key: candidate_key.clone(),
        }));
    }
    announce_regrouped(&regrouped, automatic, services).await?;
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
    Ok(FolderReadAgain { stored, watch_failures })
}

/// What reading one folder directly under a root yielded: its entries in the
/// order the walk yielded them, and every directory in it with the mtime it
/// had — `None` when one could not be read, which leaves the next cheap check
/// nothing to conclude from.
struct TopLevelReading {
    items: Vec<crate::folder_scanner::WalkItem>,
    directory_mtimes: Option<Vec<(PathBuf, i64)>>,
    /// Each directory in it that could not be watched, with the reason.
    watch_failures: Vec<String>,
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
    let mut watch_failures = Vec::new();
    if present {
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
    } else if wants_folder {
        return Err(crate::ImportError::Watch {
            detail: format!("{} is no longer there", absolute.display()),
        });
    }
    if let Err(error) = watcher.retain_directories_under(root, &absolute, &seen) {
        watch_failures.push(format!("{}: {error}", absolute.display()));
    }
    Ok(TopLevelReading { items, directory_mtimes, watch_failures })
}

#[cfg(test)]
#[path = "folder_reading_tests.rs"]
mod tests;
