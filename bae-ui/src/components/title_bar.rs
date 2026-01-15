//! Title bar view component
//!
//! Pure, props-based component for the app title bar with navigation and search.

use crate::components::icons::{ImageIcon, SettingsIcon};
use dioxus::prelude::*;

/// Navigation item for title bar
#[derive(Clone, PartialEq)]
pub struct NavItem {
    pub id: String,
    pub label: String,
    pub is_active: bool,
}

/// Search result for title bar dropdown
#[derive(Clone, PartialEq)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub cover_url: Option<String>,
}

/// Update state for the settings button
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Downloading,
    Ready,
}

/// Title bar view (pure, props-based)
/// Renders the visual structure with callbacks for all interactions.
#[component]
pub fn TitleBarView(
    // Navigation
    nav_items: Vec<NavItem>,
    on_nav_click: EventHandler<String>,
    // Search
    search_value: String,
    on_search_change: EventHandler<String>,
    search_results: Vec<SearchResult>,
    on_search_result_click: EventHandler<String>,
    show_search_results: bool,
    on_search_dismiss: EventHandler<()>,
    on_search_focus: EventHandler<()>,
    // Settings
    on_settings_click: EventHandler<()>,
    #[props(default)] settings_active: bool,
    #[props(default)] update_state: UpdateState,
    #[props(default)] on_update_click: Option<EventHandler<()>>,
    // Platform hooks (no-ops on web)
    #[props(default)] on_bar_mousedown: Option<EventHandler<()>>,
    #[props(default)] on_bar_double_click: Option<EventHandler<()>>,
    // Optional imports indicator slot
    #[props(default)] imports_indicator: Option<Element>,
    // Left padding for traffic lights on macOS
    #[props(default = 80)] left_padding: u32,
    // Use relative positioning instead of fixed (for mocks)
    #[props(default = false)] relative: bool,
) -> Element {
    let mut show_update_menu = use_signal(|| false);

    let (position_class, overlay_class, search_popover_class, update_popover_class) = if relative {
        (
            "relative",
            "absolute inset-0 z-[1500]",
            "absolute top-full left-1/2 -translate-x-1/2 w-64 z-[2000]",
            "absolute top-full right-0 w-48 z-[2000]",
        )
    } else {
        (
            "fixed top-0 left-0 right-0",
            "fixed inset-0 z-[1500]",
            "fixed top-10 left-1/2 -translate-x-1/2 w-64 z-[2000]",
            "fixed top-10 right-2 w-48 z-[2000]",
        )
    };

    let show_overlay = show_search_results || show_update_menu();

    rsx! {
        div { class: if relative { "relative" } else { "" },
            // Click-outside overlay to dismiss dropdowns
            if show_overlay {
                div {
                    class: "{overlay_class}",
                    onclick: move |_| {
                        on_search_dismiss.call(());
                        show_update_menu.set(false);
                    },
                }
            }

            // Title bar
            div {
                id: "title-bar",
                class: "{position_class} h-10 bg-[#1e222d] flex items-center justify-between px-2 cursor-default z-[1000] border-b border-[#2d3138]",
                style: "padding-left: {left_padding}px;",
                onmousedown: move |_| {
                    if let Some(handler) = &on_bar_mousedown {
                        handler.call(());
                    }
                },
                ondoubleclick: move |_| {
                    if let Some(handler) = &on_bar_double_click {
                        handler.call(());
                    }
                },

                // Left section: Navigation + imports indicator
                div {
                    class: "flex gap-2 flex-none items-center",
                    style: "-webkit-app-region: no-drag;",
                    for item in nav_items.iter() {
                        NavButtonView {
                            key: "{item.id}",
                            label: item.label.clone(),
                            is_active: item.is_active,
                            on_click: {
                                let id = item.id.clone();
                                move |_| on_nav_click.call(id.clone())
                            },
                        }
                    }

                    // Imports indicator
                    if let Some(indicator) = imports_indicator {
                        div { class: "relative ml-2", {indicator} }
                    }
                }

                // Center section: Search (absolutely positioned for true centering)
                div {
                    class: "absolute left-1/2 -translate-x-1/2",
                    style: "-webkit-app-region: no-drag;",
                    div { class: "relative w-64", id: "search-container",
                        input {
                            r#type: "text",
                            placeholder: "Search...",
                            autocomplete: "off",
                            class: "w-full h-7 px-3 bg-[#2d3138] border border-[#3d4148] rounded text-white text-xs placeholder-gray-500 focus:outline-none focus:border-blue-500",
                            value: "{search_value}",
                            oninput: move |evt| on_search_change.call(evt.value()),
                            onfocus: move |_| on_search_focus.call(()),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Escape {
                                    on_search_dismiss.call(());
                                }
                            },
                        }
                    }
                }

                // Right section: Settings button
                div { class: "flex-none", style: "-webkit-app-region: no-drag;",
                    SettingsButton {
                        is_active: settings_active,
                        update_state,
                        on_settings_click: move |_| on_settings_click.call(()),
                        on_update_click: move |_| {
                            if let Some(handler) = &on_update_click {
                                handler.call(());
                            }
                        },
                        show_menu: show_update_menu(),
                        on_toggle_menu: move |_| show_update_menu.toggle(),
                    }
                }
            }

            // Search results popover (centered)
            if show_search_results && !search_results.is_empty() {
                div {
                    class: "{search_popover_class}",
                    id: "search-popover",
                    onclick: move |evt| evt.stop_propagation(),
                    div { class: "mt-2 bg-[#2d3138] border border-[#3d4148] rounded-lg shadow-lg max-h-96 overflow-y-auto",
                        for result in search_results.iter() {
                            SearchResultItem {
                                key: "{result.id}",
                                result: result.clone(),
                                on_click: on_search_result_click,
                            }
                        }
                    }
                }
            }

            // Update menu popover (right aligned)
            if show_update_menu() && update_state != UpdateState::Idle {
                div {
                    class: "{update_popover_class}",
                    id: "update-popover",
                    onclick: move |evt| evt.stop_propagation(),
                    div { class: "mt-2 bg-[#2d3138] border border-[#3d4148] rounded-lg shadow-lg overflow-hidden",
                        match update_state {
                            UpdateState::Downloading => rsx! {
                                div { class: "px-3 py-2 text-xs text-gray-400", "Downloading update..." }
                            },
                            UpdateState::Ready => rsx! {
                                button {
                                    class: "w-full px-3 py-2 text-xs text-left text-white hover:bg-[#3d4148] transition-colors",
                                    onclick: move |_| {
                                        show_update_menu.set(false);
                                        if let Some(handler) = &on_update_click {
                                            handler.call(());
                                        }
                                    },
                                    "Restart to update"
                                }
                            },
                            UpdateState::Idle => rsx! {},
                        }
                    }
                }
            }
        }
    }
}

