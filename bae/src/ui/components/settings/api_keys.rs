use crate::config::use_config;
use crate::AppContext;
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
        div { class: "max-w-2xl",
            h2 { class: "text-xl font-semibold text-white mb-6", "API Keys" }

            div { class: "bg-gray-800 rounded-lg p-6",
                // Discogs API Key
                div { class: "space-y-4",
                    div { class: "flex items-center justify-between",
                        div {
                            h3 { class: "text-lg font-medium text-white", "Discogs" }
                            p { class: "text-sm text-gray-400 mt-1",
                                "Used for release metadata and cover art"
                            }
                        }
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
                            div {
                                label { class: "block text-sm font-medium text-gray-400 mb-2",
                                    "API Key"
                                }
                                input {
                                    r#type: "password",
                                    class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent",
                                    placeholder: "Enter your Discogs API key",
                                    value: "{discogs_key}",
                                    oninput: move |e| discogs_key.set(e.value()),
                                }
                            }

                            if let Some(error) = save_error.read().as_ref() {
                                div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                                    "{error}"
                                }
                            }

                            div { class: "flex gap-3",
                                button {
                                    class: "px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-500 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                                    disabled: !has_changes || *is_saving.read(),
                                    onclick: save_changes,
                                    if *is_saving.read() {
                                        "Saving..."
                                    } else {
                                        "Save"
                                    }
                                }
                                button {
                                    class: "px-4 py-2 bg-gray-700 text-gray-300 rounded-lg hover:bg-gray-600 transition-colors",
                                    onclick: cancel_edit,
                                    "Cancel"
                                }
                            }
                        }
                    } else {
                        div { class: "flex items-center gap-3",
                            div { class: "flex-1 px-4 py-2 bg-gray-700 rounded-lg text-gray-400 font-mono",
                                "••••••••••••••••"
                            }
                            span { class: "px-3 py-1 bg-green-900 text-green-300 rounded-full text-sm",
                                "Configured"
                            }
                        }
                    }
                }

                // Help text
                div { class: "mt-6 p-4 bg-gray-700/50 rounded-lg",
                    p { class: "text-sm text-gray-400",
                        "Get your Discogs API key from "
                        a {
                            class: "text-indigo-400 hover:text-indigo-300",
                            href: "https://www.discogs.com/settings/developers",
                            target: "_blank",
                            "discogs.com/settings/developers"
                        }
                    }
                }
            }
        }
    }
}
