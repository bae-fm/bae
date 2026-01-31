//! Import workflow helpers
//!
//! Standalone async functions that operate on AppService state for import operations.
//! These replace the methods from ImportContext.

use crate::ui::app_service::AppService;
use crate::ui::Route;
use bae_core::discogs::client::DiscogsSearchParams;
use bae_core::discogs::{DiscogsClient, DiscogsRelease};
use bae_core::import::cover_art::fetch_cover_art_from_archive;
use bae_core::import::{
    cover_art, detect_folder_contents, DetectedCandidate as CoreDetectedCandidate, ImportProgress,
    ImportRequest, MatchCandidate, MatchSource, ScanEvent,
};
use bae_core::musicbrainz::{
    lookup_by_discid, lookup_release_by_id, search_releases_with_params, ExternalUrls, MbRelease,
    ReleaseSearchParams,
};
use bae_ui::display_types::{
    AudioContentInfo, CategorizedFileInfo, FolderMetadata as DisplayFolderMetadata,
    MatchCandidate as DisplayMatchCandidate, MatchSourceType, SearchSource, SelectedCover,
};
use bae_ui::stores::import::CandidateEvent;
use bae_ui::stores::AppStateStoreExt;
use bae_ui::ImportSource;
use dioxus::prelude::*;
use dioxus::router::Navigator;
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

// ============================================================================
// Conversion helpers (from old import_context/state.rs)
// ============================================================================

/// Compute the expected filename for a remote cover download.
pub fn compute_expected_cover_filename(url: &str, source: &str) -> String {
    let extension = url
        .rsplit('/')
        .next()
        .and_then(|filename| {
            let ext = filename.rsplit('.').next()?;
            let ext_lower = ext.to_lowercase();
            if ["jpg", "jpeg", "png", "gif", "webp"].contains(&ext_lower.as_str()) {
                Some(ext_lower)
            } else {
                None
            }
        })
        .unwrap_or_else(|| "jpg".to_string());
    let base_name = match source.to_lowercase().as_str() {
        "musicbrainz" | "mb" => "cover-mb",
        "discogs" => "cover-discogs",
        _ => "cover",
    };
    format!(".bae/{}.{}", base_name, extension)
}

/// Convert bae-core FolderMetadata to display type
pub fn to_display_metadata(m: &bae_core::import::FolderMetadata) -> DisplayFolderMetadata {
    DisplayFolderMetadata {
        artist: m.artist.clone(),
        album: m.album.clone(),
        year: m.year,
        track_count: m.track_count,
        discid: m.discid.clone(),
        mb_discid: m.mb_discid.clone(),
        confidence: m.confidence,
        folder_tokens: bae_core::musicbrainz::extract_search_tokens(m),
    }
}

/// Convert display FolderMetadata back to core type (for ranking functions)
pub fn from_display_metadata(m: &DisplayFolderMetadata) -> bae_core::import::FolderMetadata {
    bae_core::import::FolderMetadata {
        artist: m.artist.clone(),
        album: m.album.clone(),
        year: m.year,
        track_count: m.track_count,
        discid: m.discid.clone(),
        mb_discid: m.mb_discid.clone(),
        confidence: m.confidence,
        folder_tokens: m.folder_tokens.clone(),
    }
}

/// Convert bae-core MatchCandidate to display type
pub fn to_display_candidate(candidate: &MatchCandidate) -> DisplayMatchCandidate {
    let (
        source_type,
        format,
        country,
        label,
        catalog_number,
        original_year,
        musicbrainz_release_id,
        musicbrainz_release_group_id,
        discogs_release_id,
        discogs_master_id,
    ) = match &candidate.source {
        MatchSource::MusicBrainz(release) => (
            MatchSourceType::MusicBrainz,
            release.format.clone(),
            release.country.clone(),
            release.label.clone(),
            release.catalog_number.clone(),
            release.first_release_date.clone(),
            Some(release.release_id.clone()),
            Some(release.release_group_id.clone()),
            None,
            None,
        ),
        MatchSource::Discogs(result) => (
            MatchSourceType::Discogs,
            result.format.as_ref().map(|v| v.join(", ")),
            result.country.clone(),
            result.label.as_ref().map(|v| v.join(", ")),
            None,
            None,
            None,
            None,
            Some(result.id.to_string()),
            result.master_id.map(|id| id.to_string()),
        ),
    };

    DisplayMatchCandidate {
        title: candidate.title(),
        artist: match &candidate.source {
            MatchSource::MusicBrainz(r) => r.artist.clone(),
            MatchSource::Discogs(r) => r.title.split(" - ").next().unwrap_or("").to_string(),
        },
        year: candidate.year(),
        cover_url: candidate.cover_art_url(),
        format,
        country,
        label,
        catalog_number,
        source_type,
        original_year,
        musicbrainz_release_id,
        musicbrainz_release_group_id,
        discogs_release_id,
        discogs_master_id,
    }
}

