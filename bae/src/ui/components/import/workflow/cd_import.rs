use super::inputs::CdRipper;
use super::shared::{
    Confirmation, DiscIdLookupError, ErrorDisplay, ExactLookup, ManualSearch, SelectedSource,
};
use crate::import::MatchCandidate;
use crate::ui::components::import::ImportSource;
use crate::ui::import_context::{detection, ImportContext, ImportPhase};
use dioxus::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;
use tracing::{info, warn};
#[component]
pub fn CdImport() -> Element {
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
    let duplicate_album_id = import_context.duplicate_album_id();
    let cd_toc_info = import_context.cd_toc_info();
    let on_drive_select = {
        let import_context = import_context.clone();
        move |drive_path: PathBuf| {
            let import_context = import_context.clone();
            let drive_path_str = drive_path.to_string_lossy().to_string();
            spawn(async move {
                use crate::cd::CdDrive;
                let drive = CdDrive {
                    device_path: drive_path.clone(),
                    name: drive_path_str.clone(),
                };
                match drive.read_toc() {
                    Ok(toc) => {
                        import_context.set_cd_toc_info(Some((
                            toc.disc_id.clone(),
                            toc.first_track,
                            toc.last_track,
                        )));
                        if let Err(e) = import_context
                            .load_cd_for_import(drive_path_str, toc.disc_id)
                            .await
                        {
                            warn!("Failed to load CD: {}", e);
                        }
                    }
                    Err(e) => {
                        import_context.set_is_looking_up(false);
                        import_context.set_import_error_message(Some(format!(
                            "Failed to read CD TOC: {}",
                            e
                        )));
                    }
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
                    .confirm_and_start_import(candidate, ImportSource::Cd, navigator)
                    .await
                {
                    warn!("Failed to confirm and start import: {}", e);
                }
            });
        }
    };
    let on_change_folder = {
        let import_context_clone = import_context.clone();
        EventHandler::new(move |()| {
            import_context_clone.reset();
        })
    };
    rsx! {
        div { class: "space-y-6",
            if *import_phase.read() == ImportPhase::FolderSelection {
                CdRipper {
                    on_drive_select,
                    on_error: {
                        let import_context = import_context.clone();
                        move |e: String| {
                            import_context.set_import_error_message(Some(e));
                        }
                    },
                }
            } else {
                div { class: "space-y-6",
                    SelectedSource {
                        title: "Selected CD".to_string(),
                        path: folder_path,
                        on_clear: on_change_folder,
                        if let Some((disc_id, first_track, last_track)) = cd_toc_info.read().as_ref() {
                            div { class: "mt-4 p-4 bg-blue-50 border border-blue-200 rounded-lg",
                                div { class: "space-y-2",
                                    div { class: "flex items-center",
                                        span { class: "text-sm font-medium text-gray-700 w-24",
                                            "DiscID:"
                                        }
                                        span { class: "text-sm text-gray-900 font-mono",
                                            "{disc_id}"
                                        }
                                    }
                                    div { class: "flex items-center",
                                        span { class: "text-sm font-medium text-gray-700 w-24",
                                            "Tracks:"
                                        }
                                        span { class: "text-sm text-gray-900",
                                            "{last_track - first_track + 1} tracks ({first_track}-{last_track})"
                                        }
                                    }
                                }
                            }
                        } else if *is_looking_up.read() {
                            div { class: "mt-4 p-4 bg-gray-50 border border-gray-200 rounded-lg text-center",
                                p { class: "text-sm text-gray-600", "Reading CD table of contents..." }
                            }
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
                        if import_context.discid_lookup_error().read().is_some() {
                            DiscIdLookupError {
                                error_message: import_context.discid_lookup_error(),
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
                                move || {
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
