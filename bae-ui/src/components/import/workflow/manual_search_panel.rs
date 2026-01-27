//! Manual search panel view component

use super::match_list::MatchListView;
use super::search_source_selector::SearchSourceSelectorView;
use super::{DiscIdPill, LoadingIndicator};
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
    on_catalog_number_change: EventHandler<String>,
    on_barcode_change: EventHandler<String>,
    on_match_select: EventHandler<usize>,
    on_search: EventHandler<()>,
    on_confirm: EventHandler<MatchCandidate>,
    on_switch_to_exact_matches: EventHandler<String>,
) -> Element {
    // Read state at this leaf component
    let st = state.read();
    let search_state = st.get_search_state();
    let disc_id_not_found = st.get_disc_id_not_found();
    let exact_matches = st.get_exact_match_candidates();
    let source_disc_id = st.get_source_disc_id();

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
    let catalog = search_state
        .as_ref()
        .map(|s| s.search_catalog_number.clone())
        .unwrap_or_default();
    let barcode = search_state
        .as_ref()
        .map(|s| s.search_barcode.clone())
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
        div { class: "p-5 space-y-4",
            // Info banner if disc ID lookup found no results
            if let Some(disc_id) = disc_id_not_found {
                div { class: "bg-blue-500/15 rounded-lg p-3 flex items-center gap-2",
                    p { class: "text-sm text-blue-300",
                        "No releases found for Disc ID "
                        DiscIdPill { disc_id }
                    }
                }
            }

            // Link back to exact matches if they exist
            if !exact_matches.is_empty() {
                if let Some(disc_id) = source_disc_id.clone() {
                    div { class: "flex items-center gap-2",
                        p { class: "text-sm text-gray-400 flex items-center gap-2",
                            "{exact_matches.len()} exact matches for"
                            DiscIdPill { disc_id: disc_id.clone() }
                        }
                        Button {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Small,
                            onclick: move |_| on_switch_to_exact_matches.call(disc_id.clone()),
                            "View"
                        }
                    }
                }
            }

            // Header row: tabs + source selector
            div { class: "flex items-center justify-between gap-4",
                div { class: "flex gap-1",
                    Button {
                        variant: if tab == SearchTab::General { ButtonVariant::Primary } else { ButtonVariant::Ghost },
                        size: ButtonSize::Small,
                        onclick: move |_| on_tab_change.call(SearchTab::General),
                        "Title"
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

                SearchSourceSelectorView {
                    selected_source: source,
                    on_select: on_search_source_change,
                }
            }

            // Search form based on active tab
            div { class: "space-y-3",
                match tab {
                    SearchTab::General => rsx! {
                        div { class: "flex gap-3",
                            div { class: "flex-1",
                                label { class: "block text-xs text-gray-400 mb-1.5", "Artist" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{artist}",
                                    oninput: move |e| on_artist_change.call(e.value()),
                                }
                            }
                            div { class: "flex-1",
                                label { class: "block text-xs text-gray-400 mb-1.5", "Album" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    value: "{album}",
                                    oninput: move |e| on_album_change.call(e.value()),
                                }
                            }
                            div { class: "flex items-end",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    size: ButtonSize::Medium,
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
                        }
                    },
                    SearchTab::CatalogNumber => rsx! {
                        div { class: "flex gap-3",
                            div { class: "flex-1",
                                label { class: "block text-xs text-gray-400 mb-1.5", "Catalog Number" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    placeholder: "e.g. WPCR-80001",
                                    value: "{catalog}",
                                    oninput: move |e| on_catalog_number_change.call(e.value()),
                                }
                            }
                            div { class: "flex items-end",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    size: ButtonSize::Medium,
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
                        }
                    },
                    SearchTab::Barcode => rsx! {
                        div { class: "flex gap-3",
                            div { class: "flex-1",
                                label { class: "block text-xs text-gray-400 mb-1.5", "Barcode" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-3 py-2 bg-surface-input rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-white placeholder-gray-500",
                                    placeholder: "e.g. 4943674251780",
                                    value: "{barcode}",
                                    oninput: move |e| on_barcode_change.call(e.value()),
                                }
                            }
                            div { class: "flex items-end",
                                Button {
                                    variant: ButtonVariant::Primary,
                                    size: ButtonSize::Medium,
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
                div { class: "flex justify-center py-8",
                    LoadingIndicator { message: format!("Searching {}...", source.display_name()) }
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
