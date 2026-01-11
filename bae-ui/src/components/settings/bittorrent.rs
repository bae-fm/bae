//! BitTorrent section view

use dioxus::prelude::*;

/// BitTorrent settings display data
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BitTorrentSettings {
    pub listen_port: Option<u16>,
    pub enable_upnp: bool,
    pub enable_natpmp: bool,
    pub max_connections: Option<i32>,
    pub max_connections_per_torrent: Option<i32>,
    pub max_uploads: Option<i32>,
    pub max_uploads_per_torrent: Option<i32>,
    pub bind_interface: Option<String>,
}

/// BitTorrent section view
#[component]
pub fn BitTorrentSectionView(
    settings: BitTorrentSettings,
    /// Which section is currently being edited (None = display mode)
    editing_section: Option<String>,
    /// Temporary values while editing
    edit_listen_port: String,
    edit_enable_upnp: bool,
    edit_max_connections: String,
    edit_max_connections_per_torrent: String,
    edit_max_uploads: String,
    edit_max_uploads_per_torrent: String,
    edit_bind_interface: String,
    /// State flags
    is_saving: bool,
    has_changes: bool,
    save_error: Option<String>,
    /// Callbacks
    on_edit_section: EventHandler<String>,
    on_cancel_edit: EventHandler<()>,
    on_save: EventHandler<()>,
    on_listen_port_change: EventHandler<String>,
    on_enable_upnp_change: EventHandler<bool>,
    on_max_connections_change: EventHandler<String>,
    on_max_connections_per_torrent_change: EventHandler<String>,
    on_max_uploads_change: EventHandler<String>,
    on_max_uploads_per_torrent_change: EventHandler<String>,
    on_bind_interface_change: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "max-w-2xl space-y-6",
            h2 { class: "text-xl font-semibold text-white mb-6", "BitTorrent" }

            // Listening Port Section
            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Listening Port" }
                    if editing_section.as_deref() != Some("port") {
                        button {
                            class: "px-3 py-1.5 text-sm bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                            onclick: move |_| on_edit_section.call("port".to_string()),
                            "Edit"
                        }
                    }
                }

                if editing_section.as_deref() == Some("port") {
                    div { class: "space-y-4",
                        div { class: "flex items-center gap-4",
                            label { class: "text-sm text-gray-400 w-48", "Port for incoming connections:" }
                            input {
                                r#type: "number",
                                class: "w-24 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                placeholder: "Random",
                                min: "1024",
                                max: "65535",
                                value: "{edit_listen_port}",
                                oninput: move |e| on_listen_port_change.call(e.value()),
                            }
                            p { class: "text-xs text-gray-500", "Leave empty for random port" }
                        }
                        div { class: "flex items-center gap-3",
                            input {
                                r#type: "checkbox",
                                class: "w-4 h-4 rounded bg-gray-700 border-gray-600 text-indigo-600 focus:ring-indigo-500",
                                checked: edit_enable_upnp,
                                onchange: move |e| on_enable_upnp_change.call(e.checked()),
                            }
                            label { class: "text-sm text-gray-300",
                                "Use UPnP / NAT-PMP port forwarding from my router"
                            }
                        }

                        SectionSaveButtons {
                            has_changes,
                            is_saving,
                            save_error: save_error.clone(),
                            on_save,
                            on_cancel: on_cancel_edit,
                        }
                    }
                } else {
                    div { class: "space-y-2 text-sm",
                        div { class: "flex items-center",
                            span { class: "text-gray-400 w-36", "Port:" }
                            span { class: "text-white font-mono",
                                if let Some(port) = settings.listen_port {
                                    "{port}"
                                } else {
                                    "Random"
                                }
                            }
                        }
                        div { class: "flex items-center",
                            span { class: "text-gray-400 w-36", "UPnP / NAT-PMP:" }
                            span { class: if settings.enable_upnp || settings.enable_natpmp { "text-green-400" } else { "text-gray-500" },
                                if settings.enable_upnp || settings.enable_natpmp {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                }
                            }
                        }
                    }
                }
            }

            // Connection Limits Section
            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Connection Limits" }
                    if editing_section.as_deref() != Some("limits") {
                        button {
                            class: "px-3 py-1.5 text-sm bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                            onclick: move |_| on_edit_section.call("limits".to_string()),
                            "Edit"
                        }
                    }
                }

                if editing_section.as_deref() == Some("limits") {
                    div { class: "space-y-3",
                        LimitRow {
                            label: "Global maximum number of connections:",
                            value: edit_max_connections.clone(),
                            placeholder: "Unlimited",
                            on_change: on_max_connections_change,
                        }
                        LimitRow {
                            label: "Maximum connections per torrent:",
                            value: edit_max_connections_per_torrent.clone(),
                            placeholder: "Unlimited",
                            on_change: on_max_connections_per_torrent_change,
                        }
                        LimitRow {
                            label: "Global maximum number of upload slots:",
                            value: edit_max_uploads.clone(),
                            placeholder: "Unlimited",
                            on_change: on_max_uploads_change,
                        }
                        LimitRow {
                            label: "Maximum upload slots per torrent:",
                            value: edit_max_uploads_per_torrent.clone(),
                            placeholder: "Unlimited",
                            on_change: on_max_uploads_per_torrent_change,
                        }

                        SectionSaveButtons {
                            has_changes,
                            is_saving,
                            save_error: save_error.clone(),
                            on_save,
                            on_cancel: on_cancel_edit,
                        }
                    }
                } else {
                    div { class: "space-y-2 text-sm",
                        LimitDisplay {
                            label: "Max connections:",
                            value: settings.max_connections,
                        }
                        LimitDisplay {
                            label: "Max connections/torrent:",
                            value: settings.max_connections_per_torrent,
                        }
                        LimitDisplay {
                            label: "Max upload slots:",
                            value: settings.max_uploads,
                        }
                        LimitDisplay {
                            label: "Max upload slots/torrent:",
                            value: settings.max_uploads_per_torrent,
                        }
                    }
                }
            }

            // Network Interface Section
            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Network Interface" }
                    if editing_section.as_deref() != Some("interface") {
                        button {
                            class: "px-3 py-1.5 text-sm bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                            onclick: move |_| on_edit_section.call("interface".to_string()),
                            "Edit"
                        }
                    }
                }

                if editing_section.as_deref() == Some("interface") {
                    div { class: "space-y-4",
                        div { class: "space-y-2",
                            input {
                                r#type: "text",
                                class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                placeholder: "e.g., eth0, tun0, 192.168.1.100",
                                value: "{edit_bind_interface}",
                                oninput: move |e| on_bind_interface_change.call(e.value()),
                            }
                            p { class: "text-xs text-gray-500",
                                "Bind to a specific interface (e.g., VPN tunnel). Leave empty for default."
                            }
                        }

                        SectionSaveButtons {
                            has_changes,
                            is_saving,
                            save_error: save_error.clone(),
                            on_save,
                            on_cancel: on_cancel_edit,
                        }
                    }
                } else {
                    div { class: "text-sm",
                        span { class: "text-gray-400", "Interface: " }
                        if let Some(ref iface) = settings.bind_interface {
                            span { class: "text-white font-mono", "{iface}" }
                        } else {
                            span { class: "text-gray-500 italic", "Default" }
                        }
                    }
                }
            }

            // About Section
            div { class: "bg-gray-800 rounded-lg p-6",
                h3 { class: "text-lg font-medium text-white mb-4", "About BitTorrent in bae" }
                div { class: "space-y-3 text-sm text-gray-400",
                    p {
                        "bae uses BitTorrent to download music from torrent files or magnet links. "
                        "Downloaded files are imported into your library using your selected storage profile."
                    }
                    p {
                        "If your storage profile has encryption enabled, all imported files (audio, cover art, metadata) "
                        "are encrypted before storage."
                    }
                }
            }
        }
    }
}

