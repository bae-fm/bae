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
use std::collections::BTreeSet;

mod root_tasks;

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
    adoption_completions: mpsc::UnboundedSender<RootAdoptionCompletion>,
    folder_state_commit: crate::import::FolderStateCommit,
    next_removal_id: u64,
}

/// What one root has going.
enum RootActivity {
    Scanning(RootScanSchedule),
    Removing(RootRemovalSchedule),
    /// A folder taking over the watched folders inside it.
    Adopting(RootAdoptionSchedule),
    /// A watched folder being folded into the folder that holds it, which is
    /// where whatever asks for it waits.
    FoldingInto(PathBuf),
}

/// What one pass over a root reads.
pub(super) enum RootPass {
    /// Every folder under the root.
    WholeRoot,
    /// One folder whose reading the person changed: the decision and the
    /// candidates it gives are stored together, and nothing else is read.
    Decision(FolderReadingRequest),
    /// Folders directly under the root whose contents changed on disk, each
    /// read again and stored in a write of its own. Nothing else is read.
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
    /// Whether the running pass reads the whole root. A folder reading that
    /// arrives meanwhile replaces such a pass rather than waiting it out; a
    /// running folder reading is short and is waited for.
    whole_root: bool,
    /// A whole-root pass is owed once the running pass and every queued
    /// folder reading are over.
    pending: bool,
    current_waiters: Vec<RefreshCompletion>,
    followup_waiters: Vec<RefreshCompletion>,
    /// Folder readings waiting their turn, in the order they were asked for.
    readings: std::collections::VecDeque<FolderReadingRequest>,
    /// Folders directly under the root that changed on disk while this pass
    /// ran, owed a reading once it and the queued decisions are over. Folded
    /// into one set, so a burst of changes to one folder reads it once.
    changed: BTreeSet<String>,
}

/// What a root's next pass inherits from the one before it: the whole-root
/// pass still owed, the callers waiting on that pass, the folder readings
/// still queued, and the folders that changed meanwhile.
#[derive(Default)]
struct Queued {
    pending: bool,
    followup_waiters: Vec<RefreshCompletion>,
    readings: std::collections::VecDeque<FolderReadingRequest>,
    changed: BTreeSet<String>,
}

struct RootAdoptionSchedule {
    id: u64,
    inner: Vec<PathBuf>,
    task: tokio::task::JoinHandle<()>,
    adopted: RefreshCompletion,
    /// Who waits for the read of the adopting folder: the caller that asked,
    /// and the refresh callers of the folders it takes over.
    read_waiters: Vec<RefreshCompletion>,
}

impl RootAdoptionSchedule {
    /// The service is going away: wait for the task, and tell everyone
    /// waiting.
    async fn shut_down(self) {
        if let Err(error) = self.task.await {
            error!("folder adoption task failed during shutdown: {error}");
        }
        for waiter in std::iter::once(self.adopted).chain(self.read_waiters) {
            if waiter
                .send(Err("folder scan service stopped".to_string()))
                .is_err()
            {
                debug!("folder caller dropped during shutdown");
            }
        }
    }
}

struct RootRemovalSchedule {
    id: u64,
    task: tokio::task::JoinHandle<()>,
    completions: Vec<RefreshCompletion>,
    scan_waiters: Vec<RefreshCompletion>,
}

