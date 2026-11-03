use crate::candidate_runtime::CandidateRuntime;
use crate::handle::ImportServiceHandle;
use crate::handle::{ScanEvent, WatcherCommand};
use crate::types::{ImportCommand, ImportDestination, ImportProgress};
use bae_diagnostics::TelemetryEvent;

use {
    crate::audio_layout::resolve_track_files,
    crate::folder_scanner::ScanItem,
    crate::types::{ImportPhase, PrepareStep, TrackFile},
    bae_db::WorkGraphRows,
    bae_db::{
        DbAlbum, DbAlbumArtist, DbRelease, DbReleaseArtistRole, DbTrackArtist, DbTrackArtistRole,
        NewReleaseFile,
    },
    std::collections::{HashMap, HashSet},
    std::path::{Path, PathBuf},
    std::sync::Arc,
};

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

mod active_roots;
mod content_type;
mod cover_image;
mod folder_reading;
mod folder_watcher;
mod format_prep;
mod importing;
mod progress;
mod reconcile;
mod root_backend;
mod root_change;
mod root_scan_cause;
mod scan_items;
mod scanning;
mod watch_batches;
mod worker;

use active_roots::{ActiveRoots, FolderReadingRequest, RemovalOutcome, RootPass};
use folder_watcher::FolderWatchSnapshot;
use root_backend::{RootRemovalBackend, ServiceRootRemovalBackend};
use root_change::{
    affected_roots, changed_events_summary, changed_paths, network_changes, root_change_of,
    roots_for_watch_error, RootChange,
};
use root_scan_cause::RootScanCause;
pub(crate) use watch_batches::WatchReport;
mod coordinator;
use crate::volume::{directory_modified_at, volume_kind, VolumeKind};
pub(crate) use folder_watcher::FolderWatcher;

use content_type::resolve_file_content_type;

/// Which import run a progress event is about: the run the pane is watching and
/// the candidate row it redraws.
#[derive(Clone, Copy)]
pub(super) struct ImportRun<'a> {
    pub(super) import_id: &'a str,
    pub(super) candidate_key: &'a crate::CandidateKey,
}

/// The files one import writes and the track rows bound to their audio, both
/// read from the same folder.
#[derive(Clone, Copy)]
pub(super) struct ImportFiles<'a> {
    pub(super) discovered: &'a [crate::folder_scanner::CandidateFile],
    pub(super) tracks: &'a [TrackFile],
}

/// The release's rows from `reconcile_prepared_release`. Every artist link
/// names one of `artist_credits`; the commit resolves those to library artists
/// in its own transaction.
struct PreparedMetadata {
    db_album: DbAlbum,
    db_release: DbRelease,
    /// The release's mediums, in order.
    mediums: Vec<bae_db::DbReleaseMedium>,
    /// The release's tracks, each bound to its audio and measured by the
    /// commit (`resolve_track_files`).
    track_rows: Vec<bae_db::TrackRow>,
    existing_album_id: Option<String>,
    track_artists: Vec<DbTrackArtist>,
    /// Empty when the release joins an album already in the library.
    album_artists: Vec<DbAlbumArtist>,
    /// Works resolved; `work_artists` still name their artist by credit.
    work_graph: WorkGraphRows,
    release_artist_roles: Vec<DbReleaseArtistRole>,
    track_artist_roles: Vec<DbTrackArtistRole>,
    /// Every artist the links above name, as the source or the person said.
    artist_credits: Vec<bae_db::DbArtist>,
    /// The credits that are library artists a person picked.
    picked_artists: Vec<String>,
    /// One per catalog that describes this release; empty for file metadata and
    /// direct entry.
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    album_title: String,
}

/// A release-file row and its original, read whole once, with where its
/// destination stages it: on this device only, or for the cloud, kept whole
/// here too once stored when the import asks for that. coven records the
/// original on this device either way, with the hashes its read found.
pub(crate) struct PreparedImportFile {
    pub(crate) row: NewReleaseFile,
    pub(crate) original: coven::PreparedUserFile,
    pub(crate) place: coven::Place,
}

fn destination_label(destination: ImportDestination) -> &'static str {
    match destination {
        ImportDestination::Remote { .. } => "remote",
        ImportDestination::Local => "local",
    }
}

/// What the import worker thread receives. `Shutdown` is sent rather than
/// closing the channel because the last sender lives in the struct whose `Drop`
/// joins the thread, so the channel could never close first.
pub(crate) enum ImportWorkerMessage {
    Import { command: ImportCommand, expectation: ImportExpectation },
    Shutdown,
}

