use super::*;
use crate::folder_scan_status::UnstoredScanFailure;
use bae_util::task_failure::TaskFailed;

/// The blocking walk of a watched root, as its failure names it.
const FOLDER_WALK: &str = "walk a watched folder";

/// Store that `root`'s scan generation failed with `message`: what the
/// folder's mark reads back.
///
/// Returns `false` when the generation is no longer the root's — a newer
/// scan owns the status and this one's failure is not the root's state.
pub(super) async fn record_scan_failure(
    root: &Path,
    generation: u64,
    message: &str,
    services: &crate::ImportServices,
) -> Result<bool, bae_store::DbError> {
    let _commit = services.folder_state_commit.lock("store a failed scan").await;
    Ok(services.tables.finish_folder_scan(root, generation, Some(message)).await?.is_some())
}

/// Say that a write of `root`'s scan state, `operation`, was refused with
/// `error`. A refusal that is a bug is reported as one, unless the store
/// reported the bug its call carried back already. One that is the
/// disk's — full, failing or damaged — or the store's as a whole is not a
/// bug: the failure it kept from being stored reaches the person through the
/// coordinator, which holds it as the root's status, and a store that failed
/// as a whole fails every later read too.
pub(super) fn report_refused_scan_write(
    root: &Path,
    operation: &str,
    error: &bae_store::DbError,
    diagnostics: &bae_diagnostics::Diagnostics,
) {
    if refusal_is_a_bug(error) {
        diagnostics.report_failure(operation, error);
    } else {
        warn!("{}: could not {operation}: {error}", coven::path_text(root));
    }
}

/// Whether a refused write of a scan's state is a bug: anything but the disk
/// failing under the store, the store failing as a whole (coven then fails
/// every later call on its handle, repeating an earlier bug as
/// `StoreFailed` once that bug has been reported) or closing with the
/// library.
fn refusal_is_a_bug(error: &bae_store::DbError) -> bool {
    use bae_store::DbError as E;
    match error {
        E::Io(_) | E::FileRead(_) => false,
        E::Rollback { operation, .. } => refusal_is_a_bug(operation),
        E::FileCleanup { write, .. } => refusal_is_a_bug(write),
        E::Sqlite(error) => !sqlite_is_the_disk(error),
        E::Coven(error) => match error.as_ref() {
            coven::CovenError::Rollback { .. } | coven::CovenError::Shutdown(_) => false,
            coven::CovenError::Database(error) => match error {
                coven::DbError::StoreClosed
                | coven::DbError::Shutdown(_)
                | coven::DbError::RollbackFailed
                | coven::DbError::StoreFailed(_)
                | coven::DbError::StatePersistence(_)
                | coven::DbError::CheckpointFailed(_)
                | coven::DbError::StatisticsFailed(_)
                | coven::DbError::WallTimer(_)
                | coven::DbError::DamagedDatabase => false,
                coven::DbError::Sqlite(error) => !sqlite_is_the_disk(error),
                _ => true,
            },
            _ => true,
        },
        E::Creation(_)
        | E::Unreadable { .. }
        | E::UnreadableJson { .. }
        | E::Encode { .. }
        | E::OutOfRange { .. }
        | E::StoredPath(_)
        | E::Cause(_)
        | E::Message(_) => true,
    }
}

/// Whether SQLite failed because of the disk under it: full, failing, gone
/// read-only, or damaged.
fn sqlite_is_the_disk(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(
            rusqlite::ErrorCode::DiskFull
                | rusqlite::ErrorCode::SystemIoFailure
                | rusqlite::ErrorCode::CannotOpen
                | rusqlite::ErrorCode::ReadOnly
                | rusqlite::ErrorCode::DatabaseCorrupt
                | rusqlite::ErrorCode::NotADatabase
        )
    )
}

