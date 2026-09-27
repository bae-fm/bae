//! What the coordinator has going for each watched root: a pass reading it,
//! or a removal, never both.
//!
//! A folder taking over the watched folders inside it is a removal of them
//! with that folder watched in their place.

use super::*;
use std::collections::BTreeSet;

mod root_tasks;

/// A caller waiting to hear that what it asked for is over.
pub(super) type RefreshCompletion = tokio::sync::oneshot::Sender<Result<(), String>>;

/// Every watched root the coordinator has work in flight for, and the only way
/// to start any of it.
///
/// One pass per root: a request that arrives while one runs is queued behind
/// it, since a second pass would write over the first one's scan generation.
pub(super) struct ActiveRoots {
    roots: HashMap<PathBuf, RootActivity>,
    /// By id, since one removal can hold several roots.
    removals: HashMap<u64, RootRemovalSchedule>,
    starter: RootScanStarter,
    scan_completions: mpsc::UnboundedSender<RootScanCompletion>,
    next_scan_id: u64,
    removal_backend: Arc<dyn RootRemovalBackend>,
    removal_completions: mpsc::UnboundedSender<RootRemovalCompletion>,
    folder_state_commit: crate::import::FolderStateCommit,
    next_removal_id: u64,
}

/// What one root has going.
enum RootActivity {
    Scanning(RootScanSchedule),
    /// Held by the removal with this id, as a root it stops watching or as
    /// the folder it watches in their place.
    Removing(u64),
}

/// What one pass over a root reads.
pub(super) enum RootPass {
    /// Every folder under the root.
    WholeRoot,
    /// One folder whose reading the person changed, stored with the
    /// candidates it gives.
    Decision(FolderReadingRequest),
    /// Folders directly under the root that changed on disk.
    Folders(BTreeSet<String>),
}

/// A person's answer for how one folder reads, and who hears whether it was
/// stored.
pub(super) struct FolderReadingRequest {
    target: (
        crate::import::folder_scanner::FolderReleaseDecisionKey,
        crate::import::folder_scanner::FolderReleaseDecision,
    ),
    completion: RefreshCompletion,
}

impl FolderReadingRequest {
    pub(super) fn new(
        target: (
            crate::import::folder_scanner::FolderReleaseDecisionKey,
            crate::import::folder_scanner::FolderReleaseDecision,
        ),
        completion: RefreshCompletion,
    ) -> Self {
        Self { target, completion }
    }

    /// The folder and how it is to read.
    pub(super) fn target(
        &self,
    ) -> &(
        crate::import::folder_scanner::FolderReleaseDecisionKey,
        crate::import::folder_scanner::FolderReleaseDecision,
    ) {
        &self.target
    }

    /// Tell whoever asked what became of it.
    pub(super) fn answer(self, result: Result<(), String>) {
        if self.completion.send(result).is_err() {
            debug!(
                "folder decision caller for {} dropped before it was answered",
                self.target.0.relative_folder_path
            );
        }
    }
}

struct RootScanSchedule {
    id: u64,
    scan: RootScanTask,
    /// A folder reading that arrives while a whole-root pass runs cancels it
    /// rather than waiting it out.
    whole_root: bool,
    /// A whole-root pass is owed once the running pass and every queued
    /// folder reading are over.
    pending: bool,
    current_waiters: Vec<RefreshCompletion>,
    followup_waiters: Vec<RefreshCompletion>,
    /// Folder readings waiting their turn, in the order they were asked for.
    readings: std::collections::VecDeque<FolderReadingRequest>,
    /// Folders that changed on disk while this pass ran, read once it and the
    /// queued readings are over.
    changed: BTreeSet<String>,
}

/// What a root's next pass inherits from the one before it.
#[derive(Default)]
struct Queued {
    pending: bool,
    followup_waiters: Vec<RefreshCompletion>,
    readings: std::collections::VecDeque<FolderReadingRequest>,
    changed: BTreeSet<String>,
}

struct RootRemovalSchedule {
    task: tokio::task::JoinHandle<()>,
    /// The roots it stops watching.
    roots: Vec<PathBuf>,
    /// The folder holding exactly `roots` that it watches in their place.
    parent: Option<PathBuf>,
    /// Everyone who asked for this removal.
    completions: Vec<RefreshCompletion>,
    /// Refresh callers waiting on a read, by the folder they asked about.
    scan_waiters: Vec<(PathBuf, RefreshCompletion)>,
}

impl RootRemovalSchedule {
    /// Whether `roots` and `parent` ask for exactly this removal.
    fn is(&self, roots: &[PathBuf], parent: Option<&Path>) -> bool {
        let mut asked = roots.to_vec();
        asked.sort();
        let mut own = self.roots.clone();
        own.sort();
        asked == own && parent == self.parent.as_deref()
    }

