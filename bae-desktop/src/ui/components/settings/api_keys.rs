//! API Keys section wrapper - handles config state, delegates UI to ApiKeysSectionView

use crate::ui::use_config;
use crate::AppContext;
use bae_ui::ApiKeysSectionView;
use dioxus::prelude::*;
use tracing::{error, info};

/// API Keys section - Discogs key management
#[component]
pub fn ApiKeysSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    let mut discogs_key = use_signal(|| config.discogs_api_key.clone());
    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let has_changes = *discogs_key.read() != config.discogs_api_key;
    let discogs_configured = config.discogs_api_key.is_some();

    let save_changes = move |_| {
        let new_key = discogs_key.read().clone();
        let mut config = app_context.config.clone();
        spawn(async move {
            is_saving.set(true);
            save_error.set(None);
            config.discogs_api_key = new_key;
            match config.save() {
                Ok(()) => {
                    info!("Saved Discogs API key");
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
        discogs_key.set(config.discogs_api_key.clone());
        is_editing.set(false);
        save_error.set(None);
    };

    rsx! {
        ApiKeysSectionView {
            discogs_configured,
            discogs_key_value: discogs_key.read().clone().unwrap_or_default(),
            is_editing: *is_editing.read(),
            is_saving: *is_saving.read(),
            has_changes,
            save_error: save_error.read().clone(),
            on_edit_start: move |_| is_editing.set(true),
            on_key_change: move |val: String| {
                discogs_key.set(if val.is_empty() { None } else { Some(val) });
            },
            on_save: save_changes,
            on_cancel: cancel_edit,
        }
    }
}
