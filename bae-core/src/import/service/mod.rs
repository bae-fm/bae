use crate::diagnostics::TelemetryEvent;
use crate::import::candidate_runtime::CandidateRuntime;
use crate::import::handle::ImportServiceHandle;
use crate::import::handle::{ScanEvent, WatcherCommand};
use crate::import::types::{ImportCommand, ImportDestination, ImportProgress, MetadataRef};
use crate::library::LibraryManager;
use crate::util::rate_limiter::CallPriority;

use {
    crate::db::{
        DbAlbum, DbAlbumArtist, DbFile, DbRelease, DbReleaseArtistRole, DbTrack, DbTrackArtist,
        DbTrackArtistRole,
    },
    crate::import::folder_scanner::{ScanItem, ScannedFile},
    crate::import::track_slots::resolve_track_files,
    crate::import::types::{
        AudioFile, Catalog, CoverSelection, ImportPhase, PrepareStep, TrackFile,
    },
    crate::import::ParsedWorkGraph,
    std::collections::{HashMap, HashSet},
    std::path::{Path, PathBuf},
    std::sync::Arc,
};

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

mod active_roots;
mod cover_image;
mod folder_reading;
mod folder_watcher;
mod format_prep;
mod importing;
mod progress;
mod reconcile;
mod root_backend;
mod root_scan_cause;
mod scanning;
mod watch_batches;

use active_roots::{ActiveRoots, FolderReadingRequest, RemovalOutcome, RootPass};
use folder_watcher::FolderWatchSnapshot;
use root_backend::{RootRemovalBackend, ServiceRootRemovalBackend};
use root_scan_cause::RootScanCause;
pub(crate) use watch_batches::WatchReport;
mod coordinator;
use crate::import::volume::{changed_directories, directory_modified_at, volume_kind, VolumeKind};
pub(crate) use folder_watcher::FolderWatcher;

use format_prep::resolve_file_content_type;

/// Which import run a progress event is about: the run the pane is watching and
/// the candidate row it redraws.
#[derive(Clone, Copy)]
pub(super) struct ImportRun<'a> {
    pub(super) import_id: &'a str,
    pub(super) candidate_key: &'a str,
}

/// The files one import writes and the track rows bound to their audio, both
/// read from the same folder.
#[derive(Clone, Copy)]
pub(super) struct ImportFiles<'a> {
    pub(super) discovered: &'a [ScannedFile],
    pub(super) tracks: &'a [TrackFile],
}

/// The release's rows from `reconcile_prepared_release`. Every artist link
/// names one of `artist_credits`; the commit resolves those to library artists
/// in its own transaction.
struct PreparedMetadata {
    db_album: DbAlbum,
    db_release: DbRelease,
    db_tracks: Vec<DbTrack>,
    /// The cover the person picked for this candidate, whatever its source.
    selected_cover: Option<CoverSelection>,
    /// The exact prepared bytes of a picked remote cover.
    remote_cover_image: Option<cover_image::CoverCandidate>,
    /// The file-tag snapshot's artwork; its content type is checked, then
    /// dropped by the resize.
    embedded_cover: Option<(Vec<u8>, crate::util::content_type::ContentType)>,
    existing_album_id: Option<String>,
    track_artists: Vec<DbTrackArtist>,
    /// Empty when the release joins an album already in the library.
    album_artists: Vec<DbAlbumArtist>,
    /// Works resolved; `work_artists` still name their artist by credit.
    work_graph: ParsedWorkGraph,
    release_artist_roles: Vec<DbReleaseArtistRole>,
    track_artist_roles: Vec<DbTrackArtistRole>,
    /// Every artist the links above name, as the source or the person said.
    artist_credits: Vec<crate::db::DbArtist>,
    /// The credits that are library artists a person picked.
    picked_artists: Vec<String>,
    /// Discogs pictures for whichever credits the commit finds are new artists.
    prepared_artist_images: Vec<crate::import::PreparedArtistImage>,
    /// One per catalog that describes this release; empty for file metadata and
    /// direct entry.
    records: Vec<crate::import::ReleaseRecord>,
    album_title: String,
}