pub(super) async fn cancel_and_join_folder_walk(
    cancellation: &crate::folder_scanner::ScanCancellation,
    item_rx: &mut mpsc::Receiver<crate::folder_scanner::WalkItem>,
    walk: FolderWalk,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<(), crate::ImportError> {
    cancellation.cancel();
    while item_rx.recv().await.is_some() {}
    let (walked, _, _) = bae_util::task_failure::joined(FOLDER_WALK, walk)
        .await
        .map_err(|failed| diagnostics.bug(failed))?;
    // The walk was stopped, so its own error says only that, unless one
    // of its threads panicked first.
    match walked {
        Err(crate::folder_scanner::FolderScanError::Panicked(bug)) => {
            Err(TaskFailed::Panicked(diagnostics.panicked(bug)).into())
        }
        Ok(()) | Err(_) => Ok(()),
    }
}

/// How a whole-root scan ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ScanEnded {
    /// It completed, or a newer pass or a removal took the root from it.
    Finished,
    /// It failed, and the root's stored status says so.
    Failed,
    /// It failed, and the store would not take the failure: the
    /// coordinator holds it as the root's status instead.
    FailedUnstored(UnstoredScanFailure),
}

impl ScanEnded {
    /// The failure the store would not take, which the coordinator holds.
    pub(crate) fn unstored_failure(self) -> Option<UnstoredScanFailure> {
        match self {
            Self::FailedUnstored(failure) => Some(failure),
            Self::Finished | Self::Failed => None,
        }
    }
}

/// Re-scan `root` and reconcile against the candidate keys last emitted for
/// it: emit every current candidate (the reducer keeps in-progress state for
/// the ones it already holds) plus a `CandidateRemoved` for any that vanished.
/// A failed scan keeps each item already committed by that pass and every
/// older item that was not explicitly replaced; only a completed scan prunes
/// paths that the walk did not report.
///
/// Each candidate goes out the moment the walk finds it, over a channel from
/// the blocking walk to this task, rather than as one batch once the walk
/// ends. Walk duration scales with the tree and with how fast the volume
/// answers — a network share is orders of magnitude slower than a local disk
/// — and a batch at the end leaves the import list empty for that whole
/// span, which is indistinguishable from a scan that found nothing.
///
/// Every way this can fail lands in the root's stored status: the walk
/// runs in [`walk_and_reconcile`] and this records whatever error it
/// returns. A failure the store will not take is handed back for the
/// coordinator to hold, and reported when the refusal is a bug.
pub(super) async fn rescan_and_reconcile(
    root: &Path,
    scan: &ScanServices,
    cancellation: &crate::folder_scanner::ScanCancellation,
) -> ScanEnded {
    let services = &scan.services;
    let volume = volume_kind(root, &services.diagnostics).await;
    let generation = match begin_scan(root, volume, services).await {
        Ok(generation) => generation,
        Err(error) => {
            report_refused_scan_write(
                root,
                "open a watched folder's scan",
                &error,
                &services.diagnostics,
            );
            return ScanEnded::FailedUnstored(UnstoredScanFailure {
                error: error.to_string(),
                volume,
            });
        }
    };
    // A walk that panics fails the scan as an error does, so the root's
    // stored status says it failed.
    let outcome = bae_util::task_failure::caught(
        "scan a watched folder",
        walk_and_reconcile(root, generation, scan, cancellation),
    )
    .await
    .unwrap_or_else(|bug| Err(TaskFailed::Panicked(services.diagnostics.panicked(bug)).into()));
    let Err(error) = outcome else {
        return ScanEnded::Finished;
    };
    // A cancelled scan is the coordinator taking the root away — a newer
    // scan or a removal — not a failure of this one.
    if cancellation.is_cancelled() {
        return ScanEnded::Finished;
    }
    warn!("scan of {} failed ({error}); keeping previous candidates", root.display());
    let error = error.to_string();
    match record_scan_failure(root, generation, &error, services).await {
        Ok(true) => ScanEnded::Failed,
        Ok(false) => {
            debug!("{}'s failed scan was not stored: a newer scan owns the root", root.display());
            ScanEnded::Failed
        }
        Err(status_error) => {
            report_refused_scan_write(
                root,
                "store a watched folder's failed scan",
                &status_error,
                &services.diagnostics,
            );
            ScanEnded::FailedUnstored(UnstoredScanFailure { error, volume })
        }
    }
}

/// Open a durable scan generation for `root`, recording `volume`, the volume
/// the root was found on.
async fn begin_scan(
    root: &Path,
    volume: crate::VolumeKind,
    services: &crate::ImportServices,
) -> Result<u64, bae_store::DbError> {
    let _commit = services.folder_state_commit.lock("begin a scan").await;
    services.tables.begin_folder_scan(root, volume).await
}

