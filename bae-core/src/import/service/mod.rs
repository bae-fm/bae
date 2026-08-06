use crate::diagnostics::TelemetryEvent;
use crate::import::handle::{ImportCandidateState, ImportServiceHandle};
use crate::import::handle::{ScanEvent, WatcherCommand};
use crate::import::types::{ImportCommand, ImportProgress, MetadataRef, StorageMode};
use crate::library::LibraryManager;
use crate::util::rate_limiter::CallPriority;

use {
    crate::db::{
        DbAlbum, DbAlbumArtist, DbFile, DbRelease, DbReleaseArtistRole, DbTrack, DbTrackArtist,
        DbTrackArtistRole,
    },
    crate::import::folder_registry::ImportFolderRegistry,
    crate::import::folder_scanner::{ScanItem, ScannedFile},
    crate::import::track_slots::{audio_units, map_source_rows, resolve_track_files},
    crate::import::types::{AudioFile, CoverSelection, ImportPhase, PrepareStep, TrackFile},
    crate::import::ParsedWorkGraph,
    notify_debouncer_full::DebounceEventResult,
    std::collections::{HashMap, HashSet},
    std::path::{Path, PathBuf},
    std::sync::{Arc, Mutex},
};

use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

mod cover_image;
mod folder_watcher;
mod format_prep;
mod progress;
mod reconcile;

use folder_watcher::FolderWatchSnapshot;
pub(crate) use folder_watcher::FolderWatcher;

use format_prep::resolve_file_content_type;

/// What `reconcile_prepared_release` yields: the release's rows with parsed
/// artist IDs already remapped to their real DB IDs, ready for the run pass.
struct PreparedMetadata {
    db_album: DbAlbum,
    db_release: DbRelease,
    db_tracks: Vec<DbTrack>,
    remote_cover_image: Option<cover_image::CoverCandidate>,
    existing_album_id: Option<String>,
    remapped_track_artists: Vec<DbTrackArtist>,
    remapped_album_artists: Vec<DbAlbumArtist>,
    work_graph: ParsedWorkGraph,
    remapped_release_artist_roles: Vec<DbReleaseArtistRole>,
    remapped_track_artist_roles: Vec<DbTrackArtistRole>,
    artists: Vec<crate::db::DbArtist>,
    artist_external_id_updates: Vec<(String, crate::db::DbArtist)>,
    artist_images: Vec<(crate::db::DbLibraryImage, Vec<u8>)>,
    /// Per-source identity rows for the release. Empty for Unknown.
    /// Commit writes one `release_identities` row per element.
    identities: Vec<crate::import::types::ReleaseIdentity>,
    album_title: String,
    artist_name: String,
}

fn storage_mode_label(mode: &StorageMode) -> &'static str {
    match mode {
        StorageMode::Remote => "remote",
        StorageMode::Local => "local",
    }
}

use crate::import::handle::send_event;

/// What the import worker thread receives: an import to run, or the teardown
/// signal `ImportServiceHandle::stop_and_join` sends. The explicit signal (vs
/// waiting for the channel to close) exists because the handle owning the last
/// sender is itself a field of the struct whose `Drop` performs the join —
/// channel closure could never arrive before the join deadlocked.
pub(crate) enum ImportWorkerMessage {
    Import {
        command: ImportCommand,
        expectation: ImportExpectation,
    },
    Shutdown,
}

pub(crate) struct ImportExpectation {
    pub content_hash: String,
    pub edit_revision: u64,
}

pub struct ImportService {
    commands_rx: mpsc::UnboundedReceiver<ImportWorkerMessage>,
    event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
    library_manager: LibraryManager,
}

/// The watched roots that contain at least one of the `changed` paths, in
/// `roots` order and without duplicates.
fn affected_roots(changed: &[&Path], roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .filter(|root| changed.iter().any(|path| path.starts_with(root)))
        .cloned()
        .collect()
}

fn roots_for_watch_error(error_paths: &[PathBuf], roots: &[PathBuf]) -> Vec<PathBuf> {
    let paths: Vec<&Path> = error_paths.iter().map(PathBuf::as_path).collect();
    let affected = affected_roots(&paths, roots);
    if affected.is_empty() {
        roots.to_vec()
    } else {
        affected
    }
}

type RefreshCompletion = tokio::sync::oneshot::Sender<Result<(), String>>;

struct RootScanSchedule {
    id: u64,
    scan: RootScanTask,
    pending: bool,
    current_waiters: Vec<RefreshCompletion>,
    followup_waiters: Vec<RefreshCompletion>,
}

struct RootScanTask {
    cancellation: crate::import::folder_scanner::ScanCancellation,
    task: tokio::task::JoinHandle<()>,
}

struct RootScanCompletion {
    id: u64,
    path: PathBuf,
    result: Result<(), String>,
}

struct RootRemovalSchedule {
    id: u64,
    task: tokio::task::JoinHandle<()>,
    completions: Vec<RefreshCompletion>,
    scan_waiters: Vec<RefreshCompletion>,
}

struct RootRemovalCompletion {
    id: u64,
    path: PathBuf,
    result: RootRemovalResult,
}

enum RootRemovalResult {
    Removed(tokio::sync::OwnedMutexGuard<()>),
    Failed(String),
}

#[async_trait::async_trait]
trait RootRemovalBackend: Send + Sync {
    async fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, String>;
    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String>;
    async fn remove_durable_root(&self, path: &Path) -> Result<(), String>;
}

struct ServiceRootRemovalBackend {
    folder_watcher: Arc<FolderWatcher>,
    library_manager: LibraryManager,
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

