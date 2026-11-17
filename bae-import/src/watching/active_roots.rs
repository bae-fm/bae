//! What the coordinator has going for each watched root: a pass reading it,
//! or a removal, never both.
//!
//! A folder taking over the watched folders inside it is a removal of them
//! with that folder watched in their place.

use super::*;
use bae_store::DbError;
use std::collections::BTreeSet;

/// A caller waiting to hear that what it asked for is over.
pub(super) type RefreshCompletion = tokio::sync::oneshot::Sender<Result<(), String>>;

/// A caller waiting to hear which releases a folder reads as once a person's
/// answer for it is stored.
pub(super) type DecisionCompletion =
    tokio::sync::oneshot::Sender<Result<Vec<crate::CandidateKey>, String>>;

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
    /// The runtime the coordinator's loop runs on, which its passes and
    /// removals run on.
    runtime: tokio::runtime::Handle,
    scan_completions: mpsc::UnboundedSender<RootScanCompletion>,
    next_scan_id: u64,
    removal_backend: Arc<dyn RootRemovalBackend>,
    removal_completions: mpsc::UnboundedSender<RootRemovalCompletion>,
    folder_state_commit: crate::FolderStateCommit,
    next_removal_id: u64,
    /// What a removal runs under, so one that panics ends.
    diagnostics: bae_diagnostics::Diagnostics,
    /// Each root's last pass failure that its scan could not store, until
    /// the root's next pass starts.
    unstored_failures: HashMap<PathBuf, crate::folder_scan_status::UnstoredScanFailure>,
    /// Where this writes what it holds of each root's scan that no scan
    /// stores, after every change, for the scan progress to read.
    tables: crate::tables::ImportTables,
    /// What the last write that succeeded stored there, so a change that
    /// leaves it as it was writes nothing.
    written: crate::folder_scan_status::HeldRootScans,
}

/// What one root has going.
enum RootActivity {
    Scanning(RootScanSchedule),
    /// Held by the removal with this id, as a root it stops watching or as
    /// the folder it watches in their place.
    Removing(u64),
}

/// What one pass over a root reads.
pub(crate) enum RootPass {
    /// Every folder under the root.
    WholeRoot,
    /// One folder whose reading the person changed, stored with the
    /// candidates it gives.
    Decision(FolderReadingRequest),
    /// Folders directly under the root that changed on disk.
    Folders(BTreeSet<std::ffi::OsString>),
}

/// A person's answer for how one folder reads, and who hears the releases it
/// stored, or why it was not stored.
pub(crate) struct FolderReadingRequest {
    target: (
        crate::folder_scanner::FolderReleaseDecisionKey,
        crate::folder_scanner::FolderReleaseDecision,
    ),
    completion: DecisionCompletion,
}

impl FolderReadingRequest {
    pub(super) fn new(
        target: (
            crate::folder_scanner::FolderReleaseDecisionKey,
            crate::folder_scanner::FolderReleaseDecision,
        ),
        completion: DecisionCompletion,
    ) -> Self {
        Self { target, completion }
    }

    /// The folder and how it is to read.
    pub(crate) fn target(
        &self,
    ) -> &(
        crate::folder_scanner::FolderReleaseDecisionKey,
        crate::folder_scanner::FolderReleaseDecision,
    ) {
        &self.target
    }

