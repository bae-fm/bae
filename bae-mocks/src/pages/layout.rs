//! Demo app layout with navigation and playback bar

use crate::demo_data;
use crate::Route;
use bae_ui::stores::{PlaybackStatus, PlaybackUiState, SidebarState, SidebarStateStoreExt};
use bae_ui::{
    ActiveImport, AppLayoutView, ImportStatus, ImportsButtonView, ImportsDropdownView, NavItem,
    NowPlayingBarView, QueueItem, QueueSidebarView, SearchResult, TitleBarView, Track,
    TrackImportState,
};
use dioxus::prelude::*;

fn mock_playing_track() -> Track {
    Track {
        id: "mock-track-1".to_string(),
        title: "Neon Frequencies".to_string(),
        track_number: Some(1),
        disc_number: Some(1),
        duration_ms: Some(245_000),
        is_available: true,
        import_state: TrackImportState::Complete,
    }
}

fn mock_active_imports() -> Vec<ActiveImport> {
    vec![
        ActiveImport {
            import_id: "import-1".to_string(),
            album_title: "Midnight Dreams".to_string(),
            artist_name: "Synthwave Collective".to_string(),
            status: ImportStatus::Importing,
            current_step_text: None,
            progress_percent: Some(67),
            release_id: Some("release-1".to_string()),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
        },
        ActiveImport {
            import_id: "import-2".to_string(),
            album_title: "Electric Horizons".to_string(),
            artist_name: "Neon Pulse".to_string(),
            status: ImportStatus::Preparing,
            current_step_text: Some("Downloading cover art...".to_string()),
            progress_percent: None,
            release_id: None,
            cover_url: None,
        },
        ActiveImport {
            import_id: "import-3".to_string(),
            album_title: "Retro Future".to_string(),
            artist_name: "Chrome Waves".to_string(),
            status: ImportStatus::Complete,
            current_step_text: None,
            progress_percent: Some(100),
            release_id: Some("release-3".to_string()),
            cover_url: Some("/covers/velvet-mathematics_proof-by-induction.png".to_string()),
        },
    ]
}

fn mock_queue() -> Vec<QueueItem> {
    vec![
        QueueItem {
            track: Track {
                id: "queue-track-1".to_string(),
                title: "Signal Lost".to_string(),
                track_number: Some(2),
                disc_number: Some(1),
                duration_ms: Some(198_000),
                is_available: true,
                import_state: TrackImportState::Complete,
            },
            album_title: "Neon Frequencies".to_string(),
            cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
        },
        QueueItem {
            track: Track {
                id: "queue-track-2".to_string(),
                title: "Proof by Induction".to_string(),
                track_number: Some(1),
                disc_number: Some(1),
                duration_ms: Some(312_000),
                is_available: true,
                import_state: TrackImportState::Complete,
            },
            album_title: "Set Theory".to_string(),
            cover_url: Some("/covers/velvet-mathematics_set-theory.png".to_string()),
        },
    ]
}

