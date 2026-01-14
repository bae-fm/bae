//! Confirmation view component

use crate::components::icons::MusicIcon;
use crate::display_types::{
    ArtworkFile, MatchCandidate, MatchSourceType, SelectedCover, StorageProfileInfo,
};
use dioxus::prelude::*;

/// Final confirmation view before import
#[component]
pub fn ConfirmationView(
    /// The confirmed match candidate
    candidate: MatchCandidate,
    /// Currently selected cover
    selected_cover: Option<SelectedCover>,
    /// URL to display for the cover (resolved from selected_cover)
    display_cover_url: Option<String>,
    /// Artwork files available in the folder (with resolved display URLs)
    artwork_files: Vec<ArtworkFile>,
    /// Remote cover URL from the match candidate
    remote_cover_url: Option<String>,
    /// Available storage profiles
    storage_profiles: Vec<StorageProfileInfo>,
    /// Currently selected storage profile ID
    selected_profile_id: Option<String>,
    /// Whether import is in progress
    is_importing: bool,
    /// Current preparing step text (if preparing)
    preparing_step_text: Option<String>,
    /// Called when user selects a remote cover
    on_select_remote_cover: EventHandler<String>,
    /// Called when user selects a local cover
    on_select_local_cover: EventHandler<String>,
    /// Called when user changes storage profile
    on_storage_profile_change: EventHandler<Option<String>>,
    /// Called when user clicks Edit to go back
    on_edit: EventHandler<()>,
    /// Called when user confirms import
    on_confirm: EventHandler<()>,
    /// Called to navigate to settings
    on_configure_storage: EventHandler<()>,
) -> Element {
    let release_year = candidate.year.clone();
    let original_year = candidate.original_year.clone();

    let (format_text, country_text, label_text) = match candidate.source_type {
        MatchSourceType::MusicBrainz => (
            candidate.format.clone(),
            candidate.country.clone(),
            candidate.label.clone(),
        ),
        MatchSourceType::Discogs => (None, None, None),
    };

    rsx! {
        div { class: "bg-gray-800 rounded-lg shadow p-6",
            h3 { class: "text-sm font-semibold text-gray-300 uppercase tracking-wide mb-4",
                "Selected Release"
            }

            // Release info card
            div { class: "bg-gray-900 rounded-lg p-5 mb-4 border border-gray-700",
                div { class: "flex gap-6",
                    // Cover art
                    if let Some(ref url) = display_cover_url {
                        div { class: "flex-shrink-0 w-32 h-32 rounded-lg border border-gray-600 shadow-lg overflow-hidden",
                            img {
                                src: "{url}",
                                alt: "Album cover",
                                class: "w-full h-full object-cover",
                            }
                        }
                    } else {
                        div { class: "flex-shrink-0 w-32 h-32 rounded-lg border border-gray-600 shadow-lg bg-gray-700 flex items-center justify-center",
                            MusicIcon { class: "w-12 h-12 text-gray-500" }
                        }
                    }

                    // Metadata
                    div { class: "flex-1 space-y-3",
                        p { class: "text-xl font-semibold text-white", "{candidate.title}" }
                        div { class: "space-y-1 text-sm text-gray-300",
                            if let Some(ref orig_year) = original_year {
                                p {
                                    span { class: "text-gray-400", "Original: " }
                                    span { class: "text-white", "{orig_year}" }
                                }
                            }
                            if let Some(ref year) = release_year {
                                p {
                                    span { class: "text-gray-400", "This Release: " }
                                    span { class: "text-white", "{year}" }
                                }
                            }
                            if let Some(ref fmt) = format_text {
                                p {
                                    span { class: "text-gray-400", "Format: " }
                                    span { class: "text-white", "{fmt}" }
                                }
                            }
                            if let Some(ref country) = country_text {
                                p {
                                    span { class: "text-gray-400", "Country: " }
                                    span { class: "text-white", "{country}" }
                                }
                            }
                            if let Some(ref label) = label_text {
                                p {
                                    span { class: "text-gray-400", "Label: " }
                                    span { class: "text-white", "{label}" }
                                }
                            }
                        }
                    }
                }
            }

            // Cover art selection
            if !artwork_files.is_empty() || remote_cover_url.is_some() {
                div { class: "mb-4",
                    h4 { class: "text-sm font-medium text-gray-400 mb-2", "Cover Art" }
                    div { class: "flex flex-wrap gap-2",
                        // Remote cover option
                        if let Some(ref url) = remote_cover_url {
                            {
                                let is_selected = matches!(
                                    selected_cover.as_ref(),
                                    Some(SelectedCover::Remote { .. })
                                );
                                let url_for_click = url.clone();
                                rsx! {
                                    button {
                                        class: if is_selected { "relative w-16 h-16 rounded border-2 border-green-500 overflow-hidden" } else { "relative w-16 h-16 rounded border-2 border-gray-600 hover:border-gray-500 overflow-hidden" },
                                        onclick: move |_| on_select_remote_cover.call(url_for_click.clone()),
                                        img {
                                            src: "{url}",
                                            alt: "Remote cover",
                                            class: "w-full h-full object-cover",
                                        }
                                        if is_selected {
                                            div { class: "absolute top-0 right-0 bg-green-500 text-white text-xs px-1 rounded-bl",
                                                "✓"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Local artwork options
                        for img in artwork_files.iter() {
                            {
                                let img_name = img.name.clone();
                                let is_selected = matches!(
                                    selected_cover.as_ref(),
                                    Some(SelectedCover::Local { filename })
                                    if filename == &img_name
                                );
                                let img_url = img.display_url.clone();
                                let name_for_click = img.name.clone();
                                rsx! {
                                    button {
                                        key: "{img_name}",
                                        class: if is_selected { "relative w-16 h-16 rounded border-2 border-green-500 overflow-hidden" } else { "relative w-16 h-16 rounded border-2 border-gray-600 hover:border-gray-500 overflow-hidden" },
                                        onclick: move |_| on_select_local_cover.call(name_for_click.clone()),
                                        img {
                                            src: "{img_url}",
                                            alt: "{img.name}",
                                            class: "w-full h-full object-cover",
                                        }
                                        if is_selected {
                                            div { class: "absolute top-0 right-0 bg-green-500 text-white text-xs px-1 rounded-bl",
                                                "✓"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    p { class: "text-xs text-gray-500 mt-1",
                        "Click an image to set it as the album cover"
                    }
                }
            }

            // Storage profile selection
            div { class: "mb-4 flex items-center gap-3",
                label { class: "text-sm text-gray-400", "Storage:" }
                select {
                    class: "bg-gray-700 text-white rounded px-3 py-1.5 text-sm border border-gray-600 focus:border-blue-500 focus:outline-none",
                    disabled: is_importing,
                    onchange: move |evt: Event<FormData>| {
                        let value = evt.value();
                        if value == "__none__" {
                            on_storage_profile_change.call(None);
                        } else if !value.is_empty() {
                            on_storage_profile_change.call(Some(value));
                        }
                    },
                    option {
                        key: "__none__",
                        value: "__none__",
                        selected: selected_profile_id.is_none(),
                        "No Storage (files stay in place)"
                    }
                    for profile in storage_profiles.iter() {
                        option {
                            key: "{profile.id}",
                            value: "{profile.id}",
                            selected: selected_profile_id.as_ref() == Some(&profile.id),
                            "{profile.name}"
                        }
                    }
                }
                button {
                    class: "text-xs text-indigo-400 hover:text-indigo-300 transition-colors",
                    onclick: move |_| on_configure_storage.call(()),
                    "Configure"
                }
            }

            // Action buttons
            div { class: "flex justify-end gap-3 items-center",
                if is_importing {
                    if let Some(ref step) = preparing_step_text {
                        span { class: "text-sm text-gray-400", "{step}" }
                    }
                }
                button {
                    class: "px-6 py-2 bg-gray-700 text-white rounded-lg hover:bg-gray-600 transition-colors border border-gray-600",
                    disabled: is_importing,
                    onclick: move |_| on_edit.call(()),
                    "Edit"
                }
                button {
                    class: if is_importing { "px-6 py-2 bg-green-600 text-white rounded-lg transition-colors opacity-75 cursor-not-allowed flex items-center gap-2" } else { "px-6 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700 transition-colors flex items-center gap-2" },
                    disabled: is_importing,
                    onclick: move |_| on_confirm.call(()),
                    if is_importing {
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-white" }
                    }
                    "Import"
                }
            }
        }
    }
}
