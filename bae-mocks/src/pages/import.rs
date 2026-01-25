//! Import page

use bae_ui::{
    CdDriveStatus, CdSelectorView, ImportSource, ImportView, TorrentInputMode, TorrentInputView,
};
use dioxus::prelude::*;

#[component]
pub fn Import() -> Element {
    let mut selected_source = use_signal(|| ImportSource::Folder);

    // Placeholder sidebar - in real app this would show releases/drives
    let sidebar = rsx! {
        div { class: "p-4 text-gray-400 text-sm", "No releases" }
    };

    rsx! {
        ImportView {
            selected_source: *selected_source.read(),
            on_source_select: move |source| selected_source.set(source),
            sidebar,

            match *selected_source.read() {
                ImportSource::Folder => rsx! {
                    FolderImportDemo {}
                },
                ImportSource::Torrent => rsx! {
                    TorrentImportDemo {}
                },
                ImportSource::Cd => rsx! {
                    CdImportDemo {}
                },
            }
        }
    }
}

#[component]
pub fn FolderImportDemo() -> Element {
    // Real folder import is mocked via FolderImportMock
    rsx! {
        div { class: "flex-1 flex items-center justify-center text-gray-400",
            "Select a folder to import (see FolderImportMock for full workflow)"
        }
    }
}

#[component]
pub fn TorrentImportDemo() -> Element {
    let mut input_mode = use_signal(|| TorrentInputMode::File);

    rsx! {
        TorrentInputView {
            input_mode: *input_mode.read(),
            on_mode_change: move |mode| input_mode.set(mode),
            on_select_click: |_| {},
            on_magnet_submit: |_| {},
        }
    }
}

#[component]
pub fn CdImportDemo() -> Element {
    rsx! {
        CdSelectorView { status: CdDriveStatus::NoDisc, on_rip_click: |_| {} }
    }
}
