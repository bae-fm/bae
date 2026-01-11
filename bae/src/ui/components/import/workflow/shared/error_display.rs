//! Error display wrappers - delegate to view components

use crate::ui::Route;
use bae_ui::components::import::{DiscIdLookupErrorView, ImportErrorDisplayView};
use dioxus::prelude::*;

#[component]
pub fn DiscIdLookupError(
    error_message: ReadSignal<Option<String>>,
    is_retrying: ReadSignal<bool>,
    on_retry: EventHandler<()>,
) -> Element {
    rsx! {
        DiscIdLookupErrorView {
            error_message: error_message.read().clone(),
            is_retrying: *is_retrying.read(),
            on_retry: move |_| on_retry.call(()),
        }
    }
}

#[component]
pub fn ErrorDisplay(
    error_message: ReadSignal<Option<String>>,
    duplicate_album_id: ReadSignal<Option<String>>,
) -> Element {
    let navigator = use_navigator();

    rsx! {
        ImportErrorDisplayView {
            error_message: error_message.read().clone(),
            duplicate_album_id: duplicate_album_id.read().clone(),
            on_view_duplicate: move |album_id: String| {
                navigator
                    .push(Route::AlbumDetail {
                        album_id,
                        release_id: String::new(),
                    });
            },
        }
    }
}
