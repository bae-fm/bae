//! Title bar wrapper for desktop app
//!
//! Wraps the shared TitleBarView with desktop-specific behavior:
//! window dragging (macOS), zoom (macOS), and database-backed search.

use crate::ui::app_service::use_app;
use crate::ui::components::imports_button::ImportsButton;
use crate::ui::components::imports_dropdown::ImportsDropdown;
use crate::ui::Route;
use bae_ui::display_types::Album;
use bae_ui::stores::{
    AppStateStoreExt, LibraryStateStoreExt, SearchStateStoreExt, UiStateStoreExt,
};
use bae_ui::{NavItem, SearchResult, TitleBarView};
#[cfg(target_os = "macos")]
use cocoa::appkit::NSApplication;
#[cfg(target_os = "macos")]
use cocoa::base::{id, nil};
#[cfg(target_os = "macos")]
use dioxus::desktop::use_window;
use dioxus::prelude::*;
#[cfg(target_os = "macos")]
use dispatch::Queue;
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

/// Custom title bar component with navigation and search
/// On macOS: includes window dragging, zoom, and space for traffic lights
/// On Linux/Windows: simpler version without native window controls
#[component]
pub fn TitleBar() -> Element {
    #[cfg(target_os = "macos")]
    let window = use_window();

    let app = use_app();
    let current_route = use_route::<Route>();
    let search_store = app.state.ui().search();
    let mut search_query_store = search_store.query();
    let mut show_results = use_signal(|| false);
    let show_results_read: ReadSignal<bool> = show_results.into();
    let mut filtered_albums = use_signal(Vec::<Album>::new);
    let imports_dropdown_open = use_signal(|| false);

    // Read albums from global store (populated by App component)
    let albums_store = app.state.library().albums();
    let artists_store = app.state.library().artists_by_album();

    // Filter albums based on search query
    use_effect({
        move || {
            let query = search_query_store.read().to_lowercase();
            if query.is_empty() {
                filtered_albums.set(Vec::new());
                show_results.set(false);
            } else {
                let albums = albums_store.read();
                let artists_map = artists_store.read();
                let filtered = albums
                    .iter()
                    .filter(|album| {
                        if album.title.to_lowercase().contains(&query) {
                            return true;
                        }
                        if let Some(artists) = artists_map.get(&album.id) {
                            return artists
                                .iter()
                                .any(|artist| artist.name.to_lowercase().contains(&query));
                        }
                        false
                    })
                    .take(10)
                    .cloned()
                    .collect();
                filtered_albums.set(filtered);
                show_results.set(true);
            }
        }
    });

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
            is_active: matches!(current_route, Route::ImportWorkflowManager {}),
        },
    ];

    // Convert filtered albums to search results
    let search_results: Vec<SearchResult> = {
        let artists_map = artists_store.read();
        filtered_albums()
            .iter()
            .map(|album| {
                let artists = artists_map.get(&album.id).cloned().unwrap_or_default();
                let artist_name = if artists.is_empty() {
                    "Unknown Artist".to_string()
                } else {
                    artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let subtitle = if let Some(year) = album.year {
                    format!("{} • {}", artist_name, year)
                } else {
                    artist_name
                };
                SearchResult {
                    id: album.id.clone(),
                    title: album.title.clone(),
                    subtitle,
                    cover_url: album.cover_url.clone(),
                }
            })
            .collect()
    };

    // Platform-specific: left padding for traffic lights on macOS
    #[cfg(target_os = "macos")]
    let left_padding = 80;
    #[cfg(not(target_os = "macos"))]
    let left_padding = 16;

    // Platform-specific: window drag handler (macOS only)
    #[cfg(target_os = "macos")]
    let on_bar_mousedown = Some(EventHandler::new(move |_| {
        let _ = window.drag_window();
    }));
    #[cfg(not(target_os = "macos"))]
    let on_bar_mousedown: Option<EventHandler<()>> = None;

    // Platform-specific: window zoom handler (macOS only)
    #[cfg(target_os = "macos")]
    let on_bar_double_click = Some(EventHandler::new(move |_| perform_zoom()));
    #[cfg(not(target_os = "macos"))]
    let on_bar_double_click: Option<EventHandler<()>> = None;

    rsx! {
        TitleBarView {
            nav_items,
            on_nav_click: move |id: String| {
                let route = match id.as_str() {
                    "library" => Route::Library {},
                    "import" => Route::ImportWorkflowManager {},
                    _ => return,
                };
                navigator().push(route);
            },
            search_value: search_query_store.read().clone(),
            on_search_change: move |value| search_query_store.set(value),
            search_results,
            on_search_result_click: move |album_id: String| {
                show_results.set(false);
                search_query_store.set(String::new());
                navigator()
                    .push(Route::AlbumDetail {
                        album_id,
                        release_id: String::new(),
                    });
            },
            show_search_results: show_results_read,
            on_search_dismiss: move |_| show_results.set(false),
            on_search_focus: move |_| {
                if !search_query_store.read().is_empty() {
                    show_results.set(true);
                }
            },
            on_settings_click: move |_| {
                navigator().push(Route::Settings {});
            },
            settings_active: matches!(current_route, Route::Settings {}),
            on_bar_mousedown,
            on_bar_double_click,
            imports_indicator: rsx! {
                ImportsButton { is_open: imports_dropdown_open }
                ImportsDropdown { is_open: imports_dropdown_open }
            },
            left_padding,
        }
    }
}

/// Perform window zoom (maximize/restore) using native macOS API
#[cfg(target_os = "macos")]
fn perform_zoom() {
    Queue::main().exec_async(|| unsafe {
        let app = NSApplication::sharedApplication(nil);
        let window: id = msg_send![app, keyWindow];
        if window != nil {
            let _: () = msg_send![window, performZoom: nil];
        }
    });
}
