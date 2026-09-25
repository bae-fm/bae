//! Deciding when a watched root is read.
//!
//! Every way a scan can be asked for arrives here: a command, a filesystem
//! event, a watch failure or a watch that lost track, and — for a folder on a
//! network volume, which has no watch worth the name — the periodic cheap
//! check that stands in for walking it. What each root has going is
//! [`ActiveRoots`]'s; this decides what to ask it for.
//!
//! A whole root is read only when something asks for all of it: the folder
//! was added or refreshed, the app started, or the watch failed or lost track.
//! A change on disk, or one the cheap check found, reads again only the
//! folders directly under the root that it reached (see [`root_change`]).
//!
//! Reading a root is [`super::scanning`]'s; this decides that it happens.

use super::*;

impl ImportService {
    /// The folder-watch reconciliation task. A `Rescan` command re-scans a folder
    /// (the handle sends one right after installing the folder's OS watch, and on
    /// every `scan_watched_folders` call), and a gathered filesystem change
    /// under a watched folder reads again the folders it reached. Every re-scan
    /// reconciles what it finds against the candidates already recorded for
    /// that folder —
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
        fs_rx: mpsc::UnboundedReceiver<WatchReport>,
        scan: ScanServices,
    ) -> std::thread::JoinHandle<()> {
        let removal_backend = Arc::new(ServiceRootRemovalBackend::new(
            scan.folder_watcher.clone(),
            scan.services.library_manager.clone(),
        ));
        let scan_for_starter = scan.clone();
        let starter: RootScanStarter = Arc::new(move |id, path, pass, completion_tx| {
            spawn_root_pass(id, path, pass, scan_for_starter.clone(), completion_tx)
        });
        Self::start_watcher_with_starter(cmd_rx, fs_rx, scan.services, starter, removal_backend)
    }

    pub(super) fn start_watcher_with_starter(
        mut cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
        mut fs_rx: mpsc::UnboundedReceiver<WatchReport>,
        services: crate::import::ImportServices,
        starter: RootScanStarter,
        removal_backend: Arc<dyn RootRemovalBackend>,
    ) -> std::thread::JoinHandle<()> {
        let crate::import::ImportServices {
            event_tx,
            library_manager,
            folder_state_commit,
            ..
        } = services;
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
            let (checked_tx, mut checked_rx) =
                mpsc::unbounded_channel::<(PathBuf, Option<Vec<PathBuf>>)>();
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
                                active_roots.change_folder_reading(
                                    path,
                                    FolderReadingRequest::new(target, completion),
                                );
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
                                    event_tx.send(crate::import::handle::ImportEvent::Scan(
                                            ScanEvent::CandidateRemoved { candidate_key },
                                        ),
                                    );
                                }
                                event_tx.send(crate::import::handle::ImportEvent::Scan(
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
                        // A backend that lost track of what changed — FSEvents
                        // dropping events, inotify's queue overflowing — says so
                        // with an event of its own, and the path it names is
                        // where to start: everything under it may have changed
                        // unseen. One naming a path inside a root reads that
                        // folder again, like any change there; one naming no
                        // path, or a path that holds the root, reads the root.
                        let roots = watched_roots(&library_manager).await;
                        let mut whole: HashSet<PathBuf> = HashSet::new();
                        for event in events.iter().filter(|event| event.need_rescan()) {
                            if event.paths.is_empty() {
                                whole.extend(roots.iter().cloned());
                            }
                            for path in &event.paths {
                                whole.extend(
                                    roots.iter().filter(|root| root.starts_with(path)).cloned(),
                                );
                            }
                        }
                        for root in &whole {
                            active_roots.request_scan(
                                root.clone(),
                                RootScanCause::EventsDropped,
                                None,
                            );
                        }
                        let changed = changed_paths(&events);
                        let affected = affected_roots(&changed, &roots);
                        let summary = changed_events_summary(&events);
                        for root in affected {
                            if whole.contains(&root) {
                                continue;
                            }
                            let under: Vec<&Path> = changed
                                .iter()
                                .copied()
                                .filter(|path| path.starts_with(&root))
                                .collect();
                            let lost_track = events.iter().any(|event| {
                                event.need_rescan()
                                    && event.paths.iter().any(|path| path.starts_with(&root))
                            });
                            let holds = holds_its_own_release(&library_manager, &root).await;
                            request_change(
                                &mut active_roots,
                                root.clone(),
                                root_change(&root, &under, holds),
                                if lost_track {
                                    RootScanCause::EventsDropped
                                } else {
                                    RootScanCause::FsChange(summary.clone())
                                },
                            );
                        }
                    }
                    Some((root, changes)) = checked_rx.recv() => {
                        checking.remove(&root);
                        let change = match changes {
                            None => RootChange::WholeRoot,
                            Some(changes) => {
                                let changes: Vec<&Path> =
                                    changes.iter().map(PathBuf::as_path).collect();
                                let holds = holds_its_own_release(&library_manager, &root).await;
                                root_change(&root, &changes, holds)
                            }
                        };
                        request_change(
                            &mut active_roots,
                            root,
                            change,
                            RootScanCause::NetworkFolderMoved,
                        );
                    }
                    _ = periodic.tick() => {
                        let roots = watched_roots(&library_manager).await;
                        for root in roots {
                            if active_roots.is_being_removed(&root) {
                                continue;
                            }
                            // A folder on this machine's own disk has a watch
                            // that reports every change to it, and says so
                            // when it loses track, so there is nothing for the
                            // tick to do. A folder on a network volume has no
                            // such watch: the tick is the only thing that will
                            // notice, and it asks the cheap question first
                            // rather than walking a share every quarter of an
                            // hour to learn nothing.
                            if volume_kind(&root) == VolumeKind::Local {
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
                                let answer_root = checked_root.clone();
                                let changes = tokio::task::spawn_blocking(move || {
                                    network_changes(&answer_root, &recorded)
                                })
                                .await
                                .unwrap_or(None);
                                if changes.as_ref().is_some_and(Vec::is_empty) {
                                    debug!(
                                        "network folder {} is as the last scan left it",
                                        checked_root.display()
                                    );
                                    return;
                                }
                                if answer.send((checked_root, changes)).is_err() {
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

/// Ask for what `change` says `root` needs read.
fn request_change(
    active_roots: &mut ActiveRoots,
    root: PathBuf,
    change: RootChange,
    cause: RootScanCause,
) {
    match change {
        RootChange::WholeRoot => active_roots.request_scan(root, cause, None),
        RootChange::Folders(folders) => {
            if folders.is_empty() {
                debug!(
                    "a change under {} reaches nothing a scan reads: {cause}",
                    root.display()
                );
            }
            active_roots.request_folders(root, folders, cause);
        }
    }
}

/// Whether `root` holds tracks of its own. A store that cannot say is read as
/// yes, which reads the whole root rather than one folder: the answer that is
/// right either way.
async fn holds_its_own_release(library_manager: &LibraryManager, root: &Path) -> bool {
    let root_key = root.to_string_lossy();
    match library_manager.load_folder_scan_item(&root_key).await {
        Ok(item) => item.is_some(),
        Err(error) => {
            warn!("could not read whether {root_key} holds a release of its own: {error}");
            true
        }
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
