//! Deciding when a watched root is read: from commands, filesystem events,
//! watch failures, and the periodic check of folders on network volumes.
//!
//! A whole root is read only when something asks for all of it; a change on
//! disk reads again only the folders directly under the root that it reached.

use super::*;

/// Commands to the folder scan coordinator.
pub(crate) enum WatcherCommand {
    /// Read every watched folder the store lists. `completion` hears once
    /// each read is asked for, or why the folders could not be listed.
    RescanAll {
        completion: tokio::sync::oneshot::Sender<Result<(), bae_store::DbError>>,
    },
    Rescan(std::path::PathBuf),
    Refresh {
        path: std::path::PathBuf,
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// Read one folder as a person decided. `completion` hears the keys of
    /// the releases the folder now reads as once they are stored, or why they
    /// were not.
    SetFolderReleaseDecision {
        target: (
            crate::folder_scanner::FolderReleaseDecisionKey,
            crate::folder_scanner::FolderReleaseDecision,
        ),
        completion: tokio::sync::oneshot::Sender<Result<Vec<crate::CandidateKey>, String>>,
    },
    /// Stop watching `roots` and, given `parent` (the folder holding exactly
    /// them), watch it in their place. `completion` hears once the change is
    /// stored, or why it was not.
    Remove {
        roots: Vec<std::path::PathBuf>,
        parent: Option<std::path::PathBuf>,
        completion: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// Take `report` as if the filesystem watch had sent it; `taken` hears
    /// once the reads it calls for are asked for.
    #[cfg(test)]
    WatchReport {
        report: crate::watching::WatchReport,
        taken: tokio::sync::oneshot::Sender<()>,
    },
    Shutdown {
        completion: std::sync::mpsc::Sender<()>,
    },
}

/// The folder-watch coordinator. The store is the one authority on what
/// is watched, so events from a watch left on a removed folder match
/// nothing. What it holds of each root's scan that no table does goes out on
/// `in_memory`.
pub(crate) fn start_watcher(
    cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
    fs_rx: mpsc::UnboundedReceiver<WatchReport>,
    services: crate::ImportServices,
    folder_watcher: Arc<FolderWatcher>,
    in_memory: tokio::sync::watch::Sender<crate::folder_scan_status::FolderScansInMemory>,
) -> std::thread::JoinHandle<()> {
    let removal_backend = Arc::new(ServiceRootRemovalBackend::new(
        folder_watcher.clone(),
        services.tables.clone(),
        services.diagnostics.clone(),
    ));
    let scan = ScanServices::new(services.clone(), folder_watcher);
    let starter: RootScanStarter = Arc::new(move |id, path, pass, completion_tx| {
        spawn_root_pass(id, path, pass, scan.clone(), completion_tx)
    });
    start_watcher_with_starter(cmd_rx, fs_rx, services, starter, removal_backend, in_memory)
}

pub(super) fn start_watcher_with_starter(
    mut cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
    mut fs_rx: mpsc::UnboundedReceiver<WatchReport>,
    services: crate::ImportServices,
    starter: RootScanStarter,
    removal_backend: Arc<dyn RootRemovalBackend>,
    in_memory: tokio::sync::watch::Sender<crate::folder_scan_status::FolderScansInMemory>,
) -> std::thread::JoinHandle<()> {
    let crate::ImportServices { event_tx, tables, folder_state_commit, diagnostics, clock, .. } =
        services;
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("folder scan coordinator runtime");
        runtime.block_on(async move {
            let (scan_completions, mut scan_completion_rx) = mpsc::unbounded_channel();
            let (removal_completions, mut removal_completion_rx) = mpsc::unbounded_channel();
            let (checked_tx, mut checked_rx) = mpsc::unbounded_channel();
            let active_roots = ActiveRoots::new(
                starter,
                removal_backend,
                folder_state_commit.clone(),
                diagnostics.clone(),
                scan_completions,
                removal_completions,
                in_memory,
            );
            let mut coordinator =
                Coordinator::new(active_roots, event_tx, tables, diagnostics, checked_tx);
            // The next check of the network folders: one period after the
            // coordinator starts, then one period after each check is taken.
            let mut next_check = clock.deadline(crate::volume::CHECK_PERIOD);
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
                                coordinator.diagnostics.panicked(failed);
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
                    Some((root, found)) = checked_rx.recv() => {
                        CoordinatorEvent::NetworkChecked { root, found }
                    }
                    () = &mut next_check => {
                        next_check = clock.deadline(crate::volume::CHECK_PERIOD);
                        CoordinatorEvent::Tick
                    }
                };
                // One event that panics is reported and ends as its own
                // failure would: a caller waiting on it hears the wait end
                // when its answer is dropped. The coordinator goes on to
                // the next.
                let operation = event.operation();
                if let Err(failed) =
                    bae_util::task_failure::caught(operation, coordinator.take(event)).await
                {
                    coordinator.diagnostics.panicked(failed);
                }
            }
        });
    })
}