/// Layout component wrapping shared AppLayoutView
#[component]
pub fn DemoLayout() -> Element {
    let current_route = use_route::<Route>();
    let mut search_query = use_signal(String::new);
    let mut show_search_results = use_signal(|| false);
    let show_search_results_read: ReadSignal<bool> = show_search_results.into();
    let mut imports_open = use_signal(|| false);
    let mock_imports = mock_active_imports();

    // Create mock track and queue data
    let mock_track = mock_playing_track();
    let current_queue_item = QueueItem {
        track: mock_track.clone(),
        album_title: "Neon Frequencies".to_string(),
        cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
    };

    // Create playback store with mock data
    let playback_store = use_store(move || PlaybackUiState {
        status: PlaybackStatus::Playing,
        queue: vec!["queue-track-1".to_string(), "queue-track-2".to_string()],
        current_track_id: Some("mock-track-1".to_string()),
        current_release_id: Some("release-1".to_string()),
        current_track: Some(current_queue_item),
        queue_items: mock_queue(),
        position_ms: 45_000,
        duration_ms: 245_000,
        pregap_ms: None,
        artist_name: "The Midnight Signal".to_string(),
        cover_url: Some("/covers/the-midnight-signal_neon-frequencies.png".to_string()),
        playback_error: None,
        repeat_mode: Default::default(),
    });

    // Create sidebar store
    let sidebar_store = use_store(|| SidebarState { is_open: false });

    // Mock search - filter albums by query
    let search_results: Vec<SearchResult> = {
        let query = search_query().to_lowercase();
        if query.is_empty() {
            vec![]
        } else {
            let albums = demo_data::get_albums();
            let artists_by_album = demo_data::get_artists_by_album();
            albums
                .into_iter()
                .filter(|album| {
                    album.title.to_lowercase().contains(&query)
                        || artists_by_album
                            .get(&album.id)
                            .map(|artists| {
                                artists
                                    .iter()
                                    .any(|a| a.name.to_lowercase().contains(&query))
                            })
                            .unwrap_or(false)
                })
                .take(5)
                .map(|album| {
                    let artists = artists_by_album.get(&album.id).cloned().unwrap_or_default();
                    let artist_name = artists
                        .first()
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "Unknown Artist".to_string());
                    SearchResult {
                        id: album.id,
                        title: album.title,
                        subtitle: artist_name,
                        cover_url: album.cover_url,
                    }
                })
                .collect()
        }
    };

    // Build nav items (Settings is now a button on the right)
    let nav_items = vec![
        NavItem {
            id: "library".to_string(),
            label: "Library".to_string(),
            is_active: matches!(current_route, Route::Library {} | Route::AlbumDetail { .. }),
        },
        NavItem {
            id: "import".to_string(),
            label: "Import".to_string(),
            is_active: matches!(current_route, Route::Import {}),
        },
    ];

    // Get mutable stores for callbacks
    let mut sidebar_is_open = sidebar_store.is_open();

    rsx! {
        AppLayoutView {
            title_bar: rsx! {
                TitleBarView {
                    nav_items,
                    on_nav_click: move |id: String| {
                        let _ = match id.as_str() {
                            "library" => navigator().push(Route::Library {}),
                            "import" => navigator().push(Route::Import {}),
                            _ => None,
                        };
                    },
                    search_value: search_query(),
                    on_search_change: move |value: String| {
                        search_query.set(value.clone());
                        show_search_results.set(!value.is_empty());
                    },
                    search_results,
                    on_search_result_click: move |album_id: String| {
                        show_search_results.set(false);
                        search_query.set(String::new());
                        navigator().push(Route::AlbumDetail { album_id });
                    },
                    show_search_results: show_search_results_read,
                    on_search_dismiss: move |_| show_search_results.set(false),
                    on_search_focus: move |_| {
                        if !search_query().is_empty() {
                            show_search_results.set(true);
                        }
                    },
                    on_settings_click: move |_| {
                        navigator().push(Route::Settings {});
                    },
                    settings_active: matches!(current_route, Route::Settings {}),
                    imports_indicator: rsx! {
                        ImportsButtonView {
                            imports: mock_imports.clone(),
                            is_open: imports_open(),
                            on_toggle: move |_| imports_open.toggle(),
                        }
                        ImportsDropdownView {
                            imports: mock_imports.clone(),
                            is_open: imports_open(),
                            on_close: move |_| imports_open.set(false),
                            on_import_click: move |_id: String| imports_open.set(false),
                            on_import_dismiss: move |_id: String| {},
                            on_clear_all: move |_| {},
                        }
                    },
                    // No window drag/zoom on web
                    left_padding: 16,
                }
            },
            playback_bar: rsx! {
                NowPlayingBarView {
                    state: playback_store,
                    on_previous: move |_| {},
                    on_pause: move |_| {},
                    on_resume: move |_| {},
                    on_next: move |_| {},
                    on_seek: move |_pos| {},
                    on_toggle_queue: move |_| {
                        let current = *sidebar_is_open.read();
                        sidebar_is_open.set(!current);
                    },
                    on_track_click: move |_track_id: String| {},
                }
            },
            queue_sidebar: rsx! {
                QueueSidebarView {
                    sidebar: sidebar_store,
                    playback: playback_store,
                    on_close: move |_| sidebar_is_open.set(false),
                    on_clear: move |_| {},
                    on_remove: move |_idx| {},
                    on_track_click: move |_track_id: String| {},
                }
            },
            Outlet::<Route> {}
        }
    }
}
