//! Title bar view component
//!
//! Pure, props-based component for the app title bar with navigation and search.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::components::icons::{ImageIcon, SettingsIcon};
use crate::components::{ChromelessButton, Dropdown, Placement};
use dioxus::prelude::*;

/// Counter for generating unique element IDs
static BUTTON_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    show_search_results: ReadSignal<bool>,
    on_search_dismiss: EventHandler<()>,
    on_search_focus: EventHandler<()>,
    // Settings
    on_settings_click: EventHandler<()>,
    #[props(default)] settings_active: bool,
    // Platform hooks (no-ops on web)
    #[props(default)] on_bar_mousedown: Option<EventHandler<()>>,
    #[props(default)] on_bar_double_click: Option<EventHandler<()>>,
    // Optional imports indicator slot
    #[props(default)] imports_indicator: Option<Element>,
    // Left padding for traffic lights on macOS
    #[props(default = 80)] left_padding: u32,
) -> Element {
    let search_input_id = use_hook(|| {
        let id = BUTTON_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("search-input-{}", id)
    });

    rsx! {
        // Title bar
        div {
            id: "title-bar",
            class: "shrink-0 h-10 bg-surface-raised flex items-center justify-between px-2 cursor-default border-b border-border-subtle",
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
                    NavButton {
                        key: "{item.id}",
                        is_active: item.is_active,
                        on_click: {
                            let id = item.id.clone();
                            move |_| on_nav_click.call(id.clone())
                        },
                        "{item.label}"
                    }
                }

                // Imports indicator
                if let Some(indicator) = imports_indicator {
                    div { class: "relative ml-2", {indicator} }
                }
            }

            // Right section: Search + Settings
            div {
                class: "flex-none flex items-center gap-2",
                style: "-webkit-app-region: no-drag;",

                // Search input with dropdown
                div { class: "relative w-40",
                    input {
                        id: "{search_input_id}",
                        r#type: "text",
                        placeholder: "Search...",
                        autocomplete: "off",
                        class: "w-full h-7 px-2 bg-surface-input border border-border-default rounded text-white text-xs placeholder-gray-400 focus:outline-none focus:border-border-strong",
                        value: "{search_value}",
                        oninput: move |evt| on_search_change.call(evt.value()),
                        onfocus: move |_| on_search_focus.call(()),
                        onkeydown: move |evt| {
                            if evt.key() == Key::Escape {
                                on_search_dismiss.call(());
                            }
                        },
                    }

                    // Search results dropdown
                    if !search_results.is_empty() {
                        Dropdown {
                            anchor_id: search_input_id.clone(),
                            is_open: show_search_results,
                            on_close: on_search_dismiss,
                            placement: Placement::Bottom,
                            class: "bg-surface-overlay border border-border-strong rounded-lg shadow-lg w-64 max-h-96 overflow-y-auto",
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

                // Settings button
                NavButton {
                    is_active: settings_active,
                    on_click: move |_| on_settings_click.call(()),
                    SettingsIcon { class: "w-4 h-4" }
                }
            }
        }
    }
}

/// Navigation button with generic children
#[component]
fn NavButton(is_active: bool, on_click: EventHandler<()>, children: Element) -> Element {
    let class = if is_active {
        "text-white text-[12px] cursor-pointer px-2 py-1.5 rounded bg-gray-700 transition-colors"
    } else {
        "text-gray-400 text-[12px] cursor-pointer px-2 py-1.5 rounded hover:bg-gray-700 hover:text-white transition-colors"
    };

    rsx! {
        span {
            class: "inline-block",
            onmousedown: move |evt| evt.stop_propagation(),
            ChromelessButton {
                class: Some(class.to_string()),
                onclick: move |_| on_click.call(()),
                {children}
            }
        }
    }
}

/// Search result item in the dropdown
#[component]
fn SearchResultItem(result: SearchResult, on_click: EventHandler<String>) -> Element {
    let id = result.id.clone();

    rsx! {
        div {
            class: "flex items-center gap-3 px-3 py-2 hover:bg-hover border-b border-border-strong last:border-b-0 cursor-pointer",
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
