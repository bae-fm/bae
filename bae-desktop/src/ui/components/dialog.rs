//! Global dialog wrapper
//!
//! Thin wrapper that bridges DialogContext to GlobalDialogView.

use crate::ui::components::dialog_context::DialogContext;
use bae_ui::GlobalDialogView;
use dioxus::prelude::*;

#[component]
pub fn GlobalDialog() -> Element {
    let dialog = use_context::<DialogContext>();

    let is_open = *dialog.is_open.read();
    let title = dialog.title();
    let message = dialog.message();
    let confirm_label = dialog.confirm_label();
    let cancel_label = dialog.cancel_label();
    let on_confirm_callback = dialog.on_confirm();

    let dialog_for_cancel = dialog.clone();
    let dialog_for_confirm = dialog.clone();

    rsx! {
        GlobalDialogView {
            is_open,
            title,
            message,
            confirm_label,
            cancel_label,
            on_cancel: move |_| {
                dialog_for_cancel.hide();
            },
            on_confirm: move |_| {
                if let Some(ref callback) = on_confirm_callback {
                    dialog_for_confirm.hide();
                    callback();
                } else {
                    dialog_for_confirm.hide();
                }
            },
        }
    }
}