    /// Why nothing else can be done with `path` while this runs.
    fn busy(&self, path: &Path) -> String {
        busy_reason(path, self.parent.as_deref())
    }

    /// The roots and the parent.
    fn paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.roots.iter().chain(self.parent.iter())
    }
}

/// Why nothing else can be done with `path` while a removal holds it.
fn busy_reason(path: &Path, parent: Option<&Path>) -> String {
    match parent {
        None => format!("{} is being removed", path.display()),
        Some(parent) if parent == path => format!(
            "{} is taking over the watched folders inside it",
            parent.display()
        ),
        Some(parent) => format!(
            "{} is being taken over by {}",
            path.display(),
            parent.display()
        ),
    }
}

/// A root names a removal that is not under way, which nothing should allow.
fn missing_removal(id: u64) -> String {
    error!("a watched root names removal {id}, which is not under way");
    format!("removal {id} is not under way")
}

/// Tell a caller waiting on a folder what became of it.
fn answer(waiter: RefreshCompletion, result: Result<(), String>) {
    if waiter.send(result).is_err() {
        debug!("folder caller dropped before it was answered");
    }
}

impl ActiveRoots {
    /// The roots, with the completion channels the coordinator's loop hands
    /// back to [`Self::finish_scan`] and [`Self::finish_removal`].
    pub(super) fn new(
        starter: RootScanStarter,
        removal_backend: Arc<dyn RootRemovalBackend>,
        folder_state_commit: crate::import::FolderStateCommit,
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
                removals: HashMap::new(),
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

    /// Whether a removal holds `path`.
    pub(super) fn is_being_removed(&self, path: &Path) -> bool {
        matches!(self.roots.get(path), Some(RootActivity::Removing(_)))
    }

    /// Ask for a pass over `path`, telling `waiter` when it is over.
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
            // The removal's outcome answers the waiter.
            Some(RootActivity::Removing(id)) => {
                if let Some(waiter) = waiter {
                    let id = *id;
                    self.wait_on_removal(id, path, waiter);
                }
            }
            None => {
                info!("folder scan of {} starting: {cause}", path.display());
                self.start_pass(
                    path,
                    RootPass::WholeRoot,
                    waiter.into_iter().collect(),
                    Queued::default(),
                );
            }
        }
    }

    /// Read again the folders directly under `path` that changed on disk.
    pub(super) fn request_folders(
        &mut self,
        path: PathBuf,
        folders: BTreeSet<String>,
        cause: RootScanCause,
    ) {
        if folders.is_empty() {
            return;
        }
        match self.roots.get_mut(&path) {
            Some(RootActivity::Scanning(schedule)) => {
                info!(
                    "reading {folders:?} under {} again once the pass running is over: {cause}",
                    path.display()
                );
                schedule.changed.extend(folders);
            }
            // Read whole after a failed removal, or no longer watched.
            Some(RootActivity::Removing(_)) => {}
            None => {
                info!(
                    "reading {folders:?} under {} again: {cause}",
                    path.display()
                );
                self.start_pass(
                    path,
                    RootPass::Folders(folders),
                    Vec::new(),
                    Queued::default(),
                );
            }
        }
    }

    /// Store a person's answer for how one folder under `path` reads, as the
    /// root's next pass. A whole-root pass under way is cancelled and owed
    /// again, since it would write the folder's old reading over the new one.
    pub(super) fn change_folder_reading(&mut self, path: PathBuf, request: FolderReadingRequest) {
        match self.roots.get_mut(&path) {
            Some(RootActivity::Scanning(schedule)) => {
                if schedule.whole_root {
                    schedule.scan.cancellation.cancel();
                    schedule.pending = true;
                    schedule
                        .followup_waiters
                        .append(&mut schedule.current_waiters);
                }
                schedule.readings.push_back(request);
            }
            Some(RootActivity::Removing(id)) => {
                let id = *id;
                request.answer(Err(self.why_held(id, &path)));
            }
            None => {
                self.start_pass(
                    path,
                    RootPass::Decision(request),
                    Vec::new(),
                    Queued::default(),
                );
            }
        }
    }

    /// A pass reported itself over: answer its waiters and start what was
    /// queued behind it.
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
        // The scan reports its own failure; a refresh caller only waits.
        for waiter in schedule.current_waiters.drain(..) {
            if waiter.send(Ok(())).is_err() {
                debug!("folder refresh caller dropped before completion");
            }
        }
        let mut queued = Queued {
            pending: schedule.pending,
            followup_waiters: std::mem::take(&mut schedule.followup_waiters),
            readings: std::mem::take(&mut schedule.readings),
            changed: std::mem::take(&mut schedule.changed),
        };
        if let Some(reading) = queued.readings.pop_front() {
            self.start_pass(
                completion.path,
                RootPass::Decision(reading),
                Vec::new(),
                queued,
            );
        } else if !queued.pending && !queued.changed.is_empty() {
            let folders = std::mem::take(&mut queued.changed);
            self.start_pass(
                completion.path,
                RootPass::Folders(folders),
                Vec::new(),
                queued,
            );
        } else if queued.pending {
            info!(
                "folder scan of {} starting again: one was queued while it ran",
                completion.path.display()
            );
            let waiters = std::mem::take(&mut queued.followup_waiters);
            self.start_pass(
                completion.path,
                RootPass::WholeRoot,
                waiters,
                Queued::default(),
            );
        }
    }

    /// Stop watching `roots`, watching `parent` — the folder holding exactly
    /// them — in their place when given. Asking again for a removal under way
    /// joins it; asking anything else of a folder it holds is refused.
    pub(super) fn remove(
        &mut self,
        roots: Vec<PathBuf>,
        parent: Option<PathBuf>,
        completion: RefreshCompletion,
    ) {
        let held = roots.iter().chain(parent.iter()).find_map(|path| {
            match self.roots.get(path) {
                Some(RootActivity::Removing(id)) => Some((path, *id)),
                Some(RootActivity::Scanning(_)) | None => None,
            }
        });
        if let Some((path, id)) = held {
            match self.removals.get_mut(&id) {
                Some(removal) if removal.is(&roots, parent.as_deref()) => {
                    removal.completions.push(completion);
                }
                Some(removal) => answer(completion, Err(removal.busy(path))),
                None => answer(completion, Err(missing_removal(id))),
            }
            return;
        }
        if let Some(parent) = &parent {
            if self.roots.contains_key(parent) {
                answer(
                    completion,
                    Err(format!("{} is already watched", parent.display())),
                );
                return;
            }
        }
        // Each root's pass is cancelled and waited out, since it could
        // install a watch this is taking down; its waiters become the
        // removal's.
        self.next_removal_id += 1;
        let id = self.next_removal_id;
        let mut scans = Vec::new();
        let mut scan_waiters = Vec::new();
        for root in &roots {
            if let Some(RootActivity::Scanning(mut schedule)) = self.roots.remove(root) {
                schedule.scan.cancellation.cancel();
                scan_waiters.extend(
                    schedule
                        .current_waiters
                        .drain(..)
                        .chain(schedule.followup_waiters.drain(..))
                        .map(|waiter| (root.clone(), waiter)),
                );
                // Nothing will run a queued reading, whatever the outcome.
                for reading in schedule.readings.drain(..) {
                    reading.answer(Err(busy_reason(root, parent.as_deref())));
                }
                scans.push(schedule.scan);
            }
            self.roots.insert(root.clone(), RootActivity::Removing(id));
        }
        if let Some(parent) = &parent {
            self.roots.insert(parent.clone(), RootActivity::Removing(id));
        }
        let backend = self.removal_backend.clone();
        let commit = self.folder_state_commit.clone();
        let completions = self.removal_completions.clone();
        let task_roots = roots.clone();
        let task_parent = parent.clone();
        let task = tokio::spawn(async move {
            let result = root_tasks::run_root_removal(
                &task_roots,
                task_parent.as_deref(),
                scans,
                backend.as_ref(),
                commit,
            )
            .await;
            if completions
                .send(RootRemovalCompletion { id, result })
                .is_err()
            {
                debug!("folder scan coordinator ended before removal completion");
            }
        });
        self.removals.insert(
            id,
            RootRemovalSchedule {
                task,
                roots,
                parent,
                completions: vec![completion],
                scan_waiters,
            },
        );
    }

    /// A removal reported itself over: start the parent's read when it
    /// landed, or read its roots again when it failed, and return what the
    /// coordinator is to announce.
    pub(super) async fn finish_removal(
        &mut self,
        completion: RootRemovalCompletion,
    ) -> Option<RemovalOutcome> {
        let removal = self.removals.remove(&completion.id)?;
        for path in removal.paths() {
            self.roots.remove(path);
        }
        if let Err(error) = removal.task.await {
            error!(
                "folder removal task failed for {:?}: {error}",
                removal.roots
            );
        }
        Some(match completion.result {
            RootRemovalResult::Removed {
                commit,
                removed_keys,
            } => {
                let scan_waiters = match removal.parent {
                    // The parent's read covers every folder a waiter asked about.
                    Some(parent) => {
                        self.start_pass(
                            parent,
                            RootPass::WholeRoot,
                            removal
                                .scan_waiters
                                .into_iter()
                                .map(|(_, waiter)| waiter)
                                .collect(),
                            Queued::default(),
                        );
                        Vec::new()
                    }
                    None => removal.scan_waiters,
                };
                RemovalOutcome::Removed {
                    commit,
                    removed_keys,
                    scan_waiters,
                    callers: removal.completions,
                }
            }
            RootRemovalResult::Failed(error) => {
                // The roots are still watched and are read again; the parent
                // never was watched.
                let mut waiters: HashMap<PathBuf, Vec<RefreshCompletion>> = HashMap::new();
                for (path, waiter) in removal.scan_waiters {
                    waiters.entry(path).or_default().push(waiter);
                }
                if let Some(parent) = &removal.parent {
                    for waiter in waiters.remove(parent).unwrap_or_default() {
                        answer(waiter, Err(error.clone()));
                    }
                }
                for root in removal.roots {
                    let waiters = waiters.remove(&root).unwrap_or_default();
                    self.start_pass(root, RootPass::WholeRoot, waiters, Queued::default());
                }
                RemovalOutcome::Failed {
                    error,
                    callers: removal.completions,
                }
            }
        })
    }

    /// Have `waiter`, who asked about `path`, answered when removal `id` ends.
    fn wait_on_removal(&mut self, id: u64, path: PathBuf, waiter: RefreshCompletion) {
        match self.removals.get_mut(&id) {
            Some(removal) => removal.scan_waiters.push((path, waiter)),
            None => answer(waiter, Err(missing_removal(id))),
        }
    }

    /// Why nothing else can be done with `path` while removal `id` holds it.
    fn why_held(&self, id: u64, path: &Path) -> String {
        self.removals
            .get(&id)
            .map_or_else(|| missing_removal(id), |removal| removal.busy(path))
    }

    /// Cancel every pass without waiting for it.
    pub(super) fn cancel_scans(&self) {
        for activity in self.roots.values() {
            if let RootActivity::Scanning(schedule) = activity {
                schedule.scan.cancellation.cancel();
            }
        }
    }

    /// Stop everything and wait for it, telling every waiter the service
    /// stopped. Every pass is cancelled before any is waited on.
    pub(super) async fn shutdown(&mut self) {
        for activity in self.roots.values_mut() {
            let RootActivity::Scanning(schedule) = activity else {
                continue;
            };
            schedule.scan.cancellation.cancel();
            schedule.pending = false;
            for reading in schedule.readings.drain(..) {
                reading.answer(Err("folder scan service stopped".to_string()));
            }
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
            if let RootActivity::Scanning(schedule) = activity {
                if let Err(error) = schedule.scan.task.await {
                    error!("folder scan task failed during shutdown: {error}");
                }
            }
        }
        for (_, removal) in self.removals.drain() {
            if let Err(error) = removal.task.await {
                error!("folder removal task failed during shutdown: {error}");
            }
            for waiter in removal
                .scan_waiters
                .into_iter()
                .map(|(_, waiter)| waiter)
                .chain(removal.completions)
            {
                answer(waiter, Err("folder scan service stopped".to_string()));
            }
        }
    }

    /// Start a pass over `path`, with its waiters and what is queued behind it.
    fn start_pass(
        &mut self,
        path: PathBuf,
        pass: RootPass,
        waiters: Vec<RefreshCompletion>,
        queued: Queued,
    ) {
        self.next_scan_id += 1;
        let id = self.next_scan_id;
        let whole_root = matches!(pass, RootPass::WholeRoot);
        let changed = if whole_root {
            BTreeSet::new()
        } else {
            queued.changed
        };
        let scan = (self.starter)(id, path.clone(), pass, self.scan_completions.clone());
        self.roots.insert(
            path,
            RootActivity::Scanning(RootScanSchedule {
                id,
                scan,
                whole_root,
                pending: queued.pending,
                current_waiters: waiters,
                followup_waiters: queued.followup_waiters,
                readings: queued.readings,
                changed,
            }),
        );
    }
}

/// What a finished removal leaves the coordinator, which holds the event
/// stream, to announce.
pub(super) enum RemovalOutcome {
    Removed {
        /// Held until the removal is announced, so nothing writes in between.
        commit: crate::import::FolderStateCommitGuard,
        /// Releases that left the queue, announced so work on them stops.
        removed_keys: Vec<String>,
        /// Refresh callers of folders no longer watched.
        scan_waiters: Vec<(PathBuf, RefreshCompletion)>,
        callers: Vec<RefreshCompletion>,
    },
    Failed {
        error: String,
        callers: Vec<RefreshCompletion>,
    },
}

pub(super) struct RootRemovalCompletion {
    id: u64,
    result: RootRemovalResult,
}

enum RootRemovalResult {
    Removed {
        commit: crate::import::FolderStateCommitGuard,
        removed_keys: Vec<String>,
    },
    Failed(String),
}
