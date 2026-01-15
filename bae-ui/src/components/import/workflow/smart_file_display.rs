//! Smart file display view component

use super::{ImageLightboxView, TextFileModalView};
use crate::components::icons::{DiscIcon, FileIcon, FileTextIcon, RowsIcon};
use crate::display_types::{AudioContentInfo, CategorizedFileInfo, CueFlacPairInfo, FileInfo};
use dioxus::prelude::*;

/// Base file tile container - captures common layout (aspect-square, rounded, padding, etc.)
#[component]
fn FileTile(
    /// Background class (e.g., "bg-gray-800/50")
    bg: &'static str,
    /// Border class (e.g., "border-blue-500/30")
    border: &'static str,
    /// Click handler - if present, renders as button with hover states
    #[props(default)]
    on_click: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    let base = "aspect-square border rounded flex flex-col items-center justify-center p-1.5";

    if let Some(handler) = on_click {
        rsx! {
            button {
                class: "{base} {bg} {border} hover:bg-gray-800/70 transition-colors cursor-pointer",
                onclick: move |_| handler.call(()),
                {children}
            }
        }
    } else {
        rsx! {
            div { class: "{base} {bg} {border}", {children} }
        }
    }
}

/// Audio tile content (icon + track count + format label)
#[component]
fn AudioTileContent(
    track_count: usize,
    format: &'static str,
    /// Text color class (e.g., "text-blue-300")
    text_color: &'static str,
    children: Element,
) -> Element {
    rsx! {
        {children}
        span { class: "text-xs font-semibold text-center leading-tight {text_color}", {format!("{} tracks", track_count)} }
        span { class: "text-[10px] text-gray-400 text-center leading-tight", "{format}" }
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
            rsx! {
                FileTile {
                    bg: "bg-gray-800/50",
                    border: "border-blue-500/30",
                    AudioTileContent {
                        track_count: tracks.len(),
                        format: "FLAC",
                        text_color: "text-blue-300",
                        RowsIcon { class: "w-5 h-5 text-blue-400 mb-0.5" }
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

    rsx! {
        FileTile {
            bg: "bg-gray-800/50",
            border: "border-purple-500/30",
            on_click: {
                let name = cue_name.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            AudioTileContent {
                track_count,
                format: "CUE/FLAC",
                text_color: "text-purple-300",
                DiscIcon { class: "w-5 h-5 text-purple-400 mb-0.5" }
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

    rsx! {
        FileTile {
            bg: "bg-gray-800",
            border: "border-gray-700",
            on_click: {
                let name = filename.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            FileTextIcon { class: "w-5 h-5 text-gray-400 mb-0.5" }
            span { class: "text-xs text-white font-medium text-center truncate w-full leading-tight",
                {file.name.clone()}
            }
        }
    }
}

/// Other file tile (square format, non-clickable)
#[component]
fn OtherFileTileView(file: FileInfo) -> Element {
    rsx! {
        FileTile {
            bg: "bg-gray-800/50",
            border: "border-gray-700",
            FileIcon { class: "w-5 h-5 text-gray-500 mb-0.5" }
            span { class: "text-xs text-gray-400 text-center truncate w-full leading-tight",
                {file.name.clone()}
            }
        }
    }
}
