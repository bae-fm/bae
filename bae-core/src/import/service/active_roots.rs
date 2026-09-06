//! What the coordinator has going for each watched root.
//!
//! A root is being read or being removed, never both, and [`ActiveRoots`] is
//! where that holds: a removal takes the running pass out and waits for it, a
//! request that arrives meanwhile finds the removal and asks for nothing, and
//! only a removal that failed hands the root back to be read. Every scan the
//! coordinator starts and every removal it performs goes through here.
//!
//! Deciding when a root is read is [`super::coordinator`]'s; reading it is
//! [`super::scanning`]'s.

use super::*;

/// A refresh, folder-decision or removal caller waiting to hear that what it
/// asked for is over.
pub(super) type RefreshCompletion = tokio::sync::oneshot::Sender<Result<(), String>>;

/// Every watched root the coordinator has work in flight for, and the only way
/// to start any of it.
///
/// One pass per root: a request that arrives while a root is being read marks
/// the running pass as owing a successor rather than starting a second pass
/// that would write over the first one's scan generation. Passes and removals
/// each carry an id from their own count, so a completion naming one this has
/// already replaced — a queued successor, a root put back to being read when
/// its removal failed — is recognized as stale and dropped.
pub(super) struct ActiveRoots {
    roots: HashMap<PathBuf, RootActivity>,
    starter: RootScanStarter,
    scan_completions: mpsc::UnboundedSender<RootScanCompletion>,
    next_scan_id: u64,
    removal_backend: Arc<dyn RootRemovalBackend>,
    removal_completions: mpsc::UnboundedSender<RootRemovalCompletion>,
    folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    next_removal_id: u64,
}

/// What one root has going.
enum RootActivity {
    Scanning(RootScanSchedule),
    Removing(RootRemovalSchedule),
}

struct RootScanSchedule {
    id: u64,
    scan: RootScanTask,
    pending: bool,
    current_waiters: Vec<RefreshCompletion>,
    followup_waiters: Vec<RefreshCompletion>,
}

struct RootRemovalSchedule {
    id: u64,
    task: tokio::task::JoinHandle<()>,
    completions: Vec<RefreshCompletion>,
    scan_waiters: Vec<RefreshCompletion>,
}

impl ActiveRoots {
    /// The roots, with the completions the coordinator's loop must hand back to
    /// [`Self::finish_scan`] and [`Self::finish_removal`]. Two channels rather
    /// than one, because the loop serves a finished removal ahead of a finished
    /// scan.
    pub(super) fn new(
        starter: RootScanStarter,
        removal_backend: Arc<dyn RootRemovalBackend>,
        folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<RootScanCompletion>,
        mpsc::UnboundedReceiver<RootRemovalCompletion>,
    ) {
        let (scan_completions, scan_rx) = mpsc::unbounded_channel();
        let (removal_completions, removal_rx) = mpsc::unbounded_channel();
        (
            Self {
                roots: HashMap::new(),
                starter,
                scan_completions,
                next_scan_id: 0,
                removal_backend,
                removal_completions,
                folder_state_commit,
                next_removal_id: 0,
            },
            scan_rx,
            removal_rx,
        )
    }

    /// Whether the root is on its way out. A caller that wants to say so in its
    /// own words — and one that must not do its own work first — asks before
    /// requesting anything.
    pub(super) fn is_being_removed(&self, path: &Path) -> bool {
        matches!(self.roots.get(path), Some(RootActivity::Removing(_)))
    }

    /// Ask for a pass over `path`, telling `waiter` when it is over.
    ///
    /// A root already being read gets its running pass marked as owing a
    /// successor instead of a second pass; a root on its way out gets nothing,
    /// and `waiter` hears why.
    pub(super) fn request_scan(
        &mut self,
        path: PathBuf,
        cause: RootScanCause,
        waiter: Option<RefreshCompletion>,
    ) {
        match self.roots.get_mut(&path) {
            Some(RootActivity::Scanning(schedule)) => {
                info!(
                    "folder scan of {} queued behind the one running: {cause}",
                    path.display()
                );
                schedule.pending = true;
                if let Some(waiter) = waiter {
                    schedule.followup_waiters.push(waiter);
                }
            }
            Some(RootActivity::Removing(_)) => {
                if let Some(waiter) = waiter {
                    if waiter
                        .send(Err(format!("{} is being removed", path.display())))
                        .is_err()
                    {
                        debug!("folder caller dropped during removal");
                    }
                }
            }
            None => {
                info!("folder scan of {} starting: {cause}", path.display());
                self.start_scan(path, waiter.into_iter().collect());
            }
        }
    }