// ============================================================================
// Discogs client helper
// ============================================================================

/// Get or create the Discogs client.
pub fn get_discogs_client() -> Result<DiscogsClient, String> {
    let api_key = keyring::Entry::new("bae", "discogs_api_key")
        .ok()
        .and_then(|e| e.get_password().ok());

    match api_key {
        Some(key) if !key.is_empty() => Ok(DiscogsClient::new(key)),
        _ => Err(
            "Discogs API key not configured. Go to Settings → API Keys to add your key."
                .to_string(),
        ),
    }
}

// ============================================================================
// Detection helpers
// ============================================================================

pub enum DiscIdLookupResult {
    NoMatches,
    SingleMatch(Box<DisplayMatchCandidate>),
    MultipleMatches(Vec<DisplayMatchCandidate>),
}

/// Detect local metadata and files for a candidate before it is shown in the UI.
pub fn detect_candidate_locally(
    candidate: &CoreDetectedCandidate,
) -> Result<(CategorizedFileInfo, DisplayFolderMetadata), String> {
    let files = categorized_files_from_scanned(&candidate.files);

    info!(
        "Detecting metadata for candidate: {} ({:?})",
        candidate.name, candidate.path
    );

    let folder_contents = detect_folder_contents(candidate.path.clone())
        .map_err(|e| format!("Failed to detect folder contents: {}", e))?;
    let core_metadata = folder_contents.metadata;

    info!(
        "Detected metadata: artist={:?}, album={:?}, year={:?}, mb_discid={:?}",
        core_metadata.artist, core_metadata.album, core_metadata.year, core_metadata.mb_discid
    );

    let metadata = to_display_metadata(&core_metadata);

    Ok((files, metadata))
}

/// Lookup a MusicBrainz release by DiscID.
pub async fn lookup_discid(mb_discid: &str) -> Result<DiscIdLookupResult, String> {
    info!("🎵 Looking up MB DiscID: {}", mb_discid);

    match lookup_by_discid(mb_discid).await {
        Ok((releases, external_urls)) => {
            Ok(handle_discid_lookup_result(releases, external_urls).await)
        }
        Err(e) => {
            info!("MB DiscID lookup failed: {}", e);
            Err(format!(
                "MusicBrainz lookup failed: {}. You can retry or search manually.",
                e,
            ))
        }
    }
}

/// Handle DiscID lookup result: process 0/1/multiple matches and return result
async fn handle_discid_lookup_result(
    releases: Vec<MbRelease>,
    external_urls: ExternalUrls,
) -> DiscIdLookupResult {
    if releases.is_empty() {
        info!("No exact matches found");
        return DiscIdLookupResult::NoMatches;
    }
    info!("Found {} exact matches", releases.len());

    // Get discogs client if available (for cover art fallback)
    let discogs_client = get_discogs_client().ok();
    let cover_art_futures: Vec<_> = releases
        .iter()
        .map(|mb_release| {
            cover_art::fetch_cover_art_for_mb_release(
                mb_release,
                &external_urls,
                discogs_client.as_ref(),
            )
        })
        .collect();
    let cover_art_urls: Vec<_> = futures::future::join_all(cover_art_futures).await;

    // Create core MatchCandidates then convert to display types
    let core_candidates: Vec<MatchCandidate> = releases
        .into_iter()
        .zip(cover_art_urls.into_iter())
        .map(|(mb_release, cover_art_url)| MatchCandidate {
            source: MatchSource::MusicBrainz(mb_release),
            confidence: 100.0,
            match_reasons: vec!["Exact DiscID match".to_string()],
            cover_art_url,
        })
        .collect();

    let display_candidates: Vec<DisplayMatchCandidate> =
        core_candidates.iter().map(to_display_candidate).collect();

    if display_candidates.len() == 1 {
        DiscIdLookupResult::SingleMatch(Box::new(display_candidates.into_iter().next().unwrap()))
    } else {
        DiscIdLookupResult::MultipleMatches(display_candidates)
    }
}

