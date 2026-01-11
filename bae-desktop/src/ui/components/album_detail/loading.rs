//! Loading spinner wrapper

use bae_ui::LoadingSpinner;
use dioxus::prelude::*;

/// Loading spinner for album detail page
#[component]
pub fn AlbumDetailLoading() -> Element {
    rsx! {
        LoadingSpinner { message: "Loading album details..." }
    }
}