    /// Cancel the pass over `path` and queue a successor: what it is reading is
    /// about to change under it. Everyone waiting on it moves to the successor,
    /// which is the pass that will see the new state.
    pub(super) fn requeue_scan(&mut self, path: &Path) {
        let Some(RootActivity::Scanning(schedule)) = self.roots.get_mut(path) else {
            return;
        };
        schedule.scan.cancellation.cancel();
        schedule.pending = true;
        schedule
            .followup_waiters
            .append(&mut schedule.current_waiters);
    }

    /// Tell `waiter` when this root's next pass is over: the successor already
    /// queued behind a running pass, or one started now.
    pub(super) fn wait_for_next_scan(
        &mut self,
        path: PathBuf,
        cause: RootScanCause,
        waiter: RefreshCompletion,
    ) {
        if let Some(RootActivity::Scanning(schedule)) = self.roots.get_mut(&path) {
            schedule.followup_waiters.push(waiter);
            return;
        }
        self.request_scan(path, cause, Some(waiter));
    }

    /// A pass reported itself over. Whoever was waiting for it hears so, and a
    /// request that arrived while it ran starts its successor.
    pub(super) async fn finish_scan(&mut self, completion: RootScanCompletion) {
        if !matches!(
            self.roots.get(&completion.path),
            Some(RootActivity::Scanning(schedule)) if schedule.id == completion.id
        ) {
            return;
        }
        let Some(RootActivity::Scanning(mut schedule)) = self.roots.remove(&completion.path) else {
            return;
        };
        if let Err(error) = schedule.scan.task.await {
            error!(
                "folder scan task failed for {}: {error}",
                completion.path.display()
            );
        }
        // The scan itself is what said whether it worked. A refresh caller is
        // only waiting for it to be over.
        for waiter in schedule.current_waiters.drain(..) {
            if waiter.send(Ok(())).is_err() {
                debug!("folder refresh caller dropped before completion");
            }
        }
        if schedule.pending {
            info!(
                "folder scan of {} starting again: one was queued while it ran",
                completion.path.display()
            );
            let waiters = std::mem::take(&mut schedule.followup_waiters);
            self.start_scan(completion.path, waiters);
        }
    }