/// The candidate an import commits, as the commit checks it.
pub(super) struct CommittedCandidate {
    pub(super) source: crate::release_candidate::CandidateSource,
    pub(super) expectation: ImportExpectation,
}

#[derive(Debug, Clone)]
pub(crate) struct ImportExpectation {
    pub(crate) candidate: crate::CandidateAsRead,
    pub(crate) file_tag_snapshot: Option<crate::file_tag_snapshot::FileTagSnapshot>,
}

impl ImportExpectation {
    /// Whether `current` is still the candidate this import was prepared from.
    /// Asked inside the library write's transaction, so the answer holds
    /// through the commit.
    pub(crate) fn verify(
        &self,
        candidate_key: &crate::CandidateKey,
        source: &crate::release_candidate::CandidateSource,
        current: Option<&crate::preparation::CommittingCandidate>,
    ) -> Result<(), String> {
        let Some(current) = current else {
            return Err(format!("{candidate_key} is no longer a valid import candidate"));
        };
        if !current.actionable
            || current.source != *source
            || current.content_hash != self.candidate.content_hash
            || current.file_edit_revision != self.candidate.file_edit_revision
        {
            return Err(format!("{candidate_key} changed before its import committed"));
        }
        if current.prepared_revisions
            != Some((self.candidate.file_edit_revision, self.candidate.metadata_revision))
        {
            return Err(format!(
                "{candidate_key}'s prepared metadata changed before its import committed"
            ));
        }
        if let Some(snapshot) = &self.file_tag_snapshot {
            if !current
                .file_tag_snapshot
                .as_ref()
                .is_some_and(|current| current.same_reading(snapshot))
            {
                return Err(format!(
                    "{candidate_key}'s file-tag reading changed before its import committed"
                ));
            }
        }
        Ok(())
    }
}

pub struct ImportService {
    commands_rx: mpsc::UnboundedReceiver<ImportWorkerMessage>,
    event_tx: crate::handle::ImportEventBus,
    /// What the bus's events have said about every candidate, held here for
    /// the standing a commit ends.
    runtime: CandidateRuntime,
    /// The library's store, which an import writes its release into.
    database: bae_db::Database,
    /// Import's tables, which an import reads its candidate from.
    tables: crate::tables::ImportTables,
    /// The catalogs an import asks for its new artists' pictures.
    catalogs: crate::CatalogSessions,
    /// Where those pictures are fetched through.
    remote_images: crate::RemoteImages,
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
    import_cancels: crate::import_cancel::ImportCancels,
    /// Where every bug the import worker finds is reported: a write no
    /// caller waits on, and an image the blocking format prep can't read.
    diagnostics: bae_diagnostics::Diagnostics,
}

/// What the blocking folder walk returns: whether it read the tree, every
/// directory it visited, and their mtimes when it could read all of them.
type FolderWalkOutcome = (
    Result<(), crate::folder_scanner::FolderScanError>,
    HashSet<PathBuf>,
    Option<Vec<(PathBuf, i64)>>,
);

type FolderWalk = tokio::task::JoinHandle<FolderWalkOutcome>;

/// How many scan items the walk hands over before it waits on the store, and
/// so the most one write stores.
const SCAN_ITEMS_WAITING: usize = 8;

struct RootScanTask {
    cancellation: crate::folder_scanner::ScanCancellation,
    /// The pass, which says it ended on the completion channel even when it
    /// panics.
    task: bae_diagnostics::ReportedTask,
}

/// One scan pass has ended. It carries no result because a scan records and
/// announces its own failure; this only frees the root and ends the wait.
struct RootScanCompletion {
    id: u64,
    path: PathBuf,
}

/// Run one pass over the root at `path` as `name`, sending its completion on
/// `completion_tx` when `pass` ends. A pass that panics is reported, and its
/// completion is sent all the same, so the root is freed and its waiters are
/// answered.
fn spawn_pass(
    name: &'static str,
    id: u64,
    path: PathBuf,
    diagnostics: &bae_diagnostics::Diagnostics,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
    pass: impl std::future::Future<Output = ()> + Send + 'static,
) -> bae_diagnostics::ReportedTask {
    let panicked_tx = completion_tx.clone();
    let panicked_path = path.clone();
    diagnostics.spawn_reported(
        &tokio::runtime::Handle::current(),
        name,
        async move {
            pass.await;
            if completion_tx.send(RootScanCompletion { id, path }).is_err() {
                debug!("folder scan coordinator ended before a pass completed");
            }
        },
        move |_| {
            if panicked_tx.send(RootScanCompletion { id, path: panicked_path }).is_err() {
                debug!("folder scan coordinator ended before a panicked pass completed");
            }
        },
    )
}

