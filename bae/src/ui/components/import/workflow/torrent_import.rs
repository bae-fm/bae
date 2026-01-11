//! Torrent import workflow - handles torrent file/magnet link imports

use super::inputs::TorrentInput;
use super::shared::{
    Confirmation, DiscIdLookupError, ErrorDisplay, ExactLookup, ManualSearch, SelectedSource,
};
use crate::import::MatchCandidate;
use crate::torrent::ffi::TorrentInfo as BaeTorrentInfo;
use crate::ui::components::import::ImportSource;
use crate::ui::import_context::{detection, ImportContext, ImportPhase};
use bae_ui::components::import::{
    MetadataDetectionPromptView, TorrentFilesDisplayView, TorrentInfoDisplayView,
    TorrentTrackerDisplayView, TrackerConnectionStatus, TrackerStatus,
};
use bae_ui::display_types::{TorrentFileInfo, TorrentInfo as DisplayTorrentInfo};
use dioxus::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;
use tracing::{info, warn};

/// Convert bae TorrentInfo to display TorrentInfo
fn to_display_torrent_info(info: &BaeTorrentInfo) -> DisplayTorrentInfo {
    DisplayTorrentInfo {
        name: info.name.clone(),
        total_size: info.total_size,
        piece_length: info.piece_length,
        num_pieces: info.num_pieces,
        is_private: info.is_private,
        comment: info.comment.clone(),
        creator: info.creator.clone(),
        creation_date: info.creation_date,
        files: info
            .files
            .iter()
            .map(|f| TorrentFileInfo {
                path: f.path.clone(),
                size: f.size,
            })
            .collect(),
        trackers: info.trackers.clone(),
    }
}

/// Generate mock tracker statuses from tracker URLs
fn generate_tracker_statuses(trackers: &[String]) -> Vec<TrackerStatus> {
    trackers
        .iter()
        .enumerate()
        .map(|(idx, url)| {
            let peer_count = 15 + (idx * 7) % 35;
            let seeders = peer_count / 4;
            let leechers = peer_count - seeders;
            let status = if url.contains("udp") {
                TrackerConnectionStatus::Connected
            } else {
                let progress = idx % 3;
                if progress == 2 {
                    TrackerConnectionStatus::Connected
                } else {
                    TrackerConnectionStatus::Announcing
                }
            };
            TrackerStatus {
                url: url.clone(),
                status,
                peer_count,
                seeders,
                leechers,
            }
        })
        .collect()
}

