use super::back_button::BackButton;
use super::error::AlbumDetailError;
use super::loading::AlbumDetailLoading;
use super::utils::{get_selected_release_id_from_params, load_album_and_releases, maybe_not_empty};
use super::AlbumDetailView;
use crate::ui::components::{use_playback_service, use_playback_state};
use crate::ui::display_types::{
    album_from_db_ref, artist_from_db_ref, playback_display_from_state, release_from_db_ref,
    track_from_db_ref, Artist, Release, Track, TrackImportState,
};
use crate::ui::Route;
use crate::ui::{use_import_service, use_library_manager};
use crate::AppContext;
use bae_core::db::ImportStatus;
use bae_core::db::{DbAlbum, DbArtist, DbRelease, DbTrack};
use bae_core::import::ImportProgress;
use bae_core::library::LibraryError;
use dioxus::prelude::*;
use rfd::AsyncFileDialog;
use tracing::error;

/// Album detail page showing album info and tracklist
#[component]
pub fn AlbumDetail(album_id: ReadSignal<String>, release_id: ReadSignal<String>) -> Element {
    let maybe_release_id = use_memo(move || maybe_not_empty(release_id()));
    let data = use_album_detail_data(album_id, maybe_release_id);
    let import_state = use_release_import_state(
        data.album_resource,
        data.selected_release_id,
        data.tracks_resource,
    );

    // Playback state and service
    let playback_state = use_playback_state();
    let playback = use_playback_service();
    let library_manager = use_library_manager();
    let app_context = use_context::<AppContext>();

    // Convert playback state to display type
    let playback_display = use_memo(move || playback_display_from_state(&playback_state()));

    // Playback callbacks (as EventHandlers for props)
    let on_track_play = EventHandler::new({
        let playback = playback.clone();
        move |track_id: String| {
            playback.play(track_id);
        }
    });
    let on_track_pause = EventHandler::new({
        let playback = playback.clone();
        move |_: ()| {
            playback.pause();
        }
    });
    let on_track_resume = EventHandler::new({
        let playback = playback.clone();
        move |_: ()| {
            playback.resume();
        }
    });
    let on_track_add_next = EventHandler::new({
        let playback = playback.clone();
        move |track_id: String| {
            playback.add_next(vec![track_id]);
        }
    });
    let on_track_add_to_queue = EventHandler::new({
        let playback = playback.clone();
        move |track_id: String| {
            playback.add_to_queue(vec![track_id]);
        }
    });
    let on_track_export = EventHandler::new({
        let library_manager = library_manager.clone();
        let cache = app_context.cache.clone();
        move |track_id: String| {
            let library_manager = library_manager.clone();
            let cache = cache.clone();
            spawn(async move {
                if let Some(file_handle) = AsyncFileDialog::new()
                    .set_title("Export Track")
                    .set_file_name(format!("{}.flac", track_id))
                    .add_filter("FLAC", &["flac"])
                    .save_file()
                    .await
                {
                    let output_path = file_handle.path().to_path_buf();
                    if let Err(e) = library_manager
                        .get()
                        .export_track(&track_id, &output_path, &cache)
                        .await
                    {
                        error!("Failed to export track: {}", e);
                    }
                }
            });
        }
    });

    // Album playback callbacks
    let on_play_album = EventHandler::new({
        let playback = playback.clone();
        move |track_ids: Vec<String>| {
            playback.play_album(track_ids);
        }
    });
    let on_add_album_to_queue = EventHandler::new({
        let playback = playback.clone();
        move |track_ids: Vec<String>| {
            playback.add_to_queue(track_ids);
        }
    });

    // Export release callback
    let on_export_release = EventHandler::new({
        let library_manager = library_manager.clone();
        let cache = app_context.cache.clone();
        move |release_id: String| {
            let library_manager = library_manager.clone();
            let cache = cache.clone();
            spawn(async move {
                if let Some(folder_handle) = AsyncFileDialog::new()
                    .set_title("Select Export Directory")
                    .pick_folder()
                    .await
                {
                    let target_dir = folder_handle.path().to_path_buf();
                    if let Err(e) = library_manager
                        .get()
                        .export_release(&release_id, &target_dir, &cache)
                        .await
                    {
                        error!("Failed to export release: {}", e);
                    }
                }
            });
        }
    });

    // Delete release callback
    let on_delete_release = EventHandler::new({
        let library_manager = library_manager.clone();
        let playback = playback.clone();
        move |release_id: String| {
            // Stop playback if current track belongs to the release being deleted
            if let bae_core::playback::PlaybackState::Playing { ref track, .. }
            | bae_core::playback::PlaybackState::Paused { ref track, .. } = playback_state()
            {
                if track.release_id == release_id {
                    playback.stop();
                }
            }

            let library_manager = library_manager.clone();
            spawn(async move {
                if let Err(e) = library_manager.get().delete_release(&release_id).await {
                    error!("Failed to delete release: {}", e);
                }
            });
        }
    });

    let on_album_deleted = EventHandler::new(move |_| {
        navigator().push(Route::Library {});
    });

    // Delete album callback
    let on_delete_album = EventHandler::new({
        let library_manager = library_manager.clone();
        let playback = playback.clone();
        move |album_id: String| {
            // Stop playback if current track belongs to the album being deleted
            if let bae_core::playback::PlaybackState::Playing { ref track, .. }
            | bae_core::playback::PlaybackState::Paused { ref track, .. } = playback_state()
            {
                if let Some(Ok((_, releases))) = data.album_resource.value().read().as_ref() {
                    if releases.iter().any(|r| r.id == track.release_id) {
                        playback.stop();
                    }
                }
            }

            let library_manager = library_manager.clone();
            spawn(async move {
                if let Err(e) = library_manager.get().delete_album(&album_id).await {
                    error!("Failed to delete album: {}", e);
                }
            });
        }
    });

    rsx! {
        PageContainer {
            BackButton {}
            match data.album_resource.value().read().as_ref() {
                None => rsx! {
                    AlbumDetailLoading {}
                },
                Some(Err(e)) => rsx! {
                    AlbumDetailError { message: format!("Failed to load album: {e}") }
                },
                Some(Ok((album, releases))) => {
                    let selected_release_result = get_selected_release_id_from_params(
                            &data.album_resource,
                            maybe_release_id(),
                        )
                        .expect("Resource value should be present");
                    if let Err(e) = selected_release_result {
                        return rsx! {
                            AlbumDetailError { message: format!("Failed to load release: {e}") }
                        };
                    }
                    let selected_release_id = selected_release_result.ok().unwrap();
                    let db_artists = data
                        .artists_resource
                        .value()
                        .read()
                        .as_ref()
                        .and_then(|r| r.as_ref().ok())
                        .cloned()
                        .unwrap_or_default();
                    let on_release_select = move |new_release_id: String| {
                        navigator()
                            .push(Route::AlbumDetail {
                                album_id: album_id().clone(),
                                release_id: new_release_id,
                            });
                    };
                    let mut album_with_cover = album.clone();
                    if let Some(cover_id) = import_state.cover_image_id.read().as_ref() {
                        album_with_cover.cover_image_id = Some(cover_id.clone());
                    }

                    // Convert to display types
                    let display_album = album_from_db_ref(&album_with_cover);
                    let display_artists: Vec<Artist> = db_artists
                        .iter()
                        .map(artist_from_db_ref)
                        .collect();
                    let mb_release_id = album_with_cover
                        .musicbrainz_release
                        .as_ref()
                        .map(|mb| mb.release_id.clone());
                    let display_releases: Vec<Release> = releases
                        .iter()
                        .map(|r| {
                            let mut release = release_from_db_ref(r);
                            release.musicbrainz_release_id = mb_release_id.clone();
                            release
                        })
                        .collect();
                    // Get track signals - these are reactive per-track
                    let mut track_signals: Vec<Signal<Track>> = import_state
                        .track_signals
                        .read()
                        .values()
                        .copied()
                        .collect();
                    track_signals
                        .sort_by(|a, b| {
                            let a = a.read();
                            let b = b.read();
                            (a.disc_number, a.track_number).cmp(&(b.disc_number, b.track_number))
                        });
                    rsx! {
                        AlbumDetailView {
                            album: display_album,
                            releases: display_releases,
                            artists: display_artists,
                            tracks: track_signals,
                            selected_release_id,
                            import_progress: import_state.progress,
                            import_error: import_state.import_error,
                            playback: playback_display(),
                            on_release_select,
                            on_album_deleted,
                            on_export_release,
                            on_delete_album,
                            on_delete_release,
                            on_track_play,
                            on_track_pause,
                            on_track_resume,
                            on_track_add_next,
                            on_track_add_to_queue,
                            on_track_export,
                            on_play_album,
                            on_add_album_to_queue,
                        }
                    }
                }
            }
        }
    }
}