// ============================================================================
// Search helpers
// ============================================================================

fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Search MusicBrainz and rank results
async fn search_mb_and_rank(
    params: ReleaseSearchParams,
    metadata: Option<bae_core::import::FolderMetadata>,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    match search_releases_with_params(&params).await {
        Ok(releases) => {
            info!("✓ MusicBrainz search returned {} result(s)", releases.len());
            let mut candidates = if let Some(ref meta) = metadata {
                use bae_core::import::rank_mb_matches;
                rank_mb_matches(meta, releases)
            } else {
                releases
                    .into_iter()
                    .map(|release| MatchCandidate {
                        source: MatchSource::MusicBrainz(release),
                        confidence: 50.0,
                        match_reasons: vec!["Manual search result".to_string()],
                        cover_art_url: None,
                    })
                    .collect()
            };

            let cover_art_futures: Vec<_> = candidates
                .iter()
                .map(|candidate| {
                    let release_id = match &candidate.source {
                        MatchSource::MusicBrainz(release) => release.release_id.clone(),
                        _ => String::new(),
                    };
                    async move {
                        if !release_id.is_empty() {
                            debug!("Fetching cover art for release {}", release_id);
                            fetch_cover_art_from_archive(&release_id).await
                        } else {
                            None
                        }
                    }
                })
                .collect();
            let cover_art_results = futures::future::join_all(cover_art_futures).await;
            for (candidate, cover_url) in candidates.iter_mut().zip(cover_art_results.into_iter()) {
                if candidate.cover_art_url.is_none() {
                    candidate.cover_art_url = cover_url;
                }
            }

            Ok(candidates.iter().map(to_display_candidate).collect())
        }
        Err(e) => {
            warn!("✗ MusicBrainz search failed: {}", e);
            Err(format!("MusicBrainz search failed: {}", e))
        }
    }
}

/// Search Discogs and rank results
async fn search_discogs_and_rank(
    client: &DiscogsClient,
    params: DiscogsSearchParams,
    metadata: Option<bae_core::import::FolderMetadata>,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    match client.search_with_params(&params).await {
        Ok(results) => {
            info!("✓ Discogs search returned {} result(s)", results.len());
            let candidates: Vec<MatchCandidate> = if let Some(ref meta) = metadata {
                use bae_core::import::rank_discogs_matches;
                rank_discogs_matches(meta, results)
            } else {
                results
                    .into_iter()
                    .map(|result| MatchCandidate {
                        source: MatchSource::Discogs(result),
                        confidence: 50.0,
                        match_reasons: vec!["Manual search result".to_string()],
                        cover_art_url: None,
                    })
                    .collect()
            };

            Ok(candidates.iter().map(to_display_candidate).collect())
        }
        Err(e) => {
            warn!("✗ Discogs search failed: {}", e);
            Err(format!("Discogs search failed: {}", e))
        }
    }
}

/// General search by artist, album, year, label
pub async fn search_general(
    metadata: Option<DisplayFolderMetadata>,
    source: SearchSource,
    artist: String,
    album: String,
    year: String,
    label: String,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let core_metadata = metadata.as_ref().map(from_display_metadata);
    match source {
        SearchSource::MusicBrainz => {
            let params = ReleaseSearchParams {
                artist: non_empty(artist),
                album: non_empty(album),
                year: non_empty(year),
                label: non_empty(label),
                catalog_number: None,
                barcode: None,
                format: None,
                country: None,
            };
            info!("🎵 MusicBrainz general search: {:?}", params);
            search_mb_and_rank(params, core_metadata).await
        }
        SearchSource::Discogs => {
            let client = get_discogs_client()?;
            let params = DiscogsSearchParams {
                artist: non_empty(artist),
                release_title: non_empty(album),
                year: non_empty(year),
                label: non_empty(label),
                catno: None,
                barcode: None,
                format: None,
                country: None,
            };
            info!("🔍 Discogs general search: {:?}", params);
            search_discogs_and_rank(&client, params, core_metadata).await
        }
    }
}

