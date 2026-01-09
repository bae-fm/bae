use super::inputs::{FolderSelector, ReleaseSelector};
use super::shared::{
    Confirmation, DetectingMetadata, DiscIdLookupError, ErrorDisplay, ExactLookup, ManualSearch,
    SelectedSource,
};
use super::smart_file_display::SmartFileDisplay;
use crate::import::MatchCandidate;
use crate::ui::components::import::ImportSource;
use crate::ui::import_context::{detection, ImportContext, ImportPhase};
use dioxus::prelude::*;
use std::rc::Rc;
use tracing::{info, warn};
#[component]
pub fn FolderImport() -> Element {
    let import_context = use_context::<Rc<ImportContext>>();
    let navigator = use_navigator();
    let folder_path = import_context.folder_path();
    let detected_metadata = import_context.detected_metadata();
    let import_phase = import_context.import_phase();
    let exact_match_candidates = import_context.exact_match_candidates();
    let selected_match_index = import_context.selected_match_index();
    let confirmed_candidate = import_context.confirmed_candidate();
    let is_looking_up = import_context.is_looking_up();
    let import_error_message = import_context.import_error_message();
    let discid_lookup_error = import_context.discid_lookup_error();
    let duplicate_album_id = import_context.duplicate_album_id();
    let folder_files = import_context.folder_files();
    let on_folder_select = {
        let import_context = import_context.clone();
        move |path: String| {
            let import_context = import_context.clone();
            spawn(async move {
                if let Err(e) = import_context.load_folder_for_import(path).await {
                    warn!("Failed to load folder: {}", e);
                }
            });
        }
    };
    let on_confirm_from_manual = {
        let import_context = import_context.clone();
        move |candidate: MatchCandidate| {
            let import_context = import_context.clone();
            let navigator = navigator;
            spawn(async move {
                if let Err(e) = import_context
                    .confirm_and_start_import(candidate, ImportSource::Folder, navigator)
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
    rsx! {
        div { class: "space-y-6",
            if *import_phase.read() == ImportPhase::FolderSelection {
                FolderSelector {
                    on_select: on_folder_select,
                    on_error: {
                        let import_context = import_context.clone();
                        move |e: String| {
                            import_context.set_import_error_message(Some(e));
                        }
                    },
                }
            } else if *import_phase.read() == ImportPhase::ReleaseSelection {
                ReleaseSelector {}
            } else {
                div { class: "space-y-6",
                    SelectedSource {
                        title: "Selected Folder".to_string(),
                        path: folder_path,
                        on_clear: on_change_folder,
                        if !folder_files.read().is_empty() {
                            div { class: "mt-4",
                                h4 { class: "text-sm font-semibold text-gray-300 uppercase tracking-wide mb-3",
                                    "Files"
                                }
                                SmartFileDisplay {
                                    files: folder_files.read().clone(),
                                    folder_path: folder_path.read().clone(),
                                }
                            }
                        }
                    }
                    if *is_looking_up.read() && *import_phase.read() == ImportPhase::MetadataDetection {
                        DetectingMetadata {
                            message: "Looking up release...".to_string(),
                            on_skip: |_| {},
                        }
                    }
                    if *import_phase.read() == ImportPhase::ManualSearch
                        && discid_lookup_error.read().is_some()
                    {
                        DiscIdLookupError {
                            error_message: discid_lookup_error,
                            is_retrying: is_looking_up,
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
                    if *import_phase.read() == ImportPhase::ExactLookup {
                        ExactLookup {
                            is_looking_up,
                            exact_match_candidates,
                            selected_match_index,
                            on_select: {
                                let import_context = import_context.clone();
                                move |index| {
                                    import_context.select_exact_match(index);
                                }
                            },
                        }
                    }
                    if *import_phase.read() == ImportPhase::ManualSearch {
                        ManualSearch {
                            detected_metadata,
                            selected_match_index,
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
                    if *import_phase.read() == ImportPhase::Confirmation {
                        Confirmation {
                            confirmed_candidate,
                            on_edit: {
                                let import_context = import_context.clone();
                                move |_| {
                                    import_context.reject_confirmation();
                                }
                            },
                            on_confirm: {
                                let on_confirm_from_manual_local = on_confirm_from_manual;
                                move || {
                                    if let Some(candidate) = confirmed_candidate.read().as_ref().cloned() {
                                        on_confirm_from_manual_local(candidate);
                                    }
                                }
                            },
                        }
                    }
                    ErrorDisplay {
                        error_message: import_error_message,
                        duplicate_album_id,
                    }
                }
            }
        }
    }
}
