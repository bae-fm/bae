//! Deciding when a watched root is read: from commands, filesystem events,
//! watch failures, and the periodic check of folders on network volumes.
//!
//! A whole root is read only when something asks for all of it; a change on
//! disk reads again only the folders directly under the root that it reached.

use super::*;

impl ImportService {
    /// The folder-watch coordinator. The store is the one authority on what
    /// is watched, so events from a watch left on a removed folder match
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
            diagnostics,
            ..
        } = services;
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("folder scan coordinator runtime");
            runtime.block_on(async move {
                let (active_roots, mut scan_completion_rx, mut removal_completion_rx) =
                    ActiveRoots::new(
                        starter,
                        removal_backend,
                        folder_state_commit.clone(),
                        diagnostics.clone(),
                    );
                // The network check runs off the loop, since it can take
                // seconds; `checking` keeps one per root at a time.
                let (checked_tx, mut checked_rx) =
                    mpsc::unbounded_channel::<(PathBuf, Option<Vec<PathBuf>>)>();
                let mut coordinator = Coordinator {
                    active_roots,
                    checking: HashSet::new(),
                    checked_tx,
                    event_tx,
                    library_manager,
                    diagnostics,
                };
                let period = crate::import::volume::CHECK_PERIOD;
                let mut periodic =
                    tokio::time::interval_at(tokio::time::Instant::now() + period, period);
                periodic.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    let event = tokio::select! {
                        biased;

                        cmd = cmd_rx.recv() => match cmd {
                            None => {
                                coordinator.active_roots.cancel_scans();
                                break;
                            }
                            Some(WatcherCommand::Shutdown { completion }) => {
                                // The thread ends here whatever the shutdown
                                // does, since its owner joins it next.
                                if let Err(failed) = bae_util::task_failure::caught(
                                    "stop the folder scan coordinator",
                                    coordinator.active_roots.shutdown(),
                                )
                                .await
                                {
                                    coordinator.diagnostics.bug(failed);
                                }
                                if completion.send(()).is_err() {
                                    debug!("import handle dropped during folder scan shutdown");
                                }
                                return;
                            }
                            Some(command) => CoordinatorEvent::Command(command),
                        },
                        Some(completion) = removal_completion_rx.recv() => {
                            CoordinatorEvent::RemovalFinished(completion)
                        }
                        Some(completion) = scan_completion_rx.recv() => {
                            CoordinatorEvent::ScanFinished(completion)
                        }
                        Some(report) = fs_rx.recv() => CoordinatorEvent::WatchReport(report),
                        Some((root, changes)) = checked_rx.recv() => {
                            CoordinatorEvent::NetworkChecked { root, changes }
                        }
                        _ = periodic.tick() => CoordinatorEvent::Tick,
                    };
                    // One event that panics is reported and ends as its own
                    // failure would: a caller waiting on it hears the wait end
                    // when its answer is dropped. The coordinator goes on to
                    // the next.
                    let operation = event.operation();
                    if let Err(failed) =
                        bae_util::task_failure::caught(operation, coordinator.take(event)).await
                    {
                        coordinator.diagnostics.bug(failed);
                    }
                }
            });
        })
    }
}

/// One thing the coordinator's loop takes up, apart from stopping.
enum CoordinatorEvent {
    Command(WatcherCommand),
    RemovalFinished(active_roots::RootRemovalCompletion),
    ScanFinished(RootScanCompletion),
    WatchReport(WatchReport),
    NetworkChecked {
        root: PathBuf,
        changes: Option<Vec<PathBuf>>,
    },
    Tick,
}

impl CoordinatorEvent {
    /// What taking it up does, as its failure names it.
    fn operation(&self) -> &'static str {
        match self {
            Self::Command(_) => "take a folder scan command",
            Self::RemovalFinished(_) => "finish removing a watched folder",
            Self::ScanFinished(_) => "finish a pass over a watched folder",
            Self::WatchReport(_) => "take a folder watch report",
            Self::NetworkChecked { .. } => "take a network folder's check",
            Self::Tick => "check the network folders",
        }
    }
}

/// What the coordinator's loop holds between events.
struct Coordinator {
    active_roots: ActiveRoots,
    /// The network roots whose check is under way.
    checking: HashSet<PathBuf>,
    checked_tx: mpsc::UnboundedSender<(PathBuf, Option<Vec<PathBuf>>)>,
    event_tx: crate::import::handle::ImportEventBus,
    library_manager: LibraryManager,
    diagnostics: crate::diagnostics::Diagnostics,
}