#[component]
pub fn TorrentImport() -> Element {
    let navigator = use_navigator();
    let import_context = use_context::<Rc<ImportContext>>();

    let on_torrent_file_select = {
        let import_context = import_context.clone();
        move |(path, seed_flag): (PathBuf, bool)| {
            let import_context = import_context.clone();
            spawn(async move {
                if let Err(e) = import_context
                    .load_torrent_for_import(path, seed_flag)
                    .await
                {
                    warn!("Failed to load torrent: {}", e);
                }
            });
        }
    };

    let on_magnet_link = move |(magnet, seed_after_download): (String, bool)| {
        let _ = (magnet, seed_after_download);
        info!("Magnet link selection not yet implemented");
    };

    let on_torrent_error = {
        let import_context = import_context.clone();
        move |error: String| {
            import_context.set_import_error_message(Some(error));
        }
    };

    let on_confirm_from_manual = {
        let import_context = import_context.clone();
        move |candidate: MatchCandidate| {
            let import_context = import_context.clone();
            let navigator = navigator;
            spawn(async move {
                if let Err(e) = import_context
                    .confirm_and_start_import(candidate, ImportSource::Torrent, navigator)
                    .await
                {
                    warn!("Failed to confirm and start import: {}", e);
                }
            });
        }
    };

    let on_change_folder = {
        let import_context = import_context.clone();
        EventHandler::new(move |()| {
            import_context.reset();
        })
    };

    let has_cue_files_for_manual = {
        let folder_files = import_context.folder_files();
        let files = folder_files.read();
        let result = files
            .documents
            .iter()
            .any(|f| f.format.to_lowercase() == "cue" || f.format.to_lowercase() == "log");
        drop(files);
        result
    };

    // Prepare torrent display data
    let torrent_info_signal = import_context.torrent_info();
    let torrent_info_read = torrent_info_signal.read();
    let tracker_statuses = torrent_info_read
        .as_ref()
        .map(|info| generate_tracker_statuses(&info.trackers))
        .unwrap_or_default();
    let display_info = torrent_info_read.as_ref().map(to_display_torrent_info);
    let torrent_files = display_info
        .as_ref()
        .map(|info| info.files.clone())
        .unwrap_or_default();
    drop(torrent_info_read);

    rsx! {
        div {
            if *import_context.import_phase().read() == ImportPhase::FolderSelection {
                TorrentInput {
                    on_file_select: on_torrent_file_select,
                    on_magnet_link,
                    on_error: on_torrent_error,
                    show_seed_checkbox: false,
                }
            } else {
                div { class: "space-y-6",
                    SelectedSource {
                        title: "Selected Torrent".to_string(),
                        path: import_context.folder_path(),
                        on_clear: on_change_folder,

                        TorrentTrackerDisplayView { trackers: tracker_statuses }

                        if let Some(info) = display_info.clone() {
                            TorrentInfoDisplayView { info }
                        }

                        TorrentFilesDisplayView { files: torrent_files }
                    }

                    if *import_context.import_phase().read() == ImportPhase::ExactLookup {
                        ExactLookup {
                            is_looking_up: import_context.is_looking_up(),
                            exact_match_candidates: import_context.exact_match_candidates(),
                            selected_match_index: import_context.selected_match_index(),
                            on_select: {
                                let import_context = import_context.clone();
                                move |index| {
                                    import_context.select_exact_match(index);
                                }
                            },
                        }
                    }

                    if *import_context.import_phase().read() == ImportPhase::ManualSearch {
                        if import_context.discid_lookup_error().read().is_some() {
                            DiscIdLookupError {
                                error_message: import_context.discid_lookup_error(),
                                is_retrying: import_context.is_looking_up(),
                                on_retry: {
                                    let import_context = import_context.clone();
                                    move |_| {
                                        let import_context = import_context.clone();
                                        spawn(async move {
                                            info!("Retrying DiscID lookup...");
                                            detection::retry_discid_lookup(&import_context).await;
                                        });
                                    }
                                },
                            }
                        }

                        if has_cue_files_for_manual && import_context.detected_metadata().read().is_none()
                            && !*import_context.is_detecting().read()
                        {
                            MetadataDetectionPromptView {
                                on_detect: {
                                    let import_context = import_context.clone();
                                    move |_| {
                                        let import_context = import_context.clone();
                                        spawn(async move {
                                            if let Err(e) = import_context.retry_torrent_metadata_detection().await {
                                                warn!("Failed to retry metadata detection: {}", e);
                                            }
                                        });
                                    }
                                },
                            }
                        }

                        ManualSearch {
                            detected_metadata: import_context.detected_metadata(),
                            selected_match_index: import_context.selected_match_index(),
                            on_match_select: {
                                let import_context = import_context.clone();
                                move |index| {
                                    import_context.set_selected_match_index(Some(index));
                                }
                            },
                            on_confirm: {
                                let import_context = import_context.clone();
                                move |candidate: MatchCandidate| {
                                    import_context.confirm_candidate(candidate);
                                }
                            },
                        }
                    }

                    if *import_context.import_phase().read() == ImportPhase::Confirmation {
                        Confirmation {
                            confirmed_candidate: import_context.confirmed_candidate(),
                            on_edit: {
                                let import_context = import_context.clone();
                                move || {
                                    import_context.reject_confirmation();
                                }
                            },
                            on_confirm: {
                                let on_confirm_from_manual_local = on_confirm_from_manual;
                                let import_context = import_context.clone();
                                move || {
                                    if let Some(candidate) = import_context
                                        .confirmed_candidate()
                                        .read()
                                        .as_ref()
                                        .cloned()
                                    {
                                        on_confirm_from_manual_local(candidate);
                                    }
                                }
                            },
                        }
                    }

                    ErrorDisplay {
                        error_message: import_context.import_error_message(),
                        duplicate_album_id: import_context.duplicate_album_id(),
                    }
                }
            }
        }
    }
}
