use crate::discogs::DiscogsClient;
use crate::import::cover_art::CoverArtArchiveClient;
use crate::import::folder_registry::{ImportFolderRegistry, WatchedFolder};
use crate::import::folder_scanner::{FolderCandidate, InvalidCandidate};
use crate::import::progress::ImportProgressHandle;
use crate::import::types::{
    DiscoveredFile, ImportCommand, ImportProgress, MetadataSource, StorageMode,
};
use crate::library::LibraryManager;
use std::collections::HashMap;
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
/// All events emitted by the import service. One channel, one subscriber (the bus).
#[derive(Debug, Clone)]
pub enum ImportEvent {
    Scan(ScanEvent),
    ImportProgress {
        candidate_key: String,
        progress: ImportProgress,
    },
    /// Per-track loudness measurement progress for an importing candidate. A
    /// high-frequency tick (one per track as its decode+measure completes,
    /// plus a 0/N start and an N/N final), routed to a native leaf view rather
    /// than the candidate row's coarse step. Separate from `ImportProgress` so
    /// it bypasses the release/import progress subscribers — it carries the
    /// candidate key, not a release/import id.
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
    /// classified text). Core emits this on every transition — extraction
    /// start, each source/OCR completion, natural end, and cancellation. The
    /// reducer writes it wholesale; no partial-update logic needed.
    SignalsUpdated {
        candidate_key: String,
        signals: crate::signals::Signals,
    },
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
/// implies. Shared by the save and revalidate paths so both fold the same
/// outcomes the same way: success confirms the key, a 401 rejects it, and a
/// network/rate-limit failure leaves it unvalidated to retry. `validate_token`
/// only ever returns those variants; the remaining `DiscogsError`s (not
/// reachable from a search request) also fold to `Unvalidated` — "couldn't
/// confirm" — so this match stays total without a catch-all.
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
/// Holds no caches; the network-layer caches in
/// `crate::musicbrainz`, `crate::discogs::client`, and
/// the Cover Art Archive client carry session hits transparently for every
/// caller. The handle is a thin orchestration layer: it dispatches
/// fire-and-forget prefetches, builds `ImportCommand`s carrying just
/// `MetadataRef`s, and forwards them to the worker.
#[derive(Clone)]
pub struct ImportServiceHandle {
    requests_tx: mpsc::UnboundedSender<ImportCommand>,
    progress_handle: ImportProgressHandle,
    library_manager: LibraryManager,
    /// Unified event channel — all import service events go here.
    /// `pub(crate)` because `app.rs` clones it to seed the identify and signals
    /// services at startup.
    pub(crate) event_tx: broadcast::Sender<ImportEvent>,
    folder_registry: Arc<Mutex<ImportFolderRegistry>>,
    watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
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
    /// A previously-emitted candidate no longer resolves on disk — the watcher
    /// re-scanned its folder and the release is gone. The reducer removes it by
    /// key (the key is the candidate's folder path).
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

/// Commands to the folder watcher. `Watch` starts watching a folder (idempotent)
/// and scans it; `Unwatch` stops watching it.
pub enum WatcherCommand {
    Watch(std::path::PathBuf),
    Unwatch(std::path::PathBuf),
}

impl ImportServiceHandle {
    /// Create a new ImportHandle with the given dependencies
    pub fn new(
        requests_tx: mpsc::UnboundedSender<ImportCommand>,
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        watcher_tx: mpsc::UnboundedSender<WatcherCommand>,
        event_tx: broadcast::Sender<ImportEvent>,
        folder_registry: Arc<Mutex<ImportFolderRegistry>>,
        cover_art_archive: CoverArtArchiveClient,
    ) -> Self {
        let progress_handle = ImportProgressHandle::new(event_tx.clone(), runtime_handle.clone());
        Self {
            requests_tx,
            progress_handle,
            library_manager,
            event_tx,
            folder_registry,
            watcher_tx,
            runtime_handle,
            cover_art_archive,
        }
    }
}

pub fn remap_artist_links<T: Clone>(
    links: &[T],
    artist_id_map: &HashMap<String, String>,
    label: &str,
    artist_id: impl Fn(&T) -> &str,
    assign_artist_id: impl Fn(&mut T, String),
) -> Result<Vec<T>, String> {
    links
        .iter()
        .map(|link| {
            let parsed_artist_id = artist_id(link);
            let actual_id = artist_id_map.get(parsed_artist_id).ok_or_else(|| {
                format!("{label} artist ID {parsed_artist_id} not found in artist map")
            })?;
            let mut remapped = link.clone();
            assign_artist_id(&mut remapped, actual_id.clone());
            Ok(remapped)
        })
        .collect()
}

/// Remap track artist IDs from the parsed (temporary) artist IDs to the actual DB IDs.
///
/// The ParsedAlbum's track_artists reference artist IDs generated during parsing, but
/// find_or_create_artists may have resolved them to existing DB artists. This function
/// applies the same ID mapping used for album_artists.
pub fn remap_track_artists(
    track_artists: &[crate::db::DbTrackArtist],
    artist_id_map: &HashMap<String, String>,
) -> Result<Vec<crate::db::DbTrackArtist>, String> {
    remap_artist_links(
        track_artists,
        artist_id_map,
        "track artist",
        |ta| &ta.artist_id,
        |ta, artist_id| ta.artist_id = artist_id,
    )
}

/// Remap album artist IDs from the parsed (temporary) artist IDs to the actual DB IDs.
pub fn remap_album_artists(
    album_artists: &[crate::db::DbAlbumArtist],
    artist_id_map: &HashMap<String, String>,
) -> Result<Vec<crate::db::DbAlbumArtist>, String> {
    remap_artist_links(
        album_artists,
        artist_id_map,
        "album artist",
        |aa| &aa.artist_id,
        |aa, artist_id| aa.artist_id = artist_id,
    )
}

/// Fetch artist images for artists that have a Discogs ID but no image yet.
/// Best-effort: never fails the import.
pub(crate) async fn fetch_artist_images(
    library_manager: &LibraryManager,
    discogs_client: &DiscogsClient,
    parsed_artists: &[crate::db::DbArtist],
    artist_id_map: &HashMap<String, String>,
) {
    for parsed_artist in parsed_artists {
        let actual_id = match artist_id_map.get(&parsed_artist.id) {
            Some(id) => id,
            None => continue,
        };

        // Only fetch if the artist has a Discogs ID
        let discogs_artist_id = match &parsed_artist.discogs_artist_id {
            Some(id) => id.clone(),
            None => continue,
        };

        // Check if artist already has an image in DB
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

        crate::import::artist_image::fetch_and_save_artist_image(
            actual_id,
            &discogs_artist_id,
            discogs_client,
            library_manager,
        )
        .await;
    }
}

/// Project a parsed album (mapper output) into the editor's
/// `ReleaseUserEdit` shape. Used by the Unknown preview path so the
/// edit-metadata form can seed itself from the file-tag projection
/// without going through a wire-shape `ImportSearchReleaseDetail` that
/// would require a synthetic release id. Also used by the reset-to-source
/// path to project cached source data back into the editor.
///
/// Track artist names are emitted positionally per existing
/// `track_artists` rows; an empty per-track list rolls up to "share
/// the album artist" in the editor's convention.
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

/// Project an `ImportSearchReleaseDetail` (the prefetch result for the
/// import confirmation pane) into the editor's `ReleaseUserEdit` shape,
/// honoring the user's identity choice:
///
/// - **Exact**: `pressing` comes from the picked release.
/// - **Approximate** / **Unknown**: `pressing` is `PressingEdit::blank()`.
///   The user explicitly didn't claim a specific pressing, so showing
///   them the source's pressing data would imply a claim they didn't
///   make. They can fill in fields they know (e.g., "JP" in country)
///   and the overlay carries those edits to commit.
///
/// Per-track artist override: when a track's source artist matches the
/// album-level artist or is missing, the per-track artist override is
/// empty (the track rolls up to "share the album artist" in the
/// editor's convention). When it differs, the override is the track's
/// artist verbatim.
///
/// Used by the Exact / Approximate import path so Swift can construct
/// the editor's seed values from a pre-shaped bridge payload — Swift
/// must not branch on `IdentityChoice` itself per the bridge-thinness
/// rule.
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