type RootScanStarter = Arc<
    dyn Fn(u64, PathBuf, RootPass, mpsc::UnboundedSender<RootScanCompletion>) -> RootScanTask
        + Send
        + Sync,
>;

/// What one folder scan runs on: the import service's dependencies and the
/// watcher the walk registers each directory with.
#[derive(Clone)]
pub(super) struct ScanServices {
    services: crate::ImportServices,
    folder_watcher: Arc<FolderWatcher>,
}

impl ScanServices {
    pub(super) fn new(services: crate::ImportServices, folder_watcher: Arc<FolderWatcher>) -> Self {
        Self { services, folder_watcher }
    }
}

/// Start one pass over the root at `path`: all of it, one folder decision, or
/// the folders whose contents changed.
fn spawn_root_pass(
    id: u64,
    path: PathBuf,
    pass: RootPass,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    match pass {
        RootPass::WholeRoot => spawn_root_scan(id, path, scan, completion_tx),
        RootPass::Decision(request) => spawn_folder_reading(id, path, request, scan, completion_tx),
        RootPass::Folders(folders) => spawn_changed_folders(id, path, folders, scan, completion_tx),
    }
}

/// Store one folder decision under `path` and answer whoever asked.
fn spawn_folder_reading(
    id: u64,
    path: PathBuf,
    request: FolderReadingRequest,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::folder_scanner::ScanCancellation::new();
    let reading_cancellation = cancellation.clone();
    let diagnostics = scan.services.diagnostics.clone();
    let pass_path = path.clone();
    let task =
        spawn_pass("store a folder decision", id, path, &diagnostics, completion_tx, async move {
            let path = pass_path;
            let target = request.target().clone();
            // A reading that panics answers its caller as one that failed.
            let result = bae_util::task_failure::caught(
                "store a folder decision",
                ImportService::change_folder_reading(&path, &target, &scan, &reading_cancellation),
            )
            .await
            .unwrap_or_else(|failed| Err(scan.services.diagnostics.bug(failed).into()))
            // The caller wraps the detail as a watch failure itself.
            .map_err(|error| match error {
                crate::ImportError::Watch { detail } => detail,
                other => other.to_string(),
            });
            if let Err(error) = &result {
                warn!(
                    "folder decision for {} under {} was not stored: {error}",
                    target.0.relative_folder.text(),
                    path.display()
                );
            }
            request.answer(result);
        });
    RootScanTask { cancellation, task }
}

/// Read again the folders under `path` whose contents changed.
fn spawn_changed_folders(
    id: u64,
    path: PathBuf,
    folders: std::collections::BTreeSet<std::ffi::OsString>,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::folder_scanner::ScanCancellation::new();
    let reading_cancellation = cancellation.clone();
    let diagnostics = scan.services.diagnostics.clone();
    let pass_path = path.clone();
    let task = spawn_pass(
        "read changed folders again",
        id,
        path,
        &diagnostics,
        completion_tx,
        async move {
            ImportService::read_changed_folders(&pass_path, &folders, &scan, &reading_cancellation)
                .await;
        },
    );
    RootScanTask { cancellation, task }
}

fn spawn_root_scan(
    id: u64,
    path: PathBuf,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::folder_scanner::ScanCancellation::new();
    let scan_cancellation = cancellation.clone();
    let diagnostics = scan.services.diagnostics.clone();
    let pass_path = path.clone();
    let task =
        spawn_pass("scan a watched folder", id, path, &diagnostics, completion_tx, async move {
            if !scan_cancellation.is_cancelled() {
                // `rescan_and_reconcile` already recorded and announced any
                // error; passing it on would show it twice.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the rescan records and announces its own error"
                )]
                let _ = ImportService::rescan_and_reconcile(&pass_path, &scan, &scan_cancellation)
                    .await;
            }
        });
    RootScanTask { cancellation, task }
}

/// The records a pick commits: the primary's and every partner's, each
/// release's own record outranking what another says about its catalog.
pub fn records_for_commit(
    primary: &bae_catalogs::source_release::SourceRelease,
    partners: &[bae_catalogs::source_release::SourceRelease],
) -> Vec<bae_catalogs::records::ReleaseRecord> {
    let claimed: Vec<_> = std::iter::once(primary)
        .chain(partners)
        .map(bae_catalogs::source_release::SourceRelease::records)
        .collect();
    bae_catalogs::source_release::claimed_records(&claimed)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
