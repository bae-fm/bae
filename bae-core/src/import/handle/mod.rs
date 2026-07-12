use crate::discogs::DiscogsClient;
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::folder_registry::{ImportFolderRegistry, WatchedFolder};
use crate::import::folder_scanner::{FolderCandidate, InvalidCandidate};
use crate::import::types::{
    ImportCommand, ImportProgress, ImportStep, MetadataSource, StorageMode,
};
use crate::library::LibraryManager;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

mod import;
mod scan;
mod search;
mod watch;

#[cfg(test)]
mod tests;

/// Send an import event on the broadcast bus, logging on send failure.
/// `broadcast::Sender::send` returns `Err` only when there are zero active
/// receivers — a warning is appropriate (something upstream lost interest)
/// but not fatal.
pub(super) fn send_event(sender: &broadcast::Sender<ImportEvent>, ev: ImportEvent) {
    if let Err(e) = sender.send(ev) {
        warn!("import event send failed: {}", e);
    }
}

#[derive(Debug, Clone)]
pub struct ImportCandidatesSnapshot {
    pub watched_folders: Vec<WatchedFolder>,
    pub folder_candidates: Vec<FolderImportCandidateSnapshot>,
    pub invalid_candidates: Vec<InvalidCandidate>,
}

#[derive(Debug, Clone)]
pub struct FolderImportCandidateSnapshot {
    pub candidate: FolderCandidate,
    pub runtime: CandidateRuntimeSnapshot,
}

#[derive(Debug, Clone)]
pub enum ImportCandidateSnapshot {
    Folder {
        candidate: FolderCandidate,
        runtime: CandidateRuntimeSnapshot,
    },
    Invalid(InvalidCandidate),
    Runtime {
        key: String,
        runtime: CandidateRuntimeSnapshot,
    },
}

#[derive(Debug, Clone)]
enum ScannedCandidate {
    Folder(FolderCandidate),
    Invalid(InvalidCandidate),
}

#[derive(Debug, Clone)]
pub struct CandidateRuntimeSnapshot {
    pub identify_state: crate::identify::IdentifyState,
    pub toolbar: Vec<crate::identify::ToolbarSignal>,
    pub signals: Option<crate::signals::Signals>,
    pub import_status: Option<CandidateImportStatusSnapshot>,
}

