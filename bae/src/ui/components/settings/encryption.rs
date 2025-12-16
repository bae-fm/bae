use crate::config::use_config;
use dioxus::prelude::*;

/// Encryption section - read-only key status
#[component]
pub fn EncryptionSection() -> Element {
    let config = use_config();

    // Show first 8 and last 8 chars of key
    let key_preview = {
        let key = &config.encryption_key;
        if key.len() > 16 {
            format!("{}...{}", &key[..8], &key[key.len() - 8..])
        } else {
            "***".to_string()
        }
    };

    let key_length = config.encryption_key.len() / 2; // hex encoded, so divide by 2

    rsx! {
        div { class: "max-w-2xl",
            h2 { class: "text-xl font-semibold text-white mb-6", "Encryption" }

            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "space-y-4",
                    // Key status
                    div { class: "flex items-center justify-between py-3 border-b border-gray-700",
                        div {
                            div { class: "text-sm font-medium text-gray-400", "Encryption Key" }
                            div { class: "text-white font-mono mt-1", "{key_preview}" }
                        }
                        span { class: "px-3 py-1 bg-green-900 text-green-300 rounded-full text-sm", "Active" }
                    }

                    // Key info
                    div { class: "flex items-center justify-between py-3 border-b border-gray-700",
                        span { class: "text-sm text-gray-400", "Key Length" }
                        span { class: "text-white", "{key_length} bytes (256-bit AES)" }
                    }

                    div { class: "flex items-center justify-between py-3",
                        span { class: "text-sm text-gray-400", "Algorithm" }
                        span { class: "text-white", "AES-256-GCM" }
                    }
                }

                // Warning
                div { class: "mt-6 p-4 bg-yellow-900/30 border border-yellow-700 rounded-lg",
                    div { class: "flex items-start gap-3",
                        svg {
                            class: "w-5 h-5 text-yellow-500 mt-0.5 flex-shrink-0",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                            }
                        }
                        div {
                            p { class: "text-sm text-yellow-200 font-medium", "Encryption key cannot be changed" }
                            p { class: "text-sm text-yellow-300/70 mt-1",
                                "Changing the encryption key would make all existing encrypted data unreadable. "
                                "If you need to re-encrypt your library, export and re-import your data."
                            }
                        }
                    }
                }
            }
        }
    }
}