pub use bae_ui::PageContainer;

struct AlbumDetailData {
    album_resource: Resource<Result<(DbAlbum, Vec<DbRelease>), LibraryError>>,
    tracks_resource: Resource<Result<Vec<DbTrack>, LibraryError>>,
    artists_resource: Resource<Result<Vec<DbArtist>, LibraryError>>,
    selected_release_id: Memo<Option<String>>,
}

fn use_album_detail_data(
    album_id: ReadSignal<String>,
    maybe_release_id_param: Memo<Option<String>>,
) -> AlbumDetailData {
    let library_manager = use_library_manager();
    let album_resource = {
        let library_manager = library_manager.clone();
        use_resource(move || {
            let album_id = album_id();
            let library_manager = library_manager.clone();
            async move { load_album_and_releases(&library_manager, &album_id).await }
        })
    };
    let selected_release_id = use_memo(move || {
        get_selected_release_id_from_params(&album_resource, maybe_release_id_param())
            .and_then(|r| r.ok())
    });
    let tracks_resource = {
        let library_manager = library_manager.clone();
        use_resource(move || {
            let release_id = selected_release_id();
            let library_manager = library_manager.clone();
            async move {
                match release_id {
                    Some(id) => library_manager.get().get_tracks(&id).await,
                    None => Ok(Vec::new()),
                }
            }
        })
    };
    let current_album_id = use_memo(move || {
        album_resource
            .value()
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .map(|(album, _)| album.id.clone())
    });
    let artists_resource = {
        let library_manager = library_manager.clone();
        use_resource(move || {
            let album_id = current_album_id();
            let library_manager = library_manager.clone();
            async move {
                match album_id {
                    Some(id) => library_manager.get().get_artists_for_album(&id).await,
                    None => Ok(Vec::new()),
                }
            }
        })
    };
    AlbumDetailData {
        album_resource,
        tracks_resource,
        artists_resource,
        selected_release_id,
    }
}