impl CandidateRuntimeSnapshot {
    fn idle() -> Self {
        Self {
            identify_state: crate::identify::IdentifyState::Idle,
            toolbar: Vec::new(),
            signals: None,
            import_status: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CandidateImportStatusSnapshot {
    Importing {
        progress_percent: u32,
        step: Option<ImportStep>,
    },
    Complete {
        release_id: String,
        album_id: String,
    },
    Error {
        error: String,
    },
}

#[derive(Debug, Default)]
pub(crate) struct ImportCandidateState {
    /// What the folder scanner last reported per candidate path.
    candidates: HashMap<String, ScannedCandidate>,
    /// Per-candidate-key runtime accumulated from recorded events (identify
    /// state, signals, import progress). A key without an entry has had no
    /// events; reads treat absence as the idle runtime. Also holds
    /// `reidentify:`-prefixed keys, which have no scanned candidate.
    runtime: HashMap<String, CandidateRuntimeSnapshot>,
}

impl ImportCandidateState {
    const REIDENTIFY_PREFIX: &str = "reidentify:";

    fn candidate_watched_folder_path(candidate: &ScannedCandidate) -> &str {
        match candidate {
            ScannedCandidate::Folder(candidate) => &candidate.watched_folder_path,
            ScannedCandidate::Invalid(candidate) => &candidate.watched_folder_path,
        }
    }

    fn runtime_entry(&mut self, candidate_key: &str) -> &mut CandidateRuntimeSnapshot {
        self.runtime
            .entry(candidate_key.to_string())
            .or_insert_with(CandidateRuntimeSnapshot::idle)
    }

    /// Runtime for a candidate key at read time. Absence from the runtime map
    /// means no identify/import events have been recorded for the key — the
    /// designed initial state — so it reads as idle.
    fn runtime_for(&self, key: &str) -> CandidateRuntimeSnapshot {
        self.runtime
            .get(key)
            .cloned()
            .unwrap_or_else(CandidateRuntimeSnapshot::idle)
    }

    pub(super) fn replace_root(
        &mut self,
        root: &Path,
        folder_candidates: Vec<FolderCandidate>,
        invalid_candidates: Vec<InvalidCandidate>,
    ) {
        self.remove_root(root);
        for candidate in folder_candidates {
            let key = candidate.path.to_string_lossy().into_owned();
            self.candidates
                .insert(key, ScannedCandidate::Folder(candidate));
        }
        for candidate in invalid_candidates {
            self.candidates.insert(
                candidate.path.to_string_lossy().into_owned(),
                ScannedCandidate::Invalid(candidate),
            );
        }
    }

    /// Drop every candidate under `root` (and its runtime), returning the
    /// removed candidate keys so the caller can announce them on the bus.
    pub(super) fn remove_root(&mut self, root: &Path) -> Vec<String> {
        let root_key = root.to_string_lossy();
        let removed: Vec<String> = self
            .candidates
            .iter()
            .filter(|(_, candidate)| {
                Self::candidate_watched_folder_path(candidate) == root_key.as_ref()
            })
            .map(|(key, _)| key.clone())
            .collect();
        self.candidates.retain(|_, candidate| {
            Self::candidate_watched_folder_path(candidate) != root_key.as_ref()
        });
        for key in &removed {
            self.runtime.remove(key);
        }
        removed
    }

    pub(super) fn set_skipped(&mut self, key: &str, skipped: bool) {
        if let Some(ScannedCandidate::Folder(candidate)) = self.candidates.get_mut(key) {
            candidate.skipped = skipped;
        }
    }

    pub(super) fn record_event(&mut self, event: &ImportEvent) {
        match event {
            ImportEvent::ImportProgress {
                candidate_key,
                progress,
            } => {
                let runtime = self.runtime_entry(candidate_key);
                runtime.import_status = match progress {
                    ImportProgress::Preparing { step, .. } => {
                        Some(CandidateImportStatusSnapshot::Importing {
                            progress_percent: 0,
                            step: Some(ImportStep::Preparing(*step)),
                        })
                    }
                    ImportProgress::Started { .. } => {
                        Some(CandidateImportStatusSnapshot::Importing {
                            progress_percent: 0,
                            step: None,
                        })
                    }
                    ImportProgress::Progress { percent, phase, .. } => {
                        Some(CandidateImportStatusSnapshot::Importing {
                            progress_percent: *percent as u32,
                            step: Some(ImportStep::Running(*phase)),
                        })
                    }
                    ImportProgress::Complete { id, album_id, .. }
                    | ImportProgress::RemoteUploadQueued { id, album_id, .. } => {
                        Some(CandidateImportStatusSnapshot::Complete {
                            release_id: id.clone(),
                            album_id: album_id.clone(),
                        })
                    }
                    ImportProgress::Failed { error, .. } => {
                        Some(CandidateImportStatusSnapshot::Error {
                            error: error.clone(),
                        })
                    }
                };
            }
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                state,
                toolbar,
            } => {
                let runtime = self.runtime_entry(candidate_key);
                runtime.identify_state = state.clone();
                runtime.toolbar = toolbar.clone();
            }
            ImportEvent::SignalsUpdated {
                candidate_key,
                signals,
            } => {
                let runtime = self.runtime_entry(candidate_key);
                runtime.signals = Some(signals.clone());
            }
            ImportEvent::Scan(_) | ImportEvent::ImportLoudnessProgress { .. } => {}
        }
    }

    pub(super) fn snapshot(&self, watched_folders: Vec<WatchedFolder>) -> ImportCandidatesSnapshot {
        let mut watched_order = HashMap::new();
        for (index, folder) in watched_folders.iter().enumerate() {
            watched_order.insert(folder.path.as_str(), index);
        }
        let order_for = |path: &str| match watched_order.get(path) {
            Some(index) => *index,
            None => usize::MAX,
        };

        let mut folder_candidates = Vec::new();
        let mut invalid_candidates = Vec::new();
        for (key, candidate) in &self.candidates {
            match candidate {
                ScannedCandidate::Folder(candidate) => folder_candidates.push((
                    key.as_str(),
                    FolderImportCandidateSnapshot {
                        candidate: candidate.clone(),
                        runtime: self.runtime_for(key),
                    },
                )),
                ScannedCandidate::Invalid(candidate) => {
                    invalid_candidates.push((key.as_str(), candidate.clone()))
                }
            }
        }
        fn sort_by_watched_folder<T, F, G>(
            candidates: &mut [(&str, T)],
            order_for: &F,
            watched_folder_path: G,
        ) where
            F: Fn(&str) -> usize,
            G: Fn(&T) -> &str,
        {
            candidates.sort_by(|(left_key, left), (right_key, right)| {
                order_for(watched_folder_path(left))
                    .cmp(&order_for(watched_folder_path(right)))
                    .then_with(|| left_key.cmp(right_key))
            });
        }
        sort_by_watched_folder(
            &mut folder_candidates,
            &order_for,
            |candidate: &FolderImportCandidateSnapshot| &candidate.candidate.watched_folder_path,
        );
        sort_by_watched_folder(
            &mut invalid_candidates,
            &order_for,
            |candidate: &InvalidCandidate| &candidate.watched_folder_path,
        );

        ImportCandidatesSnapshot {
            watched_folders,
            folder_candidates: folder_candidates
                .into_iter()
                .map(|(_, candidate)| candidate)
                .collect(),
            invalid_candidates: invalid_candidates
                .into_iter()
                .map(|(_, candidate)| candidate)
                .collect(),
        }
    }

    pub(super) fn get(&self, key: &str) -> Option<ImportCandidateSnapshot> {
        if key.starts_with(Self::REIDENTIFY_PREFIX) {
            return self.runtime.get(key).cloned().map(|runtime| {
                ImportCandidateSnapshot::Runtime {
                    key: key.to_string(),
                    runtime,
                }
            });
        }
        self.candidates.get(key).map(|candidate| match candidate {
            ScannedCandidate::Folder(candidate) => ImportCandidateSnapshot::Folder {
                candidate: candidate.clone(),
                runtime: self.runtime_for(key),
            },
            ScannedCandidate::Invalid(candidate) => {
                ImportCandidateSnapshot::Invalid(candidate.clone())
            }
        })
    }
}

/// All events emitted by the import service. One channel, one subscriber (the bus).
#[derive(Debug, Clone)]
pub enum ImportEvent {
    Scan(ScanEvent),
    ImportProgress {
        candidate_key: String,
        progress: ImportProgress,
    },
    /// Per-track loudness measurement progress for an importing candidate: a
    /// high-frequency tick routed to a native leaf view rather than the candidate
    /// row's coarse step. Separate from `ImportProgress` so it bypasses the
    /// release/import progress subscribers — it carries the candidate key, not a
    /// release or import id.
    ImportLoudnessProgress {
        candidate_key: String,
        tracks_done: u32,
        tracks_total: u32,
        /// Overall scan progress 0..1 for the determinate bar; advances within a
        /// track as it's measured, not just at track boundaries.
        fraction: f32,
    },
    /// Identify pipeline transitioned to a new state. Emitted by the
    /// `identify` module; carries the full state payload plus the pre-shaped
    /// signals toolbar (the interactive badge row) projected from the same
    /// transition, so the UI renders both from one event.
    IdentifyStateChanged {
        candidate_key: String,
        state: crate::identify::IdentifyState,
        toolbar: Vec<crate::identify::ToolbarSignal>,
    },
    /// Full snapshot of a candidate's extracted signals (disc ID, barcodes,
    /// classified text), emitted on every transition — extraction start, each
    /// source/OCR completion, natural end, and cancellation. The reducer writes
    /// it wholesale, so it needs no partial-update logic.
    SignalsUpdated {
        candidate_key: String,
        signals: crate::signals::Signals,
    },
}

impl ImportServiceHandle {
    pub(crate) fn record_candidate_event(&self, event: &ImportEvent) {
        self.candidate_state.lock().unwrap().record_event(event);
    }
}

/// Search query — one of the three search modes.
pub enum SearchQuery {
    General {
        artist: String,
        album: String,
        source: MetadataSource,
    },
    CatalogNumber {
        catalog_number: String,
        source: MetadataSource,
    },
    Barcode {
        barcode: String,
        source: MetadataSource,
    },
}

/// Search results grouped by release group, with the per-release library dupe
/// statuses the UI looks up by release id.
#[derive(Debug, Clone)]
pub struct GroupedSearchResults {
    pub groups: Vec<crate::import::release_group::ReleaseGroup>,
    pub statuses: Vec<crate::db::LibraryStatus>,
}

/// What `save_discogs_token` did with a submitted key, after validating against
/// Discogs first.
///
/// - `Valid` — Discogs accepted the key; it's stored and used.
/// - `Unvalidated` — Discogs was unreachable or rate-limited; the key is stored
///   optimistically and will be re-checked when possible.
/// - `Rejected` — Discogs returned 401; nothing is stored, so the UI keeps the
///   draft for the user to correct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscogsSaveOutcome {
    Valid,
    Unvalidated,
    Rejected,
}

/// Map a `DiscogsClient::validate_token` result to the validation state it
/// implies: success confirms the key, a 401 rejects it, and anything else —
/// network, rate-limit, or a `DiscogsError` a search request can't even reach —
/// folds to `Unvalidated`, "couldn't confirm", so the match stays total without
/// a catch-all. Shared by the save and revalidate paths so both fold alike.
fn validation_from_validate_result(
    result: Result<(), crate::discogs::client::DiscogsError>,
) -> crate::config::DiscogsValidation {
    use crate::config::DiscogsValidation;
    use crate::discogs::client::DiscogsError;
    match result {
        Ok(()) => DiscogsValidation::Valid,
        Err(DiscogsError::InvalidApiKey) => DiscogsValidation::Rejected,
        Err(
            e @ (DiscogsError::RateLimit
            | DiscogsError::Request(_)
            | DiscogsError::NotFound
            | DiscogsError::Serialization(_)),
        ) => {
            debug!("Discogs validation couldn't confirm the key ({e}); leaving it unvalidated to retry later");
            DiscogsValidation::Unvalidated
        }
    }
}

/// Handle for sending import requests and subscribing to progress updates.
///
/// A thin orchestration layer that dispatches prefetches, builds
/// `ImportCommand`s carrying just `MetadataRef`s, and forwards them to the
/// worker. It holds no caches of its own — the network-layer caches in
/// `crate::musicbrainz`, `crate::discogs::client`, and the Cover Art Archive
/// client serve every caller transparently.
#[derive(Clone)]
pub struct ImportServiceHandle {
    requests_tx: mpsc::UnboundedSender<ImportCommand>,
    library_manager: LibraryManager,
    /// Unified event channel — all import service events go here.
    /// `pub(crate)` because `app.rs` clones it to seed the identify and signals
    /// services at startup.
    pub(crate) event_tx: broadcast::Sender<ImportEvent>,
    folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    candidate_state: Arc<Mutex<ImportCandidateState>>,
    watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
    /// Installs/uninstalls OS-level folder watches, atomic with the registry
    /// mutation in `add_watched_folder`/`remove_watched_folder`.
    folder_watcher: Arc<crate::import::service::FolderWatcher>,
    runtime_handle: tokio::runtime::Handle,
    cover_art_archive: CoverArtArchiveClient,
}

#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// The watched-folder list changed (queried at load, or after add/remove).
    /// Carries the full ordered list; the reducer replaces its copy.
    WatchedFoldersChanged {
        folders: Vec<WatchedFolder>,
    },
    FolderCandidate(FolderCandidate),
    /// A leaf folder that looks like a release but failed validation
    /// (corrupt/zero-byte audio, corrupt image, CUE referencing missing audio).
    /// The reducer surfaces it under the Skipped tab with its reason. The key is
    /// the folder path, shared with `CandidateRemoved` for reconciliation.
    InvalidCandidate(InvalidCandidate),
    /// A candidate is gone: the watcher re-scanned its folder and the release
    /// no longer resolves on disk, or the folder it belonged to stopped being
    /// watched (one event per candidate the folder held). The reducer removes
    /// it by key (the key is the candidate's folder path); the extraction
    /// service cancels the key's in-flight extraction on this event.
    CandidateRemoved {
        candidate_key: String,
    },
    /// The user manually skipped or unskipped a candidate. The reducer flips the
    /// candidate's `skipped` flag in place; the import view re-tabs it from New
    /// to Skipped (or back). The key is the candidate's folder path.
    CandidateSkipChanged {
        candidate_key: String,
        skipped: bool,
    },
    /// A folder scan could not read the watched root. Previous candidates are
    /// left in place because the scan did not produce a replacement snapshot.
    Failed {
        error: String,
    },
    Finished,
}

