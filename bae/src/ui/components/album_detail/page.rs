use super::back_button::BackButton;
use super::error::AlbumDetailError;
use super::view::AlbumDetailView;
use crate::ui::Route;
use dioxus::prelude::*;

#[cfg(not(feature = "demo"))]
use super::loading::AlbumDetailLoading;
#[cfg(not(feature = "demo"))]
use super::utils::{get_selected_release_id_from_params, load_album_and_releases, maybe_not_empty};
#[cfg(not(feature = "demo"))]
use crate::db::ImportStatus;
#[cfg(not(feature = "demo"))]
use crate::db::{DbAlbum, DbArtist, DbRelease, DbTrack};
#[cfg(not(feature = "demo"))]
use crate::import::ImportProgress;
#[cfg(not(feature = "demo"))]
use crate::library::LibraryError;
#[cfg(not(feature = "demo"))]
use crate::library::{use_import_service, use_library_manager};
#[cfg(not(feature = "demo"))]
use crate::ui::components::{use_playback_service, use_playback_state};
#[cfg(not(feature = "demo"))]
use crate::ui::display_types::{Album, Artist, PlaybackDisplay, Release, Track};
#[cfg(not(feature = "demo"))]
use crate::AppContext;
#[cfg(not(feature = "demo"))]
use rfd::AsyncFileDialog;
#[cfg(not(feature = "demo"))]
use tracing::error;

