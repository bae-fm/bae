//! Encryption section view

use crate::components::icons::{AlertTriangleIcon, InfoIcon};
use dioxus::prelude::*;

/// Encryption section view (read-only display)
#[component]
pub fn EncryptionSectionView(
    /// Whether encryption is configured
    is_configured: bool,
    /// Preview of the key (e.g., "abc123...xyz789")
    key_preview: String,
    /// Key length in bytes
    key_length: usize,
) -> Element {
    rsx! {
        div { class: "max-w-2xl",
            h2 { class: "text-xl font-semibold text-white mb-6", "Encryption" }
            div { class: "bg-gray-800 rounded-lg p-6",
                div { class: "space-y-4",
                    div { class: "flex items-center justify-between py-3 border-b border-gray-700",
                        div {
                            div { class: "text-sm font-medium text-gray-400", "Encryption Key" }
                            div { class: "text-white font-mono mt-1", "{key_preview}" }
                        }
                        if is_configured {
                            span { class: "px-3 py-1 bg-green-900 text-green-300 rounded-full text-sm",
                                "Active"
                            }
                        } else {
                            span { class: "px-3 py-1 bg-gray-700 text-gray-400 rounded-full text-sm",
                                "Not Set"
                            }
                        }
                    }

                    if is_configured {
                        div { class: "flex items-center justify-between py-3 border-b border-gray-700",
                            span { class: "text-sm text-gray-400", "Key Length" }
                            span { class: "text-white", "{key_length} bytes (256-bit AES)" }
                        }
                        div { class: "flex items-center justify-between py-3",
                            span { class: "text-sm text-gray-400", "Algorithm" }
                            span { class: "text-white", "AES-256-GCM" }
                        }
                    }
                }

                if is_configured {
                    div { class: "mt-6 p-4 bg-yellow-900/30 border border-yellow-700 rounded-lg",
                        div { class: "flex items-start gap-3",
                            AlertTriangleIcon { class: "w-5 h-5 text-yellow-500 mt-0.5 flex-shrink-0" }
                            div {
                                p { class: "text-sm text-yellow-200 font-medium",
                                    "Encryption key cannot be changed"
                                }
                                p { class: "text-sm text-yellow-300/70 mt-1",
                                    "Changing the encryption key would make all existing encrypted data unreadable. "
                                    "If you need to re-encrypt your library, export and re-import your data."
                                }
                            }
                        }
                    }
                } else {
                    div { class: "mt-6 p-4 bg-blue-900/30 border border-blue-700 rounded-lg",
                        div { class: "flex items-start gap-3",
                            InfoIcon { class: "w-5 h-5 text-blue-500 mt-0.5 flex-shrink-0" }
                            div {
                                p { class: "text-sm text-blue-200 font-medium",
                                    "No encryption key configured"
                                }
                                p { class: "text-sm text-blue-300/70 mt-1",
                                    "An encryption key will be generated automatically when you create a storage profile with encryption enabled."
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
