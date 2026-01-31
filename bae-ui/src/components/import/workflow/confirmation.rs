//! Confirmation view component

use crate::components::icons::ImageIcon;
use crate::components::{
    Button, ButtonSize, ButtonVariant, ChromelessButton, Modal, Select, SelectOption,
    StorageProfile,
};
use crate::display_types::{FileInfo, MatchCandidate, MatchSourceType, SelectedCover};
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
    artwork_files: Vec<FileInfo>,
    /// Remote cover URL from the match candidate
    remote_cover_url: Option<String>,
    /// Available storage profiles
    storage_profiles: ReadSignal<Vec<StorageProfile>>,
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
    let mut show_cover_modal = use_signal(|| false);
    let is_cover_modal_open: ReadSignal<bool> = show_cover_modal.into();

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

    let has_cover_options = !artwork_files.is_empty() || remote_cover_url.is_some();

    rsx! {
        div { class: "p-5 space-y-5",
            // Release info card
            div { class: "bg-gray-800/50 rounded-lg px-5 py-4",
                div { class: "flex items-center gap-5",
                    // Cover art (clickable if options available)
                    if has_cover_options {
                        ChromelessButton {
                            class: Some("flex-shrink-0".to_string()),
                            onclick: move |_| show_cover_modal.set(true),
                            if let Some(ref url) = display_cover_url {
                                div { class: "w-20 h-20 rounded-lg overflow-clip hover:ring-2 hover:ring-gray-500 transition-all",
                                    img {
                                        src: "{url}",
                                        alt: "Album cover",
                                        class: "w-full h-full object-cover",
                                    }
                                }
                            } else {
                                div { class: "w-20 h-20 rounded-lg bg-gray-700 flex items-center justify-center hover:ring-2 hover:ring-gray-500 transition-all",
                                    ImageIcon { class: "w-8 h-8 text-gray-500" }
                                }
                            }
                        }
                    } else {
                        if let Some(ref url) = display_cover_url {
                            div { class: "flex-shrink-0 w-20 h-20 rounded-lg overflow-clip",
                                img {
                                    src: "{url}",
                                    alt: "Album cover",
                                    class: "w-full h-full object-cover",
                                }
                            }
                        } else {
                            div { class: "flex-shrink-0 w-20 h-20 rounded-lg bg-gray-700 flex items-center justify-center",
                                ImageIcon { class: "w-8 h-8 text-gray-500" }
                            }
                        }
                    }

                    // Metadata
                    div { class: "flex-1 min-w-0",
                        h4 { class: "text-base font-medium text-white truncate", "{candidate.title}" }
                        div { class: "text-xs text-gray-400 flex flex-wrap gap-x-3",
                            if let Some(ref year) = release_year {
                                span { "{year}" }
                            }
                            if let Some(ref fmt) = format_text {
                                span { "{fmt}" }
                            }
                            if let Some(ref country) = country_text {
                                span { "{country}" }
                            }
                        }
                        if label_text.is_some() || original_year.is_some() {
                            div { class: "text-xs text-gray-500 flex flex-wrap gap-x-3",
                                if let Some(ref label) = label_text {
                                    span { "{label}" }
                                }
                                if original_year.is_some() && original_year != release_year {
                                    span { "Original: {original_year.as_ref().unwrap()}" }
                                }
                            }
                        }
                    }

                    // Edit button
                    Button {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Small,
                        disabled: is_importing,
                        onclick: move |_| on_edit.call(()),
                        "Edit"
                    }
                }
            }

            // Storage profile selection + Import button
            div { class: "flex items-center gap-3 px-5",
                label { class: "text-sm text-gray-400 ml-auto", "Storage:" }
                Select {
                    value: selected_profile_id.clone().unwrap_or_else(|| "__none__".to_string()),
                    disabled: is_importing,
                    onchange: move |val: String| {
                        if val == "__none__" {
                            on_storage_profile_change.call(None);
                        } else {
                            on_storage_profile_change.call(Some(val));
                        }
                    },
                    SelectOption {
                        value: "__none__",
                        label: "No Storage (files stay in place)",
                    }
                    for profile in storage_profiles.read().iter() {
                        SelectOption {
                            key: "{profile.id}",
                            value: "{profile.id}",
                            label: profile.name.clone(),
                        }
                    }
                }
                Button {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Small,
                    onclick: move |_| on_configure_storage.call(()),
                    "Configure"
                }

                // Import status and button
                if is_importing {
                    if let Some(ref step) = preparing_step_text {
                        span { class: "text-sm text-gray-400", "{step}" }
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    size: ButtonSize::Small,
                    disabled: is_importing,
                    loading: is_importing,
                    onclick: move |_| on_confirm.call(()),
                    if is_importing {
                        div { class: "animate-spin rounded-full h-4 w-4 border-b-2 border-white" }
                    }
                    "Import"
                }
            }
        }

        // Cover art selection modal
        Modal {
            is_open: is_cover_modal_open,
            on_close: move |_| show_cover_modal.set(false),
            div { class: "bg-gray-800 rounded-xl p-6 max-w-md w-full mx-4",
                h2 { class: "text-lg font-semibold text-white mb-4", "Select Cover Art" }
                div { class: "flex flex-wrap gap-3",
                    // Remote cover option
                    if let Some(ref url) = remote_cover_url {
                        {
                            let is_selected = matches!(
                                selected_cover.as_ref(),
                                Some(SelectedCover::Remote { .. })
                            );
                            let url_for_click = url.clone();
                            rsx! {
                                ChromelessButton {
                                    class: Some(
                                        if is_selected {
                                            "relative w-20 h-20 rounded-lg ring-2 ring-green-500 overflow-clip"
                                                .to_string()
                                        } else {
                                            "relative w-20 h-20 rounded-lg ring-1 ring-gray-600 hover:ring-gray-500 overflow-clip"
                                                .to_string()
                                        },
                                    ),
                                    aria_label: Some("Select remote cover art".to_string()),
                                    onclick: move |_| {
                                        on_select_remote_cover.call(url_for_click.clone());
                                        show_cover_modal.set(false);
                                    },
                                    img {
                                        src: "{url}",
                                        alt: "Remote cover",
                                        class: "w-full h-full object-cover",
                                    }
                                    if is_selected {
                                        div { class: "absolute top-0.5 right-0.5 bg-green-500 text-white text-xs w-4 h-4 rounded-full flex items-center justify-center",
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
                                ChromelessButton {
                                    key: "{img_url}",
                                    class: Some(
                                        if is_selected {
                                            "relative w-20 h-20 rounded-lg ring-2 ring-green-500 overflow-clip"
                                                .to_string()
                                        } else {
                                            "relative w-20 h-20 rounded-lg ring-1 ring-gray-600 hover:ring-gray-500 overflow-clip"
                                                .to_string()
                                        },
                                    ),
                                    aria_label: Some(format!("Select cover art: {}", img.name)),
                                    onclick: move |_| {
                                        on_select_local_cover.call(name_for_click.clone());
                                        show_cover_modal.set(false);
                                    },
                                    img {
                                        src: "{img_url}",
                                        alt: "{img.name}",
                                        class: "w-full h-full object-cover",
                                    }
                                    if is_selected {
                                        div { class: "absolute top-0.5 right-0.5 bg-green-500 text-white text-xs w-4 h-4 rounded-full flex items-center justify-center",
                                            "✓"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "text-xs text-gray-500 mt-3",
                    "Click an image to set it as the album cover"
                }
            }
        }
    }
}