/// Album detail page showing album info and tracklist (real mode)
#[cfg(not(feature = "demo"))]
#[component]
pub fn AlbumDetail(album_id: ReadSignal<String>, release_id: ReadSignal<String>) -> Element {
    let maybe_release_id = use_memo(move || maybe_not_empty(release_id()));
    let data = use_album_detail_data(album_id, maybe_release_id);
    let import_state = use_release_import_state(data.album_resource, data.selected_release_id);

    // Playback state and service
    let playback_state = use_playback_state();
    let playback = use_playback_service();
    let library_manager = use_library_manager();
    let app_context = use_context::<AppContext>();

    // Convert playback state to display type
    let playback_display = use_memo(move || PlaybackDisplay::from(&playback_state()));

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
        move |release_id: String| {
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

    // Delete album callback (triggers confirmation dialog in view)
    let on_delete_album = EventHandler::new(move |_album_id: String| {
        // The view handles showing the confirmation dialog
        // This callback is just a pass-through since the view manages the dialog state
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
                    let db_tracks = data
                        .tracks_resource
                        .value()
                        .read()
                        .as_ref()
                        .and_then(|r| r.as_ref().ok())
                        .cloned()
                        .unwrap_or_default();
                    let mut album_with_cover = album.clone();
                    if let Some(cover_id) = import_state.cover_image_id.read().as_ref() {
                        album_with_cover.cover_image_id = Some(cover_id.clone());
                    }

                    // Convert to display types
                    let display_album = Album::from(&album_with_cover);
                    let display_artists: Vec<Artist> = db_artists
                        // Enrich releases with MusicBrainz ID from album level
                        .iter()

                        .map(Artist::from)
                        .collect();
                    let mb_release_id = album_with_cover
                        .musicbrainz_release
                        .as_ref()
                        .map(|mb| mb.release_id.clone());
                    let display_releases: Vec<Release> = releases
                        .iter()
                        .map(|r| {
                            let mut release = Release::from(r);
                            release.musicbrainz_release_id = mb_release_id.clone();
                            release
                        })
                        .collect();
                    let display_tracks: Vec<Track> = db_tracks.iter().map(Track::from).collect();
                    rsx! {
                        AlbumDetailView {
                            album: display_album,
                            releases: display_releases,
                            artists: display_artists,
                            tracks: display_tracks,
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

#[component]
fn PageContainer(children: Element) -> Element {
    rsx! {
        div { class: "container mx-auto p-6", {children} }
    }
}

#[cfg(not(feature = "demo"))]
struct AlbumDetailData {
    album_resource: Resource<Result<(DbAlbum, Vec<DbRelease>), LibraryError>>,
    tracks_resource: Resource<Result<Vec<DbTrack>, LibraryError>>,
    artists_resource: Resource<Result<Vec<DbArtist>, LibraryError>>,
    selected_release_id: Memo<Option<String>>,
}

#[cfg(not(feature = "demo"))]
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
#[cfg(not(feature = "demo"))]
struct ReleaseImportState {
    /// Current import progress percentage (None if not importing)
    progress: Signal<Option<u8>>,
    /// Cover image ID received from import completion (for reactive UI update)
    cover_image_id: Signal<Option<String>>,
    /// Error message if import failed
    import_error: Signal<Option<String>>,
}

#[cfg(not(feature = "demo"))]
fn use_release_import_state(
    album_resource: Resource<Result<(DbAlbum, Vec<DbRelease>), LibraryError>>,
    selected_release_id: Memo<Option<String>>,
) -> ReleaseImportState {
    let mut progress = use_signal(|| None::<u8>);
    let mut cover_image_id = use_signal(|| None::<String>);
    let mut import_error = use_signal(|| None::<String>);
    let import_service = use_import_service();
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
            spawn(async move {
                let mut progress_rx = import_service.subscribe_release(release_id);
                while let Some(progress_event) = progress_rx.recv().await {
                    match progress_event {
                        ImportProgress::Progress {
                            percent,
                            phase: _phase,
                            ..
                        } => {
                            progress.set(Some(percent));
                        }
                        ImportProgress::Complete {
                            cover_image_id: cid,
                            release_id: rid,
                            ..
                        } => {
                            if rid.is_none() {
                                if let Some(id) = cid {
                                    cover_image_id.set(Some(id));
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
    }
}

/// Album detail page (demo mode) - uses static fixture data
#[cfg(feature = "demo")]
#[component]
pub fn AlbumDetail(album_id: ReadSignal<String>, release_id: ReadSignal<String>) -> Element {
    use crate::ui::demo_data;
    use crate::ui::display_types::PlaybackDisplay;

    let album_id_val = album_id();

    // Get demo data
    let album = demo_data::get_album(&album_id_val);
    let artists = demo_data::get_artists_for_album(&album_id_val);
    let releases = demo_data::get_releases_for_album(&album_id_val);
    let tracks = demo_data::get_tracks_for_album(&album_id_val);

    let selected_release_id = releases.first().map(|r| r.id.clone());
    let import_progress = use_signal(|| None::<u8>);
    let import_error = use_signal(|| None::<String>);

    // Navigation (works in demo mode too)
    let on_release_select = move |new_release_id: String| {
        navigator().push(Route::AlbumDetail {
            album_id: album_id(),
            release_id: new_release_id,
        });
    };

    // Noop callbacks for demo mode
    let noop = |_: ()| {};
    let noop_string = |_: String| {};
    let noop_vec = |_: Vec<String>| {};

    rsx! {
        PageContainer {
            BackButton {}
            if let Some(album) = album {
                AlbumDetailView {
                    album,
                    releases,
                    artists,
                    tracks,
                    selected_release_id,
                    import_progress,
                    import_error,
                    playback: PlaybackDisplay::Stopped,
                    on_release_select,
                    on_album_deleted: noop,
                    on_export_release: noop_string,
                    on_delete_album: noop_string,
                    on_delete_release: noop_string,
                    on_track_play: noop_string,
                    on_track_pause: noop,
                    on_track_resume: noop,
                    on_track_add_next: noop_string,
                    on_track_add_to_queue: noop_string,
                    on_track_export: noop_string,
                    on_play_album: noop_vec,
                    on_add_album_to_queue: noop_vec,
                }
            } else {
                AlbumDetailError { message: "Album not found in demo data".to_string() }
            }
        }
    }
}