impl Coordinator {
    async fn take(&mut self, event: CoordinatorEvent) {
        match event {
            CoordinatorEvent::Command(command) => self.take_command(command).await,
            CoordinatorEvent::RemovalFinished(completion) => {
                self.finish_removal(completion).await;
            }
            CoordinatorEvent::ScanFinished(completion) => {
                self.active_roots.finish_scan(completion).await;
            }
            CoordinatorEvent::WatchReport(report) => {
                take_watch_report(
                    report,
                    &self.library_manager,
                    &mut self.active_roots,
                    &self.diagnostics,
                )
                .await;
            }
            CoordinatorEvent::NetworkChecked { root, changes } => {
                self.checking.remove(&root);
                let change = match changes {
                    None => RootChange::WholeRoot,
                    Some(changes) => {
                        let changes: Vec<&Path> = changes.iter().map(PathBuf::as_path).collect();
                        let holds = holds_its_own_release(&self.library_manager, &root).await;
                        root_change_of(&root, &changes, holds, &self.diagnostics).await
                    }
                };
                request_change(
                    &mut self.active_roots,
                    root,
                    change,
                    RootScanCause::NetworkFolderMoved,
                );
            }
            CoordinatorEvent::Tick => self.check_network_folders().await,
        }
    }

    async fn take_command(&mut self, command: WatcherCommand) {
        let library_manager = &self.library_manager;
        let active_roots = &mut self.active_roots;
        match command {
            WatcherCommand::RescanAll => {
                for root in watched_roots(library_manager).await {
                    active_roots.request_scan(
                        root,
                        RootScanCause::Asked("every watched folder was asked for"),
                        None,
                    );
                }
            }
            WatcherCommand::Rescan(path) => {
                match is_watched(library_manager, &path).await {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        error!(
                            "could not read whether {} is watched, so it is not read again: \
                             {error}",
                            path.display()
                        );
                        return;
                    }
                }
                active_roots.request_scan(
                    path,
                    RootScanCause::Asked("a rescan was asked for"),
                    None,
                );
            }
            WatcherCommand::Refresh { path, completion } => {
                let refused = match is_watched(library_manager, &path).await {
                    Ok(true) => None,
                    Ok(false) => Some(format!("{} is no longer watched", path.display())),
                    Err(error) => Some(format!(
                        "could not read whether {} is watched: {error}",
                        path.display()
                    )),
                };
                if let Some(refused) = refused {
                    if completion.send(Err(refused)).is_err() {
                        debug!("folder refresh caller dropped after removal");
                    }
                    return;
                }
                active_roots.request_scan(
                    path,
                    RootScanCause::Asked("the folder was refreshed"),
                    Some(completion),
                );
            }
            WatcherCommand::SetFolderReleaseDecision { target, completion } => {
                let path = PathBuf::from(&target.0.watched_folder_path);
                active_roots
                    .change_folder_reading(path, FolderReadingRequest::new(target, completion));
            }
            WatcherCommand::Remove {
                roots,
                parent,
                completion,
            } => {
                active_roots.remove(roots, parent, completion);
            }
            #[cfg(test)]
            WatcherCommand::WatchReport { report, taken } => {
                take_watch_report(report, library_manager, active_roots, &self.diagnostics).await;
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "a test that stopped waiting needs no answer"
                )]
                let _ = taken.send(());
            }
            WatcherCommand::Shutdown { .. } => {
                unreachable!("the coordinator's loop stops for a shutdown before taking it up")
            }
        }
    }

    async fn finish_removal(&mut self, completion: active_roots::RootRemovalCompletion) {
        let Some(outcome) = self.active_roots.finish_removal(completion).await else {
            return;
        };
        match outcome {
            RemovalOutcome::Removed {
                commit,
                removed_keys,
                scan_waiters,
                callers,
            } => {
                let folders = watched_folders(&self.library_manager).await;
                for candidate_key in removed_keys {
                    self.event_tx.send(crate::import::handle::ImportEvent::Scan(
                        ScanEvent::CandidateRemoved { candidate_key },
                    ));
                }
                self.event_tx.send(crate::import::handle::ImportEvent::Scan(
                    ScanEvent::WatchedFoldersChanged { folders },
                ));
                for (path, waiter) in scan_waiters {
                    if waiter
                        .send(Err(format!("{} is no longer watched", path.display())))
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

    /// Start the check of every network root not already being checked or
    /// removed.
    async fn check_network_folders(&mut self) {
        let roots = watched_roots(&self.library_manager).await;
        for root in roots {
            if self.active_roots.is_being_removed(&root) {
                continue;
            }
            // A local folder's watch reports every change; a network folder
            // has only this check.
            if volume_kind(&root, &self.diagnostics).await == VolumeKind::Local {
                continue;
            }
            if !self.checking.insert(root.clone()) {
                continue;
            }
            let manager = self.library_manager.clone();
            let diagnostics = self.diagnostics.clone();
            let answer = self.checked_tx.clone();
            let panicked_answer = self.checked_tx.clone();
            let checked_root = root.clone();
            // A check that panics answers "read the whole root", so the root
            // leaves `checking` and is read.
            self.diagnostics.spawn_reported(
                &tokio::runtime::Handle::current(),
                "check a network folder",
                async move {
                    let recorded = manager.load_folder_scan_directories(&checked_root).await;
                    let recorded = match recorded {
                        Ok(recorded) => recorded,
                        Err(error) => {
                            warn!(
                                "could not read what the last scan of {} saw: {error}",
                                checked_root.display()
                            );
                            Vec::new()
                        }
                    };
                    let answer_root = checked_root.clone();
                    let changes = match bae_util::task_failure::blocking(
                        "check a network folder",
                        move || network_changes(&answer_root, &recorded),
                    )
                    .await
                    {
                        Ok(changes) => changes,
                        Err(failed @ bae_util::task_failure::TaskFailed::Panicked { .. }) => {
                            diagnostics.bug(failed);
                            None
                        }
                        // The runtime is shutting down under the check;
                        // nothing is left to ask.
                        Err(failed) => {
                            debug!(
                                "the check of {} ended unfinished: {failed}",
                                checked_root.display()
                            );
                            return;
                        }
                    };
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
                },
                move |_| {
                    if panicked_answer.send((root, None)).is_err() {
                        debug!("folder scan coordinator ended before a panicked check landed");
                    }
                },
            );
        }
    }
}

/// Ask for the reads a filesystem watch's report calls for.
async fn take_watch_report(
    report: WatchReport,
    library_manager: &LibraryManager,
    active_roots: &mut ActiveRoots,
    diagnostics: &crate::diagnostics::Diagnostics,
) {
    let events = match report {
        Ok(events) => events,
        Err(errors) => {
            let roots = watched_roots(library_manager).await;
            let mut error_paths = Vec::new();
            for e in errors {
                error_paths.extend(e.paths.iter().cloned());
                warn!("folder watcher error: {e}");
            }
            let affected = roots_for_watch_error(&error_paths, &roots);
            for root in affected {
                active_roots.request_scan(root, RootScanCause::WatchError, None);
            }
            return;
        }
    };
    // A watch that lost track names where to start: no path,
    // or one holding a root, reads the whole root; one inside
    // a root reads that folder again.
    let roots = watched_roots(library_manager).await;
    let mut whole: HashSet<PathBuf> = HashSet::new();
    for event in events.iter().filter(|event| event.need_rescan()) {
        if event.paths.is_empty() {
            whole.extend(roots.iter().cloned());
        }
        for path in &event.paths {
            whole.extend(roots.iter().filter(|root| root.starts_with(path)).cloned());
        }
    }
    for root in &whole {
        active_roots.request_scan(root.clone(), RootScanCause::EventsDropped, None);
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
            event.need_rescan() && event.paths.iter().any(|path| path.starts_with(&root))
        });
        let holds = holds_its_own_release(library_manager, &root).await;
        let change = root_change_of(&root, &under, holds, diagnostics).await;
        request_change(
            active_roots,
            root.clone(),
            change,
            if lost_track {
                RootScanCause::EventsDropped
            } else {
                RootScanCause::FsChange(summary.clone())
            },
        );
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

/// Whether `root` holds tracks of its own. A failed read answers yes, which
/// reads the whole root: right either way.
async fn holds_its_own_release(library_manager: &LibraryManager, root: &Path) -> bool {
    match library_manager
        .load_folder_scan_item(&crate::import::CandidateKey::folder(root))
        .await
    {
        Ok(item) => item.is_some(),
        Err(error) => {
            warn!(
                "could not read whether {} holds a release of its own: {error}",
                coven::path_text(root)
            );
            true
        }
    }
}

/// What the store lists as watched; a failed read is logged and lists nothing.
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
        .map(|folder| folder.path)
        .collect()
}

/// Whether `path` is a watched folder, by the watched folders' key.
async fn is_watched(
    library_manager: &LibraryManager,
    path: &Path,
) -> Result<bool, crate::library::LibraryError> {
    library_manager.is_watched_import_folder(path).await
}