    // Number tracks per side, matching the per-side numbering the commit-side
    // mappers assign (musicbrainz_mapper / discogs_mapper reset their count at
    // each side, so A1,A2,B1,B2 -> 1,2,1,2). `detail.tracks` carries each
    // track's already-resolved `side` in the same order the seed was built, so
    // a counter keyed on side reproduces those numbers exactly. A release-global
    // `i + 1` index would diverge and, since `apply_user_edit_to_seed` writes
    // `track_number` verbatim onto the seed, corrupt the per-side numbers of any
    // multi-side vinyl/cassette/multi-disc release.
    let mut per_side_count: std::collections::HashMap<u32, i32> = std::collections::HashMap::new();
    let tracks = detail
        .tracks
        .iter()
        .map(|t| {
            let artist_names = match t.artist.as_deref() {
                Some(a) if !a.is_empty() && a != primary_album_artist => vec![a.to_string()],
                _ => Vec::new(),
            };
            let count = per_side_count.entry(t.side).or_insert(0);
            *count += 1;
            super::TrackUserEdit {
                title: t.title.clone(),
                side: t.side as i32,
                track_number: Some(*count),
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

/// Audio file paths from a `CategorizedFiles`, in the same order the
/// flattened pipeline produces. CUE-backed releases yield only the
/// audio file from each pair (the CUE itself carries no embedded
/// tags); per-track releases yield each track in scan order.
///
/// Used by the Unknown import path to feed `map_file_tags_to_db`.
pub fn categorized_audio_paths(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Vec<std::path::PathBuf> {
    use crate::import::folder_scanner::AudioContent;
    let mut paths = Vec::new();
    match &categorized.audio {
        AudioContent::CueFlacPairs { pairs, .. } => {
            for pair in pairs {
                paths.push(pair.audio_file.path.clone());
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

/// Flatten a `CategorizedFiles` into the `DiscoveredFile` list the downstream
/// import pipeline consumes for progress tracking and byte accounting.
/// Ordering mirrors the scan's structured output: audio first (pairs before
/// per-track, in natural sort within each), then artwork, then documents.
pub(crate) fn categorized_to_discovered_files(
    categorized: &crate::import::folder_scanner::CategorizedFiles,
) -> Vec<DiscoveredFile> {
    use crate::import::folder_scanner::AudioContent;
    let mut files: Vec<DiscoveredFile> = Vec::new();
    let push = |files: &mut Vec<DiscoveredFile>, f: &crate::import::folder_scanner::ScannedFile| {
        files.push(DiscoveredFile {
            path: f.path.clone(),
            relative_path: f.relative_path.clone(),
            size: f.size,
        });
    };
    match &categorized.audio {
        AudioContent::CueFlacPairs { pairs, .. } => {
            for pair in pairs {
                push(&mut files, &pair.cue_file);
                push(&mut files, &pair.audio_file);
            }
        }
        AudioContent::TrackFiles { tracks, .. } => {
            for f in tracks {
                push(&mut files, f);
            }
        }
    }
    for f in &categorized.artwork {
        push(&mut files, f);
    }
    for f in &categorized.documents {
        push(&mut files, f);
    }
    files
}
