use super::cd_import::CdImport;
use super::folder_import::FolderImport;
use super::torrent_import::TorrentImport;
use crate::ui::import_context::ImportContext;
use bae_ui::{ImportSource, ImportView};
use dioxus::prelude::*;
use std::rc::Rc;

#[component]
pub fn ImportPage() -> Element {
    let import_context = use_context::<Rc<ImportContext>>();
    let selected_source = import_context.selected_import_source();
    let on_source_select = {
        let import_context = import_context.clone();
        move |source: ImportSource| {
            import_context.try_switch_import_source(source);
        }
    };

    rsx! {
        ImportView { selected_source: *selected_source.read(), on_source_select,
            match *selected_source.read() {
                ImportSource::Folder => rsx! {
                    FolderImport {}
                },
                ImportSource::Torrent => rsx! {
                    TorrentImport {}
                },
                ImportSource::Cd => rsx! {
                    CdImport {}
                },
            }
        }
    }
}