/// One thing the coordinator's loop takes up, apart from stopping.
pub(super) enum CoordinatorEvent {
    Command(WatcherCommand),
    RemovalFinished(active_roots::RootRemovalCompletion),
    ScanFinished(RootScanCompletion),
    WatchReport(WatchReport),
    NetworkChecked { root: PathBuf, found: NetworkCheck },
    Tick,
}

/// What the check of one network folder found. Every check answers with one,
/// and the answer is what lets the folder be checked again.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum NetworkCheck {
    /// The folder is as the last scan left it.
    Unchanged,
    /// What moved, in the form a filesystem event gives it, or `None` when
    /// the whole folder has to be read.
    Moved(Option<Vec<PathBuf>>),
    /// The check ended unfinished: its runtime is shutting down under it, or
    /// it could not read what the last scan saw.
    Unfinished,
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
pub(super) struct Coordinator {
    active_roots: ActiveRoots,
    /// The network roots whose check is under way. The check runs off the
    /// loop, since it can take seconds, and one runs per root at a time.
    checking: HashSet<PathBuf>,
    checked_tx: mpsc::UnboundedSender<(PathBuf, NetworkCheck)>,
    event_tx: crate::ImportEventBus,
    tables: crate::tables::ImportTables,
    diagnostics: bae_diagnostics::Diagnostics,
}

impl Coordinator {
    /// The coordinator over `active_roots`, sending the answers of the
    /// network checks it starts to `checked_tx`, which the loop hands back
    /// to [`Self::take`].
    pub(super) fn new(
        active_roots: ActiveRoots,
        event_tx: crate::ImportEventBus,
        tables: crate::tables::ImportTables,
        diagnostics: bae_diagnostics::Diagnostics,
        checked_tx: mpsc::UnboundedSender<(PathBuf, NetworkCheck)>,
    ) -> Self {
        Self { active_roots, checking: HashSet::new(), checked_tx, event_tx, tables, diagnostics }
    }

