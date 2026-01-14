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

/// Smart file display view - shows release materials as a compact grid of tiles
///
/// Displays audio, artwork, documents, and other files uniformly.
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
        // Unified materials grid - all items as square tiles
        div { class: "grid grid-cols-9 gap-1.5",
            // Audio content tile
            AudioTileView {
                audio: files.audio.clone(),
                on_cue_click: {
                    let mut viewing_text_file = viewing_text_file;
                    move |(name, _path): (String, String)| {
                        viewing_text_file.set(Some(name));
                    }
                },
            }

            // Artwork tiles
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

            // Document tiles
            for doc in files.documents.iter() {
                DocumentTileView {
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

            // Other files as simple tiles
            for file in files.other.iter() {
                OtherFileTileView { key: "{file.name}", file: file.clone() }
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

/// Audio content tile (square format)
#[component]
fn AudioTileView(audio: AudioContentInfo, on_cue_click: EventHandler<(String, String)>) -> Element {
    match audio {
        AudioContentInfo::CueFlacPairs(pairs) => {
            rsx! {
                for pair in pairs.iter() {
                    CueFlacTileView {
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
                div { class: "aspect-square bg-gray-800/50 border border-blue-500/30 rounded flex flex-col items-center justify-center p-1.5",
                    span { class: "text-xs font-semibold text-blue-300 text-center leading-tight",
                        {format!("{} tracks", tracks.len())}
                    }
                    span { class: "text-xs text-gray-400 text-center leading-tight",
                        {format_file_size(total_size)}
                    }
                }
            }
        }
        AudioContentInfo::TrackFiles(_) => rsx! {},
    }
}

/// CUE/FLAC pair tile (square format)
#[component]
fn CueFlacTileView(pair: CueFlacPairInfo, on_click: EventHandler<(String, String)>) -> Element {
    let cue_name = pair.cue_name.clone();
    let track_count = pair.track_count;
    let total_size = pair.total_size;

    rsx! {
        button {
            class: "aspect-square bg-gray-800/50 border border-purple-500/30 rounded flex flex-col items-center justify-center p-1.5 hover:bg-gray-800/70 hover:border-purple-500/50 transition-colors cursor-pointer",
            onclick: {
                let name = cue_name.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            span { class: "text-xs font-semibold text-purple-300 text-center leading-tight",
                {format!("{} tracks", track_count)}
            }
            span { class: "text-xs text-gray-400 text-center leading-tight",
                {format_file_size(total_size)}
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
            class: "relative aspect-square bg-gray-800 border border-gray-700 rounded overflow-hidden hover:border-gray-500 transition-colors group",
            onclick: move |_| on_click.call(index),
            img {
                src: "{url}",
                alt: "{filename}",
                class: "w-full h-full object-cover",
            }
            div { class: "absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity flex items-end p-1.5",
                span { class: "text-xs text-white truncate w-full", {filename.clone()} }
            }
        }
    }
}

/// Document tile (square format, clickable to view)
#[component]
fn DocumentTileView(file: FileInfo, on_click: EventHandler<(String, String)>) -> Element {
    let filename = file.name.clone();
    let file_size = file.size;

    rsx! {
        button {
            class: "aspect-square bg-gray-800 border border-gray-700 rounded flex flex-col items-center justify-center p-1.5 hover:bg-gray-750 hover:border-gray-600 transition-colors cursor-pointer",
            onclick: {
                let name = filename.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            span { class: "text-xs text-white font-medium text-center truncate w-full leading-tight",
                {file.name.clone()}
            }
            span { class: "text-xs text-gray-400 text-center leading-tight",
                {format_file_size(file_size)}
            }
        }
    }
}

/// Other file tile (square format, non-clickable)
#[component]
fn OtherFileTileView(file: FileInfo) -> Element {
    rsx! {
        div { class: "aspect-square bg-gray-800/50 border border-gray-700 rounded flex flex-col items-center justify-center p-1.5",
            span { class: "text-xs text-gray-400 text-center truncate w-full leading-tight",
                {file.name.clone()}
            }
            span { class: "text-xs text-gray-500 text-center leading-tight",
                {format_file_size(file.size)}
            }
        }
    }
}
