//! Smart file display view component

use super::{ImageLightboxView, TextFileModalView};
use crate::display_types::{AudioContentInfo, CategorizedFileInfo, CueFlacPairInfo, FileInfo};
use dioxus::prelude::*;

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Smart file display view - shows categorized files with expandable sections
///
/// Handles its own modal state for viewing text files and images.
#[component]
pub fn SmartFileDisplayView(
    /// Categorized file info
    files: CategorizedFileInfo,
    /// Image data for gallery (filename, display_url)
    image_data: Vec<(String, String)>,
    /// Text file contents keyed by filename - parent provides all content upfront
    text_file_contents: std::collections::HashMap<String, String>,
) -> Element {
    let mut show_other_files = use_signal(|| false);
    let mut viewing_text_file = use_signal(|| None::<String>);
    let mut viewing_image_index = use_signal(|| None::<usize>);

    if files.is_empty() {
        return rsx! {
            div { class: "text-gray-400 text-center py-8", "No files found" }
        };
    }

    // Get content for currently viewed text file
    let text_file_content = viewing_text_file
        .read()
        .as_ref()
        .and_then(|name| text_file_contents.get(name).cloned());

    rsx! {
        div { class: "space-y-3",
            // Audio content
            AudioContentView {
                audio: files.audio.clone(),
                on_cue_click: {
                    let mut viewing_text_file = viewing_text_file;
                    move |(name, _path): (String, String)| {
                        viewing_text_file.set(Some(name));
                    }
                },
            }

            // Artwork gallery
            if !files.artwork.is_empty() && !image_data.is_empty() {
                div { class: "grid grid-cols-3 gap-2",
                    for (idx , (filename , url)) in image_data.iter().enumerate() {
                        GalleryThumbnailView {
                            key: "{filename}",
                            filename: filename.clone(),
                            url: url.clone(),
                            index: idx,
                            on_click: {
                                let mut viewing_image_index = viewing_image_index;
                                move |idx| viewing_image_index.set(Some(idx))
                            },
                        }
                    }
                }
            }

            // Document files
            for doc in files.documents.iter() {
                TextFileItemView {
                    key: "{doc.name}",
                    file: doc.clone(),
                    on_click: {
                        let mut viewing_text_file = viewing_text_file;
                        move |(name, _path): (String, String)| {
                            viewing_text_file.set(Some(name));
                        }
                    },
                }
            }

            // Other files (expandable)
            if !files.other.is_empty() {
                div { class: "pt-2",
                    button {
                        class: "w-full px-3 py-2 text-sm text-gray-400 hover:text-gray-300 bg-gray-900/50 hover:bg-gray-800/50 border border-gray-800 hover:border-gray-700 rounded transition-colors",
                        onclick: move |_| show_other_files.toggle(),
                        div { class: "flex items-center justify-between",
                            span {
                                if *show_other_files.read() {
                                    {format!("Hide other files ({})", files.other.len())}
                                } else {
                                    {format!("Show other files ({})", files.other.len())}
                                }
                            }
                            span { class: "text-xs",
                                if *show_other_files.read() {
                                    "▲"
                                } else {
                                    "▼"
                                }
                            }
                        }
                    }
                    if *show_other_files.read() {
                        div { class: "mt-3 space-y-2",
                            for file in files.other.iter() {
                                OtherFileItemView { key: "{file.name}", file: file.clone() }
                            }
                        }
                    }
                }
            }
        }

        // Text file modal
        if let Some(filename) = viewing_text_file.read().clone() {
            TextFileModalView {
                filename: filename.clone(),
                content: text_file_content.unwrap_or_else(|| "File not available".to_string()),
                on_close: move |_| viewing_text_file.set(None),
            }
        }

        // Image lightbox
        if let Some(index) = *viewing_image_index.read() {
            ImageLightboxView {
                images: image_data.clone(),
                current_index: index,
                on_close: move |_| viewing_image_index.set(None),
                on_navigate: move |new_idx| viewing_image_index.set(Some(new_idx)),
            }
        }
    }
}

