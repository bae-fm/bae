//! BitTorrent section wrapper - handles config state, delegates UI to BitTorrentSectionView

use crate::config::use_config;
use crate::AppContext;
use bae_ui::{BitTorrentSectionView, BitTorrentSettings};
use dioxus::prelude::*;
use tracing::{error, info};

#[component]
pub fn BitTorrentSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    let mut editing_section = use_signal(|| Option::<String>::None);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    // Edit state for listening port
    let mut listen_port = use_signal(|| {
        config
            .torrent_listen_port
            .map(|p| p.to_string())
            .unwrap_or_default()
    });
    let mut enable_upnp = use_signal(|| config.torrent_enable_upnp);

    // Edit state for connection limits
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

    // Edit state for network interface
    let mut bind_interface =
        use_signal(|| config.torrent_bind_interface.clone().unwrap_or_default());

    // Original values for change detection
    let original_port = config
        .torrent_listen_port
        .map(|p| p.to_string())
        .unwrap_or_default();
    let original_upnp = config.torrent_enable_upnp;
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
    let original_bind = config.torrent_bind_interface.clone().unwrap_or_default();

    let has_changes = match editing_section.read().as_deref() {
        Some("port") => {
            *listen_port.read() != original_port || *enable_upnp.read() != original_upnp
        }
        Some("limits") => {
            *max_connections.read() != original_max_conn
                || *max_connections_per_torrent.read() != original_max_conn_torrent
                || *max_uploads.read() != original_max_up
                || *max_uploads_per_torrent.read() != original_max_up_torrent
        }
        Some("interface") => *bind_interface.read() != original_bind,
        _ => false,
    };

    let settings = BitTorrentSettings {
        listen_port: config.torrent_listen_port,
        enable_upnp: config.torrent_enable_upnp,
        enable_natpmp: config.torrent_enable_natpmp,
        max_connections: config.torrent_max_connections,
        max_connections_per_torrent: config.torrent_max_connections_per_torrent,
        max_uploads: config.torrent_max_uploads,
        max_uploads_per_torrent: config.torrent_max_uploads_per_torrent,
        bind_interface: config.torrent_bind_interface.clone(),
    };

    let save_changes = {
        let app_context = app_context.clone();
        move |_| {
            let section = editing_section.read().clone();
            let mut config = app_context.config.clone();

            let new_port = listen_port.read().clone();
            let new_upnp = *enable_upnp.read();
            let new_max_conn = max_connections.read().clone();
            let new_max_conn_torrent = max_connections_per_torrent.read().clone();
            let new_max_up = max_uploads.read().clone();
            let new_max_up_torrent = max_uploads_per_torrent.read().clone();
            let new_interface = bind_interface.read().clone();

            spawn(async move {
                is_saving.set(true);
                save_error.set(None);

                match section.as_deref() {
                    Some("port") => {
                        config.torrent_listen_port = new_port.parse().ok();
                        config.torrent_enable_upnp = new_upnp;
                        config.torrent_enable_natpmp = new_upnp;
                    }
                    Some("limits") => {
                        config.torrent_max_connections = new_max_conn.parse().ok();
                        config.torrent_max_connections_per_torrent =
                            new_max_conn_torrent.parse().ok();
                        config.torrent_max_uploads = new_max_up.parse().ok();
                        config.torrent_max_uploads_per_torrent = new_max_up_torrent.parse().ok();
                    }
                    Some("interface") => {
                        config.torrent_bind_interface = if new_interface.is_empty() {
                            None
                        } else {
                            Some(new_interface)
                        };
                    }
                    _ => {}
                }

                match config.save() {
                    Ok(()) => {
                        info!("Saved BitTorrent settings");
                        editing_section.set(None);
                    }
                    Err(e) => {
                        error!("Failed to save config: {}", e);
                        save_error.set(Some(e.to_string()));
                    }
                }
                is_saving.set(false);
            });
        }
    };

    let cancel_edit = move |_| {
        // Reset to original values
        listen_port.set(original_port.clone());
        enable_upnp.set(original_upnp);
        max_connections.set(original_max_conn.clone());
        max_connections_per_torrent.set(original_max_conn_torrent.clone());
        max_uploads.set(original_max_up.clone());
        max_uploads_per_torrent.set(original_max_up_torrent.clone());
        bind_interface.set(original_bind.clone());
        editing_section.set(None);
        save_error.set(None);
    };

    rsx! {
        BitTorrentSectionView {
            settings,
            editing_section: editing_section.read().clone(),
            edit_listen_port: listen_port.read().clone(),
            edit_enable_upnp: *enable_upnp.read(),
            edit_max_connections: max_connections.read().clone(),
            edit_max_connections_per_torrent: max_connections_per_torrent.read().clone(),
            edit_max_uploads: max_uploads.read().clone(),
            edit_max_uploads_per_torrent: max_uploads_per_torrent.read().clone(),
            edit_bind_interface: bind_interface.read().clone(),
            is_saving: *is_saving.read(),
            has_changes,
            save_error: save_error.read().clone(),
            on_edit_section: move |section: String| editing_section.set(Some(section)),
            on_cancel_edit: cancel_edit,
            on_save: save_changes,
            on_listen_port_change: move |val| listen_port.set(val),
            on_enable_upnp_change: move |val| enable_upnp.set(val),
            on_max_connections_change: move |val| max_connections.set(val),
            on_max_connections_per_torrent_change: move |val| max_connections_per_torrent.set(val),
            on_max_uploads_change: move |val| max_uploads.set(val),
            on_max_uploads_per_torrent_change: move |val| max_uploads_per_torrent.set(val),
            on_bind_interface_change: move |val| bind_interface.set(val),
        }
    }
}