/// The walk itself, under an open generation. Every error it returns is
/// recorded as the root's failure by its caller, so nothing in here has to
/// record its own.
async fn walk_and_reconcile(
    root: &Path,
    generation: u64,
    scan: &ScanServices,
    cancellation: &crate::folder_scanner::ScanCancellation,
) -> Result<(), crate::ImportError> {
    let services = &scan.services;
    let event_tx = &services.event_tx;
    let tables = &services.tables;
    // What the user has decided about each candidate's files — which audio
    // each sheet describes, and which files are the release's tracks — read
    // once for the whole walk. A folder's roles are only what its filenames
    // propose until these land on top, so the walk takes them with it
    // rather than having every candidate corrected afterwards.
    let stored_edits = tables.load_stored_candidate_edits().await?;
    let decisions = tables.load_folder_release_decisions(root).await?;
    // What the last pass over this root read of each file, so this one
    // opens only the files that are new or changed since, and what this
    // one reads, which the drafts of the releases it finds are seeded
    // from.
    let reads = Arc::new(tables.load_file_reads(root, services.file_tags.clone()).await?);
    let walk_reads = reads.clone();
    let root_buf = root.to_path_buf();
    let dropped_item_root = root.to_path_buf();
    let walk_cancellation = cancellation.clone();
    let walk_watcher = scan.folder_watcher.clone();
    let walk_root = root.to_path_buf();
    let directories = services.directories.clone();
    let ids = services.ids.clone();
    // What this pass wrote and what it displaced, for the log line at the
    // end. Two passes over an unchanged folder should displace nothing;
    // one that keeps rewriting the same entry names it here.
    let mut written_keys: Vec<crate::RelativeFolder> = Vec::new();
    let mut displaced_keys: Vec<crate::CandidateKey> = Vec::new();
    // Bound the duplicate candidate payloads waiting on the store's writes.
    // The blocking producer naturally pauses when the async consumer falls
    // behind, which caps memory on fast local trees.
    let (item_tx, mut item_rx) = mpsc::channel(SCAN_ITEMS_WAITING);
    let walk = tokio::task::spawn_blocking(move || {
        let mut seen_directories = HashSet::new();
        // When each directory was last touched, for the cheap check that
        // stands in for a walk of a network folder. One directory whose
        // mtime cannot be read makes the whole record useless — a check
        // that skips a directory would miss every change inside it — so a
        // single failure abandons the record and the next pass walks.
        let mut directory_mtimes: Option<Vec<(PathBuf, i64)>> = Some(Vec::new());
        let mut watch_available = true;
        let mut watch_failures = Vec::new();
        let new_key = || super::folder_reading::new_grouping_key(ids.as_ref());
        let result =
            crate::folder_scanner::scan_for_candidates_with_reader_cancellable_and_directories(
                directories.as_ref(),
                root_buf,
                &crate::folder_scanner::ScanReadings {
                    stored: &stored_edits,
                    decisions: &decisions,
                    new_grouping_key: &new_key,
                },
                &walk_reads,
                &walk_cancellation,
                |directory| {
                    match (directory_mtimes.as_mut(), directory_modified_at(&directory)) {
                        (Some(recorded), Some(modified_at)) => {
                            recorded.push((directory.clone(), modified_at))
                        }
                        (Some(_), None) => directory_mtimes = None,
                        (None, _) => {}
                    }
                    seen_directories.insert(directory.clone());
                    if watch_available {
                        if let Err(error) = walk_watcher.install_directory(&walk_root, &directory) {
                            if directory == walk_root {
                                watch_available = false;
                            }
                            watch_failures.push(format!("{}: {error}", directory.display()));
                        }
                    }
                },
                |item| {
                    // The receiver outlives the walk except when this scan has
                    // already bailed out (an added-state lookup failed) or the
                    // service is shutting down — in both cases there is nothing left
                    // to emit to, and the walk has no way to stop early.
                    if item_tx.blocking_send(item).is_err() {
                        debug!(
                            "scanned candidate dropped: no receiver for {}",
                            dropped_item_root.display()
                        );
                    }
                },
            );
        if !watch_failures.is_empty() {
            warn!(
                "folder watch unavailable for {} ({}); periodic and manual scans remain active",
                walk_root.display(),
                watch_failures.join(", ")
            );
        }
        if result.is_ok() && watch_available {
            if let Err(error) = walk_watcher.retain_directories(&walk_root, &seen_directories) {
                warn!("could not reconcile folder watches for {}: {error}", walk_root.display());
            }
        }
        (result, seen_directories, directory_mtimes)
    });

    // A stale generation is found out by the write: `save_folder_scan_items`
    // writes nothing and returns `None` once a newer scan or decision has
    // taken the root, and that is where this walk stops.
    while let Some(first) = item_rx.recv().await {
        // Every item the walk handed over while the last write ran is
        // stored in one write, so a walk that outpaces its writes commits
        // once for the items waiting rather than once for each.
        let mut waiting = vec![first];
        while waiting.len() < SCAN_ITEMS_WAITING {
            let Ok(item) = item_rx.try_recv() else {
                break;
            };
            waiting.push(item);
        }
        let mut items = Vec::with_capacity(waiting.len());
        for item in waiting {
            // How the walk read a folder nothing was stored for. It is the
            // folder's decision from here on, so the flip control on each
            // candidate it produced has something to rewrite — and a later
            // scan reads the folder the same way even if the naming rule
            // changes under it. A reading is stored as a decision, not as a
            // scan entry, after the items the walk yielded before it.
            let (key, decision, grouping) = match item {
                crate::folder_scanner::WalkItem::Entry(item) => {
                    items.push(item);
                    continue;
                }
                crate::folder_scanner::WalkItem::Reading { key, decision, grouping } => {
                    (key, decision, grouping)
                }
            };
            if !store_scan_items(
                root,
                generation,
                std::mem::take(&mut items),
                &reads,
                services,
                &mut written_keys,
                &mut displaced_keys,
            )
            .await?
            {
                cancel_and_join_folder_walk(
                    cancellation,
                    &mut item_rx,
                    walk,
                    &services.diagnostics,
                )
                .await?;
                return Ok(());
            }
            tables.record_scanned_folder_release_decision(&key, decision, &grouping).await?;
        }
        if !store_scan_items(
            root,
            generation,
            items,
            &reads,
            services,
            &mut written_keys,
            &mut displaced_keys,
        )
        .await?
        {
            cancel_and_join_folder_walk(cancellation, &mut item_rx, walk, &services.diagnostics)
                .await?;
            return Ok(());
        }
    }

    // The walk finished (or failed) once its sender dropped and closed the
    // channel above. A root read failure preserves the progressive items
    // already committed by this pass plus older items that were not
    // explicitly replaced.
    let walked = bae_util::task_failure::joined(FOLDER_WALK, walk)
        .await
        .map_err(|failed| services.diagnostics.bug(failed))?;
    let (seen_directories, directory_mtimes) = match walked {
        (Ok(()), seen_directories, directory_mtimes) => (seen_directories, directory_mtimes),
        // One of the walk's threads panicked; the rest stopped with it.
        (Err(crate::folder_scanner::FolderScanError::Panicked(bug)), _, _) => {
            return Err(TaskFailed::Panicked(services.diagnostics.panicked(bug)).into())
        }
        (Err(e), _, _) => return Err(e.into()),
    };

    drop(seen_directories);
    // What this pass saw, so the next one can ask whether anything moved
    // instead of walking. Cleared when the walk could not read every
    // directory, which leaves the next pass with nothing to conclude from
    // and so with a walk to do.
    tables.record_folder_scan_directories(root, &directory_mtimes.unwrap_or_default()).await?;

    // The generation check, pruning, and status change share one
    // transaction: a newer decision or scan cannot be pruned by this
    // completed write, and `None` says one took the root first.
    let commit = services.folder_state_commit.lock("finish a scan").await;
    let Some(crate::tables::FinishedScan { pruned, regrouped }) =
        tables.finish_folder_scan(root, generation, None).await?
    else {
        return Ok(());
    };
    announce_regrouped(&regrouped, services).await?;
    info!(
        "folder scan of {} wrote {} entries; displaced {:?}; pruned {:?}",
        root.display(),
        written_keys.len(),
        displaced_keys,
        pruned
    );
    for candidate_key in pruned {
        event_tx.send(crate::ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key }));
    }
    event_tx.send(crate::ImportEvent::Scan(ScanEvent::Finished));
    drop(commit);
    Ok(())
}

#[cfg(test)]
#[path = "scanning_tests.rs"]
mod tests;
