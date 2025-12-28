use crate::ui::Route;
use dioxus::prelude::*;

/// Display DiscID lookup error with retry button
#[component]
pub fn DiscIdLookupError(
    error_message: ReadSignal<Option<String>>,
    is_retrying: ReadSignal<bool>,
    on_retry: EventHandler<()>,
) -> Element {
    if let Some(ref error) = error_message.read().as_ref() {
        rsx! {
            div { class: "bg-amber-900/30 border border-amber-700/50 rounded-lg p-4 mb-4",
                div { class: "flex items-start gap-3",
                    // Warning icon
                    svg {
                        class: "w-5 h-5 text-amber-500 flex-shrink-0 mt-0.5",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z",
                        }
                    }
                    div { class: "flex-1",
                        p { class: "text-sm text-amber-200", "{error}" }
                        div { class: "mt-3 flex gap-2",
                            button {
                                class: "px-3 py-1.5 text-sm font-medium rounded-md bg-amber-600 hover:bg-amber-500 text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *is_retrying.read(),
                                onclick: move |_| on_retry.call(()),
                                if *is_retrying.read() {
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
    } else {
        rsx! {}
    }
}

#[component]
pub fn ErrorDisplay(
    error_message: ReadSignal<Option<String>>,
    duplicate_album_id: ReadSignal<Option<String>>,
) -> Element {
    let navigator = use_navigator();

    if let Some(ref error) = error_message.read().as_ref() {
        rsx! {
            div { class: "bg-red-50 border border-red-200 rounded-lg p-4",
                p { class: "text-sm text-red-700 select-text break-words font-mono",
                    "Error: {error}"
                }
                {
                    let dup_id_opt = duplicate_album_id.read().clone();
                    if let Some(dup_id) = dup_id_opt {
                        let dup_id_clone = dup_id.clone();
                        rsx! {
                            div { class: "mt-2",
                                a {
                                    href: "#",
                                    class: "text-sm text-blue-600 hover:underline",
                                    onclick: move |_| {
                                        navigator
                                            .push(Route::AlbumDetail {
                                                album_id: dup_id_clone.clone(),
                                                release_id: String::new(),
                                            });
                                    },
                                    "View existing album"
                                }
                            }
                        }
                    } else {
                        rsx! {
                            div {}
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div {}
        }
    }
}
