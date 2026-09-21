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
    crate::import::folder_scanner::{ScanItem, ScannedFile},
    crate::import::track_slots::resolve_track_files,
    crate::import::types::{
        AudioFile, Catalog, CoverSelection, ImportPhase, PrepareStep, TrackFile,
    },
    crate::import::ParsedWorkGraph,
    notify_debouncer_full::DebounceEventResult,
    std::collections::{HashMap, HashSet},
    std::path::{Path, PathBuf},
    std::sync::Arc,
};

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

mod active_roots;
mod cover_image;
mod folder_watcher;
mod format_prep;
mod importing;
mod progress;
mod reconcile;
mod scanning;

use active_roots::{
    ActiveRoots, RemovalOutcome, RootRemovalBackend, RootScanCause, ServiceRootRemovalBackend,
};
use folder_watcher::FolderWatchSnapshot;
mod coordinator;
use crate::import::volume::{directories_changed, directory_modified_at, volume_kind, VolumeKind};
pub(crate) use folder_watcher::FolderWatcher;

use format_prep::resolve_file_content_type;

/// Which import run a progress event is about: the run the pane is watching and
/// the candidate row it redraws. The pair travels from the command that started
/// the import through every event it publishes.
#[derive(Clone, Copy)]
pub(super) struct ImportRun<'a> {
    pub(super) import_id: &'a str,
    pub(super) candidate_key: &'a str,
}

/// The files one import writes: every file the candidate contributes, and the
/// track rows already bound to the audio each names. Both are derived from the
/// same categorized folder, and the write needs them together.
#[derive(Clone, Copy)]
pub(super) struct ImportFiles<'a> {
    pub(super) discovered: &'a [ScannedFile],
    pub(super) tracks: &'a [TrackFile],
}

/// What `reconcile_prepared_release` yields: the release's rows with parsed
/// artist IDs already remapped to their real DB IDs, ready for the run pass.
struct PreparedMetadata {
    db_album: DbAlbum,
    db_release: DbRelease,
    db_tracks: Vec<DbTrack>,
    /// The cover the person picked for this candidate, whatever its source.
    selected_cover: Option<CoverSelection>,
    /// The exact prepared bytes of a picked remote cover.
    remote_cover_image: Option<cover_image::CoverCandidate>,
    /// The artwork the File Tags snapshot carried, with the content type the
    /// download reported — checked at read time and dropped by the resize.
    embedded_cover: Option<(Vec<u8>, crate::util::content_type::ContentType)>,
    existing_album_id: Option<String>,
    remapped_track_artists: Vec<DbTrackArtist>,
    remapped_album_artists: Vec<DbAlbumArtist>,
    work_graph: ParsedWorkGraph,
    remapped_release_artist_roles: Vec<DbReleaseArtistRole>,
    remapped_track_artist_roles: Vec<DbTrackArtistRole>,
    artists: Vec<crate::db::DbArtist>,
    artist_external_id_updates: Vec<(String, crate::db::DbArtist)>,
    artist_images: Vec<(crate::db::DbLibraryImage, Vec<u8>)>,
    /// Every catalog's description of this release. Empty for File Tags and
    /// direct entry, which name no catalog. Commit writes one
    /// record row per element.
    records: Vec<crate::import::ReleaseRecord>,
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
    pub(crate) candidate: crate::import::CandidateAsRead,
    pub(crate) file_tag_snapshot: Option<crate::import::file_tag_snapshot::FileTagSnapshot>,
}

impl ImportExpectation {
    /// Whether `current` is still the candidate this import was prepared
    /// from. The library write asks this inside its own transaction, so
    /// nothing can move the candidate between the answer and the commit.
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
}

/// One downloaded cover as the import funnel's candidate.
///
/// The content type was verified from the decoded bytes at download time. It
/// describes the download, not the stored blob — the resize re-encodes those
/// bytes — so it is checked and dropped, never recorded.
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

type RootScanStarter = Arc<
    dyn Fn(u64, PathBuf, mpsc::UnboundedSender<RootScanCompletion>) -> RootScanTask + Send + Sync,
>;

/// What one folder scan runs on: the import service's shared dependencies, plus
/// the OS watch installer the walk registers each directory it reaches with.
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
            // The error is dropped rather than passed on: `rescan_and_reconcile`
            // has already recorded it as the root's status and announced it, and
            // a refresh caller that reported it a second time would put two
            // dialogs on screen for one broken folder.
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

/// Select supplemental source credits by their stored source-track identity.
/// The draft supplies the track order and every audio binding.
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
    seed: &mut crate::import::ParsedAlbum,
    existing_artists: &HashMap<String, crate::db::DbArtist>,
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

/// Prepare a release by expanding its archived document set through known
/// relationships. Takes the bare `LibraryManager` because the sweep and library
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
    let stored = library_manager.load_release_payloads(release_ref).await?;
    let payloads = library_manager
        .fetch_release_payloads(release_ref, stored.as_ref(), priority)
        .await?;
    library_manager.store_release_payloads(&payloads).await?;
    Ok(payloads)
}

/// Prepare and retain every partner's exact documents for this selection.
///
/// A pick names one release per catalog: the primary is the document the draft
/// is read from, and every partner is a different catalog's release of the same
/// pressing. Two claims about one catalog are two answers to one question, so
/// this refuses them rather than picking one. The candidate's provenance is
/// written only after this returns, so a failed partner leaves the pick unmade.
pub(crate) async fn prepare_partners(
    library_manager: &LibraryManager,
    primary: &MetadataRef,
    partners: &[MetadataRef],
    priority: CallPriority,
) -> Result<Vec<crate::import::payloads::ReleasePayloads>, crate::import::ImportError> {
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

/// Derive committed identities from the exact documents prepared for a pick.
/// No mutable archive is read after selection has captured these sets.
pub(crate) fn records_for_commit(
    primary: &crate::import::payloads::ReleasePayloads,
    partners: &[crate::import::payloads::ReleasePayloads],
) -> Result<Vec<crate::import::ReleaseRecord>, crate::import::ImportError> {
    let claimed: Vec<_> = std::iter::once(primary)
        .chain(partners)
        .map(|payloads| (payloads.release().clone(), Some(payloads.clone())))
        .collect();
    crate::import::payloads::claimed_records(&claimed)
}

#[cfg(test)]
mod tests;
