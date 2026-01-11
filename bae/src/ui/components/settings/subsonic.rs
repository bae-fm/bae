//! Subsonic section wrapper - handles config state, delegates UI to SubsonicSectionView

use crate::config::use_config;
use crate::AppContext;
use bae_ui::SubsonicSectionView;
use dioxus::prelude::*;
use tracing::{error, info};

#[component]
pub fn SubsonicSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let mut enabled = use_signal(|| config.subsonic_enabled);
    let mut port = use_signal(|| config.subsonic_port.to_string());

    let original_enabled = config.subsonic_enabled;
    let original_port = config.subsonic_port.to_string();

    let has_changes = *enabled.read() != original_enabled || *port.read() != original_port;

    let save_changes = move |_| {
        let new_enabled = *enabled.read();
        let new_port = port.read().clone();
        let mut config = app_context.config.clone();

        spawn(async move {
            is_saving.set(true);
            save_error.set(None);

            config.subsonic_enabled = new_enabled;
            config.subsonic_port = new_port.parse().unwrap_or(4533);

            match config.save() {
                Ok(()) => {
                    info!("Saved Subsonic settings");
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
        enabled.set(original_enabled);
        port.set(original_port.clone());
        is_editing.set(false);
        save_error.set(None);
    };

    rsx! {
        SubsonicSectionView {
            enabled: config.subsonic_enabled,
            port: config.subsonic_port,
            is_editing: *is_editing.read(),
            edit_enabled: *enabled.read(),
            edit_port: port.read().clone(),
            is_saving: *is_saving.read(),
            has_changes,
            save_error: save_error.read().clone(),
            on_edit_start: move |_| is_editing.set(true),
            on_cancel: cancel_edit,
            on_save: save_changes,
            on_enabled_change: move |val| enabled.set(val),
            on_port_change: move |val| port.set(val),
        }
    }
}
