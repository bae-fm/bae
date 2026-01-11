use super::state::{ImportContext, SelectedCover};
use crate::discogs::DiscogsRelease;
use crate::import::{ImportProgress, ImportRequest, MatchCandidate, MatchSource};
use crate::ui::components::import::ImportSource;
use crate::ui::Route;
use dioxus::prelude::*;
use dioxus::router::Navigator;
use std::path::PathBuf;
use tracing::{error, info};
pub async fn import_release(
    ctx: &ImportContext,
    release_id: String,
    master_id: String,
) -> Result<DiscogsRelease, String> {
    ctx.set_error_message(None);
    let client = ctx.get_discogs_client()?;
    match client.get_release(&release_id).await {
        Ok(release) => {
            let mut release = release;
            release.master_id = master_id;
            Ok(release)
        }
        Err(e) => {
            let error = format!("Failed to fetch release details: {}", e);
            ctx.set_error_message(Some(error.clone()));
            Err(error)
        }
    }
}
/// Confirm a match candidate and start the import workflow.
pub async fn confirm_and_start_import(
    ctx: &ImportContext,
    candidate: MatchCandidate,
    import_source: ImportSource,
    navigator: Navigator,
) -> Result<(), String> {
    ctx.set_is_importing(true);
    ctx.set_preparing_step(None);
    let import_id = uuid::Uuid::new_v4().to_string();
    let import_service = ctx.import_service.clone();
    let mut progress_rx = import_service.subscribe_import(import_id.clone());
    let mut preparing_step_signal = ctx.preparing_step();
    spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            if let ImportProgress::Preparing { step, .. } = progress {
                preparing_step_signal.set(Some(step));
            }
        }
    });
    match &candidate.source {
        MatchSource::Discogs(discogs_result) => {
            let master_id = discogs_result.master_id.map(|id| id.to_string());
            let release_id = Some(discogs_result.id.to_string());
            if let Ok(Some(duplicate)) = ctx
                .library_manager
                .get()
                .find_duplicate_by_discogs(master_id.as_deref(), release_id.as_deref())
                .await
            {
                ctx.set_is_importing(false);
                ctx.set_duplicate_album_id(Some(duplicate.id));
                ctx.set_import_error_message(Some(format!(
                    "This release already exists in your library: {}",
                    duplicate.title,
                )));
                return Err("Duplicate album found".to_string());
            }
        }
        MatchSource::MusicBrainz(mb_release) => {
            let release_id = Some(mb_release.release_id.clone());
            let release_group_id = Some(mb_release.release_group_id.clone());
            if let Ok(Some(duplicate)) = ctx
                .library_manager
                .get()
                .find_duplicate_by_musicbrainz(release_id.as_deref(), release_group_id.as_deref())
                .await
            {
                ctx.set_is_importing(false);
                ctx.set_duplicate_album_id(Some(duplicate.id));
                ctx.set_import_error_message(Some(format!(
                    "This release already exists in your library: {}",
                    duplicate.title,
                )));
                return Err("Duplicate album found".to_string());
            }
        }
    }
    let storage_profile_id = ctx.storage_profile_id().read().clone();
    let metadata = ctx.detected_metadata().read().clone();
    let master_year = metadata.as_ref().and_then(|m| m.year).unwrap_or(1970);
    let (cover_art_url, selected_cover_filename) = match ctx.selected_cover().read().clone() {
        Some(SelectedCover::Remote {
            url,
            expected_filename,
        }) => (Some(url), Some(expected_filename)),
        None => (None, None),
    };
    let request = match import_source {
        ImportSource::Folder => {
            let folder_path = ctx.folder_path().read().clone();
            match candidate.source.clone() {
                MatchSource::Discogs(discogs_result) => {
                    let master_id = match discogs_result.master_id {
                        Some(id) => id.to_string(),
                        None => {
                            return Err("Discogs result has no master_id".to_string());
                        }
                    };
                    let release_id = discogs_result.id.to_string();
                    let discogs_release = import_release(ctx, release_id, master_id).await?;
                    ImportRequest::Folder {
                        import_id: import_id.clone(),
                        discogs_release: Some(discogs_release),
                        mb_release: None,
                        folder: PathBuf::from(folder_path),
                        master_year,
                        cover_art_url: cover_art_url.clone(),
                        storage_profile_id: storage_profile_id.clone(),
                        selected_cover_filename: selected_cover_filename.clone(),
                    }
                }
                MatchSource::MusicBrainz(mb_release) => {
                    info!(
                        "Starting import for MusicBrainz release: {}",
                        mb_release.title
                    );
                    ImportRequest::Folder {
                        import_id: import_id.clone(),
                        discogs_release: None,
                        mb_release: Some(mb_release.clone()),
                        folder: PathBuf::from(folder_path),
                        master_year,
                        cover_art_url: cover_art_url.clone(),
                        storage_profile_id: storage_profile_id.clone(),
                        selected_cover_filename: selected_cover_filename.clone(),
                    }
                }
            }
        }
        ImportSource::Torrent => {
            let torrent_source = ctx
                .torrent_source()
                .read()
                .clone()
                .ok_or_else(|| "No torrent source available".to_string())?;
            let seed_after_download = *ctx.seed_after_download().read();
            let torrent_metadata = ctx
                .torrent_metadata()
                .read()
                .clone()
                .ok_or_else(|| "No torrent metadata available".to_string())?;
            match candidate.source.clone() {
                MatchSource::Discogs(discogs_result) => {
                    let master_id = match discogs_result.master_id {
                        Some(id) => id.to_string(),
                        None => {
                            return Err("Discogs result has no master_id".to_string());
                        }
                    };
                    let release_id = discogs_result.id.to_string();
                    let discogs_release = import_release(ctx, release_id, master_id).await?;
                    ImportRequest::Torrent {
                        torrent_source,
                        discogs_release: Some(discogs_release),
                        mb_release: None,
                        master_year,
                        seed_after_download,
                        torrent_metadata,
                        cover_art_url: cover_art_url.clone(),
                        storage_profile_id: storage_profile_id.clone(),
                        selected_cover_filename: selected_cover_filename.clone(),
                    }
                }
                MatchSource::MusicBrainz(mb_release) => {
                    info!(
                        "Starting torrent import for MusicBrainz release: {}",
                        mb_release.title
                    );
                    ImportRequest::Torrent {
                        torrent_source,
                        discogs_release: None,
                        mb_release: Some(mb_release.clone()),
                        master_year,
                        seed_after_download,
                        torrent_metadata,
                        cover_art_url: cover_art_url.clone(),
                        storage_profile_id: storage_profile_id.clone(),
                        selected_cover_filename: selected_cover_filename.clone(),
                    }
                }
            }
        }
        ImportSource::Cd => {
            let folder_path = ctx.folder_path().read().clone();
            match candidate.source.clone() {
                MatchSource::Discogs(_discogs_result) => {
                    return Err("CD imports require MusicBrainz metadata".to_string());
                }
                MatchSource::MusicBrainz(mb_release) => {
                    info!(
                        "Starting CD import for MusicBrainz release: {}",
                        mb_release.title
                    );
                    ImportRequest::CD {
                        discogs_release: None,
                        mb_release: Some(mb_release.clone()),
                        drive_path: PathBuf::from(folder_path),
                        master_year,
                        cover_art_url: cover_art_url.clone(),
                        storage_profile_id: storage_profile_id.clone(),
                        selected_cover_filename: selected_cover_filename.clone(),
                    }
                }
            }
        }
    };
    match ctx.import_service.send_request(request).await {
        Ok((album_id, _release_id)) => {
            info!("Import started successfully: {}", album_id);
            ctx.set_is_importing(false);
            ctx.set_preparing_step(None);
            if ctx.has_more_releases() {
                info!("More releases to import, advancing to next release");
                ctx.advance_to_next_release();
                let current_idx = *ctx.current_release_index().read();
                let selected_indices = ctx.selected_release_indices().read().clone();
                if let Some(&release_idx) = selected_indices.get(current_idx) {
                    match ctx.load_selected_release(release_idx).await {
                        Ok(()) => {
                            info!("Loaded next release for import");
                        }
                        Err(e) => {
                            error!("Failed to load next release: {}", e);
                            ctx.set_import_error_message(Some(e));
                        }
                    }
                }
                Ok(())
            } else {
                info!(
                    "No more releases to import, navigating to album: {}",
                    album_id
                );
                ctx.reset();
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
            ctx.set_is_importing(false);
            ctx.set_preparing_step(None);
            ctx.set_import_error_message(Some(error_msg.clone()));
            Err(error_msg)
        }
    }
}
