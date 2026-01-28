//! Release info modal with tabs for details, files, and gallery

use crate::components::icons::XIcon;
use crate::components::utils::{format_duration, format_file_size};
use crate::components::Modal;
use crate::display_types::{File, Image, Release};
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Details,
    Files,
    Gallery,
}

/// Modal component with tabs for release details and files (props-based)
#[component]
pub fn ReleaseInfoModal(
    is_open: ReadSignal<bool>,
    release: Release,
    on_close: EventHandler<()>,
    // Files and images can be loaded externally or passed as props
    #[props(default)] files: Vec<File>,
    #[props(default)] images: Vec<Image>,
    #[props(default)] is_loading_files: bool,
    #[props(default)] is_loading_images: bool,
    #[props(default)] files_error: Option<String>,
    #[props(default)] images_error: Option<String>,
    #[props(default = Tab::Details)] initial_tab: Tab,
    #[props(default)] track_count: usize,
    #[props(default)] total_duration_ms: Option<i64>,
) -> Element {
    let mut active_tab = use_signal(|| initial_tab);

    // Reset to initial_tab when modal opens
    use_effect(move || {
        if is_open() {
            active_tab.set(initial_tab);
        }
    });

    let current_tab = *active_tab.read();

    rsx! {
        Modal { is_open, on_close: move |_| on_close.call(()),
            div { class: "bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full mx-4 max-h-[80vh] flex flex-col",
                div { class: "border-b border-gray-700",
                    div { class: "flex items-center justify-between px-6 pt-6 pb-4",
                        h2 { class: "text-xl font-bold text-white", "Release Info" }
                        button {
                            class: "text-gray-400 hover:text-white transition-colors",
                            onclick: move |_| on_close.call(()),
                            XIcon { class: "w-5 h-5" }
                        }
                    }
                    div { class: "flex px-6",
                        button {
                            class: if current_tab == Tab::Details { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white border-b-2 border-transparent" },
                            onclick: move |_| active_tab.set(Tab::Details),
                            "Details"
                        }
                        button {
                            class: if current_tab == Tab::Files { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white border-b-2 border-transparent" },
                            onclick: move |_| active_tab.set(Tab::Files),
                            "Files"
                        }
                        button {
                            class: if current_tab == Tab::Gallery { "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-white border-b-2 border-transparent" },
                            onclick: move |_| active_tab.set(Tab::Gallery),
                            "Gallery"
                        }
                    }
                }
                div { class: "p-6 overflow-y-auto flex-1",
                    match current_tab {
                        Tab::Details => rsx! {
                            DetailsTab { release: release.clone(), track_count, total_duration_ms }
                        },
                        Tab::Files => rsx! {
                            FilesTab {
                                files: files.clone(),
                                is_loading: is_loading_files,
                                error: files_error.clone(),
                            }
                        },
                        Tab::Gallery => rsx! {
                            GalleryTab {
                                images: images.clone(),
                                is_loading: is_loading_images,
                                error: images_error.clone(),
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn DetailsTab(release: Release, track_count: usize, total_duration_ms: Option<i64>) -> Element {
    rsx! {
        div { class: "space-y-4",
            if release.year.is_some() || release.format.is_some() {
                div {
                    if let Some(year) = release.year {
                        span { class: "text-gray-300", "{year}" }
                        if release.format.is_some() {
                            span { class: "text-gray-300", " " }
                        }
                    }
                    if let Some(ref format) = release.format {
                        span { class: "text-gray-300", "{format}" }
                    }
                }
            }
            // Track count and duration
            if track_count > 0 || total_duration_ms.is_some() {
                div { class: "text-gray-300",
                    if track_count > 0 {
                        span {
                            "{track_count} "
                            if track_count == 1 {
                                "track"
                            } else {
                                "tracks"
                            }
                        }
                    }
                    if track_count > 0 && total_duration_ms.is_some() {
                        span { " · " }
                    }
                    if let Some(duration) = total_duration_ms {
                        span { {format_duration(duration)} }
                    }
                }
            }
            if release.label.is_some() || release.catalog_number.is_some() {
                div { class: "text-sm text-gray-400",
                    if let Some(ref label) = release.label {
                        span { "{label}" }
                        if release.catalog_number.is_some() {
                            span { " • " }
                        }
                    }
                    if let Some(ref catalog) = release.catalog_number {
                        span { "{catalog}" }
                    }
                }
            }
            if let Some(ref country) = release.country {
                div { class: "text-sm text-gray-400",
                    span { "{country}" }
                }
            }
            if let Some(ref barcode) = release.barcode {
                div { class: "text-sm text-gray-400",
                    span { class: "font-medium", "Barcode: " }
                    span { class: "font-mono", "{barcode}" }
                }
            }
            // External links
            if release.musicbrainz_release_id.is_some() || release.discogs_release_id.is_some() {
                div { class: "pt-4 border-t border-gray-700 space-y-2",
                    if let Some(ref mb_id) = release.musicbrainz_release_id {
                        a {
                            href: "https://musicbrainz.org/release/{mb_id}",
                            target: "_blank",
                            class: "flex items-center gap-2 text-sm text-blue-400 hover:text-blue-300 transition-colors",
                            span { "View on MusicBrainz" }
                        }
                    }
                    if let Some(ref discogs_id) = release.discogs_release_id {
                        a {
                            href: "https://www.discogs.com/release/{discogs_id}",
                            target: "_blank",
                            class: "flex items-center gap-2 text-sm text-blue-400 hover:text-blue-300 transition-colors",
                            span { "View on Discogs" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FilesTab(files: Vec<File>, is_loading: bool, error: Option<String>) -> Element {
    rsx! {
        if is_loading {
            div { class: "text-gray-400 text-center py-8", "Loading files..." }
        } else if let Some(ref err) = error {
            div { class: "text-red-400 text-center py-8", {err.clone()} }
        } else if files.is_empty() {
            div { class: "text-gray-400 text-center py-8", "No files found" }
        } else {
            div { class: "space-y-2",
                for file in files.iter() {
                    div { class: "flex items-center justify-between py-2 px-3 bg-gray-700/50 rounded hover:bg-gray-700 transition-colors",
                        div { class: "flex-1",
                            div { class: "text-white text-sm font-medium", {file.filename.clone()} }
                            div { class: "text-gray-400 text-xs mt-1",
                                {format!("{} • {}", format_file_size(file.file_size), file.format)}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GalleryTab(images: Vec<Image>, is_loading: bool, error: Option<String>) -> Element {
    rsx! {
        if is_loading {
            div { class: "text-gray-400 text-center py-8", "Loading images..." }
        } else if let Some(ref err) = error {
            div { class: "text-red-400 text-center py-8", {err.clone()} }
        } else if images.is_empty() {
            div { class: "text-gray-400 text-center py-8", "No images found" }
        } else {
            div { class: "grid grid-cols-2 sm:grid-cols-3 gap-4",
                for image in images.iter() {
                    div { class: "relative group",
                        div { class: if image.is_cover { "aspect-square bg-gray-700 rounded-lg overflow-clip ring-2 ring-blue-500" } else { "aspect-square bg-gray-700 rounded-lg overflow-clip" },
                            div { class: "w-full h-full flex items-center justify-center text-gray-500",
                                "Image"
                            }
                        }
                        div { class: "absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent p-2",
                            div { class: "text-xs text-white truncate", {image.filename.clone()} }
                            div { class: "flex items-center gap-2 mt-1",
                                if image.is_cover {
                                    span { class: "text-xs px-1.5 py-0.5 bg-blue-500 text-white rounded",
                                        "Cover"
                                    }
                                }
                                span { class: "text-xs text-gray-400", {image.source.clone()} }
                            }
                        }
                    }
                }
            }
        }
    }
}