/// State returned by the release import hook
struct ReleaseImportState {
    /// Current import progress percentage (None if not importing)
    progress: Signal<Option<u8>>,
    /// Cover image ID received from import completion (for reactive UI update)
    cover_image_id: Signal<Option<String>>,
    /// Error message if import failed
    import_error: Signal<Option<String>>,
    /// Per-track signals for granular reactivity - indexed by track ID
    /// This is a Signal so we can update the map when tracks load
    track_signals: Signal<std::collections::HashMap<String, Signal<Track>>>,
}

fn use_release_import_state(
    album_resource: Resource<Result<(DbAlbum, Vec<DbRelease>), LibraryError>>,
    selected_release_id: Memo<Option<String>>,
    tracks_resource: Resource<Result<Vec<DbTrack>, LibraryError>>,
) -> ReleaseImportState {
    let mut progress = use_signal(|| None::<u8>);
    let mut cover_image_id = use_signal(|| None::<String>);
    let mut import_error = use_signal(|| None::<String>);
    let mut track_signals = use_signal(std::collections::HashMap::<String, Signal<Track>>::new);
    let import_service = use_import_service();

    // Rebuild per-track signals when tracks load
    use_effect(move || {
        if let Some(Ok(db_tracks)) = tracks_resource.value().read().as_ref() {
            let mut new_map = std::collections::HashMap::new();
            for db_track in db_tracks {
                let track = track_from_db_ref(db_track);
                new_map.insert(track.id.clone(), Signal::new(track));
            }
            track_signals.set(new_map);
        }
    });

    // Subscribe to import progress events
    use_effect(move || {
        let releases_data = album_resource
            .value()
            .read()
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(|(_, releases)| releases.clone());
        let Some(releases) = releases_data else {
            return;
        };
        let Some(ref id) = selected_release_id() else {
            return;
        };
        let Some(release) = releases.iter().find(|r| &r.id == id) else {
            return;
        };
        let is_importing = release.import_status == ImportStatus::Importing
            || release.import_status == ImportStatus::Queued;
        if is_importing {
            let release_id = release.id.clone();
            let import_service = import_service.clone();
            let track_signals = track_signals;
            spawn(async move {
                let mut progress_rx = import_service.subscribe_release(release_id);
                while let Some(progress_event) = progress_rx.recv().await {
                    match progress_event {
                        ImportProgress::Progress {
                            id: track_id,
                            percent,
                            phase: _phase,
                            ..
                        } => {
                            // Update overall progress
                            progress.set(Some(percent));

                            // Update per-track import state
                            if let Some(mut track_signal) =
                                track_signals.read().get(&track_id).copied()
                            {
                                let mut track = track_signal();
                                track.import_state = TrackImportState::Importing(percent);
                                track_signal.set(track);
                            }
                        }
                        ImportProgress::Complete {
                            id,
                            cover_image_id: cid,
                            release_id: rid,
                            ..
                        } => {
                            if rid.is_some() {
                                // Track completion - mark as available
                                if let Some(mut track_signal) =
                                    track_signals.read().get(&id).copied()
                                {
                                    let mut track = track_signal();
                                    track.import_state = TrackImportState::Complete;
                                    track.is_available = true;
                                    track_signal.set(track);
                                }
                            } else {
                                // Release completion
                                if let Some(cover_id) = cid {
                                    cover_image_id.set(Some(cover_id));
                                }
                                progress.set(None);
                                break;
                            }
                        }
                        ImportProgress::Failed { error, .. } => {
                            progress.set(None);
                            import_error.set(Some(error));
                            break;
                        }
                        ImportProgress::Started { .. } | ImportProgress::Preparing { .. } => {}
                    }
                }
            });
        } else {
            progress.set(None);
        }
    });

    ReleaseImportState {
        progress,
        cover_image_id,
        import_error,
        track_signals,
    }
}