/// Search by catalog number only
pub async fn search_by_catalog_number(
    metadata: Option<DisplayFolderMetadata>,
    source: SearchSource,
    catalog_number: String,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let core_metadata = metadata.as_ref().map(from_display_metadata);
    match source {
        SearchSource::MusicBrainz => {
            let params = ReleaseSearchParams {
                artist: None,
                album: None,
                year: None,
                label: None,
                catalog_number: Some(catalog_number),
                barcode: None,
                format: None,
                country: None,
            };
            info!("🎵 MusicBrainz catalog number search: {:?}", params);
            search_mb_and_rank(params, core_metadata).await
        }
        SearchSource::Discogs => {
            let client = get_discogs_client()?;
            let params = DiscogsSearchParams {
                artist: None,
                release_title: None,
                year: None,
                label: None,
                catno: Some(catalog_number),
                barcode: None,
                format: None,
                country: None,
            };
            info!("🔍 Discogs catalog number search: {:?}", params);
            search_discogs_and_rank(&client, params, core_metadata).await
        }
    }
}

/// Search by barcode only
pub async fn search_by_barcode(
    metadata: Option<DisplayFolderMetadata>,
    source: SearchSource,
    barcode: String,
) -> Result<Vec<DisplayMatchCandidate>, String> {
    let core_metadata = metadata.as_ref().map(from_display_metadata);
    match source {
        SearchSource::MusicBrainz => {
            let params = ReleaseSearchParams {
                artist: None,
                album: None,
                year: None,
                label: None,
                catalog_number: None,
                barcode: Some(barcode),
                format: None,
                country: None,
            };
            info!("🎵 MusicBrainz barcode search: {:?}", params);
            search_mb_and_rank(params, core_metadata).await
        }
        SearchSource::Discogs => {
            let client = get_discogs_client()?;
            let params = DiscogsSearchParams {
                artist: None,
                release_title: None,
                year: None,
                label: None,
                catno: None,
                barcode: Some(barcode),
                format: None,
                country: None,
            };
            info!("🔍 Discogs barcode search: {:?}", params);
            search_discogs_and_rank(&client, params, core_metadata).await
        }
    }
}

// ============================================================================
// Import helpers
// ============================================================================

/// Fetch full Discogs release details for import
async fn fetch_discogs_release(
    release_id: &str,
    master_id: &str,
) -> Result<DiscogsRelease, String> {
    let client = get_discogs_client()?;
    match client.get_release(release_id).await {
        Ok(release) => {
            let mut release = release;
            release.master_id = master_id.to_string();
            Ok(release)
        }
        Err(e) => Err(format!("Failed to fetch release details: {}", e)),
    }
}

