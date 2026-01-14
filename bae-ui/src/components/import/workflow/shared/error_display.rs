//! Error display views for import workflow

use crate::components::icons::AlertTriangleIcon;
use dioxus::prelude::*;

/// Display DiscID lookup error with retry button
#[component]
pub fn DiscIdLookupErrorView(
    error_message: Option<String>,
    is_retrying: bool,
    on_retry: EventHandler<()>,
) -> Element {
    let Some(ref error) = error_message else {
        return rsx! {};
    };

    rsx! {
        div { class: "bg-amber-900/30 border border-amber-700/50 rounded-lg p-4 mb-4",
            div { class: "flex items-start gap-3",
                AlertTriangleIcon { class: "w-5 h-5 text-amber-500 flex-shrink-0 mt-0.5" }
                div { class: "flex-1",
                    p { class: "text-sm text-amber-200", "{error}" }
                    div { class: "mt-3 flex gap-2",
                        button {
                            class: "px-3 py-1.5 text-sm font-medium rounded-md bg-amber-600 hover:bg-amber-500 text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: is_retrying,
                            onclick: move |_| on_retry.call(()),
                            if is_retrying {
                                "Retrying..."
                            } else {
                                "Retry Lookup"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Display import error with optional link to duplicate album
#[component]
pub fn ImportErrorDisplayView(
    error_message: Option<String>,
    duplicate_album_id: Option<String>,
    on_view_duplicate: EventHandler<String>,
) -> Element {
    let Some(ref error) = error_message else {
        return rsx! {};
    };

    rsx! {
        div { class: "bg-red-50 border border-red-200 rounded-lg p-4",
            p { class: "text-sm text-red-700 select-text break-words font-mono", "Error: {error}" }
            if let Some(ref dup_id) = duplicate_album_id {
                div { class: "mt-2",
                    button {
                        class: "text-sm text-blue-600 hover:underline",
                        onclick: {
                            let dup_id = dup_id.clone();
                            move |_| on_view_duplicate.call(dup_id.clone())
                        },
                        "View existing album"
                    }
                }
            }
        }
    }
}
