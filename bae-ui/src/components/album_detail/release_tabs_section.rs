//! Release tabs section for multi-release albums

use crate::display_types::Release;
use dioxus::prelude::*;

/// Release info for torrent display
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ReleaseTorrentInfo {
    pub has_torrent: bool,
    pub is_seeding: bool,
}

/// Release tabs section for albums with multiple releases
#[component]
pub fn ReleaseTabsSection(
    releases: Vec<Release>,
    selected_release_id: Option<String>,
    on_release_select: EventHandler<String>,
    is_deleting: ReadSignal<bool>,
    is_exporting: Signal<bool>,
    export_error: Signal<Option<String>>,
    on_view_files: EventHandler<String>,
    on_delete_release: EventHandler<String>,
    on_export: EventHandler<String>,
    // Optional: torrent info per release (keyed by release_id)
    #[props(default)] torrent_info: std::collections::HashMap<String, ReleaseTorrentInfo>,
    // Optional: torrent action callbacks
    #[props(default)] on_start_seeding: Option<EventHandler<String>>,
    #[props(default)] on_stop_seeding: Option<EventHandler<String>>,
) -> Element {
    let mut show_release_dropdown = use_signal(|| None::<String>);

    rsx! {
        div { class: "mb-6 border-b border-gray-700",
            div { class: "flex gap-2 overflow-x-auto",
                for release in releases.iter() {
                    {
                        let is_selected = selected_release_id.as_ref() == Some(&release.id);
                        let release_id = release.id.clone();
                        let release_id_for_menu = release.id.clone();
                        let torrent = torrent_info.get(&release.id).cloned().unwrap_or_default();
                        rsx! {
                            div { key: "{release.id}", class: "flex items-center gap-2 relative",
                                button {
                                    class: if is_selected { "px-4 py-2 text-sm font-medium text-blue-400 border-b-2 border-blue-400 whitespace-nowrap" } else { "px-4 py-2 text-sm font-medium text-gray-400 hover:text-gray-300 border-b-2 border-transparent whitespace-nowrap" },
                                    onclick: {
                                        let release_id = release_id.clone();
                                        move |_| on_release_select.call(release_id.clone())
                                    },
                                    {
                                        if let Some(ref name) = release.release_name {
                                            name.clone()
                                        } else if let Some(year) = release.year {
                                            format!("Release ({})", year)
                                        } else {
                                            "Release".to_string()
                                        }
                                    }
                                }
                                div { class: "relative",
                                    button {
                                        class: "px-2 py-1 text-sm text-gray-400 hover:text-gray-300 hover:bg-gray-700 rounded",
                                        disabled: is_deleting(),
                                        onclick: {
                                            let release_id = release_id_for_menu.clone();
                                            move |evt| {
                                                evt.stop_propagation();
                                                if !is_deleting() {
                                                    let current = show_release_dropdown();
                                                    if current.as_ref() == Some(&release_id) {
                                                        show_release_dropdown.set(None);
                                                    } else {
                                                        show_release_dropdown.set(Some(release_id.clone()));
                                                    }
                                                }
                                            }
                                        },
                                        "⋮"
                                    }
                                    if show_release_dropdown().as_ref() == Some(&release_id_for_menu) {
                                        ReleaseActionMenu {
                                            release_id: release_id_for_menu.clone(),
                                            has_torrent: torrent.has_torrent,
                                            is_seeding: torrent.is_seeding,
                                            is_deleting,
                                            is_exporting,
                                            on_view_files: {
                                                let release_id = release_id_for_menu.clone();
                                                move |_| {
                                                    show_release_dropdown.set(None);
                                                    on_view_files.call(release_id.clone());
                                                }
                                            },
                                            on_export: {
                                                let release_id = release_id_for_menu.clone();
                                                move |_| {
                                                    show_release_dropdown.set(None);
                                                    on_export.call(release_id.clone());
                                                }
                                            },
                                            on_delete: {
                                                let release_id = release_id_for_menu.clone();
                                                move |_| {
                                                    show_release_dropdown.set(None);
                                                    on_delete_release.call(release_id.clone());
                                                }
                                            },
                                            on_start_seeding: on_start_seeding
                                                .map(|handler| {
                                                    let release_id = release_id_for_menu.clone();
                                                    EventHandler::new(move |_: ()| {
                                                        show_release_dropdown.set(None);
                                                        handler.call(release_id.clone());
                                                    })
                                                }),
                                            on_stop_seeding: on_stop_seeding
                                                .map(|handler| {
                                                    let release_id = release_id_for_menu.clone();
                                                    EventHandler::new(move |_: ()| {
                                                        show_release_dropdown.set(None);
                                                        handler.call(release_id.clone());
                                                    })
                                                }),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if show_release_dropdown().is_some() {
            div {
                class: "fixed inset-0 z-[5]",
                onclick: move |_| show_release_dropdown.set(None),
            }
        }
    }
}

/// Release action menu (pure view component)
#[component]
fn ReleaseActionMenu(
    release_id: String,
    has_torrent: bool,
    is_seeding: bool,
    is_deleting: ReadSignal<bool>,
    is_exporting: Signal<bool>,
    on_view_files: EventHandler<()>,
    on_export: EventHandler<()>,
    on_delete: EventHandler<()>,
    #[props(default)] on_start_seeding: Option<EventHandler<()>>,
    #[props(default)] on_stop_seeding: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: "absolute right-0 top-full mt-1 bg-gray-700 rounded-lg shadow-lg overflow-hidden z-10 border border-gray-600 min-w-[160px]",
            button {
                class: "w-full px-4 py-2 text-left text-white hover:bg-gray-600 transition-colors flex items-center gap-2 text-sm",
                disabled: is_deleting() || is_exporting(),
                onclick: move |evt| {
                    evt.stop_propagation();
                    if !is_deleting() && !is_exporting() {
                        on_view_files.call(());
                    }
                },
                "Release Info"
            }
            if has_torrent {
                if is_seeding {
                    if let Some(ref handler) = on_stop_seeding {
                        button {
                            class: "w-full px-4 py-2 text-left text-white hover:bg-gray-600 transition-colors flex items-center gap-2 text-sm",
                            disabled: is_deleting() || is_exporting(),
                            onclick: {
                                let handler = *handler;
                                move |evt| {
                                    evt.stop_propagation();
                                    if !is_deleting() && !is_exporting() {
                                        handler.call(());
                                    }
                                }
                            },
                            "Stop Seeding"
                        }
                    }
                } else {
                    if let Some(ref handler) = on_start_seeding {
                        button {
                            class: "w-full px-4 py-2 text-left text-white hover:bg-gray-600 transition-colors flex items-center gap-2 text-sm",
                            disabled: is_deleting() || is_exporting(),
                            onclick: {
                                let handler = *handler;
                                move |evt| {
                                    evt.stop_propagation();
                                    if !is_deleting() && !is_exporting() {
                                        handler.call(());
                                    }
                                }
                            },
                            "Start Seeding"
                        }
                    }
                }
            }
            button {
                class: "w-full px-4 py-2 text-left text-white hover:bg-gray-600 transition-colors flex items-center gap-2 text-sm",
                disabled: is_deleting() || is_exporting(),
                onclick: move |evt| {
                    evt.stop_propagation();
                    if !is_deleting() && !is_exporting() {
                        on_export.call(());
                    }
                },
                if is_exporting() {
                    "Exporting..."
                } else {
                    "Export"
                }
            }
            button {
                class: "w-full px-4 py-2 text-left text-red-400 hover:bg-gray-600 transition-colors flex items-center gap-2 text-sm",
                disabled: is_deleting() || is_exporting(),
                onclick: move |evt| {
                    evt.stop_propagation();
                    if !is_deleting() && !is_exporting() {
                        on_delete.call(());
                    }
                },
                "Delete Release"
            }
        }
    }
}
