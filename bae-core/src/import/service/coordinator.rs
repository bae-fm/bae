//! Deciding when a watched root is read.
//!
//! Every way a scan can be asked for arrives here: a command, a filesystem
//! event, a watch failure, the periodic tick, and — for a folder on a network
//! volume, which has no watch worth the name — the cheap check that stands in
//! for walking it. What each root has going is [`ActiveRoots`]'s; this decides
//! what to ask it for.
//!
//! Reading a root is [`super::scanning`]'s; this decides that it happens.

use super::*;

impl ImportService {
    /// The folder-watch reconciliation task. A `Rescan` command re-scans a folder
    /// (the handle sends one right after installing the folder's OS watch, and on
    /// every `scan_watched_folders` call), and a debounced filesystem change under
    /// a watched folder re-scans it too. Every re-scan reconciles what it finds
    /// against the candidates already recorded for that folder —
    /// `FolderCandidate` for what's on disk, `CandidateRemoved` for what's gone —
    /// so changes propagate beyond the first scan.
    ///
    /// OS watch installation lives in `FolderWatcher`, owned by the handle; this
    /// task only receives the `fs_rx` batches its callback forwards. The store,
    /// not a task-local set, is the single authority on what's watched:
    /// `affected_roots` resolves each event batch against what it lists, so
    /// events from a watch left installed on a since-removed folder match
    /// nothing.
    pub(super) fn start_watcher(
        cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
        fs_rx: mpsc::UnboundedReceiver<DebounceEventResult>,
        event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: LibraryManager,
        preparations: crate::import::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdRef,
        folder_state_commit: Arc<tokio::sync::Mutex<()>>,
        folder_watcher: Arc<FolderWatcher>,
    ) -> std::thread::JoinHandle<()> {
        let scan_event_tx = event_tx.clone();
        let scan_library_manager = library_manager.clone();
        let scan_preparations = preparations;
        let scan_clock = clock.clone();
        let scan_ids = ids.clone();
        let scan_folder_state_commit = folder_state_commit.clone();
        let scan_folder_watcher = folder_watcher.clone();
        let removal_backend = Arc::new(ServiceRootRemovalBackend::new(
            folder_watcher.clone(),
            library_manager.clone(),
        ));
        let starter: RootScanStarter = Arc::new(move |id, path, completion_tx| {
            spawn_root_scan(
                id,
                path,
                scan_event_tx.clone(),
                scan_library_manager.clone(),
                scan_preparations.clone(),
                scan_clock.clone(),
                scan_ids.clone(),
                scan_folder_state_commit.clone(),
                scan_folder_watcher.clone(),
                completion_tx,
            )
        });
        Self::start_watcher_with_starter(
            cmd_rx,
            fs_rx,
            event_tx,
            library_manager,
            folder_state_commit,
            starter,
            removal_backend,
        )
    }

