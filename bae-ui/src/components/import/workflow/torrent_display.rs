//! Torrent-specific display components

use super::file_list::FileListView;
use crate::components::icons::{ChevronDownIcon, ChevronRightIcon};
use crate::components::{Button, ButtonVariant};
use crate::display_types::{FileInfo, TorrentFileInfo, TorrentInfo};
use dioxus::prelude::*;

/// Tracker status for display
#[derive(Clone, Debug, PartialEq)]
pub struct TrackerStatus {
    pub url: String,
    pub status: TrackerConnectionStatus,
    pub peer_count: usize,
    pub seeders: usize,
    pub leechers: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackerConnectionStatus {
    Connected,
    Announcing,
    Error,
}

/// Torrent tracker display with expandable list
#[component]
pub fn TorrentTrackerDisplayView(trackers: Vec<TrackerStatus>) -> Element {
    let mut expanded = use_signal(|| false);

    if trackers.is_empty() {
        return rsx! {
            div { "No trackers available" }
        };
    }

    let total_peers: usize = trackers.iter().map(|ts| ts.peer_count).sum();
    let total_seeders: usize = trackers.iter().map(|ts| ts.seeders).sum();
    let total_leechers: usize = trackers.iter().map(|ts| ts.leechers).sum();

    let mut connected_count = 0;
    let mut announcing_count = 0;
    let mut error_count = 0;
    for tracker in trackers.iter() {
        match tracker.status {
            TrackerConnectionStatus::Connected => connected_count += 1,
            TrackerConnectionStatus::Announcing => announcing_count += 1,
            TrackerConnectionStatus::Error => error_count += 1,
        }
    }

    let mut summary_parts = Vec::new();
    if connected_count > 0 {
        summary_parts.push(format!("{} connected", connected_count));
    }
    if announcing_count > 0 {
        summary_parts.push(format!("{} announcing", announcing_count));
    }
    if error_count > 0 {
        summary_parts.push(format!("{} error", error_count));
    }
    let summary = if summary_parts.is_empty() {
        "No status".to_string()
    } else {
        summary_parts.join(", ")
    };

    rsx! {
        div { class: "mb-4",
            Button {
                variant: ButtonVariant::Ghost,
                class: Some(
                    "w-full justify-between p-3 bg-gray-800 border border-gray-700 hover:bg-gray-700"
                        .to_string(),
                ),
                onclick: move |_| expanded.toggle(),
                div { class: "flex items-center gap-3",
                    span { class: "text-gray-400",
                        if *expanded.read() {
                            ChevronDownIcon { class: "w-3 h-3" }
                        } else {
                            ChevronRightIcon { class: "w-3 h-3" }
                        }
                    }
                    h3 { class: "text-sm font-semibold text-gray-300 uppercase tracking-wide",
                        "Trackers"
                    }
                    if !*expanded.read() {
                        span { class: "text-xs text-gray-400", {format!("({})", summary)} }
                    }
                }
                div { class: "flex items-center gap-4 text-sm text-gray-400",
                    div {
                        span { "Total peers: " }
                        span { class: "font-medium text-white", {total_peers.to_string()} }
                    }
                    div { class: "flex items-center gap-2",
                        span { class: "px-2 py-0.5 rounded bg-green-900/30 text-green-400 border border-green-700",
                            span { "Seeders: " }
                            span { class: "font-medium", {total_seeders.to_string()} }
                        }
                        span { class: "px-2 py-0.5 rounded bg-blue-900/30 text-blue-400 border border-blue-700",
                            span { "Leechers: " }
                            span { class: "font-medium", {total_leechers.to_string()} }
                        }
                    }
                }
            }
            if *expanded.read() {
                div { class: "mt-3 space-y-2",
                    for tracker in trackers.iter() {
                        TrackerItemView { key: "{tracker.url}", tracker: tracker.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn TrackerItemView(tracker: TrackerStatus) -> Element {
    let status_text = match tracker.status {
        TrackerConnectionStatus::Connected => "Connected",
        TrackerConnectionStatus::Announcing => "Announcing",
        TrackerConnectionStatus::Error => "Error",
    };

    rsx! {
        div { class: "bg-gray-800 rounded border border-gray-700 p-3",
            div { class: "flex items-center justify-between",
                div { class: "flex-1 min-w-0",
                    p { class: "text-sm font-mono text-gray-300 truncate", {tracker.url.clone()} }
                }
                div { class: "flex items-center gap-4 ml-4",
                    span {
                        class: "text-xs px-2 py-1 rounded",
                        class: if tracker.status == TrackerConnectionStatus::Connected { "bg-green-900/30 text-green-400 border border-green-700" } else { "bg-yellow-900/30 text-yellow-400 border border-yellow-700" },
                        {status_text}
                    }
                    span { class: "text-xs text-gray-400",
                        {tracker.peer_count.to_string()}
                        span { " peers" }
                    }
                }
            }
            div { class: "mt-3 flex items-center gap-4 text-xs pt-3 border-t border-gray-700",
                div {
                    span { class: "text-gray-400", "Seeders: " }
                    span { class: "text-green-400 font-medium", {tracker.seeders.to_string()} }
                }
                div {
                    span { class: "text-gray-400", "Leechers: " }
                    span { class: "text-blue-400 font-medium", {tracker.leechers.to_string()} }
                }
            }
        }
    }
}

/// Torrent info display (expandable details)
#[component]
pub fn TorrentInfoDisplayView(info: TorrentInfo) -> Element {
    let mut expanded = use_signal(|| false);

    let format_size = |bytes: i64| -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.2} KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    };

    let format_date = |timestamp: i64| -> String {
        if timestamp == 0 {
            "Not available".to_string()
        } else {
            // Simple date formatting
            format!("Timestamp: {}", timestamp)
        }
    };

    rsx! {
        div { class: "mt-4",
            Button {
                variant: ButtonVariant::Ghost,
                class: Some(
                    "w-full justify-between p-3 bg-gray-800 border border-gray-700 hover:bg-gray-700"
                        .to_string(),
                ),
                onclick: move |_| expanded.toggle(),
                h3 { class: "text-sm font-semibold text-gray-300 uppercase tracking-wide",
                    "Details"
                }
                span { class: "text-gray-400",
                    if *expanded.read() {
                        ChevronDownIcon { class: "w-3 h-3" }
                    } else {
                        ChevronRightIcon { class: "w-3 h-3" }
                    }
                }
            }
            if *expanded.read() {
                div { class: "mt-3 space-y-4",
                    div { class: "grid grid-cols-2 gap-4",
                        div {
                            h4 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2",
                                "Name"
                            }
                            p { class: "text-sm font-medium tracking-tight text-white bg-gray-800 px-3 py-2 rounded border border-gray-700",
                                {info.name.clone()}
                            }
                        }
                        div {
                            h4 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2",
                                "Total Size"
                            }
                            p { class: "text-sm font-medium tracking-tight text-white bg-gray-800 px-3 py-2 rounded border border-gray-700",
                                {format_size(info.total_size)}
                            }
                        }
                        div {
                            h4 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2",
                                "Piece Length"
                            }
                            p { class: "text-sm font-medium tracking-tight text-white bg-gray-800 px-3 py-2 rounded border border-gray-700",
                                {format_size(info.piece_length as i64)}
                            }
                        }
                        div {
                            h4 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2",
                                "Number of Pieces"
                            }
                            p { class: "text-sm font-medium tracking-tight text-white bg-gray-800 px-3 py-2 rounded border border-gray-700",
                                {info.num_pieces.to_string()}
                            }
                        }
                        div {
                            h4 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2",
                                "Private"
                            }
                            p { class: "text-sm font-medium tracking-tight text-white bg-gray-800 px-3 py-2 rounded border border-gray-700",
                                if info.is_private {
                                    "Yes"
                                } else {
                                    "No"
                                }
                            }
                        }
                    }
                    if !info.comment.is_empty() {
                        div {
                            h4 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2",
                                "Comment"
                            }
                            p { class: "text-sm font-medium tracking-tight text-white bg-gray-800 px-3 py-2 rounded border border-gray-700 break-words",
                                {info.comment.clone()}
                            }
                        }
                    }
                    if !info.creator.is_empty() {
                        div {
                            h4 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2",
                                "Created By"
                            }
                            p { class: "text-sm font-medium tracking-tight text-white bg-gray-800 px-3 py-2 rounded border border-gray-700",
                                {info.creator.clone()}
                            }
                        }
                    }
                    if info.creation_date != 0 {
                        div {
                            h4 { class: "text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2",
                                "Creation Date"
                            }
                            p { class: "text-sm font-medium tracking-tight text-white bg-gray-800 px-3 py-2 rounded border border-gray-700",
                                {format_date(info.creation_date)}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Torrent files display (expandable list)
#[component]
pub fn TorrentFilesDisplayView(files: Vec<TorrentFileInfo>) -> Element {
    let mut expanded = use_signal(|| false);

    if files.is_empty() {
        return rsx! {
            div { "No files available" }
        };
    }

    let display_files: Vec<FileInfo> = files
        .iter()
        .map(|tf| {
            let path_parts: Vec<&str> = tf.path.split('/').collect();
            let name = path_parts.last().unwrap_or(&"unknown").to_string();
            let format = name.rsplit('.').next().unwrap_or("").to_uppercase();
            FileInfo {
                name,
                path: tf.path.clone(),
                size: tf.size as u64,
                format,
                display_url: String::new(),
            }
        })
        .collect();

    rsx! {
        div { class: "mt-4",
            Button {
                variant: ButtonVariant::Ghost,
                class: Some(
                    "w-full justify-between p-3 bg-gray-800 border border-gray-700 hover:bg-gray-700"
                        .to_string(),
                ),
                onclick: move |_| expanded.toggle(),
                h3 { class: "text-sm font-semibold text-gray-300 uppercase tracking-wide",
                    "Files"
                }
                span { class: "text-gray-400",
                    if *expanded.read() {
                        ChevronDownIcon { class: "w-3 h-3" }
                    } else {
                        ChevronRightIcon { class: "w-3 h-3" }
                    }
                }
            }
            if *expanded.read() {
                div { class: "mt-3",
                    FileListView { files: display_files }
                }
            }
        }
    }
}

/// Prompt to detect metadata from CUE/log files
#[component]
pub fn MetadataDetectionPromptView(on_detect: EventHandler<()>) -> Element {
    rsx! {
        div { class: "bg-blue-50 border border-blue-200 rounded-lg p-4 mb-4",
            div { class: "flex items-center justify-between",
                div { class: "flex-1",
                    p { class: "text-sm text-blue-900 font-medium mb-1", "Metadata files detected" }
                    p { class: "text-xs text-blue-700",
                        "CUE/log files found in torrent. Download and detect metadata automatically?"
                    }
                }
                Button { onclick: move |_| on_detect.call(()), "Detect from CUE/log files" }
            }
        }
    }
}
