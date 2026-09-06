use crate::diagnostics::TelemetryEvent;
use crate::import::candidate_runtime::CandidateRuntime;
use crate::import::handle::ImportServiceHandle;
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
    crate::import::types::{
        AudioFile, CoverSelection, ImportPhase, MetadataSource, PrepareStep, TrackFile,
    },
    crate::import::ParsedWorkGraph,
    notify_debouncer_full::DebounceEventResult,
    std::collections::{HashMap, HashSet},
    std::path::{Path, PathBuf},
    std::sync::{Arc, Mutex},
};

use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

mod cover_image;
mod file_identity;
mod folder_watcher;
mod format_prep;
mod importing;
mod progress;
mod reconcile;
mod scanning;

use folder_watcher::FolderWatchSnapshot;
mod coordinator;
use crate::import::volume::{directories_changed, directory_modified_at, volume_kind, VolumeKind};
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
    /// Per-source external identity rows. Empty for File Tags and direct entry.
    /// Commit writes one `release_identities` row per element.
    identities: Vec<crate::import::types::ReleaseIdentity>,
    album_title: String,
}

/// One release-file row paired with Coven's opaque preparation of the exact
/// user-owned file that row declares.
pub(crate) struct PreparedImportFile {
    pub(crate) row: DbFile,
    pub(crate) blob: coven::PreparedExternalBlob,
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

#[derive(Debug, Clone)]
pub(crate) struct ImportExpectation {
    pub(crate) content_hash: String,
    pub(crate) edit_revision: u64,
    pub(crate) metadata_revision: u64,
    pub(crate) file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
}

impl ImportExpectation {
    pub(crate) fn content_hash(&self) -> &str {
        &self.content_hash
    }

    pub(crate) fn edit_revision(&self) -> u64 {
        self.edit_revision
    }

