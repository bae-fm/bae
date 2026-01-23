//! Smart file display view component

use super::{ImageLightboxView, TextFileModalView};
use crate::components::icons::{DiscIcon, FileIcon, FileTextIcon, RowsIcon};
use crate::display_types::{AudioContentInfo, CategorizedFileInfo, CueFlacPairInfo, FileInfo};
use dioxus::prelude::*;

/// Base file tile container - fixed 72x72px square tiles
#[component]
fn FileTile(
    /// Background class (e.g., "bg-white/5")
    bg: &'static str,
    /// Click handler - if present, renders as button with hover states
    #[props(default)]
    on_click: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    let base =
        "w-[72px] h-[72px] flex-shrink-0 rounded-xl flex flex-col items-center justify-center p-2";

    if let Some(handler) = on_click {
        rsx! {
            button {
                class: "{base} {bg} hover:bg-white/10 transition-all duration-150 cursor-pointer",
                onclick: move |_| handler.call(()),
                {children}
            }
        }
    } else {
        rsx! {
            div { class: "{base} {bg}", {children} }
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
        span { class: "text-xs font-semibold text-center leading-tight {text_color}",
            {format!("{} tracks", track_count)}
        }
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
    /// Currently viewed text file name
    selected_text_file: Option<String>,
    /// Loaded text file content (for selected file)
    text_file_content: Option<String>,
    /// Callback when user selects a text file to view
    on_text_file_select: EventHandler<String>,
    /// Callback when user closes text file modal
    on_text_file_close: EventHandler<()>,
) -> Element {
    let mut viewing_image_index = use_signal(|| None::<usize>);

    if files.is_empty() {
        return rsx! {
            div { class: "text-gray-400 text-center py-8", "No files found" }
        };
    }

    rsx! {
        // Wrapping grid of fixed-size square tiles
        div { class: "flex flex-wrap gap-1.5 content-start",
            // Audio content tile
            AudioTileView {
                audio: files.audio.clone(),
                on_cue_click: move |(name, _path): (String, String)| on_text_file_select.call(name),
            }

            // Artwork tiles
            for (idx , file) in files.artwork.iter().enumerate() {
                GalleryThumbnailView {
                    key: "{file.path}",
                    filename: file.name.clone(),
                    url: file.display_url.clone(),
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
                    key: "{doc.path}",
                    file: doc.clone(),
                    on_click: move |(name, _path): (String, String)| on_text_file_select.call(name),
                }
            }

            // Other files as simple tiles
            for file in files.other.iter() {
                OtherFileTileView { key: "{file.path}", file: file.clone() }
            }
        }

        // Text file modal
        {
            let selected_text_file_for_memo = selected_text_file.clone();
            let is_open_memo = use_memo(move || selected_text_file_for_memo.is_some());
            let is_open: ReadSignal<bool> = is_open_memo.into();
            let filename = selected_text_file.clone().unwrap_or_default();
            let content = text_file_content
                .clone()
                .unwrap_or_else(|| "File not available".to_string());
            rsx! {
                TextFileModalView {
                    is_open,
                    filename,
                    content,
                    on_close: move |_| on_text_file_close.call(()),
                }
            }
        }

        // Image lightbox
        if let Some(index) = *viewing_image_index.read() {
            ImageLightboxView {
                images: files.artwork.clone(),
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
                        key: "{pair.cue_path}",
                        pair: pair.clone(),
                        on_click: move |(name, path)| on_cue_click.call((name, path)),
                    }
                }
            }
        }
        AudioContentInfo::TrackFiles(tracks) if !tracks.is_empty() => {
            rsx! {
                FileTile { bg: "bg-blue-500/10",
                    AudioTileContent {
                        track_count: tracks.len(),
                        format: "FLAC",
                        text_color: "text-blue-300",
                        RowsIcon { class: "w-5 h-5 text-blue-400 mb-1" }
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
            bg: "bg-purple-500/10",
            on_click: {
                let name = cue_name.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            AudioTileContent {
                track_count,
                format: "CUE/FLAC",
                text_color: "text-purple-300",
                DiscIcon { class: "w-5 h-5 text-purple-400 mb-1" }
            }
        }
    }
}

/// Gallery thumbnail - fixed 72x72px to match other tiles
#[component]
fn GalleryThumbnailView(
    filename: String,
    url: String,
    index: usize,
    on_click: EventHandler<usize>,
) -> Element {
    rsx! {
        button {
            class: "relative w-[72px] h-[72px] flex-shrink-0 rounded-xl overflow-hidden hover:ring-2 hover:ring-white/20 transition-all duration-150 group",
            onclick: move |_| on_click.call(index),
            img {
                src: "{url}",
                alt: "{filename}",
                class: "w-full h-full object-cover",
            }
            div { class: "absolute inset-0 bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity flex items-end p-1.5",
                span { class: "text-[10px] text-white truncate w-full", {filename.clone()} }
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
            bg: "bg-white/5",
            on_click: {
                let name = filename.clone();
                move |_| on_click.call((name.clone(), name.clone()))
            },
            FileTextIcon { class: "w-5 h-5 text-gray-400 mb-1" }
            span { class: "text-xs text-gray-200 text-center truncate w-full leading-tight",
                {file.name.clone()}
            }
        }
    }
}

/// Other file tile (square format, non-clickable)
#[component]
fn OtherFileTileView(file: FileInfo) -> Element {
    rsx! {
        FileTile { bg: "bg-white/5",
            FileIcon { class: "w-5 h-5 text-gray-500 mb-1" }
            span { class: "text-xs text-gray-400 text-center truncate w-full leading-tight",
                {file.name.clone()}
            }
        }
    }
}
