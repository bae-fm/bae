//! Manual search panel view component

use super::match_list::MatchListView;
use super::search_source_selector::SearchSourceSelectorView;
use crate::display_types::{MatchCandidate, SearchSource, SearchTab};
use dioxus::prelude::*;

/// Manual search panel with tabs for General/Catalog#/Barcode search
#[component]
pub fn ManualSearchPanelView(
    // Search source selection
    search_source: SearchSource,
    on_search_source_change: EventHandler<SearchSource>,
    // Active tab
    active_tab: SearchTab,
    on_tab_change: EventHandler<SearchTab>,
    // General search fields
    search_artist: String,
    on_artist_change: EventHandler<String>,
    search_album: String,
    on_album_change: EventHandler<String>,
    search_year: String,
    on_year_change: EventHandler<String>,
    search_label: String,
    on_label_change: EventHandler<String>,
    // Catalog number search
    search_catalog_number: String,
    on_catalog_number_change: EventHandler<String>,
    // Barcode search
    search_barcode: String,
    on_barcode_change: EventHandler<String>,
    // Search tokens (suggestions from folder name)
    search_tokens: Vec<String>,
    // Search state
    is_searching: bool,
    error_message: Option<String>,
    has_searched: bool,
    // Results
    match_candidates: Vec<MatchCandidate>,
    selected_index: Option<usize>,
    on_match_select: EventHandler<usize>,
    // Actions
    on_search: EventHandler<()>,
    on_confirm: EventHandler<MatchCandidate>,
) -> Element {
    rsx! {
        div { class: "bg-gray-800 rounded-lg shadow p-6 space-y-4",
            // Header with search source selector
            div { class: "flex justify-between items-center",
                h3 { class: "text-lg font-semibold text-white", "Search for Release" }
                SearchSourceSelectorView {
                    selected_source: search_source,
                    on_select: on_search_source_change,
                }
            }

            // Tab bar
            div { class: "flex border-b border-gray-700",
                button {
                    class: if active_tab == SearchTab::General { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white" },
                    onclick: move |_| on_tab_change.call(SearchTab::General),
                    "General"
                }
                button {
                    class: if active_tab == SearchTab::CatalogNumber { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white" },
                    onclick: move |_| on_tab_change.call(SearchTab::CatalogNumber),
                    "Catalog #"
                }
                button {
                    class: if active_tab == SearchTab::Barcode { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white" },
                    onclick: move |_| on_tab_change.call(SearchTab::Barcode),
                    "Barcode"
                }
            }

            // Search token pills (suggestions)
            if !search_tokens.is_empty() {
                div { class: "flex flex-wrap gap-2",
                    for token in search_tokens.iter() {
                        span { class: "px-3 py-1 text-sm bg-gray-700 text-gray-300 rounded-full border border-gray-600",
                            "{token}"
                        }
                    }
                }
            }

            // Search form based on active tab
            div { class: "space-y-3",
                match active_tab {
                    SearchTab::General => rsx! {
                        div { class: "grid grid-cols-2 gap-3",
                            div {
                                label { class: "block text-sm font-medium text-gray-300 mb-1", "Artist" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                    value: "{search_artist}",
                                    oninput: move |e| on_artist_change.call(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-300 mb-1", "Album" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                    value: "{search_album}",
                                    oninput: move |e| on_album_change.call(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-300 mb-1", "Year" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                    value: "{search_year}",
                                    oninput: move |e| on_year_change.call(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium text-gray-300 mb-1", "Label" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                    value: "{search_label}",
                                    oninput: move |e| on_label_change.call(e.value()),
                                }
                            }
                        }
                        div { class: "flex justify-end pt-2",
                            button {
                                class: "px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: is_searching,
                                onclick: move |_| on_search.call(()),
                                if is_searching {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                    SearchTab::CatalogNumber => rsx! {
                        div { class: "flex gap-3",
                            div { class: "flex-1",
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                    placeholder: "Enter catalog number...",
                                    value: "{search_catalog_number}",
                                    oninput: move |e| on_catalog_number_change.call(e.value()),
                                }
                            }
                            button {
                                class: "px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: is_searching,
                                onclick: move |_| on_search.call(()),
                                if is_searching {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                    SearchTab::Barcode => rsx! {
                        div { class: "flex gap-3",
                            div { class: "flex-1",
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 text-white",
                                    placeholder: "Enter barcode...",
                                    value: "{search_barcode}",
                                    oninput: move |e| on_barcode_change.call(e.value()),
                                }
                            }
                            button {
                                class: "px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: is_searching,
                                onclick: move |_| on_search.call(()),
                                if is_searching {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                }
            }

            // Error message
            if let Some(ref error) = error_message {
                div { class: "bg-red-900/30 border border-red-700 rounded-lg p-4",
                    p { class: "text-sm text-red-300 select-text", "Error: {error}" }
                }
            }

            // Results
            if is_searching {
                div { class: "text-center py-8",
                    p { class: "text-gray-400", "Searching..." }
                }
            } else if match_candidates.is_empty() && has_searched {
                div { class: "text-center py-8",
                    p { class: "text-gray-400", "No results found" }
                }
            } else if !match_candidates.is_empty() {
                div { class: "space-y-4 mt-4",
                    MatchListView {
                        candidates: match_candidates.clone(),
                        selected_index,
                        on_select: move |index| on_match_select.call(index),
                    }

                    if selected_index.is_some() {
                        div { class: "flex justify-end",
                            button {
                                class: "px-6 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700",
                                onclick: move |_| {
                                    if let Some(index) = selected_index {
                                        if let Some(candidate) = match_candidates.get(index) {
                                            on_confirm.call(candidate.clone());
                                        }
                                    }
                                },
                                "Confirm Selection"
                            }
                        }
                    }
                }
            }
        }
    }
}
