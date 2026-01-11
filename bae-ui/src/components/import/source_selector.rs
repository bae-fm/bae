//! Import source selector view

use dioxus::prelude::*;

/// Import source type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportSource {
    #[default]
    Folder,
    Torrent,
    Cd,
}

impl ImportSource {
    pub fn label(&self) -> &'static str {
        match self {
            ImportSource::Folder => "Folder",
            ImportSource::Torrent => "Torrent",
            ImportSource::Cd => "CD",
        }
    }

    pub fn all() -> &'static [ImportSource] {
        &[
            ImportSource::Folder,
            ImportSource::Torrent,
            ImportSource::Cd,
        ]
    }
}

/// Import source selector tabs
#[component]
pub fn ImportSourceSelectorView(
    selected_source: ImportSource,
    on_source_select: EventHandler<ImportSource>,
) -> Element {
    rsx! {
        div { class: "mb-4",
            div { class: "flex space-x-4 border-b border-gray-600",
                for source in ImportSource::all() {
                    button {
                        class: if selected_source == *source { "px-4 py-2 font-medium transition-colors text-blue-400 border-b-2 border-blue-400 -mb-px" } else { "px-4 py-2 font-medium transition-colors text-gray-400 hover:text-gray-300" },
                        onclick: {
                            let source = *source;
                            move |_| on_source_select.call(source)
                        },
                        "{source.label()}"
                    }
                }
            }
        }
    }
}
