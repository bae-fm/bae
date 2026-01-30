//! Menu mock component

use super::framework::{ControlRegistryBuilder, MockPage, MockPanel, Preset};
use bae_ui::components::icons::{FolderIcon, SettingsIcon, TrashIcon};
use bae_ui::{MenuDivider, MenuItem};
use dioxus::prelude::*;

/// Static menu container matching MenuDropdown styling, without floating-ui positioning
#[component]
fn StaticMenu(children: Element) -> Element {
    rsx! {
        div { class: "bg-gray-900 rounded-lg shadow-xl border border-white/5 p-1 min-w-[120px]",
            {children}
        }
    }
}

#[component]
pub fn MenuMock(initial_state: Option<String>) -> Element {
    let registry = ControlRegistryBuilder::new()
        .with_presets(vec![Preset::new("Default")])
        .build(initial_state);

    registry.use_url_sync_button();

    rsx! {
        MockPanel { current_mock: MockPage::Menu, registry,
            div { class: "p-8 bg-gray-900 min-h-full",
                h2 { class: "text-lg font-semibold text-white mb-6", "Menu Component" }

                div { class: "flex flex-wrap gap-8",
                    div {
                        h3 { class: "text-sm text-gray-400 mb-3", "With Icons" }
                        StaticMenu {
                            MenuItem { onclick: |_| {},
                                FolderIcon { class: "w-3.5 h-3.5 text-gray-400" }
                                span { "Open" }
                            }
                            MenuItem { onclick: |_| {},
                                SettingsIcon { class: "w-3.5 h-3.5 text-gray-400" }
                                span { "Settings" }
                            }
                            MenuDivider {}
                            MenuItem { danger: true, onclick: |_| {},
                                TrashIcon { class: "w-3.5 h-3.5" }
                                span { "Delete" }
                            }
                        }
                    }

                    div {
                        h3 { class: "text-sm text-gray-400 mb-3", "With Disabled Item" }
                        StaticMenu {
                            MenuItem { onclick: |_| {}, "Enabled" }
                            MenuItem { disabled: true, onclick: |_| {}, "Disabled" }
                            MenuItem { onclick: |_| {}, "Enabled" }
                        }
                    }

                    div {
                        h3 { class: "text-sm text-gray-400 mb-3", "Text Only" }
                        StaticMenu {
                            MenuItem { onclick: |_| {}, "Add" }
                            MenuItem { onclick: |_| {}, "Clear" }
                        }
                    }

                    div {
                        h3 { class: "text-sm text-gray-400 mb-3", "Danger" }
                        StaticMenu {
                            MenuItem { onclick: |_| {}, "Edit" }
                            MenuItem { danger: true, onclick: |_| {}, "Delete" }
                        }
                    }
                }
            }
        }
    }
}
