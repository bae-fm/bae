//! Export error toast notification

use crate::components::error_toast::ErrorToast;
use dioxus::prelude::*;

#[component]
pub fn ExportErrorToast(error: String, on_dismiss: EventHandler<()>) -> Element {
    rsx! {
        ErrorToast {
            title: Some("Export Failed".to_string()),
            message: error,
            on_dismiss,
        }
    }
}
