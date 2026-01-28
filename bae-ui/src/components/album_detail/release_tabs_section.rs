//! Release tabs section for multi-release albums

use crate::components::{Button, ButtonSize, ButtonVariant, Dropdown, Placement};
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
        div { class: "mb-6 border-b border-gray-700",
            div { class: "flex gap-2 overflow-x-auto",
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

    rsx! {
        div { class: "group/tab flex items-center gap-2 relative",
            Button {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Small,
                class: Some(
                    if is_selected {
                        "text-blue-400 border-b-2 border-blue-400 whitespace-nowrap rounded-none"
                            .to_string()
                    } else {
                        "text-gray-400 hover:text-gray-300 border-b-2 border-transparent whitespace-nowrap rounded-none"
                            .to_string()
                    },
                ),
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
            Button {
                id: Some(anchor_id.clone()),
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Small,
                disabled: is_deleting(),
                class: Some("px-2 opacity-0 group-hover/tab:opacity-100 transition-opacity".to_string()),
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
                class: "bg-gray-700 rounded-lg shadow-lg overflow-clip border border-gray-600 min-w-[160px]",

                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Medium,
                    disabled: is_deleting() || is_exporting(),
                    class: Some("w-full justify-start rounded-none".to_string()),
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        if !is_deleting() && !is_exporting() {
                            show_release_dropdown.set(None);
                            on_view_files.call(());
                        }
                    },
                    "Release Info"
                }
                if torrent.has_torrent {
                    if torrent.is_seeding {
                        if let Some(ref handler) = on_stop_seeding {
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Medium,
                                disabled: is_deleting() || is_exporting(),
                                class: Some("w-full justify-start rounded-none".to_string()),
                                onclick: {
                                    let handler = *handler;
                                    move |evt: MouseEvent| {
                                        evt.stop_propagation();
                                        if !is_deleting() && !is_exporting() {
                                            show_release_dropdown.set(None);
                                            handler.call(());
                                        }
                                    }
                                },
                                "Stop Seeding"
                            }
                        }
                    } else {
                        if let Some(ref handler) = on_start_seeding {
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Medium,
                                disabled: is_deleting() || is_exporting(),
                                class: Some("w-full justify-start rounded-none".to_string()),
                                onclick: {
                                    let handler = *handler;
                                    move |evt: MouseEvent| {
                                        evt.stop_propagation();
                                        if !is_deleting() && !is_exporting() {
                                            show_release_dropdown.set(None);
                                            handler.call(());
                                        }
                                    }
                                },
                                "Start Seeding"
                            }
                        }
                    }
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Medium,
                    disabled: is_deleting() || is_exporting(),
                    loading: is_exporting(),
                    class: Some("w-full justify-start rounded-none".to_string()),
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        if !is_deleting() && !is_exporting() {
                            show_release_dropdown.set(None);
                            on_export.call(());
                        }
                    },
                    if is_exporting() {
                        "Exporting..."
                    } else {
                        "Export"
                    }
                }
                Button {
                    variant: ButtonVariant::Danger,
                    size: ButtonSize::Medium,
                    disabled: is_deleting() || is_exporting(),
                    class: Some("w-full justify-start rounded-none".to_string()),
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        if !is_deleting() && !is_exporting() {
                            show_release_dropdown.set(None);
                            on_delete.call(());
                        }
                    },
                    "Delete Release"
                }
            }
        }
    }
}
