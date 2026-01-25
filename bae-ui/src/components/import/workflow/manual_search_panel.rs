//! Manual search panel view component

use super::match_list::MatchListView;
use super::search_source_selector::SearchSourceSelectorView;
use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::display_types::{MatchCandidate, SearchSource, SearchTab};
use crate::stores::import::ImportState;
use dioxus::prelude::*;

/// Manual search panel with tabs for General/Catalog#/Barcode search
///
/// Accepts `ReadStore<ImportState>` - reads at leaf level for granular reactivity.
#[component]
pub fn ManualSearchPanelView(
    state: ReadStore<ImportState>,
    on_search_source_change: EventHandler<SearchSource>,
    on_tab_change: EventHandler<SearchTab>,
    on_artist_change: EventHandler<String>,
    on_album_change: EventHandler<String>,
    on_year_change: EventHandler<String>,
    on_label_change: EventHandler<String>,
    on_catalog_number_change: EventHandler<String>,
    on_barcode_change: EventHandler<String>,
    on_match_select: EventHandler<usize>,
    on_search: EventHandler<()>,
    on_confirm: EventHandler<MatchCandidate>,
) -> Element {
    // Read state at this leaf component
    let st = state.read();
    let search_state = st.get_search_state();
    let metadata = st.get_metadata();

    let source = search_state
        .as_ref()
        .map(|s| s.search_source)
        .unwrap_or(SearchSource::MusicBrainz);
    let tab = search_state
        .as_ref()
        .map(|s| s.search_tab)
        .unwrap_or(SearchTab::General);
    let artist = search_state
        .as_ref()
        .map(|s| s.search_artist.clone())
        .unwrap_or_default();
    let album = search_state
        .as_ref()
        .map(|s| s.search_album.clone())
        .unwrap_or_default();
    let year = search_state
        .as_ref()
        .map(|s| s.search_year.clone())
        .unwrap_or_default();
    let label = search_state
        .as_ref()
        .map(|s| s.search_label.clone())
        .unwrap_or_default();
    let catalog = search_state
        .as_ref()
        .map(|s| s.search_catalog_number.clone())
        .unwrap_or_default();
    let barcode = search_state
        .as_ref()
        .map(|s| s.search_barcode.clone())
        .unwrap_or_default();
    let tokens = metadata
        .as_ref()
        .map(|m| m.folder_tokens.clone())
        .unwrap_or_default();
    let searching = search_state
        .as_ref()
        .map(|s| s.is_searching)
        .unwrap_or(false);
    let error = search_state.as_ref().and_then(|s| s.error_message.clone());
    let searched = search_state
        .as_ref()
        .map(|s| s.has_searched)
        .unwrap_or(false);
    let candidates = search_state
        .as_ref()
        .map(|s| s.search_results.clone())
        .unwrap_or_default();
    let selected = search_state.as_ref().and_then(|s| s.selected_result_index);

    drop(st);

    rsx! {
        div { class: "space-y-4",
            // Header with search source selector
            div { class: "flex justify-between items-center",
                h3 { class: "text-sm font-medium text-gray-200", "Search for Release" }
                SearchSourceSelectorView {
                    selected_source: source,
                    on_select: on_search_source_change,
                }
            }

            // Tab bar
            div { class: "flex gap-1",
                Button {
                    variant: if tab == SearchTab::General { ButtonVariant::Primary } else { ButtonVariant::Ghost },
                    size: ButtonSize::Small,
                    onclick: move |_| on_tab_change.call(SearchTab::General),
                    "General"
                }
                Button {
                    variant: if tab == SearchTab::CatalogNumber { ButtonVariant::Primary } else { ButtonVariant::Ghost },
                    size: ButtonSize::Small,
                    onclick: move |_| on_tab_change.call(SearchTab::CatalogNumber),
                    "Catalog #"
                }
                Button {
                    variant: if tab == SearchTab::Barcode { ButtonVariant::Primary } else { ButtonVariant::Ghost },
                    size: ButtonSize::Small,
                    onclick: move |_| on_tab_change.call(SearchTab::Barcode),
                    "Barcode"
                }
            }

            // Search token pills (suggestions)
            if !tokens.is_empty() {
                div { class: "flex flex-wrap gap-1.5",
                    for token in tokens.iter() {
                        span { class: "px-2.5 py-1 text-xs bg-surface-raised text-gray-300 rounded-full",
                            "{token}"
                        }
                    }
                }
            }

            // Search form based on active tab
            div { class: "space-y-3",
                match tab {
                    SearchTab::General => rsx! {
                        div { class: "grid grid-cols-2 gap-3",
                            div {
                                label { class: "block text-xs text-gray-300 mb-1.5", "Artist" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{artist}",
                                    oninput: move |e| on_artist_change.call(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-xs text-gray-300 mb-1.5", "Album" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{album}",
                                    oninput: move |e| on_album_change.call(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-xs text-gray-300 mb-1.5", "Year" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{year}",
                                    oninput: move |e| on_year_change.call(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-xs text-gray-300 mb-1.5", "Label" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{label}",
                                    oninput: move |e| on_label_change.call(e.value()),
                                }
                            }
                        }
                        div { class: "flex justify-end pt-1",
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Small,
                                disabled: searching,
                                loading: searching,
                                onclick: move |_| on_search.call(()),
                                if searching {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                    SearchTab::CatalogNumber => rsx! {
                        div { class: "flex gap-2",
                            div { class: "flex-1",
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    placeholder: "Enter catalog number...",
                                    value: "{catalog}",
                                    oninput: move |e| on_catalog_number_change.call(e.value()),
                                }
                            }
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Small,
                                disabled: searching,
                                loading: searching,
                                onclick: move |_| on_search.call(()),
                                if searching {
                                    "Searching..."
                                } else {
                                    "Search"
                                }
                            }
                        }
                    },
                    SearchTab::Barcode => rsx! {
                        div { class: "flex gap-2",
                            div { class: "flex-1",
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    placeholder: "Enter barcode...",
                                    value: "{barcode}",
                                    oninput: move |e| on_barcode_change.call(e.value()),
                                }
                            }
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Small,
                                disabled: searching,
                                loading: searching,
                                onclick: move |_| on_search.call(()),
                                if searching {
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
            if let Some(ref err) = error {
                div { class: "bg-red-500/15 rounded-lg p-3",
                    p { class: "text-sm text-red-300 select-text", "Error: {err}" }
                }
            }

            // Results
            if searching {
                div { class: "text-center py-8",
                    p { class: "text-gray-400", "Searching..." }
                }
            } else if candidates.is_empty() && searched {
                div { class: "text-center py-8",
                    p { class: "text-gray-400", "No results found" }
                }
            } else if !candidates.is_empty() {
                div { class: "space-y-4 mt-4",
                    MatchListView {
                        candidates: candidates.clone(),
                        selected_index: selected,
                        on_select: move |index| on_match_select.call(index),
                    }

                    if selected.is_some() {
                        div { class: "flex justify-end",
                            Button {
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Small,
                                class: Some("bg-green-500/25 text-green-300 hover:bg-green-500/35".to_string()),
                                onclick: move |_| {
                                    if let Some(index) = selected {
                                        if let Some(candidate) = candidates.get(index) {
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
