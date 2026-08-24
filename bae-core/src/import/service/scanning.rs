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

    /// The folder-watch reconciliation task. A `Rescan` command re-scans a folder
    /// (the handle sends one right after installing the folder's OS watch, and on
    /// every `scan_watched_folders` call), and a debounced filesystem change under
    /// a watched folder re-scans it too. Every re-scan reconciles what it finds
    /// against the candidates already recorded for that folder —
    /// `FolderCandidate` for what's on disk, `CandidateRemoved` for what's gone —
    /// so changes propagate beyond the first scan.
    ///
    /// OS watch installation lives in `FolderWatcher`, owned by the handle; this
    /// task only receives the `fs_rx` batches its callback forwards. The registry,
    /// not a task-local set, is the single authority on what's watched:
    /// `affected_roots` resolves each event batch against it, so events from a
    /// watch left installed on a since-removed folder match nothing.
    pub(super) fn start_watcher(
        cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
        fs_rx: mpsc::UnboundedReceiver<DebounceEventResult>,
        event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: LibraryManager,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        folder_state_commit: Arc<tokio::sync::Mutex<()>>,
        folder_watcher: Arc<FolderWatcher>,
    ) -> std::thread::JoinHandle<()> {
        let scan_event_tx = event_tx.clone();
        let scan_library_manager = library_manager.clone();
        let scan_folder_registry = folder_registry.clone();
        let scan_folder_state_commit = folder_state_commit.clone();
        let scan_folder_watcher = folder_watcher.clone();
        let removal_backend = Arc::new(ServiceRootRemovalBackend {
            folder_watcher: folder_watcher.clone(),
            library_manager: library_manager.clone(),
        });
        let starter: RootScanStarter = Arc::new(move |id, path, completion_tx| {
            spawn_root_scan(
                id,
                path,
                scan_event_tx.clone(),
                scan_library_manager.clone(),
                scan_folder_registry.clone(),
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
            folder_registry,
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
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
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
            let (completion_tx, mut completion_rx) = mpsc::unbounded_channel();
            let (removal_tx, mut removal_rx) = mpsc::unbounded_channel();
            let mut schedules: HashMap<PathBuf, RootScanSchedule> = HashMap::new();
            let mut removals: HashMap<PathBuf, RootRemovalSchedule> = HashMap::new();
            let mut next_scan_id = 0;
            let mut next_removal_id = 0;
            let period = std::time::Duration::from_secs(15 * 60);
            let mut periodic = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
            periodic.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;

                    cmd = cmd_rx.recv() => {
                        let Some(cmd) = cmd else {
                            for schedule in schedules.values() {
                                schedule.scan.cancellation.cancel();
                            }
                            break;
                        };
                        match cmd {
                            WatcherCommand::Rescan(path) => {
                                if removals.contains_key(&path) {
                                    continue;
                                }
                                if !folder_registry
                                    .lock()
                                    .unwrap()
                                    .watched_folders()
                                    .iter()
                                    .any(|folder| folder.path == path.to_string_lossy())
                                {
                                    continue;
                                }
                                request_root_scan(
                                    path,
                                    RootScanCause::Asked("a rescan was asked for"),
                                    None,
                                    &mut schedules,
                                    &starter,
                                    &completion_tx,
                                    &mut next_scan_id,
                                );
                            }
                            WatcherCommand::Refresh { path, completion } => {
                                if removals.contains_key(&path) {
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
                                if !folder_registry
                                    .lock()
                                    .unwrap()
                                    .watched_folders()
                                    .iter()
                                    .any(|folder| folder.path == path.to_string_lossy())
                                {
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
                                request_root_scan(
                                    path,
                                    RootScanCause::Asked("the folder was refreshed"),
                                    Some(completion),
                                    &mut schedules,
                                    &starter,
                                    &completion_tx,
                                    &mut next_scan_id,
                                );
                            }
                            WatcherCommand::SetFolderReleaseDecision {
                                target,
                                completion,
                            } => {
                                let path = PathBuf::from(&target.0.watched_folder_path);
                                if removals.contains_key(&path) {
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
                                if let Some(schedule) = schedules.get_mut(&path) {
                                    schedule.scan.cancellation.cancel();
                                    schedule.pending = true;
                                    schedule
                                        .followup_waiters
                                        .append(&mut schedule.current_waiters);
                                }
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
                                let Some(ancestor_separate_keys) =
                                    crate::import::candidates::release_boundary_ancestor_keys(&stored_items, &target.0)
                                else {
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
                                let mut decisions = vec![target];
                                decisions.extend(ancestor_separate_keys.into_iter().map(|key| {
                                    (
                                        key,
                                        crate::import::folder_scanner::FolderReleaseDecision::KeepAsSeparateReleases,
                                    )
                                }));
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
                                        if let Some(schedule) = schedules.get_mut(&path) {
                                            schedule.followup_waiters.push(completion);
                                        } else {
                                            request_root_scan(
                                                path,
                                                RootScanCause::Asked(
                                                    "a folder release decision changed",
                                                ),
                                                Some(completion),
                                                &mut schedules,
                                                &starter,
                                                &completion_tx,
                                                &mut next_scan_id,
                                            );
                                        }
                                    }
                                    Err(error) => {
                                        if completion.send(Err(error.to_string())).is_err() {
                                            debug!("folder decision caller dropped before persistence failed");
                                        }
                                    }
                                }
                            }
                            WatcherCommand::Remove { path, completion } => {
                                if let Some(removal) = removals.get_mut(&path) {
                                    removal.completions.push(completion);
                                    continue;
                                }
                                let (scan, scan_waiters) = if let Some(mut schedule) = schedules.remove(&path) {
                                    schedule.scan.cancellation.cancel();
                                    let waiters = schedule
                                        .current_waiters
                                        .drain(..)
                                        .chain(schedule.followup_waiters.drain(..))
                                        .collect();
                                    (Some(schedule.scan), waiters)
                                } else {
                                    (None, Vec::new())
                                };
                                next_removal_id += 1;
                                let id = next_removal_id;
                                let removal_path = path.clone();
                                let removal_backend = removal_backend.clone();
                                let removal_commit = folder_state_commit.clone();
                                let removal_tx = removal_tx.clone();
                                let task = tokio::spawn(async move {
                                    let result = run_root_removal(
                                        &removal_path,
                                        scan,
                                        removal_backend.as_ref(),
                                        removal_commit,
                                    )
                                    .await;
                                    if removal_tx
                                        .send(RootRemovalCompletion {
                                            id,
                                            path: removal_path,
                                            result,
                                        })
                                        .is_err()
                                    {
                                        debug!("folder scan coordinator ended before removal completion");
                                    }
                                });
                                removals.insert(path, RootRemovalSchedule {
                                    id,
                                    task,
                                    completions: vec![completion],
                                    scan_waiters,
                                });
                            }
                            WatcherCommand::Shutdown { completion } => {
                                for schedule in schedules.values_mut() {
                                    schedule.scan.cancellation.cancel();
                                    schedule.pending = false;
                                    for waiter in schedule
                                        .current_waiters
                                        .drain(..)
                                        .chain(schedule.followup_waiters.drain(..))
                                    {
                                        if waiter
                                            .send(Err("folder scan service stopped".to_string()))
                                            .is_err()
                                        {
                                            debug!("folder refresh caller dropped during shutdown");
                                        }
                                    }
                                }
                                for (_, schedule) in schedules.drain() {
                                    if let Err(error) = schedule.scan.task.await {
                                        error!("folder scan task failed during shutdown: {error}");
                                    }
                                }
                                for (_, mut removal) in removals.drain() {
                                    if let Err(error) = removal.task.await {
                                        error!("folder removal task failed during shutdown: {error}");
                                    }
                                    for waiter in removal
                                        .scan_waiters
                                        .drain(..)
                                        .chain(removal.completions.drain(..))
                                    {
                                        if waiter
                                            .send(Err("folder scan service stopped".to_string()))
                                            .is_err()
                                        {
                                            debug!("folder caller dropped during shutdown");
                                        }
                                    }
                                }
                                if completion.send(()).is_err() {
                                    debug!("import handle dropped during folder scan shutdown");
                                }
                                return;
                            }
                        }
                    }
                    Some(completion) = removal_rx.recv() => {
                        if removals
                            .get(&completion.path)
                            .is_none_or(|removal| removal.id != completion.id)
                        {
                            continue;
                        }
                        let Some(removal) = removals.remove(&completion.path) else {
                            continue;
                        };
                        if let Err(error) = removal.task.await {
                            error!(
                                "folder removal task failed for {}: {error}",
                                completion.path.display()
                            );
                        }
                        match completion.result {
                            RootRemovalResult::Removed {
                                commit,
                                removed_keys,
                            } => {
                                let folders = {
                                    let mut registry = folder_registry.lock().unwrap();
                                    registry.apply_removed(&completion.path.to_string_lossy());
                                    registry.watched_folders()
                                };
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
                                for waiter in removal.scan_waiters {
                                    if waiter
                                        .send(Err(format!(
                                            "{} is no longer watched",
                                            completion.path.display()
                                        )))
                                        .is_err()
                                    {
                                        debug!("folder refresh caller dropped during removal");
                                    }
                                }
                                drop(commit);
                                for caller in removal.completions {
                                    if caller.send(Ok(())).is_err() {
                                        debug!("folder removal caller dropped before completion");
                                    }
                                }
                            }
                            RootRemovalResult::Failed(error) => {
                                next_scan_id += 1;
                                let scan = starter(
                                    next_scan_id,
                                    completion.path.clone(),
                                    completion_tx.clone(),
                                );
                                schedules.insert(
                                    completion.path,
                                    RootScanSchedule {
                                        id: next_scan_id,
                                        scan,
                                        pending: false,
                                        current_waiters: removal.scan_waiters,
                                        followup_waiters: Vec::new(),
                                    },
                                );
                                for caller in removal.completions {
                                    if caller.send(Err(error.clone())).is_err() {
                                        debug!("folder removal caller dropped before failure");
                                    }
                                }
                            }
                        }
                    }
                    Some(completion) = completion_rx.recv() => {
                        if schedules
                            .get(&completion.path)
                            .is_none_or(|schedule| schedule.id != completion.id)
                        {
                            continue;
                        }
                        let Some(mut schedule) = schedules.remove(&completion.path) else {
                            continue;
                        };
                        if let Err(error) = schedule.scan.task.await {
                            error!(
                                "folder scan task failed for {}: {error}",
                                completion.path.display()
                            );
                        }
                        // The scan itself is what said whether it worked. A
                        // refresh caller is only waiting for it to be over.
                        for waiter in schedule.current_waiters.drain(..) {
                            if waiter.send(Ok(())).is_err() {
                                debug!("folder refresh caller dropped before completion");
                            }
                        }
                        if schedule.pending {
                            let path = completion.path;
                            info!(
                                "folder scan of {} starting again: one was queued while it ran",
                                path.display()
                            );
                            let current_waiters = std::mem::take(&mut schedule.followup_waiters);
                            next_scan_id += 1;
                            let scan = starter(next_scan_id, path.clone(), completion_tx.clone());
                            schedules.insert(path, RootScanSchedule {
                                id: next_scan_id,
                                scan,
                                pending: false,
                                current_waiters,
                                followup_waiters: Vec::new(),
                            });
                        }
                    }
                    Some(result) = fs_rx.recv() => {
                        let events = match result {
                            Ok(events) => events,
                            Err(errors) => {
                                let roots: Vec<PathBuf> = folder_registry
                                    .lock()
                                    .unwrap()
                                    .watched_folders()
                                    .into_iter()
                                    .map(|folder| PathBuf::from(folder.path))
                                    .collect();
                                let mut error_paths = Vec::new();
                                for e in errors {
                                    error_paths.extend(e.paths.iter().cloned());
                                    warn!("folder watcher error: {e}");
                                }
                                let affected = roots_for_watch_error(&error_paths, &roots);
                                for root in affected {
                                    if removals.contains_key(&root) {
                                        continue;
                                    }
                                    request_root_scan(
                                        root,
                                        RootScanCause::WatchError,
                                        None,
                                        &mut schedules,
                                        &starter,
                                        &completion_tx,
                                        &mut next_scan_id,
                                    );
                                }
                                continue;
                            }
                        };
                        let changed = changed_paths(&events);
                        let roots: Vec<PathBuf> = folder_registry
                            .lock()
                            .unwrap()
                            .watched_folders()
                            .into_iter()
                            .map(|folder| PathBuf::from(folder.path))
                            .collect();
                        let affected = affected_roots(&changed, &roots);
                        if !affected.is_empty() {
                            let summary = changed_events_summary(&events);
                            for root in affected {
                                if removals.contains_key(&root) {
                                    continue;
                                }
                                request_root_scan(
                                    root,
                                    RootScanCause::FsChange(summary.clone()),
                                    None,
                                    &mut schedules,
                                    &starter,
                                    &completion_tx,
                                    &mut next_scan_id,
                                );
                            }
                        }
                    }
                    _ = periodic.tick() => {
                        let roots: Vec<PathBuf> = folder_registry
                            .lock()
                            .unwrap()
                            .watched_folders()
                            .into_iter()
                            .map(|folder| PathBuf::from(folder.path))
                            .collect();
                        for root in roots {
                            if removals.contains_key(&root) {
                                continue;
                            }
                            request_root_scan(
                                root,
                                RootScanCause::Timer,
                                None,
                                &mut schedules,
                                &starter,
                                &completion_tx,
                                &mut next_scan_id,
                            );
                        }
                    }
                }
            }
            });
        })
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
            let mut watch_available = true;
            let mut watch_failures = Vec::new();
            let result = crate::import::folder_scanner::scan_for_candidates_with_decisions_cancellable_and_directories(
                root_buf,
                &stored_edits,
                &decisions,
                &walk_cancellation,
                |directory| {
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
            (result, seen_directories)
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
        let seen_directories = match walk.await {
            Ok((Ok(()), seen_directories)) => seen_directories,
            Ok((Err(e), _)) => return Err(e.into()),
            Err(e) => {
                return Err(crate::import::ImportError::Internal {
                    detail: format!("folder scan task panicked: {e}"),
                })
            }
        };

        drop(seen_directories);

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