    pub(crate) fn metadata_revision(&self) -> u64 {
        self.metadata_revision
    }
}

pub struct ImportService {
    commands_rx: mpsc::UnboundedReceiver<ImportWorkerMessage>,
    event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
    library_manager: LibraryManager,
    clock: coven::ClockRef,
    ids: coven::IdRef,
}

/// One downloaded cover as the import funnel's candidate.
///
/// The content type was verified from the decoded bytes at download time. It
/// describes the download, not the stored blob — the resize re-encodes those
/// bytes — so it is checked and dropped, never recorded.
fn downloaded_cover(
    image: crate::import::cover_art::RemoteImage,
    url: &str,
    source: MetadataSource,
) -> Result<cover_image::CoverCandidate, crate::import::ImportError> {
    let crate::import::cover_art::RemoteImage {
        bytes,
        content_type,
    } = image;
    if matches!(
        content_type,
        crate::util::content_type::ContentType::OctetStream
    ) {
        return Err(crate::import::ImportError::CoverArt {
            detail: "Cover bytes aren't a recognized image format (PNG/JPEG/GIF/WebP/BMP)"
                .to_string(),
        });
    }
    Ok(cover_image::CoverCandidate {
        bytes,
        source: source.as_str().to_string(),
        source_url: Some(url.to_string()),
    })
}

/// The paths in one debounced batch that report a *change* to the watched tree.
///
/// Not every event a backend sends is a change. Linux's inotify backend watches
/// `IN_OPEN`, so every `open()` under a watched root arrives here — including
/// the scan's own: walking a directory, reading a rip log, parsing a CUE,
/// probing audio. Scheduling a scan for those would mean every scan schedules
/// the next one, for as long as the folder stays watched. A close that ended a
/// write says the file is now different; an open says only that something read
/// it.
fn changed_paths(events: &[notify_debouncer_full::DebouncedEvent]) -> Vec<&Path> {
    events
        .iter()
        .filter(|event| reports_a_change(&event.kind))
        .flat_map(|event| event.paths.iter().map(PathBuf::as_path))
        .collect()
}

/// How many events a batch reported that name paths a scan should be asked for,
/// and what the first few of them were. Capped: copying an album is hundreds of
/// events, and the first handful name the cause as well as all of them do.
fn changed_events_summary(events: &[notify_debouncer_full::DebouncedEvent]) -> String {
    const NAMED: usize = 6;
    let changes: Vec<&notify_debouncer_full::DebouncedEvent> = events
        .iter()
        .filter(|event| reports_a_change(&event.kind))
        .collect();
    let named: Vec<String> = changes
        .iter()
        .take(NAMED)
        .map(|event| {
            format!(
                "{:?} {}",
                event.kind,
                event
                    .paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    let ignored = events.len() - changes.len();
    let more = changes.len().saturating_sub(named.len());
    format!(
        "{} of {} events count as changes ({ignored} ignored){}{}",
        changes.len(),
        events.len(),
        if named.is_empty() {
            String::new()
        } else {
            format!(": {}", named.join("; "))
        },
        if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        }
    )
}

fn reports_a_change(kind: &notify::EventKind) -> bool {
    use notify::event::{AccessKind, AccessMode};
    match kind {
        // The close that ended a write is how a finished copy announces itself.
        notify::EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        notify::EventKind::Access(_) => false,
        _ => true,
    }
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

/// What the blocking folder walk hands back: whether it read the tree, every
/// directory it visited, and — where it could read all of their mtimes — when
/// each was last touched.
type FolderWalkOutcome = (
    Result<(), crate::import::folder_scanner::FolderScanError>,
    HashSet<PathBuf>,
    Option<Vec<(String, i64)>>,
);

/// The walk itself, still running.
type FolderWalk = tokio::task::JoinHandle<FolderWalkOutcome>;

/// One scan item after its durable write, with the commit lock still held so
/// the events announcing it go out before anything else writes.
struct PersistedScanItem {
    commit: tokio::sync::OwnedMutexGuard<()>,
    item: ScanItem,
    /// What the write did — and so whether there is anything to announce. A
    /// pass over an untouched folder finds every row exactly as it left it, and
    /// those rows are told to nobody.
    write: crate::db::ScanItemWrite,
}

struct RootScanTask {
    cancellation: crate::import::folder_scanner::ScanCancellation,
    task: tokio::task::JoinHandle<()>,
}

/// One scan pass has ended. It carries no result: a scan reports its own
/// failure — it records the root's failed status and puts the alert on the
/// event stream — so this only tells the coordinator that the root is free
/// again and that whoever asked for the refresh can stop waiting.
struct RootScanCompletion {
    id: u64,
    path: PathBuf,
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
    Removed {
        commit: tokio::sync::OwnedMutexGuard<()>,
        /// The scan entries the removal cascaded away, announced as
        /// `CandidateRemoved` so in-flight work on them is cancelled.
        removed_keys: Vec<String>,
    },
    Failed(String),
}

#[async_trait::async_trait]
trait RootRemovalBackend: Send + Sync {
    async fn uninstall(&self, path: &Path) -> Result<FolderWatchSnapshot, String>;
    async fn reinstall(&self, path: &Path, snapshot: &FolderWatchSnapshot) -> Result<(), String>;
    /// Delete the root's rows and return the scan entry keys that went with
    /// them.
    async fn remove_durable_root(&self, path: &Path) -> Result<Vec<String>, String>;
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

type RootScanStarter = Arc<
    dyn Fn(u64, PathBuf, mpsc::UnboundedSender<RootScanCompletion>) -> RootScanTask + Send + Sync,
>;

fn spawn_root_scan(
    id: u64,
    path: PathBuf,
    event_tx: broadcast::Sender<crate::import::handle::ImportEvent>,
    library_manager: LibraryManager,
    preparations: crate::import::CandidatePreparations,
    clock: coven::ClockRef,
    ids: coven::IdRef,
    folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    folder_state_commit: Arc<tokio::sync::Mutex<()>>,
    folder_watcher: Arc<FolderWatcher>,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::import::folder_scanner::ScanCancellation::new();
    let scan_cancellation = cancellation.clone();
    let completion_path = path.clone();
    let task = tokio::spawn(async move {
        if !scan_cancellation.is_cancelled() {
            // The error is dropped rather than passed on: `rescan_and_reconcile`
            // has already recorded it as the root's status and announced it, and
            // a refresh caller that reported it a second time would put two
            // dialogs on screen for one broken folder.
            let _ = ImportService::rescan_and_reconcile(
                &path,
                &event_tx,
                &library_manager,
                &preparations,
                &clock,
                &ids,
                &folder_registry,
                &folder_state_commit,
                &folder_watcher,
                &scan_cancellation,
            )
            .await;
        }
        if completion_tx
            .send(RootScanCompletion {
                id,
                path: completion_path,
            })
            .is_err()
        {
            debug!("folder scan coordinator ended before scan completion");
        }
    });
    RootScanTask { cancellation, task }
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

fn request_root_scan(
    path: PathBuf,
    cause: RootScanCause,
    waiter: Option<RefreshCompletion>,
    schedules: &mut HashMap<PathBuf, RootScanSchedule>,
    starter: &RootScanStarter,
    completion_tx: &mpsc::UnboundedSender<RootScanCompletion>,
    next_scan_id: &mut u64,
) {
    if let Some(schedule) = schedules.get_mut(&path) {
        info!(
            "folder scan of {} queued behind the one running: {cause}",
            path.display()
        );
        schedule.pending = true;
        if let Some(waiter) = waiter {
            schedule.followup_waiters.push(waiter);
        }
        return;
    }
    info!("folder scan of {} starting: {cause}", path.display());
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

/// Reconcile the release's track rows with the folder's audio, and report which
/// audio each surviving track is bound to.
///
/// The command's edit carries the track slots the user saw, each row naming the
/// audio bound to it — that is the mapping, and it wins. A command whose edit
/// names no audio at all changed metadata without opening the slot table (an
/// automation surface with no mapping pane), so the slots are
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
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
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
                    row.artist_assignments = edited.artist_assignments.clone();
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

    let retained_track_ids = tracks.iter().map(|track| track.id.clone()).collect();
    retain_track_metadata(parsed, &retained_track_ids);

    parsed.tracks = tracks;
    if let Some(edit) = user_edit.as_mut() {
        edit.tracks = kept_rows;
    }
    bindings
}

pub(crate) fn retain_track_metadata(
    parsed: &mut crate::import::ParsedAlbum,
    retained_track_ids: &HashSet<String>,
) {
    parsed
        .track_artists
        .retain(|link| retained_track_ids.contains(&link.track_id));
    parsed
        .track_artist_roles
        .retain(|role| retained_track_ids.contains(&role.track_id));
    parsed
        .work_graph
        .track_works
        .retain(|link| retained_track_ids.contains(&link.track_id));

    let graph = &mut parsed.work_graph;
    let mut retained: HashSet<String> = graph
        .track_works
        .iter()
        .map(|link| link.work_id.clone())
        .collect();
    loop {
        let before = retained.len();
        for part in &graph.work_parts {
            if retained.contains(&part.parent_work_id) || retained.contains(&part.child_work_id) {
                retained.insert(part.parent_work_id.clone());
                retained.insert(part.child_work_id.clone());
            }
        }
        if retained.len() == before {
            break;
        }
    }
    graph.works.retain(|work| retained.contains(&work.id));
    graph
        .work_artists
        .retain(|link| retained.contains(&link.work_id));
    graph.work_parts.retain(|part| {
        retained.contains(&part.parent_work_id) && retained.contains(&part.child_work_id)
    });
}

/// Apply the editor's overlay onto the seeded album/release/tracks.
///
/// Overwrites the album title and original year, the release's pressing fields, and each track's
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
    existing_artists: &HashMap<String, crate::db::DbArtist>,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<HashSet<String>, crate::import::ImportError> {
    use crate::db::{DbAlbumArtist, DbTrackArtist};

    if edit.album_artist_assignments.is_empty() {
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
    let mut existing_artist_ids = HashSet::new();

    db_album.title = edit.album_title.clone();
    db_album.year = edit.album_year;
    db_album.artist_id = materialize_artist_assignment(
        &edit.album_artist_assignments[0],
        artists,
        &mut existing_artist_ids,
        existing_artists,
        ids,
        now,
    )?;

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

    album_artists.clear();
    for (position, assignment) in edit.album_artist_assignments.iter().enumerate().skip(1) {
        let artist_id = materialize_artist_assignment(
            assignment,
            artists,
            &mut existing_artist_ids,
            existing_artists,
            ids,
            now,
        )?;
        album_artists.push(DbAlbumArtist::new(
            &db_album.id,
            &artist_id,
            position as i32,
            ids.new_id(),
            now,
        ));
    }

    for (track, t_edit) in db_tracks.iter().zip(edit.tracks.iter()) {
        track_artists.retain(|credit| credit.track_id != track.id);
        if let crate::import::TrackArtistAssignments::Explicit(assignments) =
            &t_edit.artist_assignments
        {
            for (position, assignment) in assignments.iter().enumerate() {
                let artist_id = materialize_artist_assignment(
                    assignment,
                    artists,
                    &mut existing_artist_ids,
                    existing_artists,
                    ids,
                    now,
                )?;
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

    Ok(existing_artist_ids)
}

fn materialize_artist_assignment(
    assignment: &crate::import::ArtistAssignment,
    artists: &mut Vec<crate::db::DbArtist>,
    existing_artist_ids: &mut HashSet<String>,
    existing_artists: &HashMap<String, crate::db::DbArtist>,
    ids: &dyn coven::IdProvider,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<String, crate::import::ImportError> {
    match assignment {
        crate::import::ArtistAssignment::Existing { artist } => {
            let artist_id = &artist.artist_id;
            let artist = existing_artists.get(artist_id).cloned().ok_or_else(|| {
                crate::import::ImportError::Internal {
                    detail: format!("selected artist {artist_id} no longer exists"),
                }
            })?;
            if !artists.iter().any(|candidate| candidate.id == artist.id) {
                artists.push(artist);
            }
            existing_artist_ids.insert(artist_id.clone());
            Ok(artist_id.clone())
        }
        crate::import::ArtistAssignment::New { seed } => {
            let id = ids.new_id();
            artists.push(crate::db::DbArtist {
                id: id.clone(),
                name: seed.name.clone(),
                sort_name: seed.sort_name.clone(),
                discogs_artist_id: seed.discogs_artist_id.clone(),
                musicbrainz_artist_id: seed.musicbrainz_artist_id.clone(),
                created_at: now,
            });
            Ok(id)
        }
    }
}

async fn load_existing_artist_assignments(
    edit: &crate::import::ReleaseUserEdit,
    library_manager: &LibraryManager,
) -> Result<HashMap<String, crate::db::DbArtist>, crate::import::ImportError> {
    let album = edit.album_artist_assignments.iter();
    let tracks = edit
        .tracks
        .iter()
        .flat_map(|track| match &track.artist_assignments {
            crate::import::TrackArtistAssignments::AlbumArtists => [].as_slice().iter(),
            crate::import::TrackArtistAssignments::Explicit(assignments) => assignments.iter(),
        });
    let mut out = HashMap::new();
    for artist_id in album
        .chain(tracks)
        .filter_map(|assignment| match assignment {
            crate::import::ArtistAssignment::Existing { artist } => Some(&artist.artist_id),
            crate::import::ArtistAssignment::New { .. } => None,
        })
    {
        if out.contains_key(artist_id) {
            continue;
        }
        let artist = library_manager
            .get_artist_by_id(artist_id)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!("selected artist {artist_id} no longer exists"),
            })?;
        out.insert(artist_id.clone(), artist);
    }
    Ok(out)
}

/// The archived documents for a release, fetching and storing them when nothing
/// has yet. Takes the bare `LibraryManager` because the sweep and library
/// re-identification do not hold an `ImportServiceHandle`.
///
/// Every path that needs a release it may not have archived comes here: the
/// sweep settling a lead in the background, selection preparing the candidate,
/// and re-identify pointing a library release at a new one. The import worker
/// consumes only the candidate revision those preparation paths already stored.
pub(crate) async fn prepare_release(
    library_manager: &LibraryManager,
    release_ref: &MetadataRef,
    priority: CallPriority,
) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
    if let Some(stored) = library_manager.load_release_payloads(release_ref).await? {
        return Ok(stored);
    }
    let payloads = library_manager
        .fetch_release_payloads(release_ref, priority)
        .await?;
    library_manager.store_release_payloads(&payloads).await?;
    Ok(payloads)
}

/// Archive every partner a pick carried, so each one's own identity reads back
/// offline later.
///
/// A pick names one release per source: the primary is the document the draft
/// is read from, and every partner is a different source's record of the same
/// pressing. Two claims about one source are two answers to one question, so
/// this refuses them rather than picking one. Nothing is written before this
/// returns, so a partner that will not prepare leaves the pick unmade.
pub(crate) async fn prepare_partners(
    library_manager: &LibraryManager,
    primary: &MetadataRef,
    partners: &[MetadataRef],
    priority: CallPriority,
) -> Result<(), crate::import::ImportError> {
    let mut claimed = vec![primary.source];
    for partner in partners {
        if claimed.contains(&partner.source) {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "a pick names two {} releases for one pressing",
                    partner.source.as_str()
                ),
            });
        }
        claimed.push(partner.source);
        prepare_release(library_manager, partner, priority).await?;
    }
    Ok(())
}

/// The identity rows a pick commits: what the primary document's mapping
/// concluded, with each partner's source replaced by that partner's own
/// identity.
///
/// `release_identities` holds one row per source, so this is a per-source
/// override rather than a union — the person picked that Discogs release, and
/// what an editor cross-linked from the MusicBrainz side does not outrank it.
/// Every partner's documents were archived when the pick was applied, so this
/// reads them and never fetches; nothing stored is a broken invariant.
pub(crate) async fn identities_with_partners(
    library_manager: &LibraryManager,
    mut identities: Vec<crate::import::ReleaseIdentity>,
    partners: &[MetadataRef],
) -> Result<Vec<crate::import::ReleaseIdentity>, crate::import::ImportError> {
    for partner in partners {
        let payloads = library_manager
            .load_release_payloads(partner)
            .await?
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!(
                    "picked {} release {} but nothing stored its lookups",
                    partner.source.as_str(),
                    partner.id
                ),
            })?;
        let identity = payloads.identity()?;
        identities.retain(|existing| existing.source != identity.source);
        identities.push(identity);
    }
    Ok(identities)
}

#[cfg(test)]
mod tests;