/// Commands to the folder-watch reconciliation task. OS watch installation
/// itself happens synchronously in the handle (`FolderWatcher`, atomic with
/// the registry mutation) before either of these is sent. `Rescan` re-scans a
/// folder and reconciles its candidates; `Forget` drops the folder's
/// last-emitted candidate keys.
pub(crate) enum WatcherCommand {
    Rescan(std::path::PathBuf),
    Forget(std::path::PathBuf),
}

impl ImportServiceHandle {
    pub(crate) fn new(
        requests_tx: mpsc::UnboundedSender<ImportCommand>,
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
        event_tx: broadcast::Sender<ImportEvent>,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        candidate_state: Arc<Mutex<ImportCandidateState>>,
        folder_watcher: Arc<crate::import::service::FolderWatcher>,
        cover_art_archive: CoverArtArchiveClient,
    ) -> Self {
        Self {
            requests_tx,
            library_manager,
            event_tx,
            folder_registry,
            candidate_state,
            watcher_tx,
            folder_watcher,
            runtime_handle,
            cover_art_archive,
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn event_sender_for_test(&self) -> broadcast::Sender<ImportEvent> {
        self.event_tx.clone()
    }

    pub fn get_import_candidates(&self) -> ImportCandidatesSnapshot {
        let watched_folders = self.watched_folders();
        self.candidate_state
            .lock()
            .unwrap()
            .snapshot(watched_folders)
    }

    pub fn get_candidate(&self, key: &str) -> Option<ImportCandidateSnapshot> {
        self.candidate_state.lock().unwrap().get(key)
    }
}

/// Remap parsed (temporary) artist IDs to their actual DB IDs.
///
/// A `ParsedAlbum`'s artist-link rows reference artist IDs generated during
/// parsing; reconcile may have resolved those to existing DB artists.
/// `label` names the link kind in the unmapped-ID error.
pub(crate) fn remap_artist_links<T: Clone>(
    links: &[T],
    artist_id_map: &HashMap<String, String>,
    label: &str,
    artist_id: impl Fn(&T) -> &str,
    assign_artist_id: impl Fn(&mut T, String),
) -> Result<Vec<T>, crate::import::ImportError> {
    links
        .iter()
        .map(|link| {
            let parsed_artist_id = artist_id(link);
            let actual_id = artist_id_map.get(parsed_artist_id).ok_or_else(|| {
                crate::import::ImportError::Internal {
                    detail: format!("{label} artist ID {parsed_artist_id} not found in artist map"),
                }
            })?;
            let mut remapped = link.clone();
            assign_artist_id(&mut remapped, actual_id.clone());
            Ok(remapped)
        })
        .collect()
}

/// Fetch artist images for artists that have a Discogs ID but no image yet.
/// Best-effort: never fails the import.
pub(crate) async fn fetch_artist_images(
    library_manager: &LibraryManager,
    discogs_client: &DiscogsClient,
    parsed_artists: &[crate::db::DbArtist],
    artist_id_map: &HashMap<String, String>,
) -> Vec<(crate::db::DbLibraryImage, Vec<u8>)> {
    let mut images = Vec::new();
    for parsed_artist in parsed_artists {
        let actual_id = match artist_id_map.get(&parsed_artist.id) {
            Some(id) => id,
            None => continue,
        };

        let discogs_artist_id = match &parsed_artist.discogs_artist_id {
            Some(id) => id.clone(),
            None => continue,
        };

        match library_manager
            .get_library_image(actual_id, &crate::db::LibraryImageType::Artist)
            .await
        {
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(e) => {
                warn!("failed to check existing artist image for artist {actual_id}: {e}");
                continue;
            }
        }

        if let Some(image) = crate::import::artist_image::fetch_artist_image(
            actual_id,
            &discogs_artist_id,
            discogs_client,
            library_manager,
        )
        .await
        {
            images.push(image);
        }
    }
    images
}

/// Project a parsed album (mapper output) into the editor's `ReleaseUserEdit`
/// shape. The Unknown preview path uses it so the edit-metadata form can seed
/// from the local-evidence projection without a wire-shape
/// `ImportSearchReleaseDetail`, which would need a synthetic release id; the
/// reset-to-source path uses it to project cached source data back into the
/// editor.
///
/// Track artist names are emitted positionally per existing `track_artists` row;
/// an empty per-track list means "share the album artist" in the editor's
/// convention.
pub fn parsed_album_to_user_edit(parsed: &super::ParsedAlbum) -> crate::import::ReleaseUserEdit {
    let primary_artist_name = parsed
        .artists
        .iter()
        .find(|a| a.id == parsed.album.artist_id)
        .map(|a| a.name.clone())
        .expect("primary artist id not found in ParsedAlbum.artists");

    let mut album_artist_names = vec![primary_artist_name];
    let mut junctions = parsed.album_artists.clone();
    junctions.sort_by_key(|aa| aa.position);
    for aa in &junctions {
        if let Some(artist) = parsed.artists.iter().find(|a| a.id == aa.artist_id) {
            album_artist_names.push(artist.name.clone());
        }
    }

    let tracks = parsed
        .tracks
        .iter()
        .map(|t| {
            let mut credits: Vec<&crate::db::DbTrackArtist> = parsed
                .track_artists
                .iter()
                .filter(|ta| ta.track_id == t.id)
                .collect();
            credits.sort_by_key(|ta| ta.position);
            let artist_names = credits
                .iter()
                .filter_map(|ta| {
                    parsed
                        .artists
                        .iter()
                        .find(|a| a.id == ta.artist_id)
                        .map(|a| a.name.clone())
                })
                .collect();
            crate::import::TrackUserEdit {
                title: t.title.clone(),
                side: t.side,
                track_number: t.track_number,
                artist_names,
            }
        })
        .collect();

    crate::import::ReleaseUserEdit {
        album_title: parsed.album.title.clone(),
        album_artist_names,
        pressing: crate::import::PressingEdit {
            year: parsed.release.pressing.year,
            format: parsed.release.pressing.format.clone(),
            label: parsed.release.pressing.label.clone(),
            catalog_number: parsed.release.pressing.catalog_number.clone(),
            country: parsed.release.pressing.country.clone(),
            barcode: parsed.release.pressing.barcode.clone(),
        },
        tracks,
    }
}

/// Project an `ImportSearchReleaseDetail` (the prefetch result for the import
/// confirmation pane) into the editor's `ReleaseUserEdit` shape, honoring the
/// user's identity choice:
///
/// - **Exact**: `pressing` comes from the picked release.
/// - **Approximate** / **Unknown**: `pressing` is `PressingEdit::blank()` — the
///   user didn't claim a specific pressing, so showing them the source's
///   pressing data would imply a claim they never made. They can still fill in
///   fields they know, and the overlay carries those edits to commit.
///
/// A track whose source artist matches the album artist, or is missing, seeds an
/// empty per-track override ("share the album artist"); one that differs seeds
/// the track's artist verbatim.
///
/// Swift calls this to build the editor's seed values, rather than branching on
/// `IdentityChoice` itself — the bridge stays thin.
pub fn shape_user_edit_from_search_detail(
    detail: &super::search::ImportSearchReleaseDetail,
    choice: &super::IdentityChoice,
) -> super::ReleaseUserEdit {
    let primary_album_artist = match &detail.artist {
        Some(name) => name.clone(),
        None => {
            warn!(
                "shape_user_edit_from_search_detail: detail.artist is None for release {}; defaulting to empty (editor save will be disabled until user fills it in)",
                detail.release_id
            );
            String::new()
        }
    };

    // Number tracks per side through `per_side_positions`, the same function the
    // commit-side assembler uses, over side values from the same derivation
    // (`medium_sides` for MB, `process_tracklist` for Discogs) — so the editor
    // seed and the committed rows can never disagree. A release-global `i + 1`
    // index would diverge, and since `apply_user_edit_to_seed` writes
    // `track_number` verbatim onto the seed, it would corrupt the per-side numbers
    // of every multi-side vinyl / cassette / multi-disc release.
    let numbers = crate::import::assemble::per_side_positions(detail.tracks.iter().map(|t| t.side));
    let tracks = detail
        .tracks
        .iter()
        .zip(numbers)
        .map(|(t, number)| {
            let artist_names = match t.artist.as_deref() {
                Some(a) if !a.is_empty() && a != primary_album_artist => vec![a.to_string()],
                _ => Vec::new(),
            };
            super::TrackUserEdit {
                title: t.title.clone(),
                side: t.side as i32,
                track_number: Some(number),
                artist_names,
            }
        })
        .collect();

    let pressing = match choice {
        super::IdentityChoice::Exact { .. } => super::PressingEdit {
            year: detail.year,
            format: detail.format.clone(),
            label: detail.label.clone(),
            catalog_number: detail.catalog_number.clone(),
            country: detail.country.clone(),
            barcode: detail.barcode.clone(),
        },
        super::IdentityChoice::Approximate { .. } | super::IdentityChoice::Unknown => {
            super::PressingEdit::blank()
        }
    };

    super::ReleaseUserEdit {
        album_title: detail.title.clone(),
        album_artist_names: vec![primary_album_artist],
        pressing,
        tracks,
    }
}

/// Audio file paths from a `CategorizedFiles`, in the order the flattened
/// pipeline produces. A CUE-backed release yields only the audio files each pair
/// references (the CUE itself carries no embedded tags); a per-track release
/// yields each track in scan order. The Unknown import path reads embedded cover
/// art from these.
pub(crate) fn categorized_audio_paths(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Vec<std::path::PathBuf> {
    use crate::import::folder_scanner::AudioContent;
    let mut paths = Vec::new();
    match &categorized.audio {
        AudioContent::CueFlacPairs { pairs, .. } => {
            for pair in pairs {
                paths.extend(pair.audio_files.iter().map(|file| file.path.clone()));
            }
        }
        AudioContent::TrackFiles { tracks, .. } => {
            for f in tracks {
                paths.push(f.path.clone());
            }
        }
    }
    paths
}

/// Flatten a `CategorizedFiles` into the flat file list the downstream
/// import pipeline consumes for progress tracking and byte accounting.
/// Ordering mirrors the scan's structured output: audio first (pairs before
/// per-track, in natural sort within each), then artwork, then documents.
pub(crate) fn flatten_categorized_files(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Vec<crate::import::folder_scanner::ScannedFile> {
    use crate::import::folder_scanner::AudioContent;
    let mut files = Vec::new();
    match &categorized.audio {
        AudioContent::CueFlacPairs { pairs, .. } => {
            for pair in pairs {
                files.push(pair.cue_file.clone());
                files.extend(pair.audio_files.iter().cloned());
            }
        }
        AudioContent::TrackFiles { tracks, .. } => {
            files.extend(tracks.iter().cloned());
        }
    }
    files.extend(categorized.artwork.iter().cloned());
    files.extend(categorized.documents.iter().cloned());
    files
}