impl ActiveRoots {
    /// The roots, with the completions the coordinator's loop must hand back to
    /// [`Self::finish_scan`], [`Self::finish_removal`] and
    /// [`Self::finish_adoption`]. A channel each, because the loop serves a
    /// finished removal or adoption ahead of a finished scan.
    pub(super) fn new(
        starter: RootScanStarter,
        removal_backend: Arc<dyn RootRemovalBackend>,
        folder_state_commit: crate::import::FolderStateCommit,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<RootScanCompletion>,
        mpsc::UnboundedReceiver<RootRemovalCompletion>,
        mpsc::UnboundedReceiver<RootAdoptionCompletion>,
    ) {
        let (scan_completions, scan_rx) = mpsc::unbounded_channel();
        let (removal_completions, removal_rx) = mpsc::unbounded_channel();
        let (adoption_completions, adoption_rx) = mpsc::unbounded_channel();
        (
            Self {
                roots: HashMap::new(),
                starter,
                scan_completions,
                next_scan_id: 0,
                removal_backend,
                removal_completions,
                adoption_completions,
                folder_state_commit,
                next_removal_id: 0,
            },
            scan_rx,
            removal_rx,
            adoption_rx,
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
            // The folder that takes it over is read whole once it has, which
            // is the pass a caller waiting on this one is answered by.
            Some(RootActivity::Adopting(_) | RootActivity::FoldingInto(_)) => {
                if let Some(waiter) = waiter {
                    self.wait_on_adoption(&path, waiter);
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
    ///
    /// Whatever the root has going, they join the folders already owed a
    /// reading; a pass over the whole root that is still owed will read them
    /// anyway. A root on its way out is read no more.
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
            // The whole pass that follows an adoption reads them.
            Some(
                RootActivity::Removing(_)
                | RootActivity::Adopting(_)
                | RootActivity::FoldingInto(_),
            ) => {}
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

    /// Store a person's answer for how one folder under `path` reads, with
    /// the candidates it gives, as the root's next pass.
    ///
    /// A whole-root pass under way is cancelled and owed again afterwards:
    /// it would otherwise write the folder's old reading after the new one
    /// landed, and it has left the root half read. A folder reading under way
    /// is let finish, and this one follows it.
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
            Some(RootActivity::Removing(_)) => {
                request.answer(Err(format!("{} is being removed", path.display())));
            }
            Some(RootActivity::Adopting(_) | RootActivity::FoldingInto(_)) => {
                request.answer(Err(format!(
                    "{} is being taken over by the folder that holds it",
                    path.display()
                )));
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

    /// Stop watching `path`: uninstall its watch and delete its rows once
    /// whatever is reading it has stopped. A caller that asks while a removal
    /// is under way waits on that one — two removals would race over the same
    /// watch.
    pub(super) fn remove(&mut self, path: PathBuf, completion: RefreshCompletion) {
        match self.roots.get_mut(&path) {
            Some(RootActivity::Removing(removal)) => {
                removal.completions.push(completion);
                return;
            }
            Some(RootActivity::Adopting(_) | RootActivity::FoldingInto(_)) => {
                if completion
                    .send(Err(format!(
                        "{} is being taken over by the folder that holds it",
                        path.display()
                    )))
                    .is_err()
                {
                    debug!("folder removal caller dropped during an adoption");
                }
                return;
            }
            Some(RootActivity::Scanning(_)) | None => {}
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
            // A queued reading is answered now: nothing will read the folder
            // again, whatever becomes of the removal.
            for reading in schedule.readings.drain(..) {
                reading.answer(Err(format!("{} is being removed", path.display())));
            }
            scan = Some(schedule.scan);
        }
        self.next_removal_id += 1;
        let id = self.next_removal_id;
        let removal_path = path.clone();
        let backend = self.removal_backend.clone();
        let commit = self.folder_state_commit.clone();
        let completions = self.removal_completions.clone();
        let task = tokio::spawn(async move {
            let result = root_tasks::run_root_removal(&removal_path, scan, backend.as_ref(), commit).await;
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
                self.start_pass(
                    completion.path,
                    RootPass::WholeRoot,
                    removal.scan_waiters,
                    Queued::default(),
                );
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
            match activity {
                RootActivity::Adopting(adoption) => adoption.shut_down().await,
                RootActivity::FoldingInto(_) => {}
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

    /// Start a pass over `path`, with the callers it is to answer when it ends
    /// and what is queued behind it.
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
        // A pass over the whole root reads every folder that changed.
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

impl ActiveRoots {
    /// Watch `parent` in place of the watched folders `inner` inside it.
    /// `adopted` hears whether the durable change landed; `read`, when given,
    /// joins the read of `parent` that follows it.
    pub(super) fn adopt(
        &mut self,
        parent: PathBuf,
        inner: Vec<PathBuf>,
        adopted: RefreshCompletion,
        read: Option<RefreshCompletion>,
    ) {
        let busy = self.roots.contains_key(&parent)
            || inner.iter().any(|root| {
                !matches!(self.roots.get(root), None | Some(RootActivity::Scanning(_)))
            });
        if busy {
            let error = format!(
                "{} or a watched folder inside it is already being removed or taken over",
                parent.display()
            );
            root_tasks::answer(adopted, Err(error.clone()));
            if let Some(read) = read {
                root_tasks::answer(read, Err(error));
            }
            return;
        }
        let mut scans = Vec::new();
        let mut read_waiters: Vec<RefreshCompletion> = read.into_iter().collect();
        for root in &inner {
            if let Some(RootActivity::Scanning(mut schedule)) = self.roots.remove(root) {
                schedule.scan.cancellation.cancel();
                read_waiters.extend(
                    schedule
                        .current_waiters
                        .drain(..)
                        .chain(schedule.followup_waiters.drain(..)),
                );
                for reading in schedule.readings.drain(..) {
                    reading.answer(Err(format!(
                        "{} is being taken over by the folder that holds it",
                        root.display()
                    )));
                }
                scans.push(schedule.scan);
            }
            self.roots
                .insert(root.clone(), RootActivity::FoldingInto(parent.clone()));
        }
        self.next_removal_id += 1;
        let id = self.next_removal_id;
        let backend = self.removal_backend.clone();
        let commit = self.folder_state_commit.clone();
        let completions = self.adoption_completions.clone();
        let task_parent = parent.clone();
        let task_inner = inner.clone();
        let task = tokio::spawn(async move {
            let result =
                root_tasks::run_root_adoption(&task_parent, &task_inner, scans, backend.as_ref(), commit).await;
            if completions
                .send(RootAdoptionCompletion {
                    id,
                    parent: task_parent,
                    result,
                })
                .is_err()
            {
                debug!("folder scan coordinator ended before adoption completion");
            }
        });
        self.roots.insert(
            parent,
            RootActivity::Adopting(RootAdoptionSchedule {
                id,
                inner,
                task,
                adopted,
                read_waiters,
            }),
        );
    }

    /// A caller waiting on `path`, which is adopting or being taken over,
    /// waits on the read of the folder that takes it over.
    pub(super) fn wait_on_adoption(&mut self, path: &Path, waiter: RefreshCompletion) {
        let parent = match self.roots.get(path) {
            Some(RootActivity::FoldingInto(parent)) => parent.clone(),
            _ => path.to_path_buf(),
        };
        match self.roots.get_mut(&parent) {
            Some(RootActivity::Adopting(adoption)) => adoption.read_waiters.push(waiter),
            _ => root_tasks::answer(
                waiter,
                Err(format!("{} is no longer watched", path.display())),
            ),
        }
    }

    /// An adoption reported itself over. A landed one starts the read of the
    /// folder that took over; a failed one puts every folder it would have
    /// taken over back to being read. Nothing comes back for one this has
    /// already replaced.
    pub(super) async fn finish_adoption(
        &mut self,
        completion: RootAdoptionCompletion,
    ) -> Option<AdoptionOutcome> {
        if !matches!(
            self.roots.get(&completion.parent),
            Some(RootActivity::Adopting(adoption)) if adoption.id == completion.id
        ) {
            return None;
        }
        let Some(RootActivity::Adopting(adoption)) = self.roots.remove(&completion.parent) else {
            return None;
        };
        if let Err(error) = adoption.task.await {
            error!(
                "folder adoption task failed for {}: {error}",
                completion.parent.display()
            );
        }
        for root in &adoption.inner {
            self.roots.remove(root);
        }
        Some(match completion.result {
            Ok(commit) => {
                self.start_pass(
                    completion.parent,
                    RootPass::WholeRoot,
                    adoption.read_waiters,
                    Queued::default(),
                );
                AdoptionOutcome::Adopted {
                    commit,
                    adopted: adoption.adopted,
                }
            }
            Err(error) => {
                for waiter in adoption.read_waiters {
                    root_tasks::answer(waiter, Err(error.clone()));
                }
                // Still watched, so read again: the pass the adoption
                // cancelled may have left them half read.
                for root in adoption.inner {
                    self.start_pass(root, RootPass::WholeRoot, Vec::new(), Queued::default());
                }
                AdoptionOutcome::Failed {
                    error,
                    adopted: adoption.adopted,
                }
            }
        })
    }
}

pub(super) struct RootAdoptionCompletion {
    id: u64,
    parent: PathBuf,
    result: Result<crate::import::FolderStateCommitGuard, String>,
}

/// What a finished adoption leaves the coordinator to announce.
pub(super) enum AdoptionOutcome {
    Adopted {
        /// Held until the change is announced, so nothing else writes folder
        /// state in between.
        commit: crate::import::FolderStateCommitGuard,
        adopted: RefreshCompletion,
    },
    Failed {
        error: String,
        adopted: RefreshCompletion,
    },
}

/// What a finished removal leaves the coordinator to announce. The coordinator
/// holds the event stream, so saying what became of the root is its to do.
pub(super) enum RemovalOutcome {
    Removed {
        path: PathBuf,
        /// Held until the events announcing the removal are out, so nothing
        /// else writes folder state in between.
        commit: crate::import::FolderStateCommitGuard,
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

pub(super) struct RootRemovalCompletion {
    id: u64,
    path: PathBuf,
    result: RootRemovalResult,
}

enum RootRemovalResult {
    Removed {
        commit: crate::import::FolderStateCommitGuard,
        /// The scan entries the removal cascaded away.
        removed_keys: Vec<String>,
    },
    Failed(String),
}