    async fn remove_durable_root(&self, path: &Path) -> Result<(), String> {
        self.library_manager
            .remove_watched_import_folder(&path.to_string_lossy())
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
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
    if let Err(error) = backend.remove_durable_root(path).await {
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
    RootRemovalResult::Removed(commit)
}

type RootScanStarter = Arc<
    dyn Fn(u64, PathBuf, mpsc::UnboundedSender<RootScanCompletion>) -> RootScanTask + Send + Sync,
>;

fn spawn_root_scan(
    id: u64,
    path: PathBuf,
    event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
    library_manager: LibraryManager,
    folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    candidate_state: Arc<Mutex<ImportCandidateState>>,
    folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    folder_watcher: Arc<FolderWatcher>,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::import::folder_scanner::ScanCancellation::new();
    let scan_cancellation = cancellation.clone();
    let completion_path = path.clone();
    let task = tokio::spawn(async move {
        let result = if scan_cancellation.is_cancelled() {
            Ok(())
        } else {
            ImportService::rescan_and_reconcile(
                &path,
                &event_tx,
                &library_manager,
                &folder_registry,
                &candidate_state,
                &folder_state_commit,
                &folder_watcher,
                &scan_cancellation,
            )
            .await
            .map_err(|error| error.to_string())
        };
        if completion_tx
            .send(RootScanCompletion {
                id,
                path: completion_path,
                result,
            })
            .is_err()
        {
            debug!("folder scan coordinator ended before scan completion");
        }
    });
    RootScanTask { cancellation, task }
}

fn request_root_scan(
    path: PathBuf,
    waiter: Option<RefreshCompletion>,
    schedules: &mut HashMap<PathBuf, RootScanSchedule>,
    starter: &RootScanStarter,
    completion_tx: &mpsc::UnboundedSender<RootScanCompletion>,
    next_scan_id: &mut u64,
) {
    if let Some(schedule) = schedules.get_mut(&path) {
        schedule.pending = true;
        if let Some(waiter) = waiter {
            schedule.followup_waiters.push(waiter);
        }
        return;
    }
    *next_scan_id += 1;
    let id = *next_scan_id;
    let scan = starter(id, path.clone(), completion_tx.clone());
    schedules.insert(
        path,
        RootScanSchedule {
            id,
            scan,
            pending: false,
            current_waiters: waiter.into_iter().collect(),
            followup_waiters: Vec::new(),
        },
    );
}

impl ImportService {
    async fn record_scan_failure(
        root: &Path,
        generation: u64,
        message: String,
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: &LibraryManager,
        candidate_state: &Arc<Mutex<ImportCandidateState>>,
        folder_state_commit: &Arc<tokio::sync::Mutex<()>>,
    ) -> Result<bool, crate::import::ImportError> {
        let _commit = folder_state_commit.lock().await;
        if !library_manager
            .finish_folder_scan(&root.to_string_lossy(), generation, Some(&message))
            .await?
        {
            return Ok(false);
        }
        if !candidate_state
            .lock()
            .unwrap()
            .fail_root_scan(root, generation, message.clone())
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "failed scan generation {generation} for {} was not current in memory",
                    root.display()
                ),
            });
        }
        let watched_folder =
            crate::import::WatchedFolder::from_path(root.to_string_lossy().into_owned());
        send_event(
            event_tx,
            crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                status: crate::import::handle::WatchedFolderScanStatus {
                    watched_folder_path: watched_folder.path,
                    watched_folder_name: watched_folder.name,
                    status: crate::import::handle::FolderScanStatus::Failed { error: message },
                },
            }),
        );
        Ok(true)
    }

    async fn persist_scan_item(
        root: &Path,
        generation: u64,
        item: &ScanItem,
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: &LibraryManager,
        candidate_state: &Arc<Mutex<ImportCandidateState>>,
        folder_state_commit: &Arc<tokio::sync::Mutex<()>>,
    ) -> Result<Option<(tokio::sync::OwnedMutexGuard<()>, ScanItem)>, crate::import::ImportError>
    {
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
        let removed_keys = candidate_state
            .lock()
            .unwrap()
            .persisted_removals_for_item(root, &item);
        match library_manager
            .save_folder_scan_item(&root.to_string_lossy(), generation, &item, &removed_keys)
            .await
        {
            Ok(true) => Ok(Some((commit, item))),
            Ok(false) => Ok(None),
            Err(write_error) => {
                drop(commit);
                let message = format!("Could not store folder scan result: {write_error}");
                match Self::record_scan_failure(
                    root,
                    generation,
                    message,
                    event_tx,
                    library_manager,
                    candidate_state,
                    folder_state_commit,
                )
                .await
                {
                    Ok(_) => Err(write_error.into()),
                    Err(status_error) => Err(crate::import::ImportError::Internal {
                        detail: format!(
                            "folder scan result write failed: {write_error}; \
                             recording the scan failure also failed: {status_error}"
                        ),
                    }),
                }
            }
        }
    }

    async fn cancel_and_join_folder_walk(
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
    fn start_watcher(
        cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
        fs_rx: mpsc::UnboundedReceiver<DebounceEventResult>,
        event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: LibraryManager,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        candidate_state: Arc<Mutex<ImportCandidateState>>,
        folder_state_commit: Arc<tokio::sync::Mutex<()>>,
        folder_watcher: Arc<FolderWatcher>,
    ) -> std::thread::JoinHandle<()> {
        let scan_event_tx = event_tx.clone();
        let scan_library_manager = library_manager.clone();
        let scan_folder_registry = folder_registry.clone();
        let scan_candidate_state = candidate_state.clone();
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
                scan_candidate_state.clone(),
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
            candidate_state,
            folder_state_commit,
            starter,
            removal_backend,
        )
    }

    fn start_watcher_with_starter(
        mut cmd_rx: mpsc::UnboundedReceiver<WatcherCommand>,
        mut fs_rx: mpsc::UnboundedReceiver<DebounceEventResult>,
        event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: LibraryManager,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        candidate_state: Arc<Mutex<ImportCandidateState>>,
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
                                let Some(ancestor_separate_keys) = candidate_state
                                    .lock()
                                    .unwrap()
                                    .release_boundary_ancestor_keys(&target.0)
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
                                    Ok((invalidation_generation, _)) => {
                                        let mut state = candidate_state.lock().unwrap();
                                        state.begin_root_scan(&path, invalidation_generation);
                                        let superseded = state.apply_release_decisions(&decisions);
                                        drop(state);
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
                            RootRemovalResult::Removed(commit) => {
                                let folders = {
                                    let mut registry = folder_registry.lock().unwrap();
                                    registry.apply_removed(&completion.path.to_string_lossy());
                                    registry.watched_folders()
                                };
                                let removed_keys = candidate_state
                                    .lock()
                                    .unwrap()
                                    .remove_root(&completion.path);
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
                        for waiter in schedule.current_waiters.drain(..) {
                            if waiter.send(completion.result.clone()).is_err() {
                                debug!("folder refresh caller dropped before completion");
                            }
                        }
                        if schedule.pending {
                            let path = completion.path;
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
                        let changed: Vec<&Path> = events
                            .iter()
                            .flat_map(|e| e.paths.iter().map(PathBuf::as_path))
                            .collect();
                        let roots: Vec<PathBuf> = folder_registry
                            .lock()
                            .unwrap()
                            .watched_folders()
                            .into_iter()
                            .map(|folder| PathBuf::from(folder.path))
                            .collect();
                        for root in affected_roots(&changed, &roots) {
                            if removals.contains_key(&root) {
                                continue;
                            }
                            request_root_scan(
                                root,
                                None,
                                &mut schedules,
                                &starter,
                                &completion_tx,
                                &mut next_scan_id,
                            );
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
    async fn rescan_and_reconcile(
        root: &Path,
        event_tx: &broadcast::Sender<crate::import::handle::ImportEvent>,
        library_manager: &LibraryManager,
        folder_registry: &Arc<Mutex<ImportFolderRegistry>>,
        candidate_state: &Arc<Mutex<ImportCandidateState>>,
        folder_state_commit: &Arc<tokio::sync::Mutex<()>>,
        folder_watcher: &Arc<FolderWatcher>,
        cancellation: &crate::import::folder_scanner::ScanCancellation,
    ) -> Result<(), crate::import::ImportError> {
        let root_key = root.to_string_lossy().into_owned();
        let generation = {
            let _commit = folder_state_commit.lock().await;
            let generation = library_manager.begin_folder_scan(&root_key).await?;
            candidate_state
                .lock()
                .unwrap()
                .begin_root_scan(root, generation);
            let watched_folder = crate::import::WatchedFolder::from_path(root_key.clone());
            send_event(
                event_tx,
                crate::import::handle::ImportEvent::Scan(ScanEvent::FolderScanStatusChanged {
                    status: crate::import::handle::WatchedFolderScanStatus {
                        watched_folder_path: watched_folder.path,
                        watched_folder_name: watched_folder.name,
                        status: crate::import::handle::FolderScanStatus::Scanning,
                    },
                }),
            );
            generation
        };
        // What the user has decided about each candidate's files — which audio
        // each sheet describes, and which files are the release's tracks — read
        // once for the whole walk. A folder's roles are only what its filenames
        // propose until these land on top, so the walk takes them with it
        // rather than having every candidate corrected afterwards.
        let stored_edits = match library_manager.load_stored_candidate_edits().await {
            Ok(stored) => stored,
            Err(e) => {
                warn!("stored file decisions could not be read; scan failed: {e}");
                Self::record_scan_failure(
                    root,
                    generation,
                    e.to_string(),
                    event_tx,
                    library_manager,
                    candidate_state,
                    folder_state_commit,
                )
                .await?;
                return Err(e.into());
            }
        };
        let decisions = match library_manager
            .load_folder_release_decisions(&root.to_string_lossy())
            .await
        {
            Ok(decisions) => decisions,
            Err(e) => {
                let message = format!("Folder release decisions could not be read: {e}");
                Self::record_scan_failure(
                    root,
                    generation,
                    message,
                    event_tx,
                    library_manager,
                    candidate_state,
                    folder_state_commit,
                )
                .await?;
                return Err(e.into());
            }
        };
        let root_buf = root.to_path_buf();
        let dropped_item_root = root.to_path_buf();
        let walk_cancellation = cancellation.clone();
        let walk_watcher = folder_watcher.clone();
        let walk_root = root.to_path_buf();
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

        // Every candidate key this walk reported, valid or invalid: a folder
        // that flipped valid → invalid (or back) keeps the same path key, so a
        // removal is only due when a path drops out of the walk entirely.
        let mut seen_keys: HashSet<String> = HashSet::new();
        let mut seen_boundaries = HashSet::new();
        while let Some(item) = item_rx.recv().await {
            if !candidate_state
                .lock()
                .unwrap()
                .generation_is_current(root, generation)
            {
                debug!(
                    "discarding scan generation {generation} for removed root {}",
                    root.display()
                );
                Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk).await?;
                return Ok(());
            }
            match item {
                item @ (ScanItem::Discovered(_) | ScanItem::Valid(_)) => {
                    let (candidate, actionable) = match item {
                        ScanItem::Discovered(candidate) => (candidate, false),
                        ScanItem::Valid(candidate) => (candidate, true),
                        ScanItem::Invalid(_) | ScanItem::Boundary(_) => {
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
                    let Some((commit, persisted_item)) = Self::persist_scan_item(
                        root,
                        generation,
                        &persisted_item,
                        event_tx,
                        library_manager,
                        candidate_state,
                        folder_state_commit,
                    )
                    .await?
                    else {
                        Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk)
                            .await?;
                        return Ok(());
                    };
                    let candidate = match &persisted_item {
                        ScanItem::Discovered(candidate) | ScanItem::Valid(candidate) => {
                            candidate.clone()
                        }
                        ScanItem::Invalid(_) | ScanItem::Boundary(_) => {
                            unreachable!("persisted candidate scan item changed variant")
                        }
                    };
                    let skipped = folder_registry
                        .lock()
                        .unwrap()
                        .is_skipped(&candidate.watched_folder_path, &candidate.path)?;
                    let is_added = library_manager
                        .is_content_hash_imported(&candidate.files.content_hash())
                        .await?;
                    let Some(superseded) =
                        candidate_state.lock().unwrap().apply_scan_item_if_current(
                            root,
                            generation,
                            persisted_item,
                            skipped,
                            is_added,
                        )
                    else {
                        drop(commit);
                        Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk)
                            .await?;
                        return Err(crate::import::ImportError::Internal {
                            detail: format!(
                                "stored scan generation {generation} for {} was not current in memory",
                                root.display()
                            ),
                        });
                    };
                    // Record before announcing: the bus turns the event into an
                    // `ImportCandidateList` invalidation and the UI answers it
                    // by reading the snapshot, so a candidate announced before
                    // it is recorded reads back as still missing.
                    seen_keys.insert(candidate.path.to_string_lossy().into_owned());
                    for candidate_key in superseded {
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
                    let Some((commit, persisted_item)) = Self::persist_scan_item(
                        root,
                        generation,
                        &persisted_item,
                        event_tx,
                        library_manager,
                        candidate_state,
                        folder_state_commit,
                    )
                    .await?
                    else {
                        Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk)
                            .await?;
                        return Ok(());
                    };
                    let Some(removed) = candidate_state.lock().unwrap().apply_scan_item_if_current(
                        root,
                        generation,
                        persisted_item,
                        false,
                        false,
                    ) else {
                        drop(commit);
                        Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk)
                            .await?;
                        return Err(crate::import::ImportError::Internal {
                            detail: format!(
                                "stored scan generation {generation} for {} was not current in memory",
                                root.display()
                            ),
                        });
                    };
                    seen_keys.insert(candidate.path.to_string_lossy().into_owned());
                    for candidate_key in removed {
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
                    let Some((commit, persisted_item)) = Self::persist_scan_item(
                        root,
                        generation,
                        &persisted_item,
                        event_tx,
                        library_manager,
                        candidate_state,
                        folder_state_commit,
                    )
                    .await?
                    else {
                        Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk)
                            .await?;
                        return Ok(());
                    };
                    let Some(removed) = candidate_state.lock().unwrap().apply_scan_item_if_current(
                        root,
                        generation,
                        persisted_item,
                        false,
                        false,
                    ) else {
                        drop(commit);
                        Self::cancel_and_join_folder_walk(root, cancellation, &mut item_rx, walk)
                            .await?;
                        return Err(crate::import::ImportError::Internal {
                            detail: format!(
                                "stored scan generation {generation} for {} was not current in memory",
                                root.display()
                            ),
                        });
                    };
                    seen_boundaries.insert(boundary.key.clone());
                    for candidate_key in removed {
                        seen_keys.remove(&candidate_key);
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
            }
        }

        // The walk finished (or failed) once its sender dropped and closed the
        // channel above. A root read failure preserves the progressive items
        // already committed by this pass plus older items that were not
        // explicitly replaced.
        let seen_directories = match walk.await {
            Ok((Ok(()), seen_directories)) => seen_directories,
            Ok((Err(e), _)) => {
                if cancellation.is_cancelled()
                    || !candidate_state
                        .lock()
                        .unwrap()
                        .generation_is_current(root, generation)
                {
                    return Ok(());
                }
                warn!(
                    "re-scan of {} failed ({e}); keeping previous candidates",
                    root.display()
                );
                Self::record_scan_failure(
                    root,
                    generation,
                    e.to_string(),
                    event_tx,
                    library_manager,
                    candidate_state,
                    folder_state_commit,
                )
                .await?;
                return Err(e.into());
            }
            Err(e) => {
                if cancellation.is_cancelled() {
                    return Ok(());
                }
                error!("folder scan task panicked for {}: {e}", root.display());
                Self::record_scan_failure(
                    root,
                    generation,
                    e.to_string(),
                    event_tx,
                    library_manager,
                    candidate_state,
                    folder_state_commit,
                )
                .await?;
                return Err(crate::import::ImportError::Internal {
                    detail: format!("folder scan task failed: {e}"),
                });
            }
        };

        if !candidate_state
            .lock()
            .unwrap()
            .generation_is_current(root, generation)
        {
            return Ok(());
        }
        drop(seen_directories);

        let commit = folder_state_commit.clone().lock_owned().await;
        let finished = match library_manager
            .finish_folder_scan(&root_key, generation, None)
            .await
        {
            Ok(finished) => finished,
            Err(write_error) => {
                drop(commit);
                let message = format!("Could not finish folder scan: {write_error}");
                if let Err(status_error) = Self::record_scan_failure(
                    root,
                    generation,
                    message,
                    event_tx,
                    library_manager,
                    candidate_state,
                    folder_state_commit,
                )
                .await
                {
                    return Err(crate::import::ImportError::Internal {
                        detail: format!(
                            "folder scan completion write failed: {write_error}; \
                             recording the scan failure also failed: {status_error}"
                        ),
                    });
                }
                return Err(write_error.into());
            }
        };
        if !finished {
            return Ok(());
        }

        // The generation check, pruning, and status change share one lock: a
        // newer decision or scan cannot be pruned by this completed DB write.
        let Some(removed) = candidate_state.lock().unwrap().finish_root_scan(
            root,
            generation,
            &seen_keys,
            &seen_boundaries,
        ) else {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "completed scan generation {generation} for {} was not current in memory",
                    root.display()
                ),
            });
        };
        for candidate_key in removed {
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
                status: crate::import::handle::WatchedFolderScanStatus {
                    watched_folder_path: watched_folder.path,
                    watched_folder_name: watched_folder.name,
                    status: crate::import::handle::FolderScanStatus::Complete,
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

    /// Start the import service worker: one task that drains the import queue
    /// sequentially, never concurrently. The returned handle is cloneable and
    /// is how the rest of the app submits import requests.
    pub async fn start(
        runtime_handle: tokio::runtime::Handle,
        library_manager: LibraryManager,
    ) -> Result<ImportServiceHandle, crate::import::ImportError> {
        let (commands_tx, commands_rx) = mpsc::unbounded_channel();
        let (watcher_tx, watcher_rx) = mpsc::unbounded_channel();
        let (fs_tx, fs_rx) = mpsc::unbounded_channel::<DebounceEventResult>();
        let (event_tx, _) = broadcast::channel(1024);
        let event_tx_for_worker = event_tx.clone();
        let library_manager_for_handle = library_manager.clone();
        // One `Arc` shared by the watcher (which reads the skip set while stamping
        // candidates) and the handle (which mutates it on add/remove/skip).
        let loaded_registry = library_manager_for_handle
            .load_import_folder_registry()
            .await?;
        let watched_roots: HashSet<String> = loaded_registry
            .watched_folders()
            .into_iter()
            .map(|folder| folder.path)
            .collect();
        let persisted_scans = library_manager_for_handle
            .load_folder_scan_snapshots()
            .await?;
        let imported_content_hashes = library_manager_for_handle.imported_content_hashes().await?;
        let mut loaded_candidates = ImportCandidateState::default();
        loaded_candidates.restore_folder_scans(
            persisted_scans,
            &watched_roots,
            &loaded_registry,
            &imported_content_hashes,
        )?;
        let folder_registry = Arc::new(Mutex::new(loaded_registry));
        let candidate_state = Arc::new(Mutex::new(loaded_candidates));
        let folder_state_commit = Arc::new(tokio::sync::Mutex::new(()));

        // Constructed before the watcher task spawns; the task doesn't need the
        // debouncer, only the `fs_rx` end of its event channel.
        let folder_watcher = Arc::new(FolderWatcher::new(fs_tx));

        let watcher_thread = ImportService::start_watcher(
            watcher_rx,
            fs_rx,
            event_tx.clone(),
            library_manager_for_handle.clone(),
            folder_registry.clone(),
            candidate_state.clone(),
            folder_state_commit.clone(),
            folder_watcher.clone(),
        );

        let worker_thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create import runtime");
            rt.block_on(async move {
                let mut service = ImportService {
                    commands_rx,
                    event_tx: event_tx_for_worker,
                    library_manager,
                };

                while let Some(message) = service.commands_rx.recv().await {
                    match message {
                        ImportWorkerMessage::Import {
                            command,
                            expectation,
                        } => service.do_import(command, expectation).await,
                        ImportWorkerMessage::Shutdown => break,
                    }
                }
            });
        });

        Ok(ImportServiceHandle::new(
            commands_tx,
            worker_thread,
            watcher_thread,
            library_manager_for_handle,
            runtime_handle,
            watcher_tx,
            event_tx,
            folder_registry,
            candidate_state,
            folder_state_commit,
        ))
    }

    async fn do_import(&self, command: ImportCommand, expectation: ImportExpectation) {
        let import_id = command.import_id.clone();
        let candidate_key = command.candidate_key.clone();
        let result = self
            .prepare_and_run_folder_import(
                import_id.clone(),
                candidate_key.clone(),
                command.folder,
                command.scope,
                expectation.content_hash,
                expectation.edit_revision,
                command.selected_cover,
                command.storage_mode,
                command.pin,
                command.identity_choice,
                command.user_edit,
            )
            .await;

        if let Err(e) = result {
            error!("Import failed: {}", e);
            self.library_manager
                .diagnostics()
                .event(TelemetryEvent::ImportFailed {});
            // The typed error becomes a user-facing string only here, at the
            // pipeline's terminal consumer. The variant Displays embed their
            // `#[from]` source messages, so `to_string()` carries the chain.
            let error = e.to_string();

            send_event(
                &self.event_tx,
                crate::import::handle::ImportEvent::ImportProgress {
                    candidate_key,
                    progress: ImportProgress::Failed { error, import_id },
                },
            );
        }
    }

    /// Prepare and run a folder import. Exact / Approximate source the release
    /// through `prepare_release` (reading the network LRU caches the UI's
    /// prefetch warmed, so normally a hit); Unknown reads the candidate's local
    /// evidence through `map_unknown_candidate_to_db`. Either way, the folder is
    /// walked for files, then track mapping and `run_import` follow.
    async fn prepare_and_run_folder_import(
        &self,
        import_id: String,
        candidate_key: String,
        folder: PathBuf,
        scope: crate::import::folder_scanner::ReleaseFileScope,
        expected_content_hash: String,
        expected_edit_revision: u64,
        selected_cover: Option<CoverSelection>,
        storage_mode: StorageMode,
        pin: bool,
        identity_choice: crate::import::IdentityChoice,
        user_edit: Option<crate::import::ReleaseUserEdit>,
    ) -> Result<(), crate::import::ImportError> {
        let library_manager = &self.library_manager;

        let import_start = std::time::Instant::now();
        let mut step_times: Vec<(&str, std::time::Duration)> = Vec::new();
        let mut last_step_start = import_start;

        // Re-walk the folder. Scan and commit are separated by user interaction,
        // and the user can move, rename, or reorganize in that window — so the
        // worker treats the disk at commit time as the source of truth. Their
        // sheet bindings come with it: what the folder is includes what they
        // said it is, and a commit that re-derived without them would import
        // the shape they corrected.
        let folder_buf = folder.clone();
        let stored_edits = library_manager.load_stored_candidate_edits().await?;
        let current_edit_revision = stored_edits.revision_for_hash(&expected_content_hash);
        let categorized = tokio::task::spawn_blocking(move || {
            crate::import::folder_scanner::collect_release_candidate_files_with_scope(
                &folder_buf,
                scope,
                &stored_edits,
            )
        })
        .await
        .map_err(|e| crate::import::ImportError::Internal {
            detail: format!("Folder scan task failed: {e}"),
        })??;
        if categorized.content_hash() != expected_content_hash
            || current_edit_revision != expected_edit_revision
        {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "{candidate_key} changed after it was selected; refresh and identify it again"
                ),
            });
        }

        // Overwrites a prior import of the same files (below), then gets stamped
        // onto the new release row.
        let content_hash = categorized.content_hash();
        let replacement_plans = library_manager
            .import_replacement_plans_for_content_hash(&content_hash)
            .await?;
        let replacement_release_ids: Vec<String> = replacement_plans
            .iter()
            .map(|plan| plan.db_delete.release_id.clone())
            .collect();

        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.clone(),
                progress: ImportProgress::Preparing {
                    import_id: import_id.clone(),
                    step: PrepareStep::ParsingMetadata,
                    album_title: String::new(),
                    artist_name: String::new(),
                },
            },
        );

        let parsed = match &identity_choice {
            crate::import::IdentityChoice::Exact { release_ref }
            | crate::import::IdentityChoice::Approximate { release_ref } => {
                // The documents are archived by `prepare_release`, keyed by the
                // picked source release — so nothing about this release's rows
                // needs to carry them, and the pointer written below is what
                // finds them again.
                prepare_release(library_manager, release_ref, CallPriority::Interactive)
                    .await?
                    .parsed(
                        library_manager.clock().as_ref(),
                        library_manager.ids().as_ref(),
                    )?
            }
            crate::import::IdentityChoice::Unknown => {
                let folder_name = folder
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());
                let clock = library_manager.clock().clone();
                let ids = library_manager.ids().clone();
                let categorized_for_seed = categorized.clone();
                let parsed = tokio::task::spawn_blocking(move || {
                    crate::import::file_tag_mapper::map_unknown_candidate_to_db(
                        &categorized_for_seed,
                        folder_name.as_deref(),
                        clock.as_ref(),
                        ids.as_ref(),
                    )
                })
                .await
                .map_err(|e| crate::import::ImportError::Internal {
                    detail: format!("unknown-seed mapping task failed: {e}"),
                })??;
                parsed
            }
        };

        // The release's track rows and the folder's audio are reconciled before
        // any metadata is applied, so everything downstream works on the track
        // list the import will actually write.
        let mut parsed = parsed;
        let mut user_edit = user_edit;
        let track_bindings = settle_track_rows(
            &mut parsed,
            &mut user_edit,
            &categorized,
            library_manager.ids().as_ref(),
            library_manager.clock().now(),
        );

        let mut prepared = self
            .reconcile_prepared_release(
                parsed,
                &identity_choice,
                user_edit,
                &replacement_release_ids,
            )
            .await?;

        let emit_preparing = {
            let import_id = import_id.clone();
            let candidate_key = candidate_key.clone();
            let album_title = prepared.album_title.clone();
            let artist_name = prepared.artist_name.clone();
            let event_tx = self.event_tx.clone();
            move |step: PrepareStep| {
                send_event(
                    &event_tx,
                    crate::import::handle::ImportEvent::ImportProgress {
                        candidate_key: candidate_key.clone(),
                        progress: ImportProgress::Preparing {
                            import_id: import_id.clone(),
                            step,
                            album_title: album_title.clone(),
                            artist_name: artist_name.clone(),
                        },
                    },
                );
            }
        };

        step_times.push(("resolve_metadata", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        prepared.db_release.source_folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        prepared.db_release.content_hash = Some(content_hash);

        // The content type comes from the HTTP response, so no magic-byte
        // sniffing is needed to reject a non-image download. It describes the
        // download, not the stored blob — the resize below re-encodes those bytes
        // — so it is checked and dropped, never recorded.
        let remote_cover_data = if let Some(CoverSelection::Remote(ref url, source)) =
            selected_cover
        {
            emit_preparing(PrepareStep::WritingCoverArt);
            let crate::import::cover_art::RemoteImage {
                bytes,
                content_type,
                validator: _,
            } = self
                .library_manager
                .remote_images()
                .fetch_required(url)
                .await?;
            if matches!(
                content_type,
                crate::util::content_type::ContentType::OctetStream
            ) {
                return Err(crate::import::ImportError::CoverArt {
                    detail: "Cover bytes aren't a recognized image format (PNG/JPEG/GIF/WebP/BMP)"
                        .to_string(),
                });
            }
            Some(cover_image::CoverCandidate {
                bytes,
                source: source.as_str().to_string(),
                source_url: Some(url.clone()),
            })
        } else {
            None
        };

        step_times.push(("write_cover_art", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        emit_preparing(PrepareStep::DiscoveringFiles);
        let discovered_files = crate::import::handle::flatten_categorized_files(&categorized);

        step_times.push(("discover_files", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        // Each DbTrack moves into its TrackFile variant, bound to the audio its
        // slot named and carrying the `duration_ms` that audio yields. Past here
        // the DbTracks live in `tracks_to_files`.
        emit_preparing(PrepareStep::ValidatingTracks);
        let tracks_to_files = resolve_track_files(
            std::mem::take(&mut prepared.db_tracks)
                .into_iter()
                .zip(track_bindings)
                .collect(),
            &categorized,
        )?;

        let selected_cover_path = match &selected_cover {
            Some(CoverSelection::Local(path)) => Some(path.as_str()),
            _ => None,
        };

        // Embedded cover art is the lowest-priority source: `run_import` uses it
        // only when neither an explicit pick nor a folder image supplies one.
        // Only tagged rips carry a picture, which is the Unknown path, so
        // Exact/Approximate imports skip the read entirely.
        let embedded_cover = if selected_cover.is_none()
            && matches!(identity_choice, crate::import::IdentityChoice::Unknown)
        {
            let audio_paths = categorized.audio_paths();
            tokio::task::spawn_blocking(move || {
                crate::import::file_tag_mapper::read_embedded_cover(&audio_paths)
            })
            .await
            .map_err(|e| crate::import::ImportError::Internal {
                detail: format!("embedded-cover read task failed: {e}"),
            })??
        } else {
            None
        };

        step_times.push(("validate_tracks", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        emit_preparing(PrepareStep::SavingToDatabase);

        // No storage yet: the winning cover's bytes go to coven's local store below
        // and its row is written by finalize.
        prepared.remote_cover_image = remote_cover_data;

        step_times.push(("save_to_database", last_step_start.elapsed()));
        last_step_start = std::time::Instant::now();

        debug!(
            "Prepared album '{}' (release: {}) with {} tracks",
            prepared.db_album.title,
            prepared.db_release.id,
            tracks_to_files.len()
        );

        self.run_import(
            &storage_mode,
            pin,
            &mut prepared,
            &discovered_files,
            &tracks_to_files,
            selected_cover_path,
            &import_id,
            &candidate_key,
            embedded_cover,
            &replacement_plans,
        )
        .await?;

        step_times.push(("storage", last_step_start.elapsed()));

        let total_duration = import_start.elapsed();
        // The release is written (`run_import` succeeded via `?` above). Report
        // the real track count and the monotonic elapsed — never a zero default.
        self.library_manager
            .diagnostics()
            .event(TelemetryEvent::ImportCompleted {
                track_count: tracks_to_files.len() as u32,
                duration_ms: total_duration,
            });
        let step_summary: Vec<String> = step_times
            .iter()
            .map(|(name, dur)| format!("{}={:.0?}", name, dur))
            .collect();
        info!(
            "Import timing for '{}': total={:.0?} [{}]",
            prepared.album_title,
            total_duration,
            step_summary.join(", ")
        );

        if std::env::var("BAE_IMPORT_TRACE").is_ok_and(|v| v == "1") {
            if let Some(home) = std::env::var_os("HOME") {
                let trace_dir = PathBuf::from(home).join(".bae-traces");
                if let Err(e) = std::fs::create_dir_all(&trace_dir) {
                    warn!("import trace dir {:?}: {}", trace_dir, e);
                }
                let trace_path = trace_dir.join("imports.jsonl");
                let line = import_trace_line(
                    library_manager.clock().now().to_rfc3339(),
                    &import_id,
                    &prepared.album_title,
                    &prepared.artist_name,
                    total_duration,
                    &step_times,
                );
                match std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&trace_path)
                {
                    Ok(mut f) => {
                        use std::io::Write;
                        if let Err(e) = f.write_all(line.as_bytes()) {
                            warn!("import trace write {:?}: {}", trace_path, e);
                        }
                    }
                    Err(e) => warn!("import trace open {:?}: {}", trace_path, e),
                }
            }
        }

        Ok(())
    }

    /// Run an import. ONE path regardless of storage mode: build DbFile +
    /// audio-format records, reference the files in place, measure loudness,
    /// then finalize atomically as a LOCAL release (playable immediately) and
    /// emit events. No bytes move here, and every DB write lands in the single
    /// transaction at the end.
    ///
    /// A `Remote` import then transitions to the cloud via `coven_make_remote`,
    /// carrying `pin` as the upload's retain-pinned intent; coven flips `remote`
    /// true once the last upload lands. `pin` is ignored for a `Local` import.
    #[allow(clippy::too_many_arguments)]
    async fn run_import(
        &self,
        storage_mode: &StorageMode,
        pin: bool,
        prepared: &mut PreparedMetadata,
        discovered_files: &[ScannedFile],
        tracks_to_files: &[TrackFile],
        selected_cover_path: Option<&str>,
        import_id: &str,
        candidate_key: &str,
        embedded_cover: Option<(Vec<u8>, crate::util::content_type::ContentType)>,
        replacement_plans: &[crate::library::manager::ImportReplacementPlan],
    ) -> Result<(), crate::import::ImportError> {
        let library_manager = &self.library_manager;
        let total_files = discovered_files.len();
        let PreparedMetadata {
            db_album,
            db_release,
            existing_album_id,
            remapped_track_artists,
            remapped_album_artists,
            work_graph,
            remapped_release_artist_roles,
            remapped_track_artist_roles,
            artists,
            artist_external_id_updates,
            artist_images,
            identities,
            remote_cover_image,
            ..
        } = prepared;
        let new_album = existing_album_id.is_none().then_some(&*db_album);
        let album_id = existing_album_id.as_deref().unwrap_or(&db_album.id);

        self.emit_started(candidate_key, &db_release.id, import_id);
        debug!(
            "Starting {} import for release {} ({} files)",
            storage_mode_label(storage_mode),
            db_release.id,
            total_files,
        );

        // Keyed by absolute path, the same key TrackFile uses, so disc-subfolder
        // siblings with identical bare filenames stay distinct.
        let files_now = library_manager.clock().now();
        let mut db_files: Vec<DbFile> = Vec::with_capacity(total_files);
        let mut file_ids: HashMap<PathBuf, String> = HashMap::new();
        for file in discovered_files.iter() {
            // coven verifies this blob's bytes against this hash on every
            // cloud fetch — required so a later make-Remote + pin round trip
            // (or another device's download) can ever read it back. See
            // `crate::util::fs::hash_file`.
            let content_hash = crate::util::fs::hash_file(&file.path).map_err(|e| {
                crate::import::ImportError::UnusableFile {
                    detail: format!("failed to hash {}: {e}", file.path.display()),
                }
            })?;
            let db_file = DbFile::new(
                &db_release.id,
                &file.relative_path,
                file.size as i64,
                resolve_file_content_type(&file.path)?,
                library_manager.ids().new_id(),
                files_now,
                content_hash,
            );
            file_ids.insert(file.path.clone(), db_file.id.clone());
            db_files.push(db_file);
        }

        // Every import lands LOCAL: reference the files in place and record their
        // common-ancestor folder as the release's local source. Until a Remote
        // import's upload lands it stays a valid, playable local release, so
        // another device never sees a release before its audio is in the cloud.
        let local_root = {
            let mut ancestor: Option<&Path> = None;
            for file in discovered_files.iter() {
                let parent =
                    file.path
                        .parent()
                        .ok_or_else(|| crate::import::ImportError::Internal {
                            detail: format!("File has no parent: {:?}", file.path),
                        })?;
                ancestor = Some(match ancestor {
                    None => parent,
                    Some(a) => common_ancestor(a, parent),
                });
            }
            ancestor.ok_or_else(|| crate::import::ImportError::Internal {
                detail: "No files to determine local path".to_string(),
            })?
        };
        let local_path = local_root
            .to_str()
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("Cannot convert path to string: {:?}", local_root),
            })?
            .to_string();

        // Per-track progress jumps to 100% immediately — files are referenced in
        // place, no bytes move.
        let file_to_tracks: HashMap<PathBuf, Vec<String>> = {
            let mut map: HashMap<PathBuf, Vec<String>> = HashMap::new();
            for tf in tracks_to_files {
                map.entry(tf.file_path().to_path_buf())
                    .or_default()
                    .push(tf.db_track().id.clone());
            }
            map
        };
        for (idx, file) in discovered_files.iter().enumerate() {
            if let Some(track_ids) = file_to_tracks.get(&file.path) {
                for track_id in track_ids {
                    self.emit_phase_progress(
                        candidate_key,
                        track_id,
                        100,
                        ImportPhase::ReferencingFiles,
                        import_id,
                    );
                }
            }
            let release_percent = ((idx + 1) * 100 / total_files.max(1)) as u8;
            self.emit_phase_progress(
                candidate_key,
                &db_release.id,
                release_percent,
                ImportPhase::ReferencingFiles,
                import_id,
            );
            debug!(
                "Recorded file {}/{}: {}",
                idx + 1,
                total_files,
                file.relative_path,
            );
        }

        let mut built_audio = Self::build_audio_formats(
            tracks_to_files,
            &file_ids,
            self.library_manager.clock().as_ref(),
            self.library_manager.ids().as_ref(),
        )?;

        // Measured from the source decode: bae stores originals verbatim (no
        // transcode), so source samples == stored samples. The sources are always
        // present here — every import references them in place and lands local, and
        // a remote import's uploads queue only after finalize. Per-track and album
        // NULLs are legitimate "not measured" results, each logged at its skip
        // point inside `measure_loudness`.
        //
        // Unconditional: `import::loudness` compiles under the same predicate this
        // module does, so there is no configuration where the import runs and the
        // measurement doesn't. A `cfg` here could say otherwise, and once did.
        self.emit_phase_progress(
            candidate_key,
            &db_release.id,
            0,
            ImportPhase::MeasuringLoudness,
            import_id,
        );
        let loudness = crate::import::loudness::measure_loudness(
            &self.event_tx,
            &mut built_audio.audio_formats,
            &built_audio.audio_segments,
            &file_ids,
            tracks_to_files,
            candidate_key,
        )
        .await;
        db_release.album_loudness_lufs = loudness.album_loudness_lufs;
        db_release.album_peak_linear = loudness.album_peak_linear;

        // A track that didn't decode fully (fatal errors, a truncated body) would
        // import fine and then fail at play time. With verify on, fail now —
        // before finalize commits anything to the library.
        if self.library_manager.get_config().verify_decode_on_import && !loudness.broken.is_empty()
        {
            return Err(crate::import::ImportError::DecodeVerification {
                broken: loudness.broken,
            });
        }

        // Cover priority: Remote > local folder image > embedded. Finalize writes
        // the winner's bytes and row in one coven batch.
        let cover_candidate = match remote_cover_image.take() {
            Some(remote) => Some(remote),
            None => match self.pick_folder_cover(discovered_files, selected_cover_path)? {
                Some(local) => Some(local),
                None => embedded_cover.map(|(bytes, _content_type)| cover_image::CoverCandidate {
                    bytes,
                    source: "embedded".to_string(),
                    source_url: None,
                }),
            },
        };
        // Resize the winner to a ≤600px JPEG thumbnail — one funnel for all three
        // sources — and build the row from that output, so its hash, size and
        // content type describe the blob that gets stored rather than the image it
        // was made from. `finalize_import_atomic` derives the readable
        // `cloud_path` extension from the same row.
        let cover_winner = match cover_candidate {
            Some(candidate) => {
                let bytes = crate::util::cover::resize_cover(&candidate.bytes)
                    .map_err(|detail| crate::import::ImportError::CoverArt { detail })?;
                let image = crate::db::DbLibraryImage::cover(
                    &db_release.id,
                    &library_manager.ids().new_id(),
                    &candidate.source,
                    candidate.source_url,
                    &bytes,
                    library_manager.clock().now(),
                );
                Some((image, bytes))
            }
            None => None,
        };
        let library_image = cover_winner
            .as_ref()
            .map(|(image, bytes)| (image, bytes.as_slice()));
        let artist_images: Vec<_> = artist_images
            .iter()
            .map(|(image, bytes)| (image, bytes.as_slice()))
            .collect();
        let cover_rel_id = Some((album_id, db_release.id.as_str()));

        self.emit_phase_progress(
            candidate_key,
            &db_release.id,
            0,
            ImportPhase::Finalizing,
            import_id,
        );

        let remote_intent = matches!(storage_mode, StorageMode::Remote);
        library_manager
            .finalize_import_atomic(
                new_album,
                db_release,
                tracks_to_files,
                remapped_track_artists,
                remapped_album_artists,
                &work_graph.works,
                &work_graph.work_artists,
                &work_graph.work_parts,
                &work_graph.track_works,
                remapped_release_artist_roles,
                remapped_track_artist_roles,
                artists,
                artist_external_id_updates,
                &db_files,
                &built_audio.audio_formats,
                &built_audio.audio_segments,
                library_image,
                &artist_images,
                cover_rel_id,
                identities,
                &local_path,
                replacement_plans,
            )
            .await?;

        // A Remote import transitions to the cloud in the background — the same
        // flow the "Make Remote" action runs: coven uploads each file from its
        // external (in-place) source, and on the last flips `remote` true, drops
        // the external refs, and re-emits the subtree (the cover rides along). The
        // user's original files stay where they are — coven never deletes a
        // user-provided source. This runs BEFORE the events below so the outbox
        // already holds the upload by the time any consumer observes the release
        // or `Complete`.
        if remote_intent {
            if let Err(e) = library_manager.coven_make_remote(&db_release.id, pin).await {
                let remote_error = format!(
                    "Remote import of {} could not start its cloud upload: {e}",
                    db_release.id
                );
                if let Err(delete_error) = library_manager
                    .fail_import_and_delete_release(&db_release.id)
                    .await
                {
                    return Err(crate::import::ImportError::Internal {
                        detail: format!(
                            "{remote_error}; removing the release it had already finalized failed: {delete_error}"
                        ),
                    });
                }
                return Err(crate::import::ImportError::Db(
                    crate::library::LibraryError::Storage(remote_error),
                ));
            }
        }

        if new_album.is_some() {
            library_manager.emit_album_added(album_id).await;
        } else {
            library_manager
                .emit_release_added(album_id, &db_release.id)
                .await;
        }

        let progress = if remote_intent {
            ImportProgress::RemoteUploadQueued {
                id: db_release.id.to_string(),
                import_id: import_id.to_string(),
                album_id: album_id.to_string(),
            }
        } else {
            ImportProgress::Complete {
                id: db_release.id.to_string(),
                import_id: import_id.to_string(),
                album_id: album_id.to_string(),
            }
        };
        send_event(
            &self.event_tx,
            crate::import::handle::ImportEvent::ImportProgress {
                candidate_key: candidate_key.to_string(),
                progress,
            },
        );

        info!("Import complete for release {}", db_release.id);
        Ok(())
    }
}

fn import_trace_line(
    ts: String,
    import_id: &str,
    album_title: &str,
    artist_name: &str,
    total_duration: std::time::Duration,
    step_times: &[(&str, std::time::Duration)],
) -> String {
    let steps: serde_json::Map<String, serde_json::Value> = step_times
        .iter()
        .map(|(name, dur)| ((*name).to_string(), serde_json::json!(dur.as_millis())))
        .collect();
    let mut line = serde_json::json!({
        "ts": ts,
        "import_id": import_id,
        "album": album_title,
        "artist": artist_name,
        "total_ms": total_duration.as_millis(),
        "steps": steps,
    })
    .to_string();
    line.push('\n');
    line
}

/// Reconcile the release's track rows with the folder's audio, and report which
/// audio each surviving track is bound to.
///
/// The command's edit carries the track slots the user saw, each row naming the
/// audio bound to it — that is the mapping, and it wins. A command whose edit
/// names no audio at all changed metadata without opening the slot table (the
/// CLI, an automation script, a surface with no mapping pane), so the slots are
/// computed here from this folder and this tracklist, exactly as picking the
/// release computes them; whatever metadata that edit does carry still applies,
/// row for row.
///
/// Rows the user left with no audio have no samples to write, so they do not
/// become tracks, and the seeded track each stood for takes its artist, role and
/// work rows with it. Rows past the end of the source's tracklist are audio the
/// source does not account for and get a fresh track row.
///
/// The returned bindings are positionally aligned with `parsed.tracks` and with
/// the edit's `tracks`, all three the same length.
fn settle_track_rows(
    parsed: &mut crate::import::ParsedAlbum,
    user_edit: &mut Option<crate::import::ReleaseUserEdit>,
    files: &crate::import::folder_scanner::CategorizedFiles,
    ids: &dyn coven::IdProvider,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<AudioFile> {
    use crate::import::TrackUserEdit;

    let carries_mapping = user_edit
        .as_ref()
        .is_some_and(|edit| edit.tracks.iter().any(|track| track.file.is_some()));

    let rows: Vec<TrackUserEdit> = if carries_mapping {
        user_edit
            .as_ref()
            .expect("an edit that carries a mapping is present")
            .tracks
            .clone()
    } else {
        let source_rows: Vec<TrackUserEdit> = parsed
            .tracks
            .iter()
            .map(|track| TrackUserEdit {
                title: track.title.clone(),
                side: track.side,
                track_number: track.track_number,
                artist_names: Vec::new(),
                file: None,
            })
            .collect();
        map_source_rows(&source_rows, &audio_units(files))
            .into_iter()
            .enumerate()
            .map(|(index, mut row)| {
                // A metadata-only edit still speaks for the rows it has.
                if let Some(edited) = user_edit.as_ref().and_then(|e| e.tracks.get(index)) {
                    row.title = edited.title.clone();
                    row.side = edited.side;
                    row.track_number = edited.track_number;
                    row.artist_names = edited.artist_names.clone();
                }
                row
            })
            .collect()
    };

    let mut seeded: Vec<Option<crate::db::DbTrack>> = std::mem::take(&mut parsed.tracks)
        .into_iter()
        .map(Some)
        .collect();
    let mut tracks = Vec::with_capacity(rows.len());
    let mut bindings = Vec::with_capacity(rows.len());
    let mut kept_rows = Vec::with_capacity(rows.len());

    for (index, row) in rows.into_iter().enumerate() {
        let Some(file) = row.file.clone() else {
            continue;
        };
        let track = match seeded.get_mut(index).and_then(Option::take) {
            Some(track) => track,
            None => crate::db::DbTrack {
                id: ids.new_id(),
                release_id: parsed.release.id.clone(),
                title: row.title.clone(),
                side: row.side,
                track_number: row.track_number,
                duration_ms: None,
                // The source knows nothing about this track, so it has no
                // position in the source's tracklist to record.
                discogs_position: None,
                created_at: now,
            },
        };
        tracks.push(track);
        bindings.push(file);
        kept_rows.push(row);
    }

    let dropped: HashSet<String> = seeded.into_iter().flatten().map(|track| track.id).collect();
    if !dropped.is_empty() {
        parsed
            .track_artists
            .retain(|link| !dropped.contains(&link.track_id));
        parsed
            .track_artist_roles
            .retain(|role| !dropped.contains(&role.track_id));
        parsed
            .work_graph
            .track_works
            .retain(|link| !dropped.contains(&link.track_id));
    }

    parsed.tracks = tracks;
    if let Some(edit) = user_edit.as_mut() {
        edit.tracks = kept_rows;
    }
    bindings
}

/// Project the user's identity choice onto the mapper's identity vec.
///
/// The MB / Discogs mappers always emit Exact rows (`source_release_id =
/// Some`); the file-tag mapper emits an empty vec, since Unknown imports make no
/// identity claim, so Unknown passes straight through.
///
/// Approximate NULLs `source_release_id` on every row — the primary AND any
/// cross-source row from MB↔Discogs url-rels. The claim is at the group level
/// for all of them.
pub(crate) fn apply_identity_choice(
    mapper_output: &[crate::import::ReleaseIdentity],
    choice: &crate::import::IdentityChoice,
) -> Vec<crate::import::ReleaseIdentity> {
    match choice {
        crate::import::IdentityChoice::Exact { .. } | crate::import::IdentityChoice::Unknown => {
            mapper_output.to_vec()
        }
        crate::import::IdentityChoice::Approximate { .. } => mapper_output
            .iter()
            .map(|id| crate::import::ReleaseIdentity {
                source: id.source,
                source_group_id: id.source_group_id.clone(),
                source_release_id: None,
            })
            .collect(),
    }
}

/// Apply the editor's overlay onto the seeded album/release/tracks.
///
/// Overwrites the album title, the release's pressing fields, and each track's
/// title/side/track_number.
///
/// Artist credits (`album_artists`, `track_artists`) are rebuilt only when the
/// edit's names differ from the seed's, so an untouched artist field keeps the
/// mapper's rows and their source-id linkage (e.g. `musicbrainz_artist_id`).
/// Comparison uses the editor's own form shape: an empty per-track list means
/// "track shares the album artist", so a seeded track whose credits match the
/// album's (positionally, case-insensitive) compares equal to an empty edit.
///
/// A rebuild resolves names against the existing `artists` vec, inserting fresh
/// `DbArtist` rows for unseen names with both source ids `None` — a
/// user-introduced name has no source binding to record. The import-artist
/// resolver canonicalizes them at DB-write time.
///
/// A `tracks` length mismatch is a structural error: the editor binds to the
/// seeded track list and never adds or removes rows.
fn apply_user_edit_to_seed(
    edit: &crate::import::ReleaseUserEdit,
    db_album: &mut crate::db::DbAlbum,
    db_release: &mut crate::db::DbRelease,
    db_tracks: &mut [crate::db::DbTrack],
    artists: &mut Vec<crate::db::DbArtist>,
    album_artists: &mut Vec<crate::db::DbAlbumArtist>,
    track_artists: &mut Vec<crate::db::DbTrackArtist>,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<(), crate::import::ImportError> {
    use crate::db::{DbAlbumArtist, DbArtist, DbTrackArtist};

    if edit.album_artist_names.is_empty() {
        return Err(crate::import::EditValidationError::NoAlbumArtist.into());
    }
    if edit.tracks.len() != db_tracks.len() {
        return Err(crate::import::ImportError::Internal {
            detail: format!(
                "Track count mismatch: seed has {} tracks, edit supplies {}",
                db_tracks.len(),
                edit.tracks.len()
            ),
        });
    }

    let now = clock.now();

    // Resolve a name to an artist id, inserting a fresh (source-id-free) row on
    // a miss. Case-insensitive — the import-artist resolver matches the same way.
    let ensure_artist = |artists: &mut Vec<DbArtist>, name: &str| -> String {
        if let Some(existing) = artists.iter().find(|a| a.name.eq_ignore_ascii_case(name)) {
            return existing.id.clone();
        }
        let new_artist = DbArtist {
            id: ids.new_id(),
            name: name.to_string(),
            sort_name: Some(name.to_string()),
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        };
        let id = new_artist.id.clone();
        artists.push(new_artist);
        id
    };

    // The seed's album-artist names (primary at [0], junction rows by ascending
    // position), to compare against the edit's list.
    // Must be the same projection parsed_album_to_user_edit fed the editor -- a
    // difference here reads as a user edit and re-mints the artists without their
    // source ids. See import::artist_names.
    let seeded_album_artist_names = crate::import::artist_names::album_artist_names(
        artists,
        album_artists,
        &db_album.artist_id,
    )
    .map_err(|missing| crate::import::ImportError::Internal {
        detail: format!("album_artist references missing artist {}", missing.0),
    })?;

    db_album.title = edit.album_title.clone();
    db_album.artist_id = ensure_artist(artists, &edit.album_artist_names[0]);

    db_release.pressing = crate::db::Pressing {
        year: edit.pressing.year,
        format: edit.pressing.format.clone(),
        label: edit.pressing.label.clone(),
        catalog_number: edit.pressing.catalog_number.clone(),
        country: edit.pressing.country.clone(),
        barcode: edit.pressing.barcode.clone(),
    };

    for (track, t_edit) in db_tracks.iter_mut().zip(edit.tracks.iter()) {
        track.title = t_edit.title.clone();
        track.side = t_edit.side;
        track.track_number = t_edit.track_number;
    }

    // Rebuild only on a real change; equality keeps the mapper's
    // source-id-bearing rows.
    if !names_equal(&seeded_album_artist_names, &edit.album_artist_names) {
        album_artists.clear();
        for (position, name) in edit.album_artist_names.iter().enumerate().skip(1) {
            let artist_id = ensure_artist(artists, name);
            album_artists.push(DbAlbumArtist::new(
                &db_album.id,
                &artist_id,
                position as i32,
                ids.new_id(),
                now,
            ));
        }
    }

    // An empty per-track edit list means "share the album artist", and a seeded
    // credit list matching the album's round-trips through the editor as empty —
    // so those compare equal. Anything else is a real change and rebuilds.
    for (track, t_edit) in db_tracks.iter().zip(edit.tracks.iter()) {
        let seeded_names =
            crate::import::artist_names::track_artist_names(artists, track_artists, &track.id)
                .map_err(|missing| crate::import::ImportError::Internal {
                    detail: format!("track_artist references missing artist {}", missing.0),
                })?;

        let edit_names = &t_edit.artist_names;
        let unchanged = if edit_names.is_empty() {
            seeded_names.is_empty() || names_equal(&seeded_names, &seeded_album_artist_names)
        } else {
            names_equal(&seeded_names, edit_names)
        };

        if !unchanged {
            track_artists.retain(|ta| ta.track_id != track.id);
            for (position, name) in edit_names.iter().enumerate() {
                let artist_id = ensure_artist(artists, name);
                track_artists.push(DbTrackArtist::new(
                    &track.id,
                    &artist_id,
                    position as i32,
                    ids.new_id(),
                    now,
                ));
            }
        }
    }

    Ok(())
}

/// Case-insensitive equality on lists of artist names. Matches the rule
/// the import-artist resolver uses for canonicalization, so two name lists
/// the DB would treat as identical compare equal here.
fn names_equal(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.eq_ignore_ascii_case(y))
}

pub(crate) fn common_ancestor<'a>(a: &'a Path, b: &Path) -> &'a Path {
    let mut longest = a;
    loop {
        if b.starts_with(longest) {
            return longest;
        }
        match longest.parent() {
            Some(parent) => longest = parent,
            None => return longest,
        }
    }
}

/// The archived documents for a release, fetching and storing them when nothing
/// has yet. Takes the bare `LibraryManager` because neither the import worker
/// nor the sweep holds an `ImportServiceHandle`.
///
/// Every path that needs a release it may not have archived comes here: the
/// sweep settling a lead in the background, the confirmation pane opening a
/// pressing identification never fetched, re-identify pointing a library
/// release at a new one, the commit worker mapping what the user confirmed. So
/// what the commit maps and what a pane showed always come out of the same
/// rows, and a release two of them want is fetched once.
pub(crate) async fn prepare_release(
    library_manager: &LibraryManager,
    release_ref: &MetadataRef,
    priority: CallPriority,
) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
    if let Some(stored) =
        crate::import::payloads::load(library_manager.database(), release_ref).await?
    {
        return Ok(stored);
    }
    // A Discogs client that will not build costs a MusicBrainz release only its
    // cross-reference, which is best-effort either way; a Discogs release has
    // nothing to fetch without one, and `fetch` refuses it by name.
    let discogs = match library_manager.discogs_client() {
        Ok(client) => client,
        Err(error) => {
            warn!("no Discogs client for {}: {error}", release_ref.id);
            None
        }
    };
    let payloads = crate::import::payloads::fetch(discogs.as_ref(), release_ref, priority).await?;
    crate::import::payloads::store(
        library_manager.database(),
        &payloads,
        library_manager.clock().now(),
    )
    .await?;
    Ok(payloads)
}

#[cfg(test)]
mod tests;
