//! Torrent input wrapper - handles file dialog and context, delegates UI to TorrentInputView

use crate::ui::components::import::TorrentInputMode;
use crate::ui::import_context::ImportContext;
use bae_ui::components::import::TorrentInputMode as ViewTorrentInputMode;
use bae_ui::components::import::TorrentInputView;
use dioxus::prelude::*;
use rfd::AsyncFileDialog;
use std::path::PathBuf;
use std::rc::Rc;

fn to_view_mode(mode: &TorrentInputMode) -> ViewTorrentInputMode {
    match mode {
        TorrentInputMode::File => ViewTorrentInputMode::File,
        TorrentInputMode::Magnet => ViewTorrentInputMode::Magnet,
    }
}

fn from_view_mode(mode: ViewTorrentInputMode) -> TorrentInputMode {
    match mode {
        ViewTorrentInputMode::File => TorrentInputMode::File,
        ViewTorrentInputMode::Magnet => TorrentInputMode::Magnet,
    }
}

#[component]
pub fn TorrentInput(
    on_file_select: EventHandler<(PathBuf, bool)>,
    on_magnet_link: EventHandler<(String, bool)>,
    on_error: EventHandler<String>,
    show_seed_checkbox: bool,
) -> Element {
    let import_context = use_context::<Rc<ImportContext>>();
    let input_mode = import_context.torrent_input_mode();
    let seed_after_download = import_context.seed_after_download();

    let on_file_button_click = {
        let import_context = import_context.clone();
        move |_| {
            let seed_flag = *import_context.seed_after_download().read();
            spawn(async move {
                if let Some(file_handle) = AsyncFileDialog::new()
                    .set_title("Select Torrent File")
                    .add_filter("Torrent", &["torrent"])
                    .pick_file()
                    .await
                {
                    on_file_select.call((file_handle.path().to_path_buf(), seed_flag));
                }
            });
        }
    };

    let on_magnet_submit = {
        let import_context = import_context.clone();
        move |link: String| {
            if link.is_empty() {
                on_error.call("Please enter a magnet link".to_string());
                return;
            }
            if !link.starts_with("magnet:") {
                on_error.call("Invalid magnet link format".to_string());
                return;
            }
            let seed_flag = *import_context.seed_after_download().read();
            on_magnet_link.call((link, seed_flag));
        }
    };

    let on_mode_change = {
        let import_context = import_context.clone();
        move |mode: ViewTorrentInputMode| {
            let bae_mode = from_view_mode(mode);
            if bae_mode == TorrentInputMode::Magnet && *input_mode.read() == TorrentInputMode::File
            {
                import_context.set_magnet_link(String::new());
            }
            import_context.set_torrent_input_mode(bae_mode);
        }
    };

    rsx! {
        div { class: "space-y-4",
            TorrentInputView {
                input_mode: to_view_mode(&input_mode.read()),
                is_dragging: false,
                on_mode_change,
                on_select_click: on_file_button_click,
                on_magnet_submit,
            }

            if show_seed_checkbox {
                div { class: "mt-4 flex items-center space-x-2",
                    input {
                        r#type: "checkbox",
                        id: "seed-after-download",
                        checked: *seed_after_download.read(),
                        onchange: {
                            let import_context = import_context.clone();
                            move |evt| {
                                import_context.set_seed_after_download(evt.checked());
                            }
                        },
                        class: "w-4 h-4 text-blue-600 border-gray-600 rounded focus:ring-blue-500 bg-gray-700",
                    }
                    label {
                        r#for: "seed-after-download",
                        class: "text-sm text-gray-300",
                        "Seed after download"
                    }
                }
            }
        }
    }
}