    pub(super) fn start_watcher_with_starter(
        mut cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
        mut fs_rx: mpsc::UnboundedReceiver<DebounceEventResult>,
        event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: LibraryManager,
        folder_state_commit: Arc<tokio::sync::Mutex<()>>,
        starter: RootScanStarter,
        removal_backend: Arc<dyn RootRemovalBackend>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("folder scan coordinator runtime");
            runtime.block_on(async move {
            let (mut active_roots, mut scan_completion_rx, mut removal_completion_rx) =
                ActiveRoots::new(starter, removal_backend, folder_state_commit.clone());
            // A root on a network volume answers the cheap check off the
            // coordinator, because asking 500 directories over SMB whether they
            // have moved takes seconds and the loop has commands to serve
            // meanwhile. The answer comes back here, and only a "yes" becomes a
            // scan. One check per root at a time: `checking` is what says one is
            // already out.
            let (checked_tx, mut checked_rx) = mpsc::unbounded_channel::<PathBuf>();
            let mut checking: HashSet<PathBuf> = HashSet::new();
            let period = crate::import::volume::CHECK_PERIOD;
            let mut periodic = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
            periodic.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;

                    cmd = cmd_rx.recv() => {
                        let Some(cmd) = cmd else {
                            active_roots.cancel_scans();
                            break;
                        };
                        match cmd {
                            WatcherCommand::RescanAll => {
                                for root in watched_roots(&library_manager).await {
                                    active_roots.request_scan(
                                        root,
                                        RootScanCause::Asked("every watched folder was asked for"),
                                        None,
                                    );
                                }
                            }
                            WatcherCommand::Rescan(path) => {
                                if active_roots.is_being_removed(&path) {
                                    continue;
                                }
                                if !is_watched(&library_manager, &path).await {
                                    continue;
                                }
                                active_roots.request_scan(
                                    path,
                                    RootScanCause::Asked("a rescan was asked for"),
                                    None,
                                );
                            }
                            WatcherCommand::Refresh { path, completion } => {
                                if active_roots.is_being_removed(&path) {
                                    if completion
                                        .send(Err(format!(
                                            "{} is being removed",
                                            path.display()
                                        )))
                                        .is_err()
                                    {
                                        debug!("folder refresh caller dropped during removal");
                                    }
                                    continue;
                                }
                                if !is_watched(&library_manager, &path).await {
                                    if completion
                                        .send(Err(format!(
                                            "{} is no longer watched",
                                            path.display()
                                        )))
                                        .is_err()
                                    {
                                        debug!("folder refresh caller dropped after removal");
                                    }
                                    continue;
                                }
                                active_roots.request_scan(
                                    path,
                                    RootScanCause::Asked("the folder was refreshed"),
                                    Some(completion),
                                );
                            }
                            WatcherCommand::SetFolderReleaseDecision {
                                target,
                                completion,
                            } => {
                                let path = PathBuf::from(&target.0.watched_folder_path);
                                if active_roots.is_being_removed(&path) {
                                    if completion
                                        .send(Err(format!(
                                            "{} is being removed",
                                            path.display()
                                        )))
                                        .is_err()
                                    {
                                        debug!("folder decision caller dropped during removal");
                                    }
                                    continue;
                                }
                                // What a pass over this folder is reading is
                                // about to change, so it is replaced by one that
                                // reads the decision this is about to write.
                                active_roots.requeue_scan(&path);
                                let _commit = folder_state_commit.lock().await;
                                let stored_items = match library_manager
                                    .load_folder_scan_items(&target.0.watched_folder_path)
                                    .await
                                {
                                    Ok(items) => items,
                                    Err(error) => {
                                        if completion.send(Err(error.to_string())).is_err() {
                                            debug!("folder decision caller dropped before the stored scan was read");
                                        }
                                        continue;
                                    }
                                };
                                if !crate::import::candidates::names_a_current_folder_reading(
                                    &stored_items,
                                    &target.0,
                                ) {
                                    if completion
                                        .send(Err(format!(
                                            "{} is not a current release boundary",
                                            target.0.relative_folder_path
                                        )))
                                        .is_err()
                                    {
                                        debug!("folder decision caller dropped before validation");
                                    }
                                    continue;
                                };
                                let decisions = vec![target];
                                match library_manager
                                    .set_folder_release_decisions(&decisions)
                                    .await
                                {
                                    Ok((_, superseded)) => {
                                        for candidate_key in superseded {
                                            send_event(
                                                &event_tx,
                                                crate::import::handle::ImportEvent::Scan(
                                                    ScanEvent::CandidateRemoved {
                                                        candidate_key,
                                                    },
                                                ),
                                            );
                                        }
                                        active_roots.wait_for_next_scan(
                                            path,
                                            RootScanCause::Asked(
                                                "a folder release decision changed",
                                            ),
                                            completion,
                                        );
                                    }
                                    Err(error) => {
                                        if completion.send(Err(error.to_string())).is_err() {
                                            debug!("folder decision caller dropped before persistence failed");
                                        }
                                    }
                                }
                            }
                            WatcherCommand::Remove { path, completion } => {
                                active_roots.remove(path, completion);
                            }
                            WatcherCommand::Shutdown { completion } => {
                                active_roots.shutdown().await;
                                if completion.send(()).is_err() {
                                    debug!("import handle dropped during folder scan shutdown");
                                }
                                return;
                            }
                        }
                    }
                    Some(completion) = removal_completion_rx.recv() => {
                        let Some(outcome) = active_roots.finish_removal(completion).await else {
                            continue;
                        };
                        match outcome {
                            RemovalOutcome::Removed {
                                path,
                                commit,
                                removed_keys,
                                scan_waiters,
                                callers,
                            } => {
                                let folders = watched_folders(&library_manager).await;
                                for candidate_key in removed_keys {
                                    send_event(
                                        &event_tx,
                                        crate::import::handle::ImportEvent::Scan(
                                            ScanEvent::CandidateRemoved { candidate_key },
                                        ),
                                    );
                                }
                                send_event(
                                    &event_tx,
                                    crate::import::handle::ImportEvent::Scan(
                                        ScanEvent::WatchedFoldersChanged { folders },
                                    ),
                                );
                                for waiter in scan_waiters {
                                    if waiter
                                        .send(Err(format!(
                                            "{} is no longer watched",
                                            path.display()
                                        )))
                                        .is_err()
                                    {
                                        debug!("folder refresh caller dropped during removal");
                                    }
                                }
                                drop(commit);
                                for caller in callers {
                                    if caller.send(Ok(())).is_err() {
                                        debug!("folder removal caller dropped before completion");
                                    }
                                }
                            }
                            RemovalOutcome::Failed { error, callers } => {
                                for caller in callers {
                                    if caller.send(Err(error.clone())).is_err() {
                                        debug!("folder removal caller dropped before failure");
                                    }
                                }
                            }
                        }
                    }
                    Some(completion) = scan_completion_rx.recv() => {
                        active_roots.finish_scan(completion).await;
                    }
                    Some(result) = fs_rx.recv() => {
                        let events = match result {
                            Ok(events) => events,
                            Err(errors) => {
                                let roots = watched_roots(&library_manager).await;
                                let mut error_paths = Vec::new();
                                for e in errors {
                                    error_paths.extend(e.paths.iter().cloned());
                                    warn!("folder watcher error: {e}");
                                }
                                let affected = roots_for_watch_error(&error_paths, &roots);
                                for root in affected {
                                    active_roots.request_scan(
                                        root,
                                        RootScanCause::WatchError,
                                        None,
                                    );
                                }
                                continue;
                            }
                        };
                        let changed = changed_paths(&events);
                        let roots = watched_roots(&library_manager).await;
                        let affected = affected_roots(&changed, &roots);
                        if !affected.is_empty() {
                            let summary = changed_events_summary(&events);
                            for root in affected {
                                active_roots.request_scan(
                                    root,
                                    RootScanCause::FsChange(summary.clone()),
                                    None,
                                );
                            }
                        }
                    }
                    Some(root) = checked_rx.recv() => {
                        checking.remove(&root);
                        active_roots.request_scan(root, RootScanCause::NetworkFolderMoved, None);
                    }
                    _ = periodic.tick() => {
                        let roots = watched_roots(&library_manager).await;
                        for root in roots {
                            if active_roots.is_being_removed(&root) {
                                continue;
                            }
                            // A folder on this machine's own disk has a watch
                            // that reports every change to it, so the tick is
                            // only a backstop and re-reading is what it does. A
                            // folder on a network volume has no such watch: the
                            // tick is the only thing that will notice, and it
                            // asks the cheap question first rather than walking
                            // a share every quarter of an hour to learn nothing.
                            if volume_kind(&root) == VolumeKind::Local {
                                active_roots.request_scan(root, RootScanCause::Timer, None);
                                continue;
                            }
                            if !checking.insert(root.clone()) {
                                continue;
                            }
                            let manager = library_manager.clone();
                            let answer = checked_tx.clone();
                            let checked_root = root.clone();
                            tokio::spawn(async move {
                                let recorded = manager
                                    .load_folder_scan_directories(
                                        &checked_root.to_string_lossy(),
                                    )
                                    .await;
                                let recorded = match recorded {
                                    Ok(recorded) => recorded,
                                    Err(error) => {
                                        warn!(
                                            "could not read what the last scan of {} saw: \
                                             {error}",
                                            checked_root.display()
                                        );
                                        Vec::new()
                                    }
                                };
                                let moved = tokio::task::spawn_blocking(move || {
                                    directories_changed(&recorded)
                                })
                                .await
                                .unwrap_or(true);
                                if !moved {
                                    debug!(
                                        "network folder {} is as the last scan left it",
                                        checked_root.display()
                                    );
                                    return;
                                }
                                if answer.send(checked_root).is_err() {
                                    debug!("folder scan coordinator ended before a check landed");
                                }
                            });
                        }
                    }
                }
            }
            });
        })
    }
}

/// What the store lists as watched. A read that fails is logged and answers
/// nothing: there is nothing to schedule against, and the next trigger reads
/// the store again.
async fn watched_folders(library_manager: &LibraryManager) -> Vec<crate::import::WatchedFolder> {
    match library_manager.load_watched_import_folders().await {
        Ok(folders) => folders,
        Err(error) => {
            error!("could not read the watched folders: {error}");
            Vec::new()
        }
    }
}

async fn watched_roots(library_manager: &LibraryManager) -> Vec<PathBuf> {
    watched_folders(library_manager)
        .await
        .into_iter()
        .map(|folder| PathBuf::from(folder.path))
        .collect()
}

async fn is_watched(library_manager: &LibraryManager, path: &Path) -> bool {
    watched_folders(library_manager)
        .await
        .iter()
        .any(|folder| folder.path == path.to_string_lossy())
}