/// Settings button with optional update badge
#[component]
fn SettingsButton(
    is_active: bool,
    update_state: UpdateState,
    on_settings_click: EventHandler<()>,
    on_update_click: EventHandler<()>,
    show_menu: bool,
    on_toggle_menu: EventHandler<()>,
) -> Element {
    let has_update = update_state != UpdateState::Idle;
    let icon_class = if is_active {
        "text-white"
    } else {
        "text-gray-400 hover:text-white"
    };

    rsx! {
        div {
            class: "relative flex items-center",
            onmousedown: move |evt| evt.stop_propagation(),

            if has_update {
                // Split button: settings + update indicator
                div { class: "flex items-center rounded overflow-hidden",
                    // Settings button
                    button {
                        class: "p-1.5 {icon_class} hover:bg-gray-700 transition-colors",
                        title: "Settings",
                        onclick: move |_| on_settings_click.call(()),
                        SettingsIcon { class: "w-4 h-4" }
                    }
                    // Divider
                    div { class: "w-px h-4 bg-gray-600" }
                    // Update indicator button
                    button {
                        class: "p-1.5 hover:bg-gray-700 transition-colors flex items-center gap-1",
                        title: if update_state == UpdateState::Ready { "Update ready" } else { "Downloading update" },
                        onclick: move |_| on_toggle_menu.call(()),
                        match update_state {
                            UpdateState::Downloading => rsx! {
                                // Spinning indicator
                                svg {
                                    class: "animate-spin h-3 w-3 text-gray-400",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    circle {
                                        class: "opacity-25",
                                        cx: "12",
                                        cy: "12",
                                        r: "10",
                                        stroke: "currentColor",
                                        stroke_width: "4",
                                    }
                                    path {
                                        class: "opacity-75",
                                        fill: "currentColor",
                                        d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
                                    }
                                }
                            },
                            UpdateState::Ready => rsx! {
                                // Pulsing green dot
                                span { class: "relative flex h-2 w-2",
                                    span { class: "animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" }
                                    span { class: "relative inline-flex rounded-full h-2 w-2 bg-emerald-500" }
                                }
                            },
                            UpdateState::Idle => rsx! {},
                        }
                    }
                }
            } else {
                // Simple settings button
                button {
                    class: "p-1.5 {icon_class} hover:bg-gray-700 rounded transition-colors",
                    title: "Settings",
                    onclick: move |_| on_settings_click.call(()),
                    SettingsIcon { class: "w-4 h-4" }
                }
            }
        }
    }
}

/// Navigation button in the title bar
#[component]
fn NavButtonView(label: String, is_active: bool, on_click: EventHandler<()>) -> Element {
    let class = if is_active {
        "text-white no-underline text-[12px] cursor-pointer px-2 py-1 rounded bg-gray-700"
    } else {
        "text-gray-400 no-underline text-[12px] cursor-pointer px-2 py-1 rounded hover:bg-gray-800 hover:text-white transition-colors"
    };

    rsx! {
        span {
            class: "inline-block",
            onmousedown: move |evt| evt.stop_propagation(),
            button { class: "{class}", onclick: move |_| on_click.call(()), "{label}" }
        }
    }
}

/// Search result item in the dropdown
#[component]
fn SearchResultItem(result: SearchResult, on_click: EventHandler<String>) -> Element {
    let id = result.id.clone();

    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 hover:bg-[#3d4148] border-b border-[#3d4148] last:border-b-0 cursor-pointer",
            onclick: {
                let id = id.clone();
                move |evt| {
                    evt.stop_propagation();
                    on_click.call(id.clone());
                }
            },
            if let Some(url) = &result.cover_url {
                img {
                    src: "{url}",
                    class: "w-10 h-10 rounded object-cover flex-shrink-0",
                    alt: "{result.title}",
                }
            } else {
                div { class: "w-10 h-10 bg-gray-700 rounded flex items-center justify-center flex-shrink-0",
                    ImageIcon { class: "w-5 h-5 text-gray-500" }
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "text-white text-xs font-medium truncate", "{result.title}" }
                div { class: "text-gray-400 text-xs truncate", "{result.subtitle}" }
            }
        }
    }
}
