//! Album cover section with action menu

use super::album_art::AlbumArt;
use crate::components::{Dropdown, Placement};
use crate::display_types::Album;
use dioxus::prelude::*;
use web_sys_x::js_sys;

/// Album cover section with action menu
/// All callbacks are required - pass noops if actions are not needed.
#[component]
pub fn AlbumCoverSection(
    album: Album,
    import_progress: Option<u8>,
    is_deleting: bool,
    is_exporting: bool,
    first_release_id: Option<String>,
    has_single_release: bool,
    // Callbacks - all required
    on_export: EventHandler<String>,
    on_delete_album: EventHandler<String>,
    on_view_release_info: EventHandler<String>,
) -> Element {
    let mut show_dropdown = use_signal(|| false);
    let is_open: ReadSignal<bool> = show_dropdown.into();
    let mut hover_cover = use_signal(|| false);
    let anchor_id = use_hook(|| format!("album-cover-{}", js_sys::Math::random() as u64));

    rsx! {
        div {
            class: "mb-6 relative",
            onmouseenter: move |_| hover_cover.set(true),
            onmouseleave: move |_| hover_cover.set(false),
            AlbumArt {
                title: album.title.clone(),
                cover_url: album.cover_url.clone(),
                import_progress,
                is_ephemeral: false,
            }

            // Show dropdown button on hover
            if hover_cover() || show_dropdown() {
                div { class: "absolute top-2 right-2 z-10",
                    button {
                        id: "{anchor_id}",
                        class: "w-8 h-8 bg-gray-800/40 hover:bg-gray-800/60 text-white rounded-lg flex items-center justify-center transition-colors",
                        disabled: import_progress.is_some() || is_deleting,
                        class: if import_progress.is_some() || is_deleting { "opacity-50 cursor-not-allowed" } else { "" },
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if !is_deleting && import_progress.is_none() {
                                show_dropdown.set(!show_dropdown());
                            }
                        },
                        div { class: "flex flex-col gap-1",
                            div { class: "w-1 h-1 bg-white rounded-full" }
                            div { class: "w-1 h-1 bg-white rounded-full" }
                            div { class: "w-1 h-1 bg-white rounded-full" }
                        }
                    }
                }
            }

            // Dropdown menu
            Dropdown {
                anchor_id: anchor_id.clone(),
                is_open,
                on_close: move |_| show_dropdown.set(false),
                placement: Placement::BottomEnd,
                class: "bg-gray-700 rounded-lg shadow-lg overflow-hidden border border-gray-600 min-w-[160px]",

                // Release Info - only for single release
                if has_single_release {
                    if let Some(ref release_id) = first_release_id {
                        button {
                            class: "w-full px-4 py-3 text-left text-white hover:bg-gray-600 transition-colors flex items-center gap-2",
                            disabled: is_deleting || is_exporting,
                            onclick: {
                                let release_id = release_id.clone();
                                move |evt| {
                                    evt.stop_propagation();
                                    show_dropdown.set(false);
                                    on_view_release_info.call(release_id.clone());
                                }
                            },
                            "Release Info"
                        }
                        button {
                            class: "w-full px-4 py-3 text-left text-white hover:bg-gray-600 transition-colors flex items-center gap-2",
                            disabled: is_deleting || is_exporting,
                            onclick: {
                                let release_id = release_id.clone();
                                move |evt| {
                                    evt.stop_propagation();
                                    show_dropdown.set(false);
                                    on_export.call(release_id.clone());
                                }
                            },
                            if is_exporting {
                                "Exporting..."
                            } else {
                                "Export"
                            }
                        }
                    }
                }
                button {
                    class: "w-full px-4 py-3 text-left text-red-400 hover:bg-gray-600 transition-colors flex items-center gap-2",
                    disabled: is_deleting,
                    onclick: {
                        let album_id = album.id.clone();
                        move |evt| {
                            evt.stop_propagation();
                            show_dropdown.set(false);
                            on_delete_album.call(album_id.clone());
                        }
                    },
                    "Delete Album"
                }
            }
        }
    }
}
