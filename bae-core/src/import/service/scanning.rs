use super::*;

impl ImportService {
    /// Mark `root`'s scan generation failed and say so. The stored status is
    /// what the import list's live query reads for the folder's mark; the
    /// event is the moment it happened, which the desktops raise as an alert.
    ///
    /// Returns `false` when the generation is no longer the root's — a newer
    /// scan owns the status and this one's failure is not the root's state.
    pub(super) async fn record_scan_failure(
        root: &Path,
        generation: u64,
        message: String,
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: &LibraryManager,
        folder_state_commit: &Arc<tokio::sync::Mutex<()>>,
    ) -> Result<bool, crate::import::ImportError> {
        let _commit = folder_state_commit.lock().await;
        if library_manager
            .finish_folder_scan(&root.to_string_lossy(), generation, Some(&message))
            .await?
            .is_none()
        {
            return Ok(false);
        }
        Self::announce_scan_failure(root, message, event_tx);
        Ok(true)
    }

    /// Say a scan of `root` failed, without a stored status behind it — the
    /// generation row could not be opened, or the status write on top of it
    /// failed. The user still has to hear that the folder they just added was
    /// not read, so the event goes out even when nothing durable can.
    fn announce_scan_failure(
        root: &Path,
        message: String,
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
    ) {
        let watched_folder =
            crate::import::WatchedFolder::from_path(root.to_string_lossy().into_owned());
        send_event(
            event_tx,
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status: crate::import::WatchedFolderScanStatus {
                    watched_folder_path: watched_folder.path,
                    watched_folder_name: watched_folder.name,
                    status: crate::import::FolderScanStatus::Failed { error: message },
                },
            }),
        );
    }

    pub(super) async fn persist_scan_item(
        root: &Path,
        generation: u64,
        item: &ScanItem,
        library_manager: &LibraryManager,
        folder_state_commit: &Arc<tokio::sync::Mutex<()>>,
    ) -> Result<Option<PersistedScanItem>, crate::import::ImportError> {
        let commit = folder_state_commit.clone().lock_owned().await;
        let mut item = item.clone();
        if let ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) = &mut item {
            let content_hash = candidate.files.content_hash();
            let edits = library_manager
                .load_candidate_file_edits(&content_hash)
                .await?;
            candidate.files.apply_candidate_file_edits(&edits)?;
            candidate.file_edit_revision = edits.revision;
        }
        let superseded = library_manager
            .save_folder_scan_item(&root.to_string_lossy(), generation, &item)
            .await?;
        Ok(superseded.map(|write| PersistedScanItem {
            commit,
            item,
            write,
        }))
    }

    pub(super) async fn cancel_and_join_folder_walk(
        root: &Path,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
        item_rx: &mut mpsc::Receiver<ScanItem>,
        walk: tokio::task::JoinHandle<(
            Result<(), crate::import::folder_scanner::FolderScanError>,
            HashSet<PathBuf>,
            Option<Vec<(String, i64)>>,
        )>,
    ) -> Result<(), crate::import::ImportError> {
        cancellation.cancel();
        while item_rx.recv().await.is_some() {}
        match walk.await {
            Ok(_) => Ok(()),
            Err(error) => {
                tracing::error!("folder scan task failed for {}: {error}", root.display());
                Err(crate::import::ImportError::Internal {
                    detail: format!("folder scan task failed: {error}"),
                })
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
    /// runs in [`Self::walk_and_reconcile`] and this records whatever error it
    /// returns, so no failure between opening a generation and finishing it
    /// can leave the root reading `scanning` forever with only a log line to
    /// say otherwise.
    pub(super) async fn rescan_and_reconcile(
        root: &Path,
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: &LibraryManager,
        folder_registry: &Arc<Mutex<ImportFolderRegistry>>,
        folder_state_commit: &Arc<tokio::sync::Mutex<()>>,
        folder_watcher: &Arc<FolderWatcher>,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<(), crate::import::ImportError> {
        let root_key = root.to_string_lossy().into_owned();
        let generation = match Self::begin_scan(
            root,
            &root_key,
            event_tx,
            library_manager,
            folder_state_commit,
        )
        .await
        {
            Ok(generation) => generation,
            Err(error) => {
                error!("folder scan of {} could not start: {error}", root.display());
                Self::announce_scan_failure(root, error.to_string(), event_tx);
                return Err(error);
            }
        };
        let outcome = Self::walk_and_reconcile(
            root,
            &root_key,
            generation,
            event_tx,
            library_manager,
            folder_registry,
            folder_state_commit,
            folder_watcher,
            cancellation,
        )
        .await;
        let Err(error) = outcome else {
            return Ok(());
        };
        // A cancelled scan is the coordinator taking the root away — a newer
        // scan or a removal — not a failure of this one.
        if cancellation.is_cancelled() {
            return Ok(());
        }
        warn!(
            "scan of {} failed ({error}); keeping previous candidates",
            root.display()
        );
        // Reported whether or not it can be stored. The stored status is what
        // the folder's mark reads back; the event is what raises the alert, and
        // a database that will not take the status is one more reason the user
        // needs to hear that their folder was not read.
        if let Err(status_error) = Self::record_scan_failure(
            root,
            generation,
            error.to_string(),
            event_tx,
            library_manager,
            folder_state_commit,
        )
        .await
        {
            error!(
                "{}'s failed scan could not be stored: {status_error}",
                root.display()
            );
            Self::announce_scan_failure(root, error.to_string(), event_tx);
        }
        Err(error)
    }

    /// Open a durable scan generation for `root` and say the walk has started.
    async fn begin_scan(
        root: &Path,
        root_key: &str,
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: &LibraryManager,
        folder_state_commit: &Arc<tokio::sync::Mutex<()>>,
    ) -> Result<u64, crate::import::ImportError> {
        let _commit = folder_state_commit.lock().await;
        let generation = library_manager.begin_folder_scan(root_key).await?;
        let watched_folder =
            crate::import::WatchedFolder::from_path(root.to_string_lossy().into_owned());
        send_event(
            event_tx,
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status: crate::import::WatchedFolderScanStatus {
                    watched_folder_path: watched_folder.path,
                    watched_folder_name: watched_folder.name,
                    status: crate::import::FolderScanStatus::Scanning,
                },
            }),
        );
        Ok(generation)
    }

    /// The walk itself, under an open generation. Every error it returns is
    /// recorded as the root's failure by its caller, so nothing in here has to
    /// record its own.
    #[allow(clippy::too_many_arguments)]
    async fn walk_and_reconcile(
        root: &Path,
        root_key: &str,
        generation: u64,
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: &LibraryManager,
        folder_registry: &Arc<Mutex<ImportFolderRegistry>>,
        folder_state_commit: &Arc<tokio::sync::Mutex<()>>,
        folder_watcher: &Arc<FolderWatcher>,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<(), crate::import::ImportError> {
        // What the user has decided about each candidate's files — which audio
        // each sheet describes, and which files are the release's tracks — read
        // once for the whole walk. A folder's roles are only what its filenames
        // propose until these land on top, so the walk takes them with it
        // rather than having every candidate corrected afterwards.
        let stored_edits = library_manager.load_stored_candidate_edits().await?;
        let decisions = library_manager
            .load_folder_release_decisions(root_key)
            .await?;
        let root_buf = root.to_path_buf();
        let dropped_item_root = root.to_path_buf();
        let walk_cancellation = cancellation.clone();
        let walk_watcher = folder_watcher.clone();
        let walk_root = root.to_path_buf();
        // What this pass wrote and what it displaced, for the log line at the
        // end. Two passes over an unchanged folder should displace nothing;
        // one that keeps rewriting the same entry names it here.
        let mut written_keys: Vec<String> = Vec::new();
        let mut displaced_keys: Vec<String> = Vec::new();
        // Bound the duplicate candidate payloads waiting on per-item DB commits.
        // The blocking producer naturally pauses when the async consumer falls
        // behind, which caps memory on fast local trees.
        let (item_tx, mut item_rx) = mpsc::channel(8);
        let walk = tokio::task::spawn_blocking(move || {
            let mut seen_directories = HashSet::new();
            // When each directory was last touched, for the cheap check that
            // stands in for a walk of a network folder. One directory whose
            // mtime cannot be read makes the whole record useless — a check
            // that skips a directory would miss every change inside it — so a
            // single failure abandons the record and the next pass walks.
            let mut directory_mtimes: Option<Vec<(String, i64)>> = Some(Vec::new());
            let mut watch_available = true;
            let mut watch_failures = Vec::new();
            let result = crate::import::folder_scanner::scan_for_candidates_with_decisions_cancellable_and_directories(
                root_buf,
                &stored_edits,
                &decisions,
                &walk_cancellation,
                |directory| {
                    match (directory_mtimes.as_mut(), directory_modified_at(&directory)) {
                        (Some(recorded), Some(modified_at)) => recorded.push((
                            directory.to_string_lossy().into_owned(),
                            modified_at,
                        )),
                        (Some(_), None) => directory_mtimes = None,
                        (None, _) => {}
                    }
                    seen_directories.insert(directory.clone());
                    if watch_available {
                        if let Err(error) =
                            walk_watcher.install_directory(&walk_root, &directory)
                        {
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
                    warn!(
                        "could not reconcile folder watches for {}: {error}",
                        walk_root.display()
                    );
                }
            }
            (result, seen_directories, directory_mtimes)
        });

        // A stale generation is found out by the write: `save_folder_scan_item`
        // writes nothing and returns `None` once a newer scan or decision has
        // taken the root, and that is where this walk stops.
        while let Some(item) = item_rx.recv().await {
            match item {
                item @ (ScanItem::Discovered(_) | ScanItem::Valid(_)) => {
                    let (candidate, actionable) = match item {
                        ScanItem::Discovered(candidate) => (candidate, false),
                        ScanItem::Valid(candidate) => (candidate, true),
                        ScanItem::Invalid(_) | ScanItem::Boundary(_) | ScanItem::Decided { .. } => {
                            unreachable!("matched candidate scan item")
                        }
                    };
                    // The walk yields folder facts. Registry skip state and
                    // imported content hashes are joined here before the item
                    // is persisted and announced.
                    let persisted_item = if actionable {
                        ScanItem::Valid(candidate.clone())
                    } else {
                        ScanItem::Discovered(candidate.clone())
                    };
                    let Some(persisted) = Self::persist_scan_item(
                        root,
                        generation,
                        &persisted_item,
                        library_manager,
                        folder_state_commit,
                    )
                    .await?
                    else {
                        Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk)
                            .await?;
                        return Ok(());
                    };
                    let PersistedScanItem {
                        commit: _commit,
                        item: persisted_item,
                        write,
                    } = persisted;
                    let candidate = match persisted_item {
                        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => candidate,
                        ScanItem::Invalid(_) | ScanItem::Boundary(_) | ScanItem::Decided { .. } => {
                            unreachable!("persisted candidate scan item changed variant")
                        }
                    };
                    if !write.changed() {
                        continue;
                    }
                    let superseded_keys = write.superseded_keys().to_vec();
                    written_keys.push(candidate.display_path.clone());
                    displaced_keys.extend(superseded_keys.iter().cloned());
                    let skipped = folder_registry
                        .lock()
                        .unwrap()
                        .is_skipped(&candidate.watched_folder_path, &candidate.path)?;
                    let is_added = library_manager
                        .is_content_hash_imported(&candidate.files.content_hash())
                        .await?;
                    for candidate_key in superseded_keys {
                        send_event(
                            event_tx,
                            crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                                candidate_key,
                            }),
                        );
                    }
                    send_event(
                        event_tx,
                        crate::import::handle::ImportEvent::Scan(if actionable {
                            ScanEvent::FolderCandidate {
                                candidate,
                                skipped,
                                is_added,
                            }
                        } else {
                            ScanEvent::CandidateDiscovered {
                                candidate,
                                skipped,
                                is_added,
                            }
                        }),
                    );
                }
                // Invalid candidates have no tab state, so they need no stamping.
                ScanItem::Invalid(candidate) => {
                    let persisted_item = ScanItem::Invalid(candidate.clone());
                    let Some(persisted) = Self::persist_scan_item(
                        root,
                        generation,
                        &persisted_item,
                        library_manager,
                        folder_state_commit,
                    )
                    .await?
                    else {
                        Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk)
                            .await?;
                        return Ok(());
                    };
                    let PersistedScanItem {
                        commit: _commit,
                        item: _,
                        write,
                    } = persisted;
                    if !write.changed() {
                        continue;
                    }
                    written_keys.push(candidate.display_path.clone());
                    let superseded_keys = write.superseded_keys().to_vec();
                    for candidate_key in superseded_keys {
                        displaced_keys.push(candidate_key.clone());
                        send_event(
                            event_tx,
                            crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                                candidate_key,
                            }),
                        );
                    }
                    send_event(
                        event_tx,
                        crate::import::handle::ImportEvent::Scan(ScanEvent::InvalidCandidate(
                            candidate,
                        )),
                    );
                }
                ScanItem::Boundary(boundary) => {
                    let persisted_item = ScanItem::Boundary(boundary.clone());
                    let Some(persisted) = Self::persist_scan_item(
                        root,
                        generation,
                        &persisted_item,
                        library_manager,
                        folder_state_commit,
                    )
                    .await?
                    else {
                        Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk)
                            .await?;
                        return Ok(());
                    };
                    let PersistedScanItem {
                        commit: _commit,
                        item: _,
                        write,
                    } = persisted;
                    if !write.changed() {
                        continue;
                    }
                    written_keys.push(boundary.display_path.clone());
                    let superseded_keys = write.superseded_keys().to_vec();
                    for candidate_key in superseded_keys {
                        displaced_keys.push(candidate_key.clone());
                        send_event(
                            event_tx,
                            crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                                candidate_key,
                            }),
                        );
                    }
                    send_event(
                        event_tx,
                        crate::import::handle::ImportEvent::Scan(ScanEvent::FolderReleaseBoundary(
                            boundary,
                        )),
                    );
                }
                // How the walk read a folder nothing was stored for. It is the
                // folder's decision from here on, so the flip control on each
                // candidate it produced has something to rewrite — and a later
                // scan reads the folder the same way even if the naming rule
                // changes under it.
                ScanItem::Decided { key, decision } => {
                    library_manager
                        .record_scanned_folder_release_decision(&key, decision)
                        .await?;
                }
            }
        }

        // The walk finished (or failed) once its sender dropped and closed the
        // channel above. A root read failure preserves the progressive items
        // already committed by this pass plus older items that were not
        // explicitly replaced.
        let (seen_directories, directory_mtimes) = match walk.await {
            Ok((Ok(()), seen_directories, directory_mtimes)) => {
                (seen_directories, directory_mtimes)
            }
            Ok((Err(e), _, _)) => return Err(e.into()),
            Err(e) => {
                return Err(crate::import::ImportError::Internal {
                    detail: format!("folder scan task panicked: {e}"),
                })
            }
        };

        drop(seen_directories);
        // What this pass saw, so the next one can ask whether anything moved
        // instead of walking. Cleared when the walk could not read every
        // directory, which leaves the next pass with nothing to conclude from
        // and so with a walk to do.
        library_manager
            .record_folder_scan_directories(root_key, &directory_mtimes.unwrap_or_default())
            .await?;

        // The generation check, pruning, and status change share one
        // transaction: a newer decision or scan cannot be pruned by this
        // completed write, and `None` says one took the root first.
        let commit = folder_state_commit.clone().lock_owned().await;
        let Some(pruned) = library_manager
            .finish_folder_scan(root_key, generation, None)
            .await?
        else {
            return Ok(());
        };
        info!(
            "folder scan of {} wrote {} entries; displaced {:?}; pruned {:?}",
            root.display(),
            written_keys.len(),
            displaced_keys,
            pruned
        );
        for candidate_key in pruned {
            send_event(
                event_tx,
                crate::import::handle::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                    candidate_key,
                }),
            );
        }
        let watched_folder =
            crate::import::WatchedFolder::from_path(root.to_string_lossy().into_owned());
        send_event(
            event_tx,
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status: crate::import::WatchedFolderScanStatus {
                    watched_folder_path: watched_folder.path,
                    watched_folder_name: watched_folder.name,
                    status: crate::import::FolderScanStatus::Complete,
                },
            }),
        );

        send_event(
            event_tx,
            crate::import::handle::ImportEvent::Scan(ScanEvent::Finished),
        );
        drop(commit);
        Ok(())
    }
}
