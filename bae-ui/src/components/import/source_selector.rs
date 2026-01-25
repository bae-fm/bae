//! Import source selector view

use crate::components::{Button, ButtonSize, ButtonVariant};
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
            #[cfg(feature = "torrent")]
            ImportSource::Torrent,
            #[cfg(feature = "cd-rip")]
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
        div { class: "flex items-center gap-2 rounded-lg bg-gray-800/40 p-1",
            for source in ImportSource::all() {
                Button {
                    variant: if selected_source == *source { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    size: ButtonSize::Small,
                    class: Some("text-xs".to_string()),
                    onclick: {
                        let source = *source;
                        move |_| on_source_select.call(source)
                    },
                    "{source.label()}"
                }
            }
            if !cfg!(feature = "torrent") {
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Small,
                    disabled: true,
                    class: Some("text-xs text-gray-600 cursor-not-allowed".to_string()),
                    onclick: move |_| {},
                    "Torrent"
                }
            }
            if !cfg!(feature = "cd-rip") {
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Small,
                    disabled: true,
                    class: Some("text-xs text-gray-600 cursor-not-allowed".to_string()),
                    onclick: move |_| {},
                    "CD"
                }
            }
        }
    }
}