    /// Tell whoever asked what became of it: the keys of the releases the
    /// folder now reads as, or why it was not stored.
    pub(crate) fn answer(self, result: Result<Vec<crate::CandidateKey>, String>) {
        if self.completion.send(result).is_err() {
            debug!(
                "folder decision caller for {} dropped before it was answered",
                self.target.0.relative_folder.text()
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
    changed: BTreeSet<std::ffi::OsString>,
}

/// What a root's next pass inherits from the one before it.
#[derive(Default)]
struct Queued {
    pending: bool,
    followup_waiters: Vec<RefreshCompletion>,
    readings: std::collections::VecDeque<FolderReadingRequest>,
    changed: BTreeSet<std::ffi::OsString>,
}

struct RootRemovalSchedule {
    /// The removal, which sends its completion even when it panics.
    task: bae_diagnostics::ReportedTask,
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
        Some(parent) if parent == path => {
            format!("{} is taking over the watched folders inside it", parent.display())
        }
        Some(parent) => format!("{} is being taken over by {}", path.display(), parent.display()),
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
    /// The roots, sending each finished scan and removal to
    /// `scan_completions` and `removal_completions`, which the coordinator's
    /// loop hands back to [`Self::finish_scan`] and [`Self::finish_removal`],
    /// and writing what they hold of each root's scan to `tables`.
    pub(super) fn new(
        runtime: tokio::runtime::Handle,
        starter: RootScanStarter,
        removal_backend: Arc<dyn RootRemovalBackend>,
        folder_state_commit: crate::FolderStateCommit,
        diagnostics: bae_diagnostics::Diagnostics,
        scan_completions: mpsc::UnboundedSender<RootScanCompletion>,
        removal_completions: mpsc::UnboundedSender<RootRemovalCompletion>,
        tables: crate::tables::ImportTables,
    ) -> Self {
        Self {
            roots: HashMap::new(),
            removals: HashMap::new(),
            starter,
            runtime,
            scan_completions,
            next_scan_id: 0,
            removal_backend,
            removal_completions,
            folder_state_commit,
            next_removal_id: 0,
            diagnostics,
            unstored_failures: HashMap::new(),
            tables,
            written: crate::folder_scan_status::HeldRootScans::new(),
        }
    }

    /// What the roots hold of each one's scan: running while a whole-root
    /// pass reads it or waits to run, or a removal holds it; otherwise the
    /// last failure its scan could not store.
    fn held(&self) -> crate::folder_scan_status::HeldRootScans {
        use crate::folder_scan_status::HeldRootScan;
        let mut held = crate::folder_scan_status::HeldRootScans::new();
        for (path, activity) in &self.roots {
            let running = match activity {
                RootActivity::Scanning(schedule) => schedule.whole_root || schedule.pending,
                RootActivity::Removing(_) => true,
            };
            if running {
                held.insert(path.clone(), HeldRootScan::Running);
            }
        }
        for (path, failure) in &self.unstored_failures {
            held.entry(path.clone())
                .or_insert_with(|| HeldRootScan::UnstoredFailure(failure.clone()));
        }
        held
    }

    /// Store `held` as what the roots hold of each one's scan, unless the
    /// last write that succeeded stored the same. A step that starts or
    /// queues work stores what it will hold before it changes anything, and
    /// changes nothing when the store refuses; a step that records what
    /// already happened stores it after, and tells whoever waits on it when
    /// the store refuses.
    async fn hold(
        &mut self,
        held: crate::folder_scan_status::HeldRootScans,
    ) -> Result<(), DbError> {
        if self.written == held {
            return Ok(());
        }
        self.tables.hold_root_scans(held.clone()).await?;
        self.written = held;
        Ok(())
    }

    /// Store what the roots hold now, after a step that records what
    /// already happened.
    async fn hold_current(&mut self) -> Result<(), DbError> {
        let held = self.held();
        self.hold(held).await
    }

    /// Whether a removal holds `path`.
    pub(super) fn is_being_removed(&self, path: &Path) -> bool {
        matches!(self.roots.get(path), Some(RootActivity::Removing(_)))
    }

    /// Ask for a pass over `path`, telling `waiter` when it is over. When the
    /// store refuses to hold the root as running, nothing is queued or
    /// started, and `waiter` hears why.
    pub(super) async fn request_scan(
        &mut self,
        path: PathBuf,
        cause: RootScanCause,
        waiter: Option<RefreshCompletion>,
    ) -> Result<(), DbError> {
        let becomes_running = matches!(
            self.roots.get(&path),
            Some(RootActivity::Scanning(schedule)) if !schedule.whole_root && !schedule.pending
        );
        if becomes_running {
            let mut held = self.held();
            held.insert(path.clone(), crate::folder_scan_status::HeldRootScan::Running);
            if let Err(error) = self.hold(held).await {
                if let Some(waiter) = waiter {
                    answer(waiter, Err(not_held(&path, &error)));
                }
                return Err(error);
            }
        }
        match self.roots.get_mut(&path) {
            Some(RootActivity::Scanning(schedule)) => {
                info!("folder scan of {} queued behind the one running: {cause}", path.display());
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
                return self
                    .start_pass(
                        path,
                        RootPass::WholeRoot,
                        waiter.into_iter().collect(),
                        Queued::default(),
                    )
                    .await;
            }
        }
        Ok(())
    }

    /// Read again the folders directly under `path` that changed on disk.
    pub(super) async fn request_folders(
        &mut self,
        path: PathBuf,
        folders: BTreeSet<std::ffi::OsString>,
        cause: RootScanCause,
    ) -> Result<(), DbError> {
        if folders.is_empty() {
            return Ok(());
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
                info!("reading {folders:?} under {} again: {cause}", path.display());
                return self
                    .start_pass(path, RootPass::Folders(folders), Vec::new(), Queued::default())
                    .await;
            }
        }
        Ok(())
    }

    /// Store a person's answer for how one folder under `path` reads, as the
    /// root's next pass. A whole-root pass under way is cancelled and owed
    /// again, since it would write the folder's old reading over the new one.
    pub(super) async fn change_folder_reading(
        &mut self,
        path: PathBuf,
        request: FolderReadingRequest,
    ) -> Result<(), DbError> {
        match self.roots.get_mut(&path) {
            Some(RootActivity::Scanning(schedule)) => {
                if schedule.whole_root {
                    schedule.scan.cancel();
                    schedule.pending = true;
                    schedule.followup_waiters.append(&mut schedule.current_waiters);
                }
                schedule.readings.push_back(request);
            }
            Some(RootActivity::Removing(id)) => {
                let id = *id;
                request.answer(Err(self.why_held(id, &path)));
            }
            None => {
                return self
                    .start_pass(path, RootPass::Decision(request), Vec::new(), Queued::default())
                    .await;
            }
        }
        Ok(())
    }

    /// A pass reported itself over: start what was queued behind it, store
    /// what the root holds now, and answer the pass's waiters. When the store
    /// refuses, the waiters hear why instead of hearing the pass ended.
    pub(super) async fn finish_scan(
        &mut self,
        completion: RootScanCompletion,
    ) -> Result<(), DbError> {
        if !matches!(
            self.roots.get(&completion.path),
            Some(RootActivity::Scanning(schedule)) if schedule.id == completion.id
        ) {
            return Ok(());
        }
        let Some(RootActivity::Scanning(mut schedule)) = self.roots.remove(&completion.path) else {
            return Ok(());
        };
        // A pass that panicked was reported where it ran.
        schedule.scan.ended().await;
        if let Some(failure) = completion.unstored_failure {
            self.unstored_failures.insert(completion.path.clone(), failure);
        }
        let current_waiters = std::mem::take(&mut schedule.current_waiters);
        let mut queued = Queued {
            pending: schedule.pending,
            followup_waiters: std::mem::take(&mut schedule.followup_waiters),
            readings: std::mem::take(&mut schedule.readings),
            changed: std::mem::take(&mut schedule.changed),
        };
        let path = completion.path;
        let next = if let Some(reading) = queued.readings.pop_front() {
            self.start_pass(path.clone(), RootPass::Decision(reading), Vec::new(), queued).await
        } else if !queued.pending && !queued.changed.is_empty() {
            let folders = std::mem::take(&mut queued.changed);
            self.start_pass(path.clone(), RootPass::Folders(folders), Vec::new(), queued).await
        } else if queued.pending {
            info!("folder scan of {} starting again: one was queued while it ran", path.display());
            let waiters = std::mem::take(&mut queued.followup_waiters);
            self.start_pass(path.clone(), RootPass::WholeRoot, waiters, Queued::default()).await
        } else {
            Ok(())
        };
        // The coordinator stores what the root holds before the waiters hear
        // the pass is over, so what they read next is where the pass left
        // the root.
        let held = match next {
            Ok(()) => self.hold_current().await,
            Err(error) => Err(error),
        };
        // The scan stores its own failure; a refresh caller only waits.
        let result = held.as_ref().map(|_| ()).map_err(|error| not_held(&path, error));
        for waiter in current_waiters {
            if waiter.send(result.clone()).is_err() {
                debug!("folder refresh caller dropped before completion");
            }
        }
        held
    }

    /// Stop watching `roots`, watching `parent` — the folder holding exactly
    /// them — in their place when given. Asking again for a removal under way
    /// joins it; asking anything else of a folder it holds is refused.
    pub(super) async fn remove(
        &mut self,
        roots: Vec<PathBuf>,
        parent: Option<PathBuf>,
        completion: RefreshCompletion,
    ) -> Result<(), DbError> {
        let held = roots.iter().chain(parent.iter()).find_map(|path| match self.roots.get(path) {
            Some(RootActivity::Removing(id)) => Some((path, *id)),
            Some(RootActivity::Scanning(_)) | None => None,
        });
        if let Some((path, id)) = held {
            match self.removals.get_mut(&id) {
                Some(removal) if removal.is(&roots, parent.as_deref()) => {
                    removal.completions.push(completion);
                }
                Some(removal) => answer(completion, Err(removal.busy(path))),
                None => answer(completion, Err(missing_removal(id))),
            }
            return Ok(());
        }
        if let Some(parent) = &parent {
            if self.roots.contains_key(parent) {
                answer(completion, Err(format!("{} is already watched", parent.display())));
                return Ok(());
            }
        }
        // The removal holds every root it names, and the parent, as running
        // from its start, so the coordinator stores that before it cancels
        // anything.
        let mut running = self.held();
        for path in roots.iter().chain(parent.iter()) {
            running.insert(path.clone(), crate::folder_scan_status::HeldRootScan::Running);
        }
        if let Err(error) = self.hold(running).await {
            let named: Vec<String> =
                roots.iter().chain(parent.iter()).map(|path| path.display().to_string()).collect();
            answer(
                completion,
                Err(format!("bae could not store the scan state of {}: {error}", named.join(", "))),
            );
            return Err(error);
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
                schedule.scan.cancel();
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
        let panicked_completions = completions.clone();
        let task_roots = roots.clone();
        let task_parent = parent.clone();
        // A removal that panics ends as one that failed: its callers hear why,
        // and the roots it held are read again.
        let task = self.diagnostics.spawn_reported(
            &self.runtime,
            "remove a watched folder",
            async move {
                let result = run_root_removal(
                    &task_roots,
                    task_parent.as_deref(),
                    scans,
                    backend.as_ref(),
                    commit,
                )
                .await;
                if completions.send(RootRemovalCompletion { id, result }).is_err() {
                    debug!("folder scan coordinator ended before removal completion");
                }
            },
            move |failed| {
                let result = RootRemovalResult::Failed(failed.to_string());
                if panicked_completions.send(RootRemovalCompletion { id, result }).is_err() {
                    debug!("folder scan coordinator ended before a panicked removal completed");
                }
            },
        );
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
        Ok(())
    }

    /// A removal reported itself over: start the parent's read when it
    /// landed, or read its roots again when it failed, and return what the
    /// coordinator is to announce, with whether the store took what the
    /// roots hold now.
    pub(super) async fn finish_removal(
        &mut self,
        completion: RootRemovalCompletion,
    ) -> Option<(RemovalOutcome, Result<(), DbError>)> {
        let removal = self.removals.remove(&completion.id)?;
        for path in removal.paths() {
            self.roots.remove(path);
        }
        // A removal that panicked was reported where it ran.
        removal.task.ended().await;
        let mut started_passes = Ok(());
        let outcome = match completion.result {
            RootRemovalResult::Removed { commit, removed } => {
                // The roots it removed are watched no more, and nothing is
                // shown of them.
                for root in &removal.roots {
                    self.unstored_failures.remove(root);
                }
                let (scan_waiters, started) = match removal.parent {
                    // The parent's read covers every folder a waiter asked about.
                    Some(parent) => {
                        let started = self
                            .start_pass(
                                parent,
                                RootPass::WholeRoot,
                                removal
                                    .scan_waiters
                                    .into_iter()
                                    .map(|(_, waiter)| waiter)
                                    .collect(),
                                Queued::default(),
                            )
                            .await;
                        (Vec::new(), started)
                    }
                    None => (removal.scan_waiters, Ok(())),
                };
                started_passes = started;
                RemovalOutcome::Removed {
                    commit,
                    removed,
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
                    let started = self
                        .start_pass(root, RootPass::WholeRoot, waiters, Queued::default())
                        .await;
                    if started_passes.is_ok() {
                        started_passes = started;
                    }
                }
                RemovalOutcome::Failed { error, callers: removal.completions }
            }
        };
        let held = match started_passes {
            Ok(()) => self.hold_current().await,
            Err(error) => Err(error),
        };
        Some((outcome, held))
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
        self.removals.get(&id).map_or_else(|| missing_removal(id), |removal| removal.busy(path))
    }

    /// Cancel every pass without waiting for it.
    pub(super) fn cancel_scans(&self) {
        for activity in self.roots.values() {
            if let RootActivity::Scanning(schedule) = activity {
                schedule.scan.cancel();
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
            schedule.scan.cancel();
            schedule.pending = false;
            for reading in schedule.readings.drain(..) {
                reading.answer(Err("folder scan service stopped".to_string()));
            }
            for waiter in
                schedule.current_waiters.drain(..).chain(schedule.followup_waiters.drain(..))
            {
                if waiter.send(Err("folder scan service stopped".to_string())).is_err() {
                    debug!("folder refresh caller dropped during shutdown");
                }
            }
        }
        for (_, activity) in self.roots.drain() {
            if let RootActivity::Scanning(schedule) = activity {
                schedule.scan.ended().await;
            }
        }
        // Nothing waits on the shutdown's outcome, so a refused write goes to
        // diagnostics; the library empties the rows as it next opens.
        if let Err(error) = self.hold_current().await {
            self.diagnostics.report_failure(
                "store that the folder scan coordinator holds no scan as it stops",
                &error,
            );
        }
        for (_, removal) in self.removals.drain() {
            removal.task.ended().await;
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

    /// Start a pass over `path`, with its waiters and what is queued behind
    /// it. The coordinator first stores what the root holds once the pass
    /// starts: running for a whole-root pass or one queued behind it, since the
    /// pass opens its generation at once with no outcome and a reader must
    /// find the root running by then, and no failure, since the pass clears
    /// the last one. When the store refuses, the pass does not start, and
    /// everyone waiting on it or queued behind it hears why.
    async fn start_pass(
        &mut self,
        path: PathBuf,
        pass: RootPass,
        waiters: Vec<RefreshCompletion>,
        queued: Queued,
    ) -> Result<(), DbError> {
        let whole_root = matches!(pass, RootPass::WholeRoot);
        let mut held = self.held();
        held.remove(&path);
        if whole_root || queued.pending {
            held.insert(path.clone(), crate::folder_scan_status::HeldRootScan::Running);
        }
        if let Err(error) = self.hold(held).await {
            let why = not_held(&path, &error);
            if let RootPass::Decision(request) = pass {
                request.answer(Err(why.clone()));
            }
            for waiter in waiters.into_iter().chain(queued.followup_waiters) {
                answer(waiter, Err(why.clone()));
            }
            for reading in queued.readings {
                reading.answer(Err(why.clone()));
            }
            return Err(error);
        }
        self.next_scan_id += 1;
        let id = self.next_scan_id;
        self.unstored_failures.remove(&path);
        let changed = if whole_root { BTreeSet::new() } else { queued.changed };
        let scan =
            (self.starter)(&self.runtime, id, path.clone(), pass, self.scan_completions.clone());
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
        Ok(())
    }
}

/// Why a pass over `path` did not start or end as asked: the store refused
/// what the coordinator holds of its scan.
fn not_held(path: &Path, error: &DbError) -> String {
    format!("bae could not store the scan state of {}: {error}", path.display())
}

/// What a finished removal leaves the coordinator, which holds the event
/// stream, to announce.
pub(super) enum RemovalOutcome {
    Removed {
        /// Held until the removal is announced, so nothing writes in between.
        commit: crate::FolderStateCommitGuard,
        /// The releases that left the queue, announced so work on them
        /// stops, and the folders the removal left watched.
        removed: crate::tables::WatchedFoldersRemoved,
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

#[cfg(test)]
#[path = "active_roots_tests.rs"]
mod tests;