    pub(super) async fn take(&mut self, event: CoordinatorEvent) {
        match event {
            CoordinatorEvent::Command(command) => self.take_command(command).await,
            CoordinatorEvent::RemovalFinished(completion) => {
                self.finish_removal(completion).await;
            }
            CoordinatorEvent::ScanFinished(completion) => {
                self.active_roots.finish_scan(completion).await;
            }
            CoordinatorEvent::WatchReport(report) => {
                take_watch_report(report, &self.tables, &mut self.active_roots, &self.diagnostics)
                    .await;
            }
            CoordinatorEvent::NetworkChecked { root, found } => {
                self.checking.remove(&root);
                let changes = match found {
                    NetworkCheck::Moved(changes) => changes,
                    NetworkCheck::Unchanged | NetworkCheck::Unfinished => return,
                };
                let change = match changes {
                    None => RootChange::WholeRoot,
                    Some(changes) => {
                        let changes: Vec<&Path> = changes.iter().map(PathBuf::as_path).collect();
                        let holds = holds_its_own_release(&self.tables, &root).await;
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
        let tables = &self.tables;
        let active_roots = &mut self.active_roots;
        match command {
            WatcherCommand::RescanAll { completion } => {
                let roots = match watched_roots(tables).await {
                    Ok(roots) => roots,
                    Err(error) => {
                        if completion.send(Err(error)).is_err() {
                            debug!("rescan caller dropped before the watched folders failed");
                        }
                        return;
                    }
                };
                for root in roots {
                    active_roots.request_scan(
                        root,
                        RootScanCause::Asked("every watched folder was asked for"),
                        None,
                    );
                }
                if completion.send(Ok(())).is_err() {
                    debug!("rescan caller dropped before the reads were asked for");
                }
            }
            WatcherCommand::Rescan(path) => {
                match tables.is_watched_import_folder(&path).await {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        self.diagnostics.report_failure(
                            &format!("check whether {} is watched before a rescan", path.display()),
                            &error,
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
                let refused = match tables.is_watched_import_folder(&path).await {
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
            WatcherCommand::Remove { roots, parent, completion } => {
                active_roots.remove(roots, parent, completion);
            }
            #[cfg(test)]
            WatcherCommand::WatchReport { report, taken } => {
                take_watch_report(report, tables, active_roots, &self.diagnostics).await;
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
            RemovalOutcome::Removed { commit, removed, scan_waiters, callers } => {
                let crate::tables::WatchedFoldersRemoved { removed_keys, watched: folders } =
                    removed;
                for candidate_key in removed_keys {
                    self.event_tx.send(crate::ImportEvent::Scan(ScanEvent::CandidateRemoved {
                        candidate_key,
                    }));
                }
                self.event_tx
                    .send(crate::ImportEvent::Scan(ScanEvent::WatchedFoldersChanged { folders }));
                for (path, waiter) in scan_waiters {
                    if waiter.send(Err(format!("{} is no longer watched", path.display()))).is_err()
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

    /// Start the check of every watched root whose last scan found it on a
    /// network volume, unless it is already being checked or removed.
    ///
    /// The volume is the one the scan recorded, which is the one the import
    /// list shows, and reading it never waits on a mount. A local folder's
    /// watch reports every change; a network folder has only this check, and
    /// a folder no scan has opened has nothing recorded to check against.
    async fn check_network_folders(&mut self) {
        let scans = match self.tables.load_stored_folder_scans().await {
            Ok(scans) => scans,
            Err(error) => {
                self.diagnostics.report_failure("list the watched folders to check them", &error);
                return;
            }
        };
        for root in scans {
            if !root.last_scan.is_some_and(|scan| scan.on_network_volume) {
                continue;
            }
            if self.active_roots.is_being_removed(&root.watched_folder_path) {
                continue;
            }
            self.start_network_check(root.watched_folder_path);
        }
    }

    /// Start checking the network folder `root` against what its last scan
    /// saw, unless a check of it is under way. Whether one started.
    pub(super) fn start_network_check(&mut self, root: PathBuf) -> bool {
        if !self.checking.insert(root.clone()) {
            return false;
        }
        let tables = self.tables.clone();
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
                let found = check_network_folder(&tables, &checked_root, &diagnostics).await;
                if answer.send((checked_root, found)).is_err() {
                    debug!("folder scan coordinator ended before a check landed");
                }
            },
            move |_| {
                if panicked_answer.send((root, NetworkCheck::Moved(None))).is_err() {
                    debug!("folder scan coordinator ended before a panicked check landed");
                }
            },
        );
        true
    }
}

/// What the network folder `root` holds against what its last scan saw.
async fn check_network_folder(
    tables: &crate::tables::ImportTables,
    root: &Path,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> NetworkCheck {
    let recorded = match tables.load_folder_scan_directories(root).await {
        Ok(recorded) => recorded,
        Err(error) => {
            diagnostics.report_failure(
                &format!("read what the last scan of {} saw", root.display()),
                &error,
            );
            return NetworkCheck::Unfinished;
        }
    };
    let checked_root = root.to_path_buf();
    let changes = match diagnostics
        .blocking("check a network folder", move || network_changes(&checked_root, &recorded))
        .await
    {
        Ok(changes) => changes,
        // Reported as it failed.
        Err(bae_util::task_failure::TaskFailed::Panicked(_)) => None,
        Err(failed) => {
            debug!("the check of {} ended unfinished: {failed}", root.display());
            return NetworkCheck::Unfinished;
        }
    };
    if changes.as_ref().is_some_and(Vec::is_empty) {
        debug!("network folder {} is as the last scan left it", root.display());
        return NetworkCheck::Unchanged;
    }
    NetworkCheck::Moved(changes)
}

/// Ask for the reads a filesystem watch's report calls for.
async fn take_watch_report(
    report: WatchReport,
    tables: &crate::tables::ImportTables,
    active_roots: &mut ActiveRoots,
    diagnostics: &bae_diagnostics::Diagnostics,
) {
    let events = match report {
        Ok(events) => events,
        Err(errors) => {
            let roots = match watched_roots(tables).await {
                Ok(roots) => roots,
                Err(error) => {
                    diagnostics
                        .report_failure("list the watched folders for a watch error", &error);
                    return;
                }
            };
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
    let roots = match watched_roots(tables).await {
        Ok(roots) => roots,
        Err(error) => {
            diagnostics.report_failure("list the watched folders for a watch report", &error);
            return;
        }
    };
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
        let under: Vec<&Path> =
            changed.iter().copied().filter(|path| path.starts_with(&root)).collect();
        let lost_track = events.iter().any(|event| {
            event.need_rescan() && event.paths.iter().any(|path| path.starts_with(&root))
        });
        let holds = holds_its_own_release(tables, &root).await;
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
                debug!("a change under {} reaches nothing a scan reads: {cause}", root.display());
            }
            active_roots.request_folders(root, folders, cause);
        }
    }
}

/// Whether `root` holds tracks of its own. A failed read answers yes, which
/// reads the whole root: right either way.
async fn holds_its_own_release(tables: &crate::tables::ImportTables, root: &Path) -> bool {
    match tables.load_folder_scan_item(&crate::CandidateKey::folder(root)).await {
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

/// The roots the store lists as watched. A failed read is the caller's to
/// act on: it never reads as nothing being watched.
async fn watched_roots(
    tables: &crate::tables::ImportTables,
) -> Result<Vec<PathBuf>, bae_store::DbError> {
    let folders = tables.load_watched_import_folders().await?;
    Ok(folders.into_iter().map(|folder| folder.path).collect())
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