    /// Stop watching `path`: uninstall its watch and delete its rows once
    /// whatever is reading it has stopped. A caller that asks while a removal
    /// is under way waits on that one — two removals would race over the same
    /// watch.
    pub(super) fn remove(&mut self, path: PathBuf, completion: RefreshCompletion) {
        if let Some(RootActivity::Removing(removal)) = self.roots.get_mut(&path) {
            removal.completions.push(completion);
            return;
        }
        // A removal was ruled out just above, so what is here is a pass or
        // nothing. The pass is cancelled and handed over to be waited on: it
        // could otherwise install a watch on the folder this stops watching.
        // The refresh callers it was going to answer become the removal's, and
        // hear what became of the root instead.
        let mut scan = None;
        let mut scan_waiters = Vec::new();
        if let Some(RootActivity::Scanning(mut schedule)) = self.roots.remove(&path) {
            schedule.scan.cancellation.cancel();
            scan_waiters = schedule
                .current_waiters
                .drain(..)
                .chain(schedule.followup_waiters.drain(..))
                .collect();
            scan = Some(schedule.scan);
        }
        self.next_removal_id += 1;
        let id = self.next_removal_id;
        let removal_path = path.clone();
        let backend = self.removal_backend.clone();
        let commit = self.folder_state_commit.clone();
        let completions = self.removal_completions.clone();
        let task = tokio::spawn(async move {
            let result = run_root_removal(&removal_path, scan, backend.as_ref(), commit).await;
            if completions
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
        self.roots.insert(
            path,
            RootActivity::Removing(RootRemovalSchedule {
                id,
                task,
                completions: vec![completion],
                scan_waiters,
            }),
        );
    }

    /// A removal reported itself over. What it leaves the coordinator to
    /// announce comes back; a failed one has already put the root back to being
    /// read. Nothing comes back for a removal this has already replaced.
    pub(super) async fn finish_removal(
        &mut self,
        completion: RootRemovalCompletion,
    ) -> Option<RemovalOutcome> {
        if !matches!(
            self.roots.get(&completion.path),
            Some(RootActivity::Removing(removal)) if removal.id == completion.id
        ) {
            return None;
        }
        let Some(RootActivity::Removing(removal)) = self.roots.remove(&completion.path) else {
            return None;
        };
        if let Err(error) = removal.task.await {
            error!(
                "folder removal task failed for {}: {error}",
                completion.path.display()
            );
        }
        Some(match completion.result {
            RootRemovalResult::Removed {
                commit,
                removed_keys,
            } => RemovalOutcome::Removed {
                path: completion.path,
                commit,
                removed_keys,
                scan_waiters: removal.scan_waiters,
                callers: removal.completions,
            },
            RootRemovalResult::Failed(error) => {
                // The root is still watched, so it goes back to being read, and
                // the refresh callers the removal took over wait on that pass.
                self.start_scan(completion.path, removal.scan_waiters);
                RemovalOutcome::Failed {
                    error,
                    callers: removal.completions,
                }
            }
        })
    }

    /// Stop every pass. Their tasks are left to end on their own: nothing is
    /// waiting on them any more.
    pub(super) fn cancel_scans(&self) {
        for activity in self.roots.values() {
            if let RootActivity::Scanning(schedule) = activity {
                schedule.scan.cancellation.cancel();
            }
        }
    }

    /// Stop everything and wait for it. Every pass is cancelled before any is
    /// waited on, so they end alongside each other, and everyone waiting on one
    /// hears that the service is going away.
    pub(super) async fn shutdown(&mut self) {
        for activity in self.roots.values_mut() {
            let RootActivity::Scanning(schedule) = activity else {
                continue;
            };
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
        for (_, activity) in self.roots.drain() {
            match activity {
                RootActivity::Scanning(schedule) => {
                    if let Err(error) = schedule.scan.task.await {
                        error!("folder scan task failed during shutdown: {error}");
                    }
                }
                RootActivity::Removing(mut removal) => {
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
            }
        }
    }

    /// Start a pass over `path`, with the callers it is to answer when it ends.
    fn start_scan(&mut self, path: PathBuf, waiters: Vec<RefreshCompletion>) {
        self.next_scan_id += 1;
        let id = self.next_scan_id;
        let scan = (self.starter)(id, path.clone(), self.scan_completions.clone());
        self.roots.insert(
            path,
            RootActivity::Scanning(RootScanSchedule {
                id,
                scan,
                pending: false,
                current_waiters: waiters,
                followup_waiters: Vec::new(),
            }),
        );
    }
}

/// What a finished removal leaves the coordinator to announce. The coordinator
/// holds the event stream, so saying what became of the root is its to do.
pub(super) enum RemovalOutcome {
    Removed {
        path: PathBuf,
        /// Held until the events announcing the removal are out, so nothing
        /// else writes folder state in between.
        commit: tokio::sync::OwnedMutexGuard<()>,
        /// The scan entries the removal cascaded away, announced as
        /// `CandidateRemoved` so in-flight work on them is cancelled.
        removed_keys: Vec<String>,
        /// Refresh callers the removal took over from the pass it cancelled.
        scan_waiters: Vec<RefreshCompletion>,
        /// Everyone who asked for this removal.
        callers: Vec<RefreshCompletion>,
    },
    Failed {
        error: String,
        callers: Vec<RefreshCompletion>,
    },
}

/// Why a root scan was asked for.
///
/// Logged wherever one is requested, because "the scans never stop" is a
/// question only the thing that keeps asking for them can answer — and until
/// now nothing recorded that. A watched network share whose own reads come
/// back as writes would look exactly like a folder somebody keeps editing.
pub(super) enum RootScanCause {
    /// The filesystem reported changes under the root: the events that passed
    /// the change filter, kind and path, and how many were filtered out.
    FsChange(String),
    /// The watcher itself failed, so the root is re-read to catch up on
    /// whatever it missed.
    WatchError,
    /// The periodic sweep.
    Timer,
    /// The periodic check of a network folder found a directory that moved.
    /// Such a folder has no watch worth the name, so this is the only thing
    /// that notices a change made on the server or by another machine.
    NetworkFolderMoved,
    /// Something a person did — naming which.
    Asked(&'static str),
}

impl std::fmt::Display for RootScanCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FsChange(events) => write!(f, "filesystem change ({events})"),
            Self::WatchError => write!(f, "the folder watcher reported an error"),
            Self::Timer => write!(f, "the periodic sweep"),
            Self::NetworkFolderMoved => {
                write!(f, "the periodic check found a directory that moved")
            }
            Self::Asked(what) => write!(f, "{what}"),
        }
    }
}

pub(super) struct RootRemovalCompletion {
    id: u64,
    path: PathBuf,
    result: RootRemovalResult,
}

enum RootRemovalResult {
    Removed {
        commit: tokio::sync::OwnedMutexGuard<()>,
        /// The scan entries the removal cascaded away.
        removed_keys: Vec<String>,
    },
    Failed(String),
}

#[async_trait::async_trait]
pub(super) trait RootRemovalBackend: Send + Sync {
    async fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, String>;
    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String>;
    /// Delete the root's rows and return the scan entry keys that went with
    /// them.
    async fn remove_durable_root(&self, path: &Path) -> Result<Vec<String>, String>;
}

pub(super) struct ServiceRootRemovalBackend {
    folder_watcher: Arc<FolderWatcher>,
    library_manager: LibraryManager,
}

impl ServiceRootRemovalBackend {
    pub(super) fn new(folder_watcher: Arc<FolderWatcher>, library_manager: LibraryManager) -> Self {
        Self {
            folder_watcher,
            library_manager,
        }
    }
}

#[async_trait::async_trait]
impl RootRemovalBackend for ServiceRootRemovalBackend {
    async fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, String> {
        let watcher = self.folder_watcher.clone();
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || watcher.uninstall(&path))
            .await
            .map_err(|error| format!("folder watch removal task panicked: {error}"))?
            .map_err(|error| error.to_string())
    }

    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String> {
        let watcher = self.folder_watcher.clone();
        let path = path.to_path_buf();
        let snapshot = snapshot.clone();
        tokio::task::spawn_blocking(move || watcher.reinstall(&path, &snapshot))
            .await
            .map_err(|error| format!("folder watch restore task panicked: {error}"))?
            .map_err(|error| error.to_string())
    }

    async fn remove_durable_root(&self, path: &Path) -> Result<Vec<String>, String> {
        self.library_manager
            .remove_watched_import_folder(&path.to_string_lossy())
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("{} is not a watched folder", path.display()))
    }
}

