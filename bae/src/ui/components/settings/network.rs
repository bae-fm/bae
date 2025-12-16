use crate::config::use_config;
use crate::AppContext;
use dioxus::prelude::*;
use tracing::{error, info};

/// Network section - torrent bind interface
#[component]
pub fn NetworkSection() -> Element {
    let config = use_config();
    let app_context = use_context::<AppContext>();

    let mut bind_interface =
        use_signal(|| config.torrent_bind_interface.clone().unwrap_or_default());
    let mut is_editing = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    let original_value = config.torrent_bind_interface.clone().unwrap_or_default();
    let has_changes = *bind_interface.read() != original_value;

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
                    info!("Saved network settings");
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
        bind_interface.set(original_value.clone());
        is_editing.set(false);
        save_error.set(None);
    };

    rsx! {
        div { class: "max-w-2xl",
            h2 { class: "text-xl font-semibold text-white mb-6", "Network" }

            div { class: "bg-gray-800 rounded-lg p-6",
                // Torrent bind interface
                div { class: "space-y-4",
                    div { class: "flex items-center justify-between",
                        div {
                            h3 { class: "text-lg font-medium text-white", "Torrent Bind Interface" }
                            p { class: "text-sm text-gray-400 mt-1", "Network interface for torrent downloads" }
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
                                label { class: "block text-sm font-medium text-gray-400 mb-2", "Interface" }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent",
                                    placeholder: "e.g., eth0, tun0, 0.0.0.0:6881",
                                    value: "{bind_interface}",
                                    oninput: move |e| bind_interface.set(e.value())
                                }
                                p { class: "text-xs text-gray-500 mt-1", "Leave empty to use the system default" }
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
                                    if *is_saving.read() { "Saving..." } else { "Save" }
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
                            div { class: "flex-1 px-4 py-2 bg-gray-700 rounded-lg font-mono",
                                if config.torrent_bind_interface.is_some() {
                                    span { class: "text-white", "{config.torrent_bind_interface.as_ref().unwrap()}" }
                                } else {
                                    span { class: "text-gray-500 italic", "Not set (uses default)" }
                                }
                            }
                        }
                    }
                }

                // Help text
                div { class: "mt-6 p-4 bg-gray-700/50 rounded-lg",
                    p { class: "text-sm text-gray-400",
                        "Use this to route torrent traffic through a specific network interface, such as a VPN tunnel. "
                        "Changes take effect on next torrent download."
                    }
                }
            }
        }
    }
}