#[component]
fn SectionSaveButtons(
    has_changes: bool,
    is_saving: bool,
    save_error: Option<String>,
    on_save: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "pt-4 space-y-3",
            if let Some(error) = save_error {
                div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                    "{error}"
                }
            }

            div { class: "flex gap-3",
                button {
                    class: "px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: !has_changes || is_saving,
                    onclick: move |_| on_save.call(()),
                    if is_saving {
                        "Saving..."
                    } else {
                        "Save"
                    }
                }
                button {
                    class: "px-4 py-2 bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }

            p { class: "text-xs text-gray-500", "Changes take effect on next torrent download." }
        }
    }
}

#[component]
fn LimitRow(
    label: &'static str,
    value: String,
    placeholder: &'static str,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "flex items-center gap-4",
            label { class: "text-sm text-gray-300 flex-1", "{label}" }
            input {
                r#type: "number",
                class: "w-24 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white text-right focus:outline-none focus:ring-2 focus:ring-indigo-500",
                placeholder,
                min: "1",
                value: "{value}",
                oninput: move |e| on_change.call(e.value()),
            }
        }
    }
}

#[component]
fn LimitDisplay(label: &'static str, value: Option<i32>) -> Element {
    rsx! {
        div { class: "flex items-center",
            span { class: "text-gray-400 w-48", "{label}" }
            span { class: "text-white",
                if let Some(v) = value {
                    "{v}"
                } else {
                    "Unlimited"
                }
            }
        }
    }
}