async fn run_root_removal(
    path: &Path,
    scan: Option<RootScanTask>,
    backend: &dyn RootRemovalBackend,
    folder_state_commit: Arc<tokio::sync::Mutex<()>>,
) -> RootRemovalResult {
    if let Some(scan) = scan {
        if let Err(error) = scan.task.await {
            return RootRemovalResult::Failed(format!(
                "folder scan task failed while removing {}: {error}",
                path.display()
            ));
        }
    }
    let watch_snapshot = match backend.uninstall(path).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return RootRemovalResult::Failed(format!(
                "could not remove folder watch for {}: {error}",
                path.display()
            ));
        }
    };
    let commit = folder_state_commit.lock_owned().await;
    let removed_keys = match backend.remove_durable_root(path).await {
        Ok(removed_keys) => removed_keys,
        Err(error) => {
            drop(commit);
            let rollback = backend.reinstall(path, &watch_snapshot).await;
            let detail = match rollback {
                Ok(()) => format!(
                    "could not remove watched folder {}: {error}",
                    path.display()
                ),
                Err(rollback_error) => format!(
                    "could not remove watched folder {}: {error}; restoring its folder watch also \
                 failed: {rollback_error}",
                    path.display()
                ),
            };
            return RootRemovalResult::Failed(detail);
        }
    };
    RootRemovalResult::Removed {
        commit,
        removed_keys,
    }
}