/// One release-file row and Coven's preparation of the file it names.
pub(crate) struct PreparedImportFile {
    pub(crate) row: DbFile,
    pub(crate) blob: coven::PreparedExternalBlob,
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
    Import {
        command: ImportCommand,
        expectation: ImportExpectation,
    },
    Shutdown,
}

#[derive(Debug, Clone)]
pub(crate) struct ImportExpectation {
    pub(crate) candidate: crate::import::CandidateAsRead,
    pub(crate) file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
}

impl ImportExpectation {
    /// Whether `current` is still the candidate this import was prepared from.
    /// Asked inside the library write's transaction, so the answer holds
    /// through the commit.
    pub(crate) fn verify(
        &self,
        candidate_key: &str,
        source: &crate::import::release_candidate::CandidateSource,
        current: Option<&crate::import::preparation::CommittingCandidate>,
    ) -> Result<(), String> {
        let Some(current) = current else {
            return Err(format!(
                "{candidate_key} is no longer a valid import candidate"
            ));
        };
        if !current.actionable
            || current.source != *source
            || current.content_hash != self.candidate.content_hash
            || current.file_edit_revision != self.candidate.file_edit_revision
        {
            return Err(format!(
                "{candidate_key} changed before its import committed"
            ));
        }
        if current.prepared_revisions
            != Some((
                self.candidate.file_edit_revision,
                self.candidate.metadata_revision,
            ))
        {
            return Err(format!(
                "{candidate_key}'s prepared metadata changed before its import committed"
            ));
        }
        if let Some(snapshot) = &self.file_tag_snapshot {
            if current.file_tag_snapshot.as_ref() != Some(snapshot) {
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
    event_tx: crate::import::handle::ImportEventBus,
    library_manager: LibraryManager,
    clock: coven::ClockRef,
    ids: coven::IdRef,
    import_cancels: crate::import::import_cancel::ImportCancels,
}

/// One downloaded cover as an import cover candidate. Its content type is
/// checked and dropped: the resize re-encodes the bytes, so it does not
/// describe what is stored.
fn downloaded_cover(
    image: crate::import::cover_art::RemoteImage,
    url: &str,
    source: Catalog,
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

/// The paths in one batch of events that report a change to the watched tree.
///
/// Linux's inotify backend reports every `open()`, including the scan's own
/// reads, so counting those as changes would make every scan schedule the next.
fn changed_paths(events: &[notify::Event]) -> Vec<&Path> {
    events
        .iter()
        .filter(|event| reports_a_change(&event.kind))
        .flat_map(|event| event.paths.iter().map(PathBuf::as_path))
        .collect()
}

/// How many events in a batch count as changes, naming the first few; copying
/// an album is hundreds of events, and the first few name the cause.
fn changed_events_summary(events: &[notify::Event]) -> String {
    const NAMED: usize = 6;
    let changes: Vec<&notify::Event> = events
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

/// What a set of changed paths under one watched root asks to be read again.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RootChange {
    /// The root itself: something only a pass over all of it reads right.
    WholeRoot,
    /// These folders directly under the root, and nothing else. Empty when no
    /// change reaches anything a scan reads.
    Folders(std::collections::BTreeSet<String>),
}

/// [`root_change`] on a blocking thread: its stats can take as long as a
/// network mount does to answer, and the coordinator must not wait on that.
async fn root_change_of(root: &Path, changed: &[&Path], holds_its_own_release: bool) -> RootChange {
    let root = root.to_path_buf();
    let changed: Vec<PathBuf> = changed.iter().map(|path| path.to_path_buf()).collect();
    let asked = tokio::task::spawn_blocking(move || {
        let changed: Vec<&Path> = changed.iter().map(PathBuf::as_path).collect();
        root_change(&root, &changed, holds_its_own_release)
    })
    .await;
    match asked {
        Ok(change) => change,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// What `changed` (paths under `root`) asks to be read again.
///
/// A root is read one top-level folder at a time, so a change inside one reads
/// that folder again, and an entry that went away is read as a folder that is
/// now empty. The root is read whole when the root itself changed, an audio
/// file directly in it came or went, or it holds tracks of its own (whose
/// release takes in the audio-free folders beside them). Hidden entries are
/// skipped because the scan never lists them.
fn root_change(root: &Path, changed: &[&Path], holds_its_own_release: bool) -> RootChange {
    let mut folders = std::collections::BTreeSet::new();
    for path in changed {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let names: Vec<String> = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        if names.iter().any(|name| name.starts_with('.')) {
            continue;
        }
        let Some(first) = names.first() else {
            return RootChange::WholeRoot;
        };
        if holds_its_own_release {
            return RootChange::WholeRoot;
        }
        let entry = root.join(first);
        match std::fs::symlink_metadata(&entry).map(|_| entry.is_dir()) {
            Ok(true) => {
                folders.insert(first.clone());
            }
            Ok(false) => {
                if crate::import::folder_scanner::is_audio_file(&entry) {
                    return RootChange::WholeRoot;
                }
            }
            Err(_) => {
                if names.len() == 1 && crate::import::folder_scanner::is_audio_file(&entry) {
                    return RootChange::WholeRoot;
                }
                folders.insert(first.clone());
            }
        }
    }
    RootChange::Folders(folders)
}

/// The paths the directory-mtime check of a network root found changed, in the
/// form a filesystem event gives them, or `None` when the root has to be walked.
///
/// When the root's own mtime moved, the folders that came or went are found by
/// listing it against the record, and a root with audio directly in it names
/// itself, since the record cannot tell which of its files moved.
fn network_changes(root: &Path, recorded: &[(String, i64)]) -> Option<Vec<PathBuf>> {
    let moved = changed_directories(recorded)?;
    let mut changes = Vec::new();
    for directory in moved {
        if directory != root {
            changes.push(directory);
            continue;
        }
        let listing = std::fs::read_dir(root).ok()?;
        let mut present = HashSet::new();
        for entry in listing {
            let path = entry.ok()?.path();
            if path.is_dir() {
                present.insert(path);
            } else if crate::import::folder_scanner::is_audio_file(&path) {
                changes.push(root.to_path_buf());
            }
        }
        let known: HashSet<PathBuf> = recorded
            .iter()
            .map(|(path, _)| PathBuf::from(path))
            .filter(|path| path.parent() == Some(root))
            .collect();
        changes.extend(present.symmetric_difference(&known).cloned());
    }
    Some(changes)
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

/// What the blocking folder walk returns: whether it read the tree, every
/// directory it visited, and their mtimes when it could read all of them.
type FolderWalkOutcome = (
    Result<(), crate::import::folder_scanner::FolderScanError>,
    HashSet<PathBuf>,
    Option<Vec<(String, i64)>>,
);

type FolderWalk = tokio::task::JoinHandle<FolderWalkOutcome>;

/// One scan item after its durable write, with the commit lock still held so
/// the events announcing it go out before anything else writes.
struct PersistedScanItem {
    commit: crate::import::FolderStateCommitGuard,
    item: ScanItem,
    /// What the write changed; rows it left as they were are not announced.
    write: crate::db::ScanItemWrite,
}

struct RootScanTask {
    cancellation: crate::import::folder_scanner::ScanCancellation,
    task: tokio::task::JoinHandle<()>,
}

/// One scan pass has ended. It carries no result because a scan records and
/// announces its own failure; this only frees the root and ends the wait.
struct RootScanCompletion {
    id: u64,
    path: PathBuf,
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
    services: crate::import::ImportServices,
    folder_watcher: Arc<FolderWatcher>,
}

impl ScanServices {
    pub(super) fn new(
        services: crate::import::ImportServices,
        folder_watcher: Arc<FolderWatcher>,
    ) -> Self {
        Self {
            services,
            folder_watcher,
        }
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
    let cancellation = crate::import::folder_scanner::ScanCancellation::new();
    let reading_cancellation = cancellation.clone();
    let task = tokio::spawn(async move {
        let target = request.target().clone();
        let result =
            ImportService::change_folder_reading(&path, &target, &scan, &reading_cancellation)
                .await
                // The caller wraps the detail as a watch failure itself.
                .map_err(|error| match error {
                    crate::import::ImportError::Watch { detail } => detail,
                    other => other.to_string(),
                });
        if let Err(error) = &result {
            warn!(
                "folder decision for {} under {} was not stored: {error}",
                target.0.relative_folder_path,
                path.display()
            );
        }
        request.answer(result);
        if completion_tx.send(RootScanCompletion { id, path }).is_err() {
            debug!("folder scan coordinator ended before a folder reading completed");
        }
    });
    RootScanTask { cancellation, task }
}

/// Read again the folders under `path` whose contents changed.
fn spawn_changed_folders(
    id: u64,
    path: PathBuf,
    folders: std::collections::BTreeSet<String>,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::import::folder_scanner::ScanCancellation::new();
    let reading_cancellation = cancellation.clone();
    let task = tokio::spawn(async move {
        ImportService::read_changed_folders(&path, &folders, &scan, &reading_cancellation).await;
        if completion_tx.send(RootScanCompletion { id, path }).is_err() {
            debug!("folder scan coordinator ended before changed folders were read");
        }
    });
    RootScanTask { cancellation, task }
}

fn spawn_root_scan(
    id: u64,
    path: PathBuf,
    scan: ScanServices,
    completion_tx: mpsc::UnboundedSender<RootScanCompletion>,
) -> RootScanTask {
    let cancellation = crate::import::folder_scanner::ScanCancellation::new();
    let scan_cancellation = cancellation.clone();
    let completion_path = path.clone();
    let task = tokio::spawn(async move {
        if !scan_cancellation.is_cancelled() {
            // `rescan_and_reconcile` already recorded and announced any error;
            // passing it on would show it twice.
            let _ = ImportService::rescan_and_reconcile(&path, &scan, &scan_cancellation).await;
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

/// Rebuild the tracks in the draft's order, reusing the source track each row
/// names and dropping the credits and works of source tracks left out. Returns
/// each track's audio binding.
fn settle_track_rows(
    parsed: &mut crate::import::ParsedAlbum,
    draft: &crate::import::CandidateDraft,
    ids: &dyn coven::IdProvider,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<AudioFile>, crate::import::ImportError> {
    let mut seeded: Vec<Option<crate::db::DbTrack>> = std::mem::take(&mut parsed.tracks)
        .into_iter()
        .map(Some)
        .collect();
    let mut tracks = Vec::with_capacity(draft.tracks.len());
    let mut bindings = Vec::with_capacity(draft.tracks.len());
    for row in &draft.tracks {
        let track = match row.source_index {
            Some(index) => seeded
                .get_mut(index as usize)
                .and_then(Option::take)
                .ok_or_else(|| crate::import::ImportError::Internal {
                    detail: format!(
                        "draft track {} names unavailable source track {index}",
                        row.edit.id
                    ),
                })?,
            None => crate::db::DbTrack {
                id: ids.new_id(),
                release_id: parsed.release.id.clone(),
                title: row.edit.title.clone(),
                side: row.edit.side,
                track_number: Some(row.edit.track_number),
                duration_ms: None,
                discogs_position: None,
                created_at: now,
            },
        };
        tracks.push(track);
        bindings.push(row.edit.file.clone());
    }
    let retained_track_ids = tracks.iter().map(|track| track.id.clone()).collect();
    retain_track_metadata(parsed, &retained_track_ids);
    parsed.tracks = tracks;
    Ok(bindings)
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

/// Apply the person's edit to the seeded album, release, and tracks, and
/// rebuild the album's and each explicit track's artist links from it. Returns
/// the ids of picked library artists, which the commit links as they are.
fn apply_user_edit_to_seed(
    edit: &crate::import::ReleaseUserEdit,
    seed: &mut crate::import::ParsedAlbum,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<HashSet<String>, crate::import::ImportError> {
    use crate::db::{DbAlbumArtist, DbTrackArtist};

    let crate::import::ParsedAlbum {
        album: db_album,
        release: db_release,
        tracks: db_tracks,
        artists,
        album_artists,
        track_artists,
        ..
    } = seed;

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
    let mut picked_artist_ids = HashSet::new();

    db_album.title = edit.album_title.clone();
    db_album.year = edit.album_year;
    db_album.artist_id = materialize_artist_assignment(
        &edit.album_artist_assignments[0],
        artists,
        &mut picked_artist_ids,
        ids,
        now,
    );

    db_release.pressing = edit.pressing.clone();

    for (track, t_edit) in db_tracks.iter_mut().zip(edit.tracks.iter()) {
        track.title = t_edit.title.clone();
        track.side = t_edit.side;
        track.track_number = t_edit.track_number;
    }

    album_artists.clear();
    for (position, assignment) in edit.album_artist_assignments.iter().enumerate().skip(1) {
        let artist_id =
            materialize_artist_assignment(assignment, artists, &mut picked_artist_ids, ids, now);
        album_artists.push(DbAlbumArtist::new(
            &db_album.id,
            &artist_id,
            position as i32,
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
                    &mut picked_artist_ids,
                    ids,
                    now,
                );
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

    Ok(picked_artist_ids)
}

/// The artist row one assignment links: a picked library artist as itself,
/// recorded in `picked`; a credit as a new credit row.
fn materialize_artist_assignment(
    assignment: &crate::import::ArtistAssignment,
    artists: &mut Vec<crate::db::DbArtist>,
    picked: &mut HashSet<String>,
    ids: &dyn coven::IdProvider,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let artist = assignment.write_row(ids, now);
    let id = artist.id.clone();
    if matches!(assignment, crate::import::ArtistAssignment::Picked { .. }) {
        picked.insert(id.clone());
        if artists.iter().any(|candidate| candidate.id == id) {
            return id;
        }
    }
    artists.push(artist);
    id
}

/// The release `release_ref` names, from storage or fetched and stored now. A
/// stored release is fetched again only when that could add what its fetch
/// missed. Takes the bare `LibraryManager` because the sweep and library
/// re-identification do not hold an `ImportServiceHandle`.
pub(crate) async fn prepare_release(
    library_manager: &LibraryManager,
    release_ref: &MetadataRef,
    priority: CallPriority,
) -> Result<crate::import::source_release::SourceRelease, crate::import::ImportError> {
    if let Some(stored) = library_manager.load_source_release(release_ref).await? {
        let discogs_configured =
            library_manager.can_fetch_releases_from(crate::import::Catalog::Discogs)?;
        if !stored.fetch_could_add(discogs_configured) {
            return Ok(stored);
        }
    }
    let payloads = library_manager
        .fetch_release_payloads(release_ref, priority)
        .await?;
    library_manager
        .save_source_release(&payloads.extract()?)
        .await?;
    // Read back: the stored records carry what its album's group added.
    library_manager
        .load_source_release(release_ref)
        .await?
        .ok_or_else(|| crate::import::ImportError::Internal {
            detail: format!(
                "{} release {} was stored and did not read back",
                release_ref.catalog.as_str(),
                release_ref.key
            ),
        })
}

/// Prepare every partner release of this pick: other catalogs' releases of the
/// primary's pressing. Two releases from one catalog are refused rather than
/// one chosen.
pub(crate) async fn prepare_partners(
    library_manager: &LibraryManager,
    primary: &MetadataRef,
    partners: &[MetadataRef],
    priority: CallPriority,
) -> Result<Vec<crate::import::source_release::SourceRelease>, crate::import::ImportError> {
    let mut claimed = vec![primary.catalog];
    let mut prepared = Vec::with_capacity(partners.len());
    for partner in partners {
        if claimed.contains(&partner.catalog) {
            return Err(crate::import::ImportError::Internal {
                detail: format!(
                    "a pick names two {} releases for one pressing",
                    partner.catalog.as_str()
                ),
            });
        }
        claimed.push(partner.catalog);
        prepared.push(prepare_release(library_manager, partner, priority).await?);
    }
    Ok(prepared)
}

/// The records a pick commits: the primary's and every partner's, each
/// release's own record outranking what another says about its catalog.
pub(crate) fn records_for_commit(
    primary: &crate::import::source_release::SourceRelease,
    partners: &[crate::import::source_release::SourceRelease],
) -> Vec<crate::import::ReleaseRecord> {
    let claimed: Vec<_> = std::iter::once(primary).chain(partners).collect();
    crate::import::source_release::claimed_records(&claimed)
}

#[cfg(test)]
mod tests;
