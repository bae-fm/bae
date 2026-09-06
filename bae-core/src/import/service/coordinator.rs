//! Deciding when a watched root is read.
//!
//! The coordinator owns one schedule per root and one removal per root, and
//! every way a scan can be asked for arrives here: a command, a filesystem
//! event, a watch failure, the periodic tick, and — for a folder on a network
//! volume, which has no watch worth the name — the cheap check that stands in
//! for walking it.
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
    /// task only receives the `fs_rx` batches its callback forwards. The registry,
    /// not a task-local set, is the single authority on what's watched:
    /// `affected_roots` resolves each event batch against it, so events from a
    /// watch left installed on a since-removed folder match nothing.
    pub(super) fn start_watcher(
        cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
        fs_rx: mpsc::UnboundedReceiver<DebounceEventResult>,
        event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: LibraryManager,
        preparations: crate::import::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdRef,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        folder_state_commit: Arc<tokio::sync::Mutex<()>>,
        folder_watcher: Arc<FolderWatcher>,
    ) -> std::thread::JoinHandle<()> {
        let scan_event_tx = event_tx.clone();
        let scan_library_manager = library_manager.clone();
        let scan_preparations = preparations;
        let scan_clock = clock.clone();
        let scan_ids = ids.clone();
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
                scan_preparations.clone(),
                scan_clock.clone(),
                scan_ids.clone(),
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
                    Some(root) = checked_rx.recv() => {
                        checking.remove(&root);
                        if removals.contains_key(&root) {
                            continue;
                        }
                        request_root_scan(
                            root,
                            RootScanCause::NetworkFolderMoved,
                            None,
                            &mut schedules,
                            &starter,
                            &completion_tx,
                            &mut next_scan_id,
                        );
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
                            // A folder on this machine's own disk has a watch
                            // that reports every change to it, so the tick is
                            // only a backstop and re-reading is what it does. A
                            // folder on a network volume has no such watch: the
                            // tick is the only thing that will notice, and it
                            // asks the cheap question first rather than walking
                            // a share every quarter of an hour to learn nothing.
                            if volume_kind(&root) == VolumeKind::Local {
                                request_root_scan(
                                    root,
                                    RootScanCause::Timer,
                                    None,
                                    &mut schedules,
                                    &starter,
                                    &completion_tx,
                                    &mut next_scan_id,
                                );
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
