//! Release tabs section for multi-release albums

use crate::components::{ChromelessButton, Dropdown, Placement};
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
    let show_release_dropdown = use_signal(|| None::<String>);

    rsx! {
        div { class: "mb-4",
            div { class: "flex gap-1 overflow-x-auto",
                for release in releases.iter() {
                    {
                        let is_selected = selected_release_id.as_ref() == Some(&release.id);
                        let release_id = release.id.clone();
                        let torrent = torrent_info.get(&release.id).cloned().unwrap_or_default();
                        rsx! {
                            ReleaseTab {
                                key: "{release.id}",
                                release: release.clone(),
                                is_selected,
                                show_release_dropdown,
                                on_release_select: {
                                    let release_id = release_id.clone();
                                    move |_| on_release_select.call(release_id.clone())
                                },
                                is_deleting,
                                is_exporting,
                                torrent,
                                on_view_files: {
                                    let release_id = release_id.clone();
                                    move |_| on_view_files.call(release_id.clone())
                                },
                                on_export: {
                                    let release_id = release_id.clone();
                                    move |_| on_export.call(release_id.clone())
                                },
                                on_delete: {
                                    let release_id = release_id.clone();
                                    move |_| on_delete_release.call(release_id.clone())
                                },
                                on_start_seeding: on_start_seeding
                                    .map(|handler| {
                                        let release_id = release_id.clone();
                                        EventHandler::new(move |_: ()| handler.call(release_id.clone()))
                                    }),
                                on_stop_seeding: on_stop_seeding
                                    .map(|handler| {
                                        let release_id = release_id.clone();
                                        EventHandler::new(move |_: ()| handler.call(release_id.clone()))
                                    }),
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Individual release tab with dropdown menu
#[component]
fn ReleaseTab(
    release: Release,
    is_selected: bool,
    show_release_dropdown: Signal<Option<String>>,
    on_release_select: EventHandler<()>,
    is_deleting: ReadSignal<bool>,
    is_exporting: Signal<bool>,
    torrent: ReleaseTorrentInfo,
    on_view_files: EventHandler<()>,
    on_export: EventHandler<()>,
    on_delete: EventHandler<()>,
    #[props(default)] on_start_seeding: Option<EventHandler<()>>,
    #[props(default)] on_stop_seeding: Option<EventHandler<()>>,
) -> Element {
    let release_id = release.id.clone();
    let anchor_id = format!("release-tab-{}", release.id);

    // Derive is_open from the shared signal
    let is_open_memo = use_memo({
        let release_id = release_id.clone();
        move || show_release_dropdown().as_ref() == Some(&release_id)
    });
    let is_open: ReadSignal<bool> = is_open_memo.into();
    let menu_is_open = is_open();

    // Tab button styling - pill style
    let tab_class = if is_selected {
        "px-3 py-1.5 text-sm rounded-lg bg-surface-raised text-white whitespace-nowrap transition-colors"
    } else {
        "px-3 py-1.5 text-sm rounded-lg text-gray-400 hover:text-white hover:bg-hover whitespace-nowrap transition-colors"
    };

    // Three-dot menu visibility: show on hover OR when menu is open
    let menu_button_class = if menu_is_open {
        "px-2 py-1.5 text-sm rounded-lg text-gray-400 hover:text-white hover:bg-hover transition-all"
    } else {
        "px-2 py-1.5 text-sm rounded-lg text-gray-400 hover:text-white hover:bg-hover opacity-0 group-hover/tab:opacity-100 transition-all"
    };

    rsx! {
        div { class: "group/tab flex items-center gap-0.5 relative",
            ChromelessButton {
                class: Some(tab_class.to_string()),
                onclick: move |_| on_release_select.call(()),
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
            ChromelessButton {
                id: Some(anchor_id.clone()),
                disabled: is_deleting(),
                class: Some(menu_button_class.to_string()),
                onclick: {
                    let release_id = release_id.clone();
                    move |evt: MouseEvent| {
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

            Dropdown {
                anchor_id: anchor_id.clone(),
                is_open,
                on_close: move |_| show_release_dropdown.set(None),
                placement: Placement::BottomEnd,
                class: "bg-surface-overlay rounded-lg shadow-lg border border-border-subtle p-1 min-w-[160px]",

                MenuItem {
                    disabled: is_deleting() || is_exporting(),
                    onclick: move |_| {
                        show_release_dropdown.set(None);
                        on_view_files.call(());
                    },
                    "Release Info"
                }
                if torrent.has_torrent {
                    if torrent.is_seeding {
                        if let Some(ref handler) = on_stop_seeding {
                            MenuItem {
                                disabled: is_deleting() || is_exporting(),
                                onclick: {
                                    let handler = *handler;
                                    move |_| {
                                        show_release_dropdown.set(None);
                                        handler.call(());
                                    }
                                },
                                "Stop Seeding"
                            }
                        }
                    } else {
                        if let Some(ref handler) = on_start_seeding {
                            MenuItem {
                                disabled: is_deleting() || is_exporting(),
                                onclick: {
                                    let handler = *handler;
                                    move |_| {
                                        show_release_dropdown.set(None);
                                        handler.call(());
                                    }
                                },
                                "Start Seeding"
                            }
                        }
                    }
                }
                MenuItem {
                    disabled: is_deleting() || is_exporting(),
                    onclick: move |_| {
                        show_release_dropdown.set(None);
                        on_export.call(());
                    },
                    if is_exporting() {
                        "Exporting..."
                    } else {
                        "Export"
                    }
                }
                MenuItem {
                    disabled: is_deleting() || is_exporting(),
                    danger: true,
                    onclick: move |_| {
                        show_release_dropdown.set(None);
                        on_delete.call(());
                    },
                    "Delete Release"
                }
            }
        }
    }
}

/// Menu item component for dropdown menus
#[component]
fn MenuItem(
    #[props(default)] disabled: bool,
    #[props(default)] danger: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    let base = "w-full text-left px-3 py-2 text-sm rounded-md transition-colors";
    let variant = if danger {
        "text-red-400 hover:bg-red-500/10"
    } else {
        "text-gray-300 hover:bg-hover hover:text-white"
    };
    let disabled_class = if disabled {
        "opacity-50 cursor-not-allowed"
    } else {
        ""
    };

    rsx! {
        ChromelessButton {
            disabled,
            class: Some(format!("{base} {variant} {disabled_class}")),
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                if !disabled {
                    onclick.call(e);
                }
            },
            {children}
        }
    }
}