/// Audio content display (CUE/FLAC pairs or track files)
#[component]
fn AudioContentView(
    audio: AudioContentInfo,
    on_cue_click: EventHandler<(String, String)>,
) -> Element {
    match audio {
        AudioContentInfo::CueFlacPairs(pairs) => {
            rsx! {
                for pair in pairs.iter() {
                    CueFlacPairView {
                        key: "{pair.cue_name}",
                        pair: pair.clone(),
                        on_click: move |(name, path)| on_cue_click.call((name, path)),
                    }
                }
            }
        }
        AudioContentInfo::TrackFiles(tracks) if !tracks.is_empty() => {
            let total_size: u64 = tracks.iter().map(|f| f.size).sum();
            rsx! {
                div { class: "p-4 bg-gray-800/50 border border-blue-500/30 rounded-lg",
                    div { class: "flex items-start gap-3",
                        div { class: "flex-shrink-0 w-10 h-10 bg-blue-600 rounded flex items-center justify-center",
                            span { class: "text-white text-lg", "🎼" }
                        }
                        div { class: "flex-1",
                            div { class: "flex items-center gap-2 mb-1",
                                span { class: "text-sm font-semibold text-blue-300",
                                    "Track Files"
                                }
                                span { class: "px-2 py-0.5 bg-blue-600/50 text-blue-200 text-xs rounded",
                                    {format!("{} tracks", tracks.len())}
                                }
                            }
                            div { class: "text-xs text-gray-400",
                                {format!("{} total", format_file_size(total_size))}
                            }
                        }
                    }
                }
            }
        }
        AudioContentInfo::TrackFiles(_) => rsx! {},
    }
}

/// CUE/FLAC pair display
#[component]
fn CueFlacPairView(pair: CueFlacPairInfo, on_click: EventHandler<(String, String)>) -> Element {
    let cue_name = pair.cue_name.clone();
    let flac_name = pair.flac_name.clone();
    let track_count = pair.track_count;
    let total_size = pair.total_size;

    rsx! {
        div {
            class: "p-4 bg-gray-800/50 border border-purple-500/30 rounded-lg hover:bg-gray-800/70 hover:border-purple-500/50 transition-colors cursor-pointer",
            onclick: {
                let name = cue_name.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            div { class: "flex items-start gap-3",
                div { class: "flex-shrink-0 w-10 h-10 bg-purple-600 rounded flex items-center justify-center",
                    span { class: "text-white text-lg", "💿" }
                }
                div { class: "flex-1",
                    div { class: "flex items-center gap-2 mb-1",
                        span { class: "text-sm font-semibold text-purple-300", "CUE/FLAC" }
                        span { class: "px-2 py-0.5 bg-purple-600/50 text-purple-200 text-xs rounded",
                            {format!("{} tracks", track_count)}
                        }
                    }
                    div { class: "text-xs text-gray-400",
                        {format!("{} total • Click to view CUE", format_file_size(total_size))}
                    }
                    div { class: "text-xs text-gray-500 mt-1 truncate", {flac_name} }
                }
            }
        }
    }
}

/// Gallery thumbnail
#[component]
fn GalleryThumbnailView(
    filename: String,
    url: String,
    index: usize,
    on_click: EventHandler<usize>,
) -> Element {
    rsx! {
        button {
            class: "relative aspect-square bg-gray-800 border border-gray-700 rounded-lg overflow-hidden hover:border-gray-500 transition-colors group",
            onclick: move |_| on_click.call(index),
            img {
                src: "{url}",
                alt: "{filename}",
                class: "w-full h-full object-cover",
            }
            div { class: "absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity flex items-end p-2",
                span { class: "text-xs text-white truncate w-full", {filename.clone()} }
            }
        }
    }
}

/// Text file item
#[component]
fn TextFileItemView(file: FileInfo, on_click: EventHandler<(String, String)>) -> Element {
    let filename = file.name.clone();
    let file_size = file.size;

    rsx! {
        div {
            class: "p-3 bg-gray-800 border border-gray-700 rounded-lg hover:bg-gray-750 hover:border-gray-600 transition-colors cursor-pointer",
            onclick: {
                let name = filename.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            div { class: "flex items-center gap-3",
                div { class: "flex-shrink-0 w-8 h-8 bg-gray-700 rounded flex items-center justify-center",
                    span { class: "text-gray-400 text-sm", "📄" }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "text-sm text-white font-medium truncate", {file.name.clone()} }
                    div { class: "text-xs text-gray-400 mt-0.5",
                        {format!("{} • Click to view", format_file_size(file_size))}
                    }
                }
            }
        }
    }
}

/// Other file item (non-clickable)
#[component]
fn OtherFileItemView(file: FileInfo) -> Element {
    rsx! {
        div { class: "p-2 bg-gray-800 border border-gray-700 rounded",
            div { class: "flex items-center justify-between",
                div { class: "flex-1 min-w-0",
                    div { class: "text-sm text-gray-300 truncate", {file.name.clone()} }
                }
                div { class: "text-xs text-gray-500 ml-2",
                    {format!("{} • {}", format_file_size(file.size), file.format)}
                }
            }
        }
    }
}