/// Confirm a match candidate and start the import workflow.
pub async fn confirm_and_start_import(
    app: &AppService,
    candidate: DisplayMatchCandidate,
    import_source: ImportSource,
    navigator: Navigator,
) -> Result<(), String> {
    let mut import_store = app.state.import();

    // Get candidate key early for completion tracking
    let candidate_key = {
        let state = import_store.read();
        state
            .current_candidate_key
            .clone()
            .ok_or_else(|| "No candidate selected for import".to_string())?
    };

    // Signal that import is starting
    import_store.write().dispatch(CandidateEvent::StartImport);

    let import_id = uuid::Uuid::new_v4().to_string();

    // Check for duplicates based on source type
    match candidate.source_type {
        MatchSourceType::Discogs => {
            if let Ok(Some(duplicate)) = app
                .library_manager
                .get()
                .find_duplicate_by_discogs(
                    candidate.discogs_master_id.as_deref(),
                    candidate.discogs_release_id.as_deref(),
                )
                .await
            {
                import_store
                    .write()
                    .dispatch(CandidateEvent::ImportFailed(format!(
                        "This release already exists in your library: {}",
                        duplicate.title,
                    )));
                return Err("Duplicate album found".to_string());
            }
        }
        MatchSourceType::MusicBrainz => {
            if let Ok(Some(duplicate)) = app
                .library_manager
                .get()
                .find_duplicate_by_musicbrainz(
                    candidate.musicbrainz_release_id.as_deref(),
                    candidate.musicbrainz_release_group_id.as_deref(),
                )
                .await
            {
                import_store
                    .write()
                    .dispatch(CandidateEvent::ImportFailed(format!(
                        "This release already exists in your library: {}",
                        duplicate.title,
                    )));
                return Err("Duplicate album found".to_string());
            }
        }
    }

    // Get state from store
    let (storage_profile_id, metadata, selected_cover) = {
        let state = import_store.read();
        (
            state.get_storage_profile_id(),
            state.get_metadata(),
            state.get_selected_cover(),
        )
    };
    let master_year = metadata.as_ref().and_then(|m| m.year).unwrap_or(1970);

    let (cover_art_url, selected_cover_filename) = match selected_cover {
        Some(SelectedCover::Remote { url, source }) => {
            let filename = compute_expected_cover_filename(&url, &source);
            (Some(url), Some(filename))
        }
        Some(SelectedCover::Local { filename }) => (None, Some(filename)),
        None => (None, None),
    };

    let request = match import_source {
        ImportSource::Folder => match candidate.source_type {
            MatchSourceType::Discogs => {
                let release_id = candidate
                    .discogs_release_id
                    .as_ref()
                    .ok_or_else(|| "Missing Discogs release ID".to_string())?;
                let master_id = candidate
                    .discogs_master_id
                    .as_ref()
                    .ok_or_else(|| "Discogs result has no master_id".to_string())?;

                let discogs_release = fetch_discogs_release(release_id, master_id).await?;

                ImportRequest::Folder {
                    import_id: import_id.clone(),
                    discogs_release: Some(discogs_release),
                    mb_release: None,
                    folder: PathBuf::from(&candidate_key),
                    master_year,
                    cover_art_url: cover_art_url.clone(),
                    storage_profile_id: storage_profile_id.clone(),
                    selected_cover_filename: selected_cover_filename.clone(),
                }
            }
            MatchSourceType::MusicBrainz => {
                let release_id = candidate
                    .musicbrainz_release_id
                    .as_ref()
                    .ok_or_else(|| "Missing MusicBrainz release ID".to_string())?;

                info!(
                    "Starting import for MusicBrainz release: {}",
                    candidate.title
                );

                let (mb_release, _external_urls, _raw) = lookup_release_by_id(release_id)
                    .await
                    .map_err(|e| format!("Failed to fetch MusicBrainz release: {}", e))?;

                ImportRequest::Folder {
                    import_id: import_id.clone(),
                    discogs_release: None,
                    mb_release: Some(mb_release),
                    folder: PathBuf::from(&candidate_key),
                    master_year,
                    cover_art_url: cover_art_url.clone(),
                    storage_profile_id: storage_profile_id.clone(),
                    selected_cover_filename: selected_cover_filename.clone(),
                }
            }
        },
        _ => return Err("This import source is not yet supported".to_string()),
    };

    let import_handle = app.import_handle.clone();
    match import_handle.send_request(request).await {
        Ok((album_id, _release_id)) => {
            info!("Import started successfully: {}", album_id);
            import_store.write().dispatch(CandidateEvent::ImportStarted);

            // Spawn a task to listen for import completion
            let progress_handle = import_handle.progress_handle.clone();
            let mut import_store_clone = app.state.import();
            spawn(async move {
                let mut progress_rx = progress_handle.subscribe_import(import_id.clone());
                while let Some(event) = progress_rx.recv().await {
                    match event {
                        ImportProgress::Complete { .. } => {
                            info!("Import completed for candidate: {}", candidate_key);
                            import_store_clone.write().dispatch_to_candidate(
                                &candidate_key,
                                CandidateEvent::ImportComplete,
                            );
                            break;
                        }
                        ImportProgress::Failed { error, .. } => {
                            warn!("Import failed for candidate {}: {}", candidate_key, error);
                            import_store_clone.write().dispatch_to_candidate(
                                &candidate_key,
                                CandidateEvent::ImportFailed(error),
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            });

            // Check for more releases
            let has_more = import_store.read().has_more_releases();
            if has_more {
                info!("More releases to import, advancing to next release");
                import_store.write().advance_to_next_release();
                let (current_idx, selected_indices, detected_candidates) = {
                    let state = import_store.read();
                    (
                        state.current_release_index,
                        state.selected_release_indices.clone(),
                        state.detected_candidates.clone(),
                    )
                };
                if let Some(&release_idx) = selected_indices.get(current_idx) {
                    if let Err(e) =
                        load_selected_release(app, release_idx, &detected_candidates).await
                    {
                        error!("Failed to load next release: {}", e);
                        import_store
                            .write()
                            .dispatch(CandidateEvent::ImportFailed(e));
                    }
                }
                Ok(())
            } else {
                info!(
                    "No more releases to import, navigating to album: {}",
                    album_id
                );
                import_store.write().reset();
                navigator.push(Route::AlbumDetail {
                    album_id,
                    release_id: String::new(),
                });
                Ok(())
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to start import: {}", e);
            error!("{}", error_msg);
            import_store
                .write()
                .dispatch(CandidateEvent::ImportFailed(error_msg.clone()));
            Err(error_msg)
        }
    }
}

/// Load a selected release by index, performing DiscID lookup if needed
pub async fn load_selected_release(
    app: &AppService,
    release_index: usize,
    detected_candidates: &[bae_ui::display_types::DetectedCandidate],
) -> Result<(), String> {
    let mut import_store = app.state.import();

    let release = detected_candidates
        .get(release_index)
        .ok_or_else(|| format!("Invalid release index: {}", release_index))?;
    let release_path = release.path.clone();

    // Switch to this candidate
    {
        let mut state = import_store.write();
        state.switch_candidate(Some(release_path.clone()));
        state.current_release_index = release_index;
    }

    // Only perform DiscID lookup once per candidate
    let already_attempted = import_store
        .read()
        .discid_lookup_attempted
        .contains(&release_path);
    if already_attempted {
        return Ok(());
    }

    import_store
        .write()
        .discid_lookup_attempted
        .insert(release_path.clone());

    let mb_discid = import_store
        .read()
        .get_metadata()
        .and_then(|m| m.mb_discid.clone());

    if let Some(mb_discid) = mb_discid {
        import_store
            .write()
            .dispatch(CandidateEvent::StartDiscIdLookup(mb_discid.clone()));
        import_store.write().is_looking_up = true;

        let result = lookup_discid(&mb_discid).await;

        import_store.write().is_looking_up = false;

        match result {
            Ok(DiscIdLookupResult::NoMatches) => {
                import_store
                    .write()
                    .dispatch(CandidateEvent::DiscIdLookupComplete {
                        matches: vec![],
                        error: None,
                    });
            }
            Ok(DiscIdLookupResult::SingleMatch(candidate)) => {
                import_store
                    .write()
                    .dispatch(CandidateEvent::DiscIdLookupComplete {
                        matches: vec![*candidate],
                        error: None,
                    });
            }
            Ok(DiscIdLookupResult::MultipleMatches(candidates)) => {
                import_store
                    .write()
                    .dispatch(CandidateEvent::DiscIdLookupComplete {
                        matches: candidates,
                        error: None,
                    });
            }
            Err(error) => {
                import_store
                    .write()
                    .dispatch(CandidateEvent::DiscIdLookupComplete {
                        matches: vec![],
                        error: Some(error),
                    });
            }
        }
    } else {
        import_store
            .write()
            .dispatch(CandidateEvent::DiscIdLookupComplete {
                matches: vec![],
                error: None,
            });
    }

    Ok(())
}

// ============================================================================
// Scan event consumption
// ============================================================================

/// Consume folder scan events and update import state
pub async fn consume_scan_events(app: AppService, mut rx: broadcast::Receiver<ScanEvent>) {
    loop {
        let mut import_store = app.state.import();
        let existing_paths: HashSet<String> = {
            let state = import_store.read();
            state
                .detected_candidates
                .iter()
                .map(|c| c.path.clone())
                .collect()
        };

        let mut first_selected_index = None;

        loop {
            match rx.recv().await {
                Ok(ScanEvent::Candidate(candidate)) => {
                    let key = candidate.path.to_string_lossy().to_string();
                    if existing_paths.contains(&key) {
                        continue;
                    }

                    let (files, metadata) = match detect_candidate_locally(&candidate) {
                        Ok(result) => result,
                        Err(e) => {
                            warn!(
                                "Skipping candidate {} due to detection failure: {}",
                                candidate.name, e
                            );
                            continue;
                        }
                    };

                    // Convert to display type
                    let display_candidate = bae_ui::display_types::DetectedCandidate {
                        name: candidate.name.clone(),
                        path: key.clone(),
                        status: bae_ui::display_types::DetectedCandidateStatus::Pending,
                    };

                    {
                        let mut state = import_store.write();
                        state.init_state_machine(&key, files, metadata);
                        state.detected_candidates.push(display_candidate);

                        if state.current_candidate_key.is_none() {
                            let index = state.detected_candidates.len() - 1;
                            state.switch_candidate(Some(key));
                            state.current_release_index = index;
                            first_selected_index = Some(index);
                        }
                    }
                }
                Ok(ScanEvent::Error(error)) => {
                    warn!("Scan error: {}", error);
                    import_store.write().is_scanning_candidates = false;
                    break;
                }
                Ok(ScanEvent::Finished) => {
                    import_store.write().is_scanning_candidates = false;
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Scan event receiver lagged, missed {} events", n);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    import_store.write().is_scanning_candidates = false;
                    return;
                }
            }
        }

        // After scan completes, load the first selected release if any
        if let Some(index) = first_selected_index {
            let detected = import_store.read().detected_candidates.clone();
            if let Err(e) = load_selected_release(&app, index, &detected).await {
                warn!("Failed to load selected release: {}", e);
            }
        }
    }
}

// ============================================================================
// Navigation helpers
// ============================================================================

/// Check if there is unclean state for the current import source
pub fn has_unclean_state(app: &AppService) -> bool {
    let import_store = app.state.import();
    let state = import_store.read();
    match state.selected_import_source {
        ImportSource::Folder => !state.detected_candidates.is_empty(),
        ImportSource::Torrent => false, // TODO: implement torrent state check
        ImportSource::Cd => state.current_candidate_key.is_some(),
    }
}

// ============================================================================
// File categorization helper
// ============================================================================

/// Convert scanned file to display FileInfo
fn scanned_to_file_info(
    f: &bae_core::import::folder_scanner::ScannedFile,
) -> bae_ui::display_types::FileInfo {
    let ext_lower = f
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let format = ext_lower.to_uppercase();
    let name = f
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let path = f.path.to_string_lossy().to_string();
    let display_url = crate::ui::local_file_url::local_file_url(&f.path);

    bae_ui::display_types::FileInfo {
        name,
        path,
        size: f.size,
        format,
        display_url,
    }
}

/// Convert CategorizedFiles from core to display type
pub fn categorized_files_from_scanned(
    files: &bae_core::import::CategorizedFiles,
) -> CategorizedFileInfo {
    use bae_core::import::folder_scanner::AudioContent;
    use bae_ui::display_types::{CueFlacPairInfo, FileInfo};

    let audio = match &files.audio {
        AudioContent::CueFlacPairs(pairs) => {
            let display_pairs: Vec<CueFlacPairInfo> = pairs
                .iter()
                .map(|p| CueFlacPairInfo {
                    cue_name: p
                        .cue_file
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string(),
                    cue_path: p.cue_file.path.to_string_lossy().to_string(),
                    flac_name: p
                        .audio_file
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string(),
                    total_size: p.cue_file.size + p.audio_file.size,
                    track_count: p.track_count,
                })
                .collect();
            AudioContentInfo::CueFlacPairs(display_pairs)
        }
        AudioContent::TrackFiles(tracks) => {
            let mut display_tracks: Vec<FileInfo> =
                tracks.iter().map(scanned_to_file_info).collect();
            display_tracks.sort_by(|a, b| a.name.cmp(&b.name));
            AudioContentInfo::TrackFiles(display_tracks)
        }
    };

    let mut artwork: Vec<FileInfo> = files.artwork.iter().map(scanned_to_file_info).collect();
    artwork.sort_by(|a, b| a.name.cmp(&b.name));

    let mut documents: Vec<FileInfo> = files.documents.iter().map(scanned_to_file_info).collect();
    documents.sort_by(|a, b| a.name.cmp(&b.name));

    let mut managed_artwork: Vec<FileInfo> = files
        .managed_artwork
        .iter()
        .map(scanned_to_file_info)
        .collect();
    managed_artwork.sort_by(|a, b| a.name.cmp(&b.name));

    CategorizedFileInfo {
        audio,
        artwork,
        documents,
        managed_artwork,
        bad_audio_count: files.bad_audio_count,
        bad_image_count: files.bad_image_count,
    }
}
