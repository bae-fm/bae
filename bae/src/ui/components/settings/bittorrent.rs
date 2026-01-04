use crate::config::use_config;
use crate::AppContext;
use dioxus::prelude::*;
use tracing::{error, info};

#[component]
pub fn BitTorrentSection() -> Element {
    rsx! {
        div { class: "max-w-2xl space-y-6",
            h2 { class: "text-xl font-semibold text-white mb-6", "BitTorrent" }
            ListeningPortSection {}
            ConnectionLimitsSection {}
            NetworkInterfaceSection {}
            AboutSection {}
        }
    }
}

#[component]
fn ListeningPortSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let mut listen_port = use_signal(|| {
        config
            .torrent_listen_port
            .map(|p| p.to_string())
            .unwrap_or_default()
    });
    let mut enable_upnp = use_signal(|| config.torrent_enable_upnp);

    let original_port = config
        .torrent_listen_port
        .map(|p| p.to_string())
        .unwrap_or_default();
    let original_upnp = config.torrent_enable_upnp;

    let has_changes = *listen_port.read() != original_port || *enable_upnp.read() != original_upnp;

    let save_changes = move |_| {
        let new_port = listen_port.read().clone();
        let new_upnp = *enable_upnp.read();
        let mut config = app_context.config.clone();

        spawn(async move {
            is_saving.set(true);
            save_error.set(None);

            config.torrent_listen_port = new_port.parse().ok();
            config.torrent_enable_upnp = new_upnp;
            config.torrent_enable_natpmp = new_upnp; // Keep in sync

            match config.save() {
                Ok(()) => {
                    info!("Saved listening port settings");
                    is_editing.set(false);
                }
                Err(e) => {
                    error!("Failed to save config: {}", e);
                    save_error.set(Some(e.to_string()));
                }
            }
            is_saving.set(false);
        });
    };

    let cancel_edit = move |_| {
        listen_port.set(original_port.clone());
        enable_upnp.set(original_upnp);
        is_editing.set(false);
        save_error.set(None);
    };

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-6",
            div { class: "flex items-center justify-between mb-4",
                h3 { class: "text-lg font-medium text-white", "Listening Port" }
                if !*is_editing.read() {
                    button {
                        class: "px-3 py-1.5 text-sm bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                        onclick: move |_| is_editing.set(true),
                        "Edit"
                    }
                }
            }

            if *is_editing.read() {
                div { class: "space-y-4",
                    div { class: "flex items-center gap-4",
                        label { class: "text-sm text-gray-400 w-48", "Port for incoming connections:" }
                        input {
                            r#type: "number",
                            class: "w-24 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                            placeholder: "Random",
                            min: "1024",
                            max: "65535",
                            value: "{listen_port}",
                            oninput: move |e| listen_port.set(e.value()),
                        }
                        p { class: "text-xs text-gray-500", "Leave empty for random port" }
                    }
                    div { class: "flex items-center gap-3",
                        input {
                            r#type: "checkbox",
                            class: "w-4 h-4 rounded bg-gray-700 border-gray-600 text-indigo-600 focus:ring-indigo-500",
                            checked: *enable_upnp.read(),
                            onchange: move |e| enable_upnp.set(e.checked()),
                        }
                        label { class: "text-sm text-gray-300",
                            "Use UPnP / NAT-PMP port forwarding from my router"
                        }
                    }

                    SectionSaveButtons {
                        has_changes,
                        is_saving: *is_saving.read(),
                        save_error: save_error.read().clone(),
                        on_save: save_changes,
                        on_cancel: cancel_edit,
                    }
                }
            } else {
                div { class: "space-y-2 text-sm",
                    div { class: "flex items-center",
                        span { class: "text-gray-400 w-36", "Port:" }
                        span { class: "text-white font-mono",
                            if let Some(port) = config.torrent_listen_port {
                                "{port}"
                            } else {
                                "Random"
                            }
                        }
                    }
                    div { class: "flex items-center",
                        span { class: "text-gray-400 w-36", "UPnP / NAT-PMP:" }
                        span { class: if config.torrent_enable_upnp || config.torrent_enable_natpmp { "text-green-400" } else { "text-gray-500" },
                            if config.torrent_enable_upnp || config.torrent_enable_natpmp {
                                "Enabled"
                            } else {
                                "Disabled"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ConnectionLimitsSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let mut max_connections = use_signal(|| {
        config
            .torrent_max_connections
            .map(|c| c.to_string())
            .unwrap_or_default()
    });
    let mut max_connections_per_torrent = use_signal(|| {
        config
            .torrent_max_connections_per_torrent
            .map(|c| c.to_string())
            .unwrap_or_default()
    });
    let mut max_uploads = use_signal(|| {
        config
            .torrent_max_uploads
            .map(|c| c.to_string())
            .unwrap_or_default()
    });
    let mut max_uploads_per_torrent = use_signal(|| {
        config
            .torrent_max_uploads_per_torrent
            .map(|c| c.to_string())
            .unwrap_or_default()
    });

    let original_max_conn = config
        .torrent_max_connections
        .map(|c| c.to_string())
        .unwrap_or_default();
    let original_max_conn_torrent = config
        .torrent_max_connections_per_torrent
        .map(|c| c.to_string())
        .unwrap_or_default();
    let original_max_up = config
        .torrent_max_uploads
        .map(|c| c.to_string())
        .unwrap_or_default();
    let original_max_up_torrent = config
        .torrent_max_uploads_per_torrent
        .map(|c| c.to_string())
        .unwrap_or_default();

    let has_changes = *max_connections.read() != original_max_conn
        || *max_connections_per_torrent.read() != original_max_conn_torrent
        || *max_uploads.read() != original_max_up
        || *max_uploads_per_torrent.read() != original_max_up_torrent;

    let save_changes = move |_| {
        let new_max_conn = max_connections.read().clone();
        let new_max_conn_torrent = max_connections_per_torrent.read().clone();
        let new_max_up = max_uploads.read().clone();
        let new_max_up_torrent = max_uploads_per_torrent.read().clone();
        let mut config = app_context.config.clone();

        spawn(async move {
            is_saving.set(true);
            save_error.set(None);

            config.torrent_max_connections = new_max_conn.parse().ok();
            config.torrent_max_connections_per_torrent = new_max_conn_torrent.parse().ok();
            config.torrent_max_uploads = new_max_up.parse().ok();
            config.torrent_max_uploads_per_torrent = new_max_up_torrent.parse().ok();

            match config.save() {
                Ok(()) => {
                    info!("Saved connection limits");
                    is_editing.set(false);
                }
                Err(e) => {
                    error!("Failed to save config: {}", e);
                    save_error.set(Some(e.to_string()));
                }
            }
            is_saving.set(false);
        });
    };

    let cancel_edit = move |_| {
        max_connections.set(original_max_conn.clone());
        max_connections_per_torrent.set(original_max_conn_torrent.clone());
        max_uploads.set(original_max_up.clone());
        max_uploads_per_torrent.set(original_max_up_torrent.clone());
        is_editing.set(false);
        save_error.set(None);
    };

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-6",
            div { class: "flex items-center justify-between mb-4",
                h3 { class: "text-lg font-medium text-white", "Connection Limits" }
                if !*is_editing.read() {
                    button {
                        class: "px-3 py-1.5 text-sm bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                        onclick: move |_| is_editing.set(true),
                        "Edit"
                    }
                }
            }

            if *is_editing.read() {
                div { class: "space-y-3",
                    LimitRow {
                        label: "Global maximum number of connections:",
                        value: max_connections,
                        placeholder: "Unlimited",
                    }
                    LimitRow {
                        label: "Maximum connections per torrent:",
                        value: max_connections_per_torrent,
                        placeholder: "Unlimited",
                    }
                    LimitRow {
                        label: "Global maximum number of upload slots:",
                        value: max_uploads,
                        placeholder: "Unlimited",
                    }
                    LimitRow {
                        label: "Maximum upload slots per torrent:",
                        value: max_uploads_per_torrent,
                        placeholder: "Unlimited",
                    }

                    SectionSaveButtons {
                        has_changes,
                        is_saving: *is_saving.read(),
                        save_error: save_error.read().clone(),
                        on_save: save_changes,
                        on_cancel: cancel_edit,
                    }
                }
            } else {
                div { class: "space-y-2 text-sm",
                    LimitDisplay {
                        label: "Max connections:",
                        value: config.torrent_max_connections,
                    }
                    LimitDisplay {
                        label: "Max connections/torrent:",
                        value: config.torrent_max_connections_per_torrent,
                    }
                    LimitDisplay {
                        label: "Max upload slots:",
                        value: config.torrent_max_uploads,
                    }
                    LimitDisplay {
                        label: "Max upload slots/torrent:",
                        value: config.torrent_max_uploads_per_torrent,
                    }
                }
            }
        }
    }
}

#[component]
fn NetworkInterfaceSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let mut bind_interface =
        use_signal(|| config.torrent_bind_interface.clone().unwrap_or_default());

    let original_bind = config.torrent_bind_interface.clone().unwrap_or_default();

    let has_changes = *bind_interface.read() != original_bind;

    let save_changes = move |_| {
        let new_interface = bind_interface.read().clone();
        let mut config = app_context.config.clone();

        spawn(async move {
            is_saving.set(true);
            save_error.set(None);

            config.torrent_bind_interface = if new_interface.is_empty() {
                None
            } else {
                Some(new_interface)
            };

            match config.save() {
                Ok(()) => {
                    info!("Saved network interface");
                    is_editing.set(false);
                }
                Err(e) => {
                    error!("Failed to save config: {}", e);
                    save_error.set(Some(e.to_string()));
                }
            }
            is_saving.set(false);
        });
    };

    let cancel_edit = move |_| {
        bind_interface.set(original_bind.clone());
        is_editing.set(false);
        save_error.set(None);
    };

    rsx! {
        div { class: "bg-gray-800 rounded-lg p-6",
            div { class: "flex items-center justify-between mb-4",
                h3 { class: "text-lg font-medium text-white", "Network Interface" }
                if !*is_editing.read() {
                    button {
                        class: "px-3 py-1.5 text-sm bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                        onclick: move |_| is_editing.set(true),
                        "Edit"
                    }
                }
            }

            if *is_editing.read() {
                div { class: "space-y-4",
                    div { class: "space-y-2",
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500",
                            placeholder: "e.g., eth0, tun0, 192.168.1.100",
                            value: "{bind_interface}",
                            oninput: move |e| bind_interface.set(e.value()),
                        }
                        p { class: "text-xs text-gray-500",
                            "Bind to a specific interface (e.g., VPN tunnel). Leave empty for default."
                        }
                    }

                    SectionSaveButtons {
                        has_changes,
                        is_saving: *is_saving.read(),
                        save_error: save_error.read().clone(),
                        on_save: save_changes,
                        on_cancel: cancel_edit,
                    }
                }
            } else {
                div { class: "text-sm",
                    span { class: "text-gray-400", "Interface: " }
                    if config.torrent_bind_interface.is_some() {
                        span { class: "text-white font-mono",
                            "{config.torrent_bind_interface.as_ref().unwrap()}"
                        }
                    } else {
                        span { class: "text-gray-500 italic", "Default" }
                    }
                }
            }
        }
    }
}

#[component]
fn AboutSection() -> Element {
    rsx! {
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

#[component]
fn SectionSaveButtons(
    has_changes: bool,
    is_saving: bool,
    save_error: Option<String>,
    on_save: EventHandler<MouseEvent>,
    on_cancel: EventHandler<MouseEvent>,
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
                    onclick: move |e| on_save.call(e),
                    if is_saving {
                        "Saving..."
                    } else {
                        "Save"
                    }
                }
                button {
                    class: "px-4 py-2 bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                    onclick: move |e| on_cancel.call(e),
                    "Cancel"
                }
            }

            p { class: "text-xs text-gray-500", "Changes take effect on next torrent download." }
        }
    }
}

#[component]
fn LimitRow(label: &'static str, value: Signal<String>, placeholder: &'static str) -> Element {
    rsx! {
        div { class: "flex items-center gap-4",
            label { class: "text-sm text-gray-300 flex-1", "{label}" }
            input {
                r#type: "number",
                class: "w-24 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white text-right focus:outline-none focus:ring-2 focus:ring-indigo-500",
                placeholder,
                min: "1",
                value: "{value}",
                oninput: move |e| value.set(e.value()),
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
