//! Subsonic section view

use crate::components::{Button, ButtonSize, ButtonVariant};
use dioxus::prelude::*;

/// Subsonic section view
#[component]
pub fn SubsonicSectionView(
    /// Whether Subsonic server is enabled
    enabled: bool,
    /// Port number
    port: u16,
    /// Whether currently in edit mode
    is_editing: bool,
    /// Temporary values while editing
    edit_enabled: bool,
    edit_port: String,
    /// State flags
    is_saving: bool,
    has_changes: bool,
    save_error: Option<String>,
    /// Callbacks
    on_edit_start: EventHandler<()>,
    on_cancel: EventHandler<()>,
    on_save: EventHandler<()>,
    on_enabled_change: EventHandler<bool>,
    on_port_change: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "max-w-2xl space-y-6",
            h2 { class: "text-xl font-semibold text-white mb-6", "Subsonic Server" }

            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "flex items-center justify-between mb-4",
                    h3 { class: "text-lg font-medium text-white", "Server Settings" }
                    if !is_editing {
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Small,
                            onclick: move |_| on_edit_start.call(()),
                            "Edit"
                        }
                    }
                }

                if is_editing {
                    div { class: "space-y-4",
                        div { class: "flex items-center gap-3",
                            input {
                                r#type: "checkbox",
                                class: "w-4 h-4 rounded bg-gray-700 border-gray-600 text-indigo-600 focus:ring-indigo-500",
                                checked: edit_enabled,
                                onchange: move |e| on_enabled_change.call(e.checked()),
                            }
                            label { class: "text-sm text-gray-300", "Enable Subsonic API server" }
                        }
                        div { class: "flex items-center gap-4",
                            label { class: "text-sm text-gray-400 w-32", "Port:" }
                            input {
                                r#type: "number",
                                class: "w-24 px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white focus:outline-none focus:ring-2 focus:ring-indigo-500",
                                min: "1024",
                                max: "65535",
                                value: "{edit_port}",
                                oninput: move |e| on_port_change.call(e.value()),
                            }
                        }
                    }
                } else {
                    div { class: "space-y-2 text-sm",
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Status:" }
                            span { class: if enabled { "text-green-400" } else { "text-gray-500" },
                                if enabled {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                }
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "text-gray-400", "Port:" }
                            span { class: "text-white font-mono", "{port}" }
                        }
                        if enabled {
                            div { class: "flex items-center gap-2",
                                span { class: "text-gray-400", "URL:" }
                                span { class: "text-indigo-400 font-mono", "http://127.0.0.1:{port}" }
                            }
                        }
                    }
                }
            }

            if is_editing {
                if let Some(error) = save_error {
                    div { class: "p-3 bg-red-900/30 border border-red-700 rounded-lg text-sm text-red-300",
                        "{error}"
                    }
                }

                div { class: "flex gap-3",
                    Button {
                        disabled: !has_changes || is_saving,
                        loading: is_saving,
                        onclick: move |_| on_save.call(()),
                        if is_saving {
                            "Saving..."
                        } else {
                            "Save Changes"
                        }
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                }

                div { class: "p-4 bg-yellow-900/20 border border-yellow-700/50 rounded-lg",
                    p { class: "text-sm text-yellow-200/80",
                        "Changes require an app restart to take effect."
                    }
                }
            }

            div { class: "bg-gray-800 rounded-lg p-6",
                h3 { class: "text-lg font-medium text-white mb-4", "About Subsonic" }
                div { class: "space-y-3 text-sm text-gray-400",
                    p {
                        "The Subsonic API allows you to stream your music library to compatible apps like "
                        "Symfonium, Ultrasonic, or any Subsonic/Navidrome-compatible client."
                    }
                    p {
                        "Connect your mobile app to "
                        span { class: "font-mono text-indigo-400", "http://YOUR_IP:{port}" }
                        " (or use localhost for the same device)."
                    }
                }
            }
        }
    }
}
