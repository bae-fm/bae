//! Title bar view component
//!
//! Pure, props-based component for the app title bar with navigation and search.

use crate::components::icons::MusicIcon;
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
    // Platform hooks (no-ops on web)
    #[props(default)] on_bar_mousedown: Option<EventHandler<()>>,
    #[props(default)] on_bar_double_click: Option<EventHandler<()>>,
    // Optional imports indicator slot
    #[props(default)] imports_indicator: Option<Element>,
    // Left padding for traffic lights on macOS
    #[props(default = 80)] left_padding: u32,
) -> Element {
    rsx! {
        // Click-outside overlay to dismiss search
        if show_search_results {
            div {
                class: "fixed inset-0 z-[1500]",
                onclick: move |_| on_search_dismiss.call(()),
            }
        }

        // Title bar
        div {
            id: "title-bar",
            class: "fixed top-0 left-0 right-0 h-10 bg-[#1e222d] flex items-center pr-2 cursor-default z-[1000] border-b border-[#2d3138]",
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

            // Navigation buttons
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
            }

            // Optional imports indicator
            if let Some(indicator) = imports_indicator {
                div {
                    class: "relative ml-4",
                    style: "-webkit-app-region: no-drag;",
                    {indicator}
                }
            }

            // Search
            div {
                class: "flex-1 flex justify-end items-center relative",
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
        }

        // Search results popover
        if show_search_results && !search_results.is_empty() {
            div {
                class: "fixed top-10 right-2 w-64 z-[2000]",
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
                    MusicIcon { class: "w-5 h-5 text-gray-500" }
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "text-white text-xs font-medium truncate", "{result.title}" }
                div { class: "text-gray-400 text-xs truncate", "{result.subtitle}" }
            }
        }
    }
}
